//! Port of `conrod/vlm_providers.py` and `conrod/vlm.py`.
//!
//! Everything that is *what to ask* -- the prompt, the schema, how a reply
//! maps onto a [`VehicleDescription`] -- is the same regardless of which
//! provider answers. Everything provider-specific -- the endpoint, the auth
//! header, how an image and a schema get shaped into that API's own
//! request, and how to pull the model's JSON back out of its own response
//! envelope -- is behind the four `*_request` / `parse_*_response`
//! functions below, which is what `vlm_providers.py` used to be.
//!
//! Uses `ureq` (blocking) rather than an async client: nothing else in this
//! crate runs an async runtime, and pulling in tokio for one HTTP call per
//! crop would be the tail wagging the dog.

use conrod_vision::imageops::{Filter, Rgb};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::settings::Settings;

pub const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
const GEMINI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent";

// ── errors ───────────────────────────────────────────────────────────────

/// Every way a call to a provider can fail.
///
/// `Stopped` and `Misconfigured` are `BaseException` in the Python, not
/// `Exception` -- deliberately off the branch every per-crop reader wraps
/// itself in with `except Exception`, so a Stop or a run that has given up
/// cannot be swallowed as "this one frame could not be read". The Rust
/// equivalent of "off that branch" is simply not being caught by whatever
/// turns the other variants into an empty [`VehicleDescription`]; see
/// [`describe`].
#[derive(Debug, Clone, PartialEq)]
pub enum VlmError {
    /// The scan was stopped while waiting for a rate limit to lift.
    Stopped,
    /// Every call is failing the same way and the configuration is why.
    Misconfigured(String),
    /// An HTTP status the retry/rate-limit gate did not resolve: a fatal
    /// status (400/401/403/404/405/422), or the retry budget ran out on a
    /// 5xx/408.
    Status { status: u16, message: String },
    /// A timeout or connection failure.
    Transport(String),
    /// The provider answered, but not with something `describe`/`identify_burst`
    /// can use: malformed JSON, or a reply missing the part that carries it.
    BadReply(String),
    /// `settings.vlm_provider` names a provider this build does not have.
    UnknownProvider(String),
}

impl std::fmt::Display for VlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VlmError::Stopped => write!(f, "stopped while waiting for the provider"),
            VlmError::Misconfigured(m) => write!(f, "{m}"),
            VlmError::Status { status, message } => {
                if message.is_empty() {
                    write!(f, "HTTP {status}")
                } else {
                    write!(f, "HTTP {status} {message}")
                }
            }
            VlmError::Transport(m) => write!(f, "{m}"),
            VlmError::BadReply(m) => write!(f, "{m}"),
            VlmError::UnknownProvider(p) => write!(f, "unknown vision provider '{p}'"),
        }
    }
}

impl std::error::Error for VlmError {}

// ── rate limits ──────────────────────────────────────────────────────────
// A shoot is thousands of crops and every cloud provider meters them, so a
// 429 is a normal part of a scan rather than an error in it. The wait is
// shared across every worker calling into one `RateGate`: without a common
// gate each discovers the limit separately and keeps hammering while the
// others back off.

/// Statuses worth retrying at all (as opposed to a fatal 4xx, which retrying
/// cannot fix).
pub const RETRY_STATUSES: [u16; 7] = [408, 429, 500, 502, 503, 504, 529];
/// A rate limit: waited out for as long as it takes rather than spending the
/// retry budget, because giving up on it produces a hole in the shoot that
/// looks exactly like a frame the model had nothing to say about.
pub const RATE_LIMIT_STATUSES: [u16; 2] = [429, 529];
/// The request itself is wrong; retrying changes nothing.
pub const FATAL_STATUSES: [u16; 6] = [400, 401, 403, 404, 405, 422];
/// Consecutive unusable answers before the run gives up on the configuration.
pub const GIVE_UP_AFTER: u32 = 5;
/// The longest backoff invented for ourselves. A provider's own Retry-After
/// is obeyed exactly as given, however long -- it is a fact about when the
/// limit lifts, not a guess, and clamping it just buys another refusal.
pub const MAX_WAIT: f64 = 60.0;
/// How long a single sleep runs before the stop flag is checked again.
pub const WAIT_SLICE: f64 = 0.5;

/// What lets [`RateGate`] be driven by a fake clock in tests instead of
/// actually sleeping: `tests/rate_limits.rs`' fakes hold a virtual clock
/// that `sleep` advances, exactly mirroring how `tests/test_rate_limits.py`
/// monkeypatches `time.sleep` while leaving `time.monotonic` alone.
pub trait Clock: Send + Sync {
    /// Monotonic seconds. Only differences between two calls are meaningful.
    fn now(&self) -> f64;
    fn sleep(&self, seconds: f64);
}

/// The real clock: `std::time::Instant` and `std::thread::sleep`.
pub struct RealClock {
    start: std::time::Instant,
}

impl Default for RealClock {
    fn default() -> Self {
        RealClock {
            start: std::time::Instant::now(),
        }
    }
}

impl Clock for RealClock {
    fn now(&self) -> f64 {
        self.start.elapsed().as_secs_f64()
    }
    fn sleep(&self, seconds: f64) {
        if seconds > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(seconds));
        }
    }
}

/// The shared "not before" gate plus the stop flag every wait is
/// interruptible by. Port of the module-level `_gate`/`_not_before` and
/// `_stop_check` in `vlm_providers.py`.
pub struct RateGate {
    clock: Arc<dyn Clock>,
    not_before: Mutex<f64>,
    stop_check: Mutex<Option<Arc<dyn Fn() -> bool + Send + Sync>>>,
}

impl RateGate {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        RateGate {
            clock,
            not_before: Mutex::new(0.0),
            stop_check: Mutex::new(None),
        }
    }

    /// Give the waiters a way to notice the scan was stopped. Port of
    /// `set_stop_check`.
    pub fn set_stop_check(&self, check: Option<Arc<dyn Fn() -> bool + Send + Sync>>) {
        *self.stop_check.lock().unwrap() = check;
    }

    /// Sleep, looking up often enough to notice a Stop. Port of `_sleep`.
    /// Sliced only when there is a stop check to run -- without one, that
    /// would be a loop waking up hundreds of times to ask a question nobody
    /// is answering.
    pub fn sleep_interruptible(&self, seconds: f64) -> Result<(), VlmError> {
        if seconds <= 0.0 {
            return Ok(());
        }
        let check = self.stop_check.lock().unwrap().clone();
        let Some(check) = check else {
            self.clock.sleep(seconds);
            return Ok(());
        };
        let deadline = self.clock.now() + seconds;
        loop {
            if check() {
                return Err(VlmError::Stopped);
            }
            let left = deadline - self.clock.now();
            if left <= 0.0 {
                return Ok(());
            }
            self.clock.sleep(left.min(WAIT_SLICE));
        }
    }

    /// Ask every worker to wait, not just the one that was refused. Port of
    /// `_hold_off`: never moved backwards.
    pub fn hold_off(&self, seconds: f64) {
        let mut not_before = self.not_before.lock().unwrap();
        let candidate = self.clock.now() + seconds;
        if candidate > *not_before {
            *not_before = candidate;
        }
    }

    /// Port of `_wait_turn`.
    pub fn wait_turn(&self) -> Result<(), VlmError> {
        let delay = *self.not_before.lock().unwrap() - self.clock.now();
        self.sleep_interruptible(delay)
    }
}

/// How long the provider asked for, or a backed-off guess. Port of
/// `_retry_after`.
///
/// A stated `Retry-After` is taken at face value and uncapped, whether it is
/// plain seconds or an HTTP-date (parsed with the `httpdate` crate, which --
/// unlike Python's more permissive `email.utils.parsedate_to_datetime` --
/// only reads the RFC 7231 IMF-fixdate form; that is the only form a real
/// `Retry-After` header uses in practice).
pub fn retry_after(headers: &[(String, String)], attempt: u32, jitter: f64) -> f64 {
    if let Some(header) = header_get(headers, "retry-after") {
        if let Ok(seconds) = header.trim().parse::<f64>() {
            return seconds.max(0.0);
        }
        if let Ok(when) = httpdate::parse_http_date(header.trim()) {
            let seconds = when
                .duration_since(SystemTime::now())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            return seconds.max(0.0);
        }
    }
    // Jittered, so several workers refused at once do not all come back in
    // the same instant and trip the limit again together.
    (2f64.powi(attempt as i32) + jitter).min(MAX_WAIT)
}

fn header_get<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// A cheap, non-cryptographic `[0, 1)` value for the jitter in [`retry_after`].
/// Exactness does not matter here -- only that concurrent workers do not all
/// land on the same instant -- so this avoids pulling in a `rand` crate for
/// one call site.
pub fn jitter() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    // splitmix64
    let mut z = nanos.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// A response reduced to what the retry gate and the parsers need: no
/// binding to any particular HTTP client, so [`send_with_retries`] can be
/// unit-tested with a canned sequence of these and no network at all.
#[derive(Debug, Clone)]
pub struct RawResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// Make the request, waiting out rate limits rather than failing on one.
/// Port of `_send`.
///
/// `send` is retried in place: it performs the actual HTTP call (or, in a
/// test, returns the next canned outcome) and is invoked again for every
/// attempt, exactly like the `lambda: client.post(...)` the Python passes
/// in. A rate limit (429/529) is waited out for as long as it takes and
/// never spends the retry budget; a possibly-broken 5xx/408 or a transport
/// error does, and still gives up once it is spent.
pub fn send_with_retries(
    gate: &RateGate,
    settings: &Settings,
    what: &str,
    mut send: impl FnMut() -> Result<RawResponse, VlmError>,
    mut on_wait: impl FnMut(&str),
) -> Result<RawResponse, VlmError> {
    let attempts = settings.vlm_max_retries.max(1);
    let mut spent = 0u32;
    let mut waited = 0.0f64;
    let last = loop {
        gate.wait_turn()?;
        match send() {
            Ok(resp) if resp.status < 400 => return Ok(resp),
            Ok(resp) => {
                let status = resp.status;
                let message = response_error_message(&resp.body);
                if !RETRY_STATUSES.contains(&status) {
                    return Err(VlmError::Status { status, message });
                }
                let limited = RATE_LIMIT_STATUSES.contains(&status);
                if !limited {
                    spent += 1;
                    if spent >= attempts {
                        break VlmError::Status { status, message };
                    }
                }
                let wait = retry_after(&resp.headers, spent, jitter());
                gate.hold_off(wait);
                if limited {
                    waited += wait;
                    on_wait(&format!(
                        "{what} is rate limiting; waiting {wait:.0}s ({:.0} min so far). \
                         The scan is paused, not skipping frames.",
                        waited / 60.0
                    ));
                } else {
                    on_wait(&format!(
                        "{what} returned {status}; waiting {wait:.0}s (attempt {spent} of {attempts})"
                    ));
                }
            }
            Err(VlmError::Transport(msg)) => {
                spent += 1;
                if spent >= attempts {
                    break VlmError::Transport(msg);
                }
                let wait = retry_after(&[], spent, jitter());
                gate.hold_off(wait);
                on_wait(&format!(
                    "{what} did not answer ({msg}); waiting {wait:.0}s (attempt {spent} of {attempts})"
                ));
            }
            Err(other) => return Err(other),
        }
    };
    Err(last)
}

// ── fatal-run counting ──────────────────────────────────────────────────
// A model name the provider does not have, or a credential that cannot call
// this endpoint, comes back the same way for every crop; retrying changes
// nothing, so this is what notices and stops the run rather than quietly
// finishing with 3% of the album named. Port of `note_fatal`/`clear_fatal`/
// `given_up`.

#[derive(Default)]
struct FatalState {
    run: u32,
    said: String,
}

pub struct FatalTracker {
    state: Mutex<FatalState>,
}

impl Default for FatalTracker {
    fn default() -> Self {
        FatalTracker {
            state: Mutex::new(FatalState::default()),
        }
    }
}

impl FatalTracker {
    /// Count one answer that means the configuration is wrong. Returns the
    /// new run length.
    pub fn note_fatal(&self, said: &str) -> u32 {
        let mut state = self.state.lock().unwrap();
        state.run += 1;
        state.said = said.to_string();
        state.run
    }

    /// A call worked, so whatever came before it was not the configuration.
    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap();
        state.run = 0;
        state.said.clear();
    }

    /// The reason the run should stop, or `None` while it should carry on.
    pub fn given_up(&self) -> Option<String> {
        let state = self.state.lock().unwrap();
        (state.run >= GIVE_UP_AFTER).then(|| state.said.clone())
    }
}

/// Record one failed call and say whether the configuration -- not the
/// frame -- is why. Port of the fatal-tracking half of `vlm._failed` (the
/// log line it also writes belongs to a log module this crate does not
/// have yet).
pub fn note_failure(tracker: &FatalTracker, who: &str, err: &VlmError) -> Result<(), VlmError> {
    let status = match err {
        VlmError::Status { status, .. } => Some(*status),
        _ => None,
    };
    if !status.is_some_and(|s| FATAL_STATUSES.contains(&s)) {
        tracker.clear();
        return Ok(());
    }
    let said = brief(err);
    let run = tracker.note_fatal(&said);
    if run < GIVE_UP_AFTER {
        return Ok(());
    }
    Err(VlmError::Misconfigured(format!(
        "{who} refused the last {run} vehicles the same way, so it will refuse the rest: \
         {said}. Nothing was identified. Check the model name and API key in Settings, \
         then run Identify again."
    )))
}

/// One line naming the cause, in the provider's own words where there is one.
/// Port of `vlm._brief`.
fn brief(err: &VlmError) -> String {
    match err {
        VlmError::Status { status, message } => {
            format!("HTTP {status} {message}").trim().to_string()
        }
        other => other.to_string(),
    }
}

/// What the provider actually said, whichever envelope it said it in. Port
/// of `vlm.provider_message`, taking the already-fetched body rather than
/// an httpx response.
fn response_error_message(body: &[u8]) -> String {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return String::from_utf8_lossy(body)
            .trim()
            .chars()
            .take(200)
            .collect();
    };
    match value.get("error") {
        Some(Value::String(s)) => s.chars().take(200).collect(),
        Some(Value::Object(map)) => map
            .get("message")
            .or_else(|| map.get("type"))
            .or_else(|| map.get("status"))
            .map(|v| python_str(v).chars().take(200).collect())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

// ── Ollama hosts ─────────────────────────────────────────────────────────
// One machine's GPU is the common case; vlm_extra_hosts adds more, typically
// a second GPU elsewhere on the network. Port of `HostPool` and the pool
// registry in `vlm_providers.py`.

/// Hands out an Ollama host to whichever worker asks, one at a time per
/// host. Backed by a bounded channel pre-loaded with one token per host --
/// `acquire` blocks on the receiver until a token (a host) is available,
/// `release` sends it back. A faster host's token comes back sooner and is
/// handed out again sooner, so it naturally does more of the work without
/// this needing to know which host that is.
pub struct HostPool {
    tx: std::sync::mpsc::SyncSender<String>,
    rx: Mutex<std::sync::mpsc::Receiver<String>>,
}

impl HostPool {
    pub fn new(hosts: Vec<String>) -> Self {
        let (tx, rx) = std::sync::mpsc::sync_channel(hosts.len().max(1));
        for host in hosts {
            tx.send(host).expect("channel sized for every host");
        }
        HostPool {
            tx,
            rx: Mutex::new(rx),
        }
    }

    pub fn acquire(&self) -> String {
        self.rx
            .lock()
            .unwrap()
            .recv()
            .expect("a HostPool is never dropped while a caller still holds a sender")
    }

    pub fn release(&self, host: String) {
        let _ = self.tx.send(host);
    }
}

/// The shared pool for one exact set of hosts, keyed by the host list
/// itself: concurrent workers must contend over the *same* pool of tokens,
/// so a fresh pool per call would let every worker think every host was
/// free. Port of `_pool_for`.
#[derive(Default)]
pub struct HostPools {
    pools: Mutex<HashMap<Vec<String>, Arc<HostPool>>>,
}

impl HostPools {
    pub fn pool_for(&self, hosts: Vec<String>) -> Arc<HostPool> {
        let mut pools = self.pools.lock().unwrap();
        pools
            .entry(hosts.clone())
            .or_insert_with(|| Arc::new(HostPool::new(hosts)))
            .clone()
    }
}

// ── request building ─────────────────────────────────────────────────────
// Every provider takes the same inputs -- a prompt, already-encoded base64
// JPEG images, a JSON schema and a token budget -- and is asked for
// something to POST: the URL, headers, query parameters and JSON body.
// Kept pure (no network) so the fixtures generated from the Python side by
// `tools/gen_vlm_fixtures.py` can be compared directly; see `tests/vlm.rs`.

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub json: Value,
}

pub fn ollama_request(
    host: &str,
    model: &str,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> ProviderRequest {
    let host = host.trim_end_matches('/');
    ProviderRequest {
        url: format!("{host}/api/generate"),
        headers: vec![],
        query: vec![],
        json: json!({
            "model": model,
            "prompt": prompt,
            "images": images,
            "stream": false,
            "format": schema,
            "options": {"temperature": 0.0, "num_predict": num_predict},
        }),
    }
}

pub fn openai_request(
    model: &str,
    api_key: &str,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> ProviderRequest {
    let mut content = vec![json!({"type": "text", "text": prompt})];
    for image in images {
        content.push(json!({
            "type": "image_url",
            "image_url": {"url": format!("data:image/jpeg;base64,{image}")},
        }));
    }
    // Strict structured outputs require every property listed as required
    // and additionalProperties set explicitly -- both already true of the
    // schema except the second, so it is added to a clone rather than the
    // shared schema every other provider also uses as-is.
    let mut strict_schema = schema.clone();
    if let Value::Object(map) = &mut strict_schema {
        map.insert("additionalProperties".into(), Value::Bool(false));
    }
    ProviderRequest {
        url: OPENAI_URL.into(),
        headers: vec![("Authorization".into(), format!("Bearer {api_key}"))],
        query: vec![],
        json: json!({
            "model": model,
            "messages": [{"role": "user", "content": content}],
            "response_format": {
                "type": "json_schema",
                "json_schema": {"name": "vehicle", "schema": strict_schema, "strict": true},
            },
            "max_tokens": num_predict,
            "temperature": 0.0,
        }),
    }
}

/// Claude Code's tokens are prefixed distinctly enough to recognise, which
/// is what makes an unset key type safe to infer rather than assume.
const CLAUDE_CODE_PREFIXES: [&str; 2] = ["sk-ant-oat01-", "sk-ant-ort01-"];

/// Which kind of credential this is: `"api-key"` or `"claude-code"`. Port
/// of `anthropic_key_kind`.
pub fn anthropic_key_kind(settings: &Settings) -> &'static str {
    let chosen = settings.anthropic_key_kind.trim().to_lowercase();
    if chosen == "api-key" || chosen == "claude-code" {
        return if chosen == "api-key" {
            "api-key"
        } else {
            "claude-code"
        };
    }
    let key = settings.vlm_api_key.trim();
    if CLAUDE_CODE_PREFIXES.iter().any(|p| key.starts_with(p)) {
        "claude-code"
    } else {
        "api-key"
    }
}

/// Whichever single header the chosen kind of credential belongs on. Port
/// of `anthropic_auth`.
pub fn anthropic_auth(settings: &Settings) -> Vec<(String, String)> {
    if anthropic_key_kind(settings) == "claude-code" {
        vec![(
            "Authorization".into(),
            format!("Bearer {}", settings.vlm_api_key),
        )]
    } else {
        vec![("x-api-key".into(), settings.vlm_api_key.clone())]
    }
}

pub fn anthropic_request(
    settings: &Settings,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> ProviderRequest {
    let mut content = vec![json!({"type": "text", "text": prompt})];
    for image in images {
        content.push(json!({
            "type": "image",
            "source": {"type": "base64", "media_type": "image/jpeg", "data": image},
        }));
    }
    let mut headers = vec![("anthropic-version".into(), ANTHROPIC_VERSION.to_string())];
    headers.extend(anthropic_auth(settings));
    ProviderRequest {
        url: ANTHROPIC_URL.into(),
        headers,
        query: vec![],
        json: json!({
            "model": settings.vlm_model,
            "max_tokens": num_predict,
            "tools": [{
                "name": "describe_vehicle",
                "description": "Record the extracted vehicle fields.",
                "input_schema": schema,
            }],
            "tool_choice": {"type": "tool", "name": "describe_vehicle"},
            "messages": [{"role": "user", "content": content}],
        }),
    }
}

pub fn gemini_request(
    model: &str,
    api_key: &str,
    prompt: &str,
    images: &[String],
    num_predict: i64,
) -> ProviderRequest {
    let mut parts = vec![json!({"text": prompt})];
    for image in images {
        parts.push(json!({"inline_data": {"mime_type": "image/jpeg", "data": image}}));
    }
    ProviderRequest {
        url: GEMINI_URL.replace("{model}", model),
        headers: vec![],
        query: vec![("key".into(), api_key.to_string())],
        json: json!({
            "contents": [{"role": "user", "parts": parts}],
            // Gemini's schema dialect is not the one Ollama/OpenAI use --
            // nullable fields are "nullable": true rather than a
            // ["string", "null"] union -- so the shared schema is not
            // reusable here; responseMimeType alone still forces valid
            // JSON, and the prompt spells out the wanted structure.
            "generationConfig": {
                "temperature": 0.0,
                "maxOutputTokens": num_predict,
                "responseMimeType": "application/json",
            },
        }),
    }
}

/// How each provider is spelled when shown to someone.
pub fn display_name(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "ollama" => "Ollama".into(),
        "openai" => "OpenAI".into(),
        "anthropic" => "Anthropic".into(),
        "gemini" => "Gemini".into(),
        other => other.to_string(),
    }
}

/// Build the request for whichever provider is configured. Port of the
/// dispatch half of `vlm_providers.call` (the sending half is
/// [`VlmClient::call`]).
pub fn build_request(
    settings: &Settings,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> Result<ProviderRequest, VlmError> {
    let provider = if settings.vlm_provider.is_empty() {
        "ollama".to_string()
    } else {
        settings.vlm_provider.to_lowercase()
    };
    match provider.as_str() {
        "ollama" => Ok(ollama_request(
            &settings.vlm_host,
            &settings.vlm_model,
            prompt,
            images,
            schema,
            num_predict,
        )),
        "openai" => Ok(openai_request(
            &settings.vlm_model,
            &settings.vlm_api_key,
            prompt,
            images,
            schema,
            num_predict,
        )),
        "anthropic" => Ok(anthropic_request(
            settings,
            prompt,
            images,
            schema,
            num_predict,
        )),
        "gemini" => Ok(gemini_request(
            &settings.vlm_model,
            &settings.vlm_api_key,
            prompt,
            images,
            num_predict,
        )),
        other => Err(VlmError::UnknownProvider(other.to_string())),
    }
}

// ── response parsing ─────────────────────────────────────────────────────

pub fn parse_ollama_response(body: &Value) -> Result<Value, VlmError> {
    let response = body.get("response").and_then(Value::as_str).unwrap_or("");
    let stripped = response.trim();
    let text = if !stripped.is_empty() {
        stripped
    } else {
        body.get("thinking").and_then(Value::as_str).unwrap_or("")
    };
    serde_json::from_str(text).map_err(|e| VlmError::BadReply(e.to_string()))
}

pub fn parse_openai_response(body: &Value) -> Result<Value, VlmError> {
    let content = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| VlmError::BadReply("no choices[0].message.content in the reply".into()))?;
    serde_json::from_str(content).map_err(|e| VlmError::BadReply(e.to_string()))
}

pub fn parse_anthropic_response(body: &Value) -> Result<Value, VlmError> {
    let blocks = body
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for block in blocks {
        if block.get("type").and_then(Value::as_str) == Some("tool_use") {
            return Ok(block.get("input").cloned().unwrap_or_else(|| json!({})));
        }
    }
    Err(VlmError::BadReply(
        "Claude did not return the forced tool call".into(),
    ))
}

pub fn parse_gemini_response(body: &Value) -> Result<Value, VlmError> {
    let text = body
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            VlmError::BadReply("no candidates[0].content.parts[0].text in the reply".into())
        })?;
    serde_json::from_str(text).map_err(|e| VlmError::BadReply(e.to_string()))
}

fn parse_response(provider: &str, body: &Value) -> Result<Value, VlmError> {
    match provider {
        "ollama" => parse_ollama_response(body),
        "openai" => parse_openai_response(body),
        "anthropic" => parse_anthropic_response(body),
        "gemini" => parse_gemini_response(body),
        other => Err(VlmError::UnknownProvider(other.to_string())),
    }
}

// ── the client: builds, sends, and gates ─────────────────────────────────

/// The state one running program shares across every VLM call: the rate
/// gate, the fatal-run counter, and the Ollama host pools. Port of the
/// module-level globals in `vlm_providers.py`.
pub struct VlmClient {
    agent: ureq::Agent,
    pub gate: RateGate,
    pub fatal: FatalTracker,
    hosts: HostPools,
}

impl VlmClient {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        VlmClient {
            agent: ureq::Agent::new_with_defaults(),
            gate: RateGate::new(clock),
            fatal: FatalTracker::default(),
            hosts: HostPools::default(),
        }
    }

    /// Ask whichever provider is configured. Returns the parsed JSON reply.
    /// Port of `vlm_providers.call`.
    pub fn call(
        &self,
        settings: &Settings,
        prompt: &str,
        images: &[String],
        schema: &Value,
        num_predict: i64,
    ) -> Result<Value, VlmError> {
        let provider = if settings.vlm_provider.is_empty() {
            "ollama".to_string()
        } else {
            settings.vlm_provider.to_lowercase()
        };
        let resp = if provider == "ollama" {
            // Deliberately not through the rate gate: nothing meters a
            // program talking to itself, and Ollama blocks while it loads a
            // model rather than refusing -- there is nothing here worth
            // waiting out.
            let pool = self.hosts.pool_for(settings.ollama_hosts());
            let host = pool.acquire();
            let req = ollama_request(
                &host,
                &settings.vlm_model,
                prompt,
                images,
                schema,
                num_predict,
            );
            let result = self.send_once(
                &req,
                Duration::from_secs_f64(settings.vlm_timeout),
                Some(Duration::from_secs(5)),
            );
            pool.release(host);
            result?
        } else {
            let req = build_request(settings, prompt, images, schema, num_predict)?;
            let who = display_name(&provider);
            send_with_retries(
                &self.gate,
                settings,
                &who,
                || self.send_once(&req, Duration::from_secs_f64(settings.vlm_timeout), None),
                |_msg| {},
            )?
        };

        let body: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| VlmError::BadReply(format!("not JSON: {e}")))?;
        let answer = parse_response(&provider, &body)?;
        self.fatal.clear();
        Ok(answer)
    }

    fn send_once(
        &self,
        req: &ProviderRequest,
        global_timeout: Duration,
        connect_timeout: Option<Duration>,
    ) -> Result<RawResponse, VlmError> {
        let mut builder = self.agent.post(&req.url);
        for (k, v) in &req.headers {
            builder = builder.header(k.as_str(), v.as_str());
        }
        for (k, v) in &req.query {
            builder = builder.query(k.as_str(), v.as_str());
        }
        let mut config = builder
            .config()
            .http_status_as_error(false)
            .timeout_global(Some(global_timeout));
        if let Some(connect) = connect_timeout {
            config = config.timeout_connect(Some(connect));
        }
        let builder = config.build();
        match builder.send_json(&req.json) {
            Ok(mut resp) => {
                let status = resp.status().as_u16();
                let headers = resp
                    .headers()
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                    .collect();
                let body = resp
                    .body_mut()
                    .read_to_vec()
                    .map_err(|e| VlmError::Transport(e.to_string()))?;
                Ok(RawResponse {
                    status,
                    headers,
                    body,
                })
            }
            Err(e) => Err(VlmError::Transport(e.to_string())),
        }
    }
}

// ── vehicle description ──────────────────────────────────────────────────

pub const CAR_PROMPT: &str = include_str!("prompts/car.txt");
pub const BIKE_PROMPT: &str = include_str!("prompts/bike.txt");
pub const BURST_PROMPT: &str = include_str!("prompts/burst.txt");

/// Add the shoot's known visual context instead of asking a local model to
/// decide whether incidental digits are a plate or a competition number.
pub fn vehicle_prompt(settings: &Settings, is_bike: bool) -> String {
    let base = if is_bike { BIKE_PROMPT } else { CAR_PROMPT };
    let preset = conrod_core::profile::ShootPreset::parse(&settings.scan_profile);
    let targets = match (settings.read_plates, settings.read_numbers) {
        (true, true) => "A separate reader handles registration plates. Read a competition number only from the subject and never copy plate characters.",
        (true, false) => "A separate reader handles registration plates. Set race_number to null.",
        (false, true) => "Read a competition number only from the subject; ignore unrelated digits.",
        (false, false) => "Set race_number to null and ignore registration plates.",
    };
    format!(
        "{}\n\nShoot context: {} Context guides attention only; never infer unseen details. {}",
        base.trim(),
        preset.prompt_context(),
        targets
    )
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "make": {"type": ["string", "null"]},
            "model": {"type": ["string", "null"]},
            "colour": {"type": ["string", "null"]},
            "body_type": {"type": ["string", "null"]},
            "race_number": {"type": ["string", "null"]},
            "team": {"type": ["string", "null"]},
            "driver": {"type": ["string", "null"]},
            "country": {"type": ["string", "null"]},
            "sponsors": {"type": "array", "items": {"type": "string"}},
            "livery_text": {"type": "array", "items": {"type": "string"}},
            "is_competition": {"type": "boolean"},
            "confidence": {"type": "number"},
        },
        "required": ["make", "model", "colour", "body_type", "race_number",
                     "team", "driver", "country", "sponsors", "livery_text", "confidence"],
    })
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VehicleDescription {
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub body_type: Option<String>,
    pub race_number: Option<String>,
    pub team: Option<String>,
    pub driver: Option<String>,
    pub country: Option<String>,
    pub sponsors: Vec<String>,
    pub livery_text: Vec<String>,
    pub is_competition: bool,
    pub confidence: f64,
}

impl VehicleDescription {
    pub fn title(&self) -> String {
        [&self.colour, &self.make, &self.model]
            .into_iter()
            .filter_map(|p| p.as_deref())
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Even with a JSON schema the model sometimes emits the four characters
/// `"null"` as a string rather than a JSON null, so every field is filtered
/// through this. Port of `_NULLISH`.
const NULLISH: [&str; 10] = [
    "",
    "null",
    "none",
    "n/a",
    "na",
    "unknown",
    "not visible",
    "not legible",
    "unreadable",
    "-",
];

/// `str(value)` on whatever a JSON reply put in a field. Strings, numbers,
/// bools and null map exactly onto Python's `str()`.
///
/// ponytail: an array or object value takes JSON's own text form rather
/// than Python's `repr`-flavoured one (`{'a': 1}` vs `{"a":1}`). The schema
/// declares every one of these fields as a plain string or null, so a model
/// sending a list here is already off-contract; upgrade only if a real
/// reply is ever seen doing it.
fn python_str(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => "None".to_string(),
        other => other.to_string(),
    }
}

/// Port of `vlm._text`.
fn text(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let text = python_str(value).trim().to_string();
    if NULLISH.contains(&text.to_lowercase().as_str()) {
        None
    } else {
        Some(text)
    }
}

/// Port of `vlm._text_list`.
fn text_list(value: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(items)) = value else {
        return vec![];
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if let Some(t) = text(Some(item)) {
            if seen.insert(t.to_uppercase()) {
                out.push(t);
            }
        }
    }
    out
}

/// Port of `vlm._digits`.
///
/// ponytail: Python's `str.isdigit()` also accepts Unicode digits (e.g.
/// full-width or Devanagari digits); this keeps only ASCII ones. A race
/// number read off a car in these events is always ASCII, so widening this
/// is not worth it until a reply proves otherwise.
fn digits(value: Option<&Value>, settings: &Settings) -> Option<String> {
    let text = text(value)?;
    let token: String = text.chars().filter(char::is_ascii_digit).collect();
    if token.is_empty() {
        return None;
    }
    if !(settings.number_min_len..=settings.number_max_len).contains(&token.len()) {
        return None;
    }
    Some(token)
}

/// Port of `vlm._number`.
fn number(value: Option<&Value>) -> f64 {
    let parsed = match value {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        Some(Value::Bool(b)) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    };
    parsed.map_or(0.0, |f| f.clamp(0.0, 1.0))
}

/// `bool(x or False)`: Python truthiness on a JSON value that may be
/// missing. `None`, `False`, `0`, `0.0`, `""` and an empty list/object are
/// falsy; every other value -- including the string `"false"` -- is
/// truthy. Port of the `is_competition` line in `vlm.describe`.
fn is_competition(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_none_or(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Map a provider's parsed reply onto a [`VehicleDescription`]. Shared by
/// [`describe`] (which also reads a race number) and [`identify_burst`]
/// (which does not ask for one).
fn map_vehicle(parsed: &Value, settings: &Settings, with_number: bool) -> VehicleDescription {
    let make = text(parsed.get("make"));
    let model = text(parsed.get("model"));
    // The nameplate is read more reliably than the badge; where the two
    // contradict each other and the nameplate belongs to exactly one
    // marque, the nameplate wins. See conrod-core's marques module.
    let make = conrod_core::marques::correct_make(make.as_deref(), model.as_deref());
    VehicleDescription {
        make,
        model,
        colour: text(parsed.get("colour")),
        body_type: text(parsed.get("body_type")),
        race_number: if with_number {
            digits(parsed.get("race_number"), settings)
        } else {
            None
        },
        team: text(parsed.get("team")),
        driver: text(parsed.get("driver")),
        country: text(parsed.get("country")),
        sponsors: text_list(parsed.get("sponsors")),
        livery_text: text_list(parsed.get("livery_text")),
        is_competition: is_competition(parsed.get("is_competition")),
        confidence: number(parsed.get("confidence")),
    }
}

/// Downscale and JPEG-encode a crop for the model. Port of `vlm._encode`.
///
/// The resize is Pillow-exact (`conrod_vision::imageops`'s Lanczos filter,
/// checked against Pillow in that crate's own tests) because the pixels the
/// model sees have to match what the Python side would have sent it. The
/// JPEG *encoder* is the `image` crate's, not libjpeg, so the encoded bytes
/// themselves are not byte-for-byte what Pillow would produce -- nothing
/// downstream compares them, only the request-building functions above are
/// checked against fixtures, and those take already-encoded strings.
pub fn encode_for_model(image: &Rgb, long_edge: u32) -> String {
    let long_edge = long_edge as usize;
    let resized = if image.width.max(image.height) > long_edge {
        let scale = long_edge as f64 / image.width.max(image.height) as f64;
        let w = ((image.width as f64 * scale) as usize).max(1);
        let h = ((image.height as f64 * scale) as usize).max(1);
        image.resize(w, h, Filter::Lanczos)
    } else {
        image.clone()
    };
    let mut jpeg = Vec::new();
    {
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 92);
        encoder
            .encode(
                &resized.data,
                resized.width as u32,
                resized.height as u32,
                image::ExtendedColorType::Rgb8,
            )
            .expect("encoding a freshly-built RGB buffer cannot fail");
    }
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(jpeg)
}

/// Ask the model what this vehicle is. Port of `vlm.describe`.
///
/// Transport and response failures propagate so the activity panel can report them.
pub fn describe(
    client: &VlmClient,
    image: &Rgb,
    settings: &Settings,
    is_bike: bool,
) -> Result<VehicleDescription, VlmError> {
    let payload_image = encode_for_model(image, settings.vlm_input_edge);
    let prompt = vehicle_prompt(settings, is_bike);
    match client.call(settings, &prompt, &[payload_image], &schema(), 500) {
        Ok(parsed) => Ok(map_vehicle(&parsed, settings, settings.read_numbers)),
        Err(VlmError::Stopped) => Err(VlmError::Stopped),
        Err(err) => {
            let who = display_name(&settings.vlm_provider);
            note_failure(&client.fatal, &who, &err)?;
            Err(err)
        }
    }
}

/// Ask about several views of one vehicle in a single call. Port of
/// `vlm.identify_burst`.
pub fn identify_burst(
    client: &VlmClient,
    images: &[Rgb],
    settings: &Settings,
) -> Result<VehicleDescription, VlmError> {
    if images.is_empty() {
        return Ok(VehicleDescription::default());
    }
    let encoded: Vec<String> = images
        .iter()
        .map(|img| encode_for_model(img, settings.vlm_input_edge))
        .collect();
    let prompt = BURST_PROMPT.replace("{count}", &encoded.len().to_string());
    match client.call(settings, &prompt, &encoded, &schema(), 400) {
        Ok(parsed) => Ok(map_vehicle(&parsed, settings, false)),
        Err(VlmError::Stopped) => Err(VlmError::Stopped),
        Err(err) => {
            let who = display_name(&settings.vlm_provider);
            note_failure(&client.fatal, &who, &err)?;
            Err(err)
        }
    }
}
