//! Talking to a vision-language model to describe a vehicle crop.
//!
//! Everything that is *what to ask* -- the prompt, the schema, how a reply
//! maps onto a [`VehicleDescription`] -- is the same regardless of which
//! provider answers. Everything provider-specific -- the endpoint, the auth
//! header, how an image and a schema get shaped into that API's own
//! request, and how to pull the model's JSON back out of its own response
//! envelope -- is behind the four `*_request` / `parse_*_response`
//! functions below.
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

/// Lock a mutex, carrying on with the data if a panicking thread poisoned it.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

pub const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
/// OpenRouter's OpenAI-compatible chat endpoint and its model/key metadata.
pub const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const OPENROUTER_MODELS_URL: &str = "https://openrouter.ai/api/v1/models";
pub const OPENROUTER_KEY_URL: &str = "https://openrouter.ai/api/v1/key";
/// OpenCode Console (pay-as-you-go credits): one key for the models it lists,
/// OpenAI-compatible. The chat endpoint lives under `inference/openai/v1`; the
/// model list under `inference/v1`. Console credits are billed per request from
/// the workspace balance (`opencode.billing_source: "balance"`).
///
/// Conrod deliberately has no Go subscription endpoint here. `/inference/go`
/// draws on the coding-assistant subscription, which OpenCode sells for coding
/// agents only, not for a photo app; the same key on the Console inference base
/// bills credits instead. See docs/user/ai-models.md.
pub const OPENCODE_CHAT_URL: &str = "https://opencode.ai/inference/openai/v1/chat/completions";
pub const OPENCODE_MODELS_URL: &str = "https://opencode.ai/inference/v1/models";
pub(crate) const OPENCODE_USER_AGENT: &str = concat!("conrod/", env!("CARGO_PKG_VERSION"));
pub const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
const GEMINI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent";

// ── errors ───────────────────────────────────────────────────────────────

/// Every way a call to a provider can fail.
///
/// `Stopped` and `Misconfigured` are deliberately not handled by the
/// per-crop `match` that turns the other variants into an empty
/// [`VehicleDescription`], so a Stop or a run that has given up cannot be
/// swallowed as "this one frame could not be read"; see [`describe`].
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
/// is obeyed as given up to [`MAX_RETRY_AFTER`] -- it is a fact about when the
/// limit lifts, not a guess, and clamping it short just buys another refusal.
pub const MAX_WAIT: f64 = 60.0;
/// The longest a provider's Retry-After is honoured. Far beyond any real
/// limit window; it only stops a nonsense header parking a worker forever or
/// reaching `Duration` as a value it cannot hold.
pub const MAX_RETRY_AFTER: f64 = 3600.0;
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
        if seconds.is_finite() && seconds > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(seconds.min(MAX_RETRY_AFTER)));
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
        *lock(&self.stop_check) = check;
    }

    /// Sleep, looking up often enough to notice a Stop. Port of `_sleep`.
    /// Sliced only when there is a stop check to run -- without one, that
    /// would be a loop waking up hundreds of times to ask a question nobody
    /// is answering.
    pub fn sleep_interruptible(&self, seconds: f64) -> Result<(), VlmError> {
        if !seconds.is_finite() || seconds <= 0.0 {
            return Ok(());
        }
        let seconds = seconds.min(MAX_RETRY_AFTER);
        let check = lock(&self.stop_check).clone();
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
        let mut not_before = lock(&self.not_before);
        let candidate = self.clock.now() + seconds.min(MAX_RETRY_AFTER);
        if candidate.is_finite() && candidate > *not_before {
            *not_before = candidate;
        }
    }

    /// Port of `_wait_turn`.
    pub fn wait_turn(&self) -> Result<(), VlmError> {
        let delay = *lock(&self.not_before) - self.clock.now();
        self.sleep_interruptible(delay)
    }
}

/// How long the provider asked for, or a backed-off guess.
///
/// A stated `Retry-After` is taken at face value, whether it is plain seconds
/// or an HTTP-date (parsed with the `httpdate` crate, which only reads the
/// RFC 7231 IMF-fixdate form; that is the only form a real `Retry-After`
/// header uses in practice), up to [`MAX_RETRY_AFTER`]. A value that is not a
/// finite number (`inf`, `NaN`) is ignored like any unreadable header.
pub fn retry_after(headers: &[(String, String)], attempt: u32, jitter: f64) -> f64 {
    if let Some(header) = header_get(headers, "retry-after") {
        if let Ok(seconds) = header.trim().parse::<f64>() {
            if seconds.is_finite() {
                return seconds.clamp(0.0, MAX_RETRY_AFTER);
            }
        } else if let Ok(when) = httpdate::parse_http_date(header.trim()) {
            let seconds = when
                .duration_since(SystemTime::now())
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0);
            return seconds.clamp(0.0, MAX_RETRY_AFTER);
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
///
/// `send` is retried in place: it performs the actual HTTP call (or, in a
/// test, returns the next canned outcome) and is invoked again for every
/// attempt. A rate limit (429/529) is waited out for as long as it takes and
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
        let mut state = lock(&self.state);
        state.run += 1;
        state.said = said.to_string();
        state.run
    }

    /// A call worked, so whatever came before it was not the configuration.
    pub fn clear(&self) {
        let mut state = lock(&self.state);
        state.run = 0;
        state.said.clear();
    }

    /// The reason the run should stop, or `None` while it should carry on.
    pub fn given_up(&self) -> Option<String> {
        let state = lock(&self.state);
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
            .map(|v| {
                conrod_core::text::value_to_string(v)
                    .chars()
                    .take(200)
                    .collect()
            })
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
        lock(&self.rx)
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
        let mut pools = lock(&self.pools);
        pools
            .entry(hosts.clone())
            .or_insert_with(|| Arc::new(HostPool::new(hosts)))
            .clone()
    }
}

/// Where to start among `ids`: the id a `least_busy` token handed out, the
/// next turn for `round_robin`, or clock nanos for `random`.
pub fn pick_service(
    strategy: &str,
    ids: &[String],
    held: Option<&str>,
    turn: &std::sync::atomic::AtomicUsize,
) -> usize {
    let n = ids.len().max(1);
    match (strategy, held) {
        (_, Some(id)) => ids.iter().position(|i| i == id).unwrap_or(0),
        ("round_robin", _) => turn.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % n,
        _ => {
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(0)
                % n
        }
    }
}

/// A copy of `settings` that talks to just this one service.
fn for_service(settings: &Settings, service: &conrod_core::settings::VlmService) -> Settings {
    Settings {
        vlm_provider: service.provider.clone(),
        vlm_model: service.model.clone(),
        vlm_host: service.host.clone(),
        vlm_extra_hosts: String::new(),
        vlm_api_key: service.api_key.clone(),
        anthropic_key_kind: service.key_kind.clone(),
        vlm_services: Vec::new(),
        ..settings.clone()
    }
}

// ── request building ─────────────────────────────────────────────────────
// Every provider takes the same inputs -- a prompt, already-encoded base64
// JPEG images, a JSON schema and a token budget -- and is asked for
// something to POST: the URL, headers, query parameters and JSON body.
// Kept pure (no network) so it can be compared directly against recorded
// fixtures; see `tests/vlm.rs`.

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub query: Vec<(String, String)>,
    pub json: Value,
}

/// Ollama's context window per request. One 1568 px image is about 2000
/// tokens, the prompt and schema a few hundred, the reply at most 600: 8192
/// fits all of it. Without this Ollama uses the model's 128k default, whose
/// KV cache alone is several GB and spills qwen2.5vl:7b off the GPU.
pub const OLLAMA_NUM_CTX: i64 = 8192;

/// Add a provider's own switch for model thinking. `on` is the user's
/// `vlm_reasoning` setting. Only parameters the provider documents are sent:
///
/// - Ollama takes `"think"` (boolean) on `/api/generate` and `/api/chat`:
///   <https://docs.ollama.com/api>.
/// - OpenRouter takes `"reasoning": {"enabled": bool}` on chat completions:
///   <https://openrouter.ai/docs/use-cases/reasoning-tokens>.
/// - Anthropic takes `"thinking": {"type": "disabled"}`; thinking is on by
///   default on its newest models. A few models reject `disabled`, so the
///   form is chosen per model from the table at
///   <https://platform.claude.com/docs/en/build-with-claude/thinking>.
/// - Gemini takes `generationConfig.thinkingConfig`; the field is an error on
///   models without thinking, and `thinkingLevel` is an error before Gemini 3,
///   so the form is chosen per model:
///   <https://ai.google.dev/api/generate-content>.
/// - OpenAI's `reasoning_effort` has no value that disables reasoning on every
///   model (the newest reject `none` and `minimal`), and OpenCode Console
///   documents no reasoning field at all. Both are left untouched.
///
/// Turning thinking on never sends anything for Anthropic or Gemini: their
/// model default is to think, and the on form differs by model family.
fn apply_reasoning(request: &mut ProviderRequest, provider: &str, model: &str, on: bool) {
    match provider {
        "ollama" => request.json["think"] = json!(on),
        "openrouter" => request.json["reasoning"] = json!({"enabled": on}),
        "anthropic" if !on => {
            if let Some(thinking) = anthropic_thinking_off(model) {
                request.json["thinking"] = thinking;
            }
        }
        "gemini" if !on => {
            if let Some(thinking) = gemini_thinking_off(model) {
                request.json["generationConfig"]["thinkingConfig"] = thinking;
            }
        }
        _ => {}
    }
}

/// The documented lowest-thinking form for Claude, or `None` when the model
/// rejects every form that turns thinking off (Opus 5.5, Fable, Mythos). On
/// Sonnet 5.5 `disabled` is a 400; `between_tools` is its lowest accepted
/// value and turns off up-front thinking.
fn anthropic_thinking_off(model: &str) -> Option<Value> {
    let model = model.trim().to_lowercase();
    if model.starts_with("claude-sonnet-5-5") {
        Some(json!({"type": "between_tools"}))
    } else if model.starts_with("claude-opus-5-5")
        || model.starts_with("claude-fable")
        || model.starts_with("claude-mythos")
    {
        None
    } else {
        Some(json!({"type": "disabled"}))
    }
}

/// The documented lowest-thinking form for Gemini, or `None` when the model
/// cannot turn thinking off. Gemini 3 and later reject `thinkingBudget` and
/// always think, so their lowest `thinkingLevel` is sent instead. Gemini 2.5
/// Pro has a minimum budget and cannot go to zero; 2.0 has no thinking, and
/// setting `thinkingConfig` on it is an error.
fn gemini_thinking_off(model: &str) -> Option<Value> {
    let model = model.trim().to_lowercase();
    if model.starts_with("gemini-3") || model.starts_with("gemini-4") {
        Some(json!({"thinkingLevel": "low"}))
    } else if model.starts_with("gemini-2.5-flash") {
        Some(json!({"thinkingBudget": 0}))
    } else {
        None
    }
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
            "keep_alive": "2m",
            "options": {
                "temperature": 0.0,
                "num_predict": num_predict,
                "num_ctx": OLLAMA_NUM_CTX,
            },
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
    openai_request_at(
        OPENAI_URL,
        true,
        model,
        api_key,
        prompt,
        images,
        schema,
        num_predict,
    )
}

/// An OpenAI-shaped chat request against any compatible gateway. `strict` asks
/// for the schema as `response_format.json_schema`; a gateway whose models do
/// not support strict schemas gets `json_object` plus the schema in the prompt,
/// which the reply parser already tolerates.
#[allow(clippy::too_many_arguments)]
pub fn openai_request_at(
    url: &str,
    strict: bool,
    model: &str,
    api_key: &str,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> ProviderRequest {
    let prompt = if strict {
        prompt.to_string()
    } else {
        format!("{prompt}\n\nReply with one JSON object matching this schema:\n{schema}")
    };
    let mut content = vec![json!({"type": "text", "text": prompt})];
    for image in images {
        content.push(json!({
            "type": "image_url",
            "image_url": {"url": format!("data:image/jpeg;base64,{image}")},
        }));
    }
    // Strict structured outputs require every property listed as required
    // and additionalProperties set explicitly.
    let mut strict_schema = schema.clone();
    if let Value::Object(map) = &mut strict_schema {
        map.insert("additionalProperties".into(), Value::Bool(false));
        if let Some(Value::Object(props)) = map.get("properties") {
            let mut req_set: Vec<String> = match map.get("required").and_then(Value::as_array) {
                Some(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
                None => Vec::new(),
            };
            for k in props.keys() {
                if !req_set.iter().any(|existing| existing == k) {
                    req_set.push(k.clone());
                }
            }
            map.insert(
                "required".into(),
                Value::Array(req_set.into_iter().map(Value::String).collect()),
            );
        }
    }
    let response_format = if strict {
        json!({
            "type": "json_schema",
            "json_schema": {"name": "vehicle", "schema": strict_schema, "strict": true},
        })
    } else {
        json!({"type": "json_object"})
    };
    ProviderRequest {
        url: url.into(),
        headers: vec![("Authorization".into(), format!("Bearer {api_key}"))],
        query: vec![],
        json: json!({
            "model": model,
            "messages": [{"role": "user", "content": content}],
            "response_format": response_format,
            "max_tokens": num_predict,
            "temperature": 0.0,
        }),
    }
}

const OPENAI_MODELS_URL: &str = "https://api.openai.com/v1/models";

/// Model lists are a short UI call; fail fast instead of leaving the Settings
/// screen hanging on an unreachable proxy.
const MODELS_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MODELS_TOTAL_TIMEOUT: Duration = Duration::from_secs(20);

fn models_agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_connect(Some(MODELS_CONNECT_TIMEOUT))
            .timeout_global(Some(MODELS_TOTAL_TIMEOUT))
            .build(),
    )
}

/// `provider_models` is a webview command that carries an API key, so it may
/// only talk to hosts this app already trusts: the known providers' fixed
/// endpoints, and OpenCode's documented custom base URL (a proxy or a
/// self-hosted gateway) over http(s) only. Anything else is refused before the
/// key is ever attached to a request.
fn models_url(provider: &str, host: &str) -> Result<(String, bool), String> {
    match provider {
        "openai" => Ok((OPENAI_MODELS_URL.to_string(), true)),
        "openrouter" => Ok((OPENROUTER_MODELS_URL.to_string(), true)),
        "opencode" | "opencode-go" => {
            // `opencode-go` is a settings alias kept from older builds: it now
            // means the Console (credits) endpoint, never the Go subscription.
            let host = host.trim().trim_end_matches('/');
            if host.is_empty() {
                return Ok((OPENCODE_MODELS_URL.to_string(), true));
            }
            if !is_http_url(host) {
                return Err(format!(
                    "'{provider}' needs an http(s) model-list URL, not {host:?}"
                ));
            }
            if !host.to_ascii_lowercase().starts_with("https://") && !is_local_host(host) {
                return Err(format!(
                    "'{provider}' sends the API key, so a plain http URL must be localhost or a private-network address; use https for {host:?}"
                ));
            }
            let url = if host.ends_with("/models") {
                host.to_string()
            } else {
                format!("{host}/models")
            };
            Ok((url, true))
        }
        other => Err(format!(
            "'{other}' does not list models; type the model name"
        )),
    }
}

/// An `http`/`https` URL with a host: not `file:`, not a UNC path, not a bare
/// host name that ureq would otherwise have to guess a scheme for.
fn is_http_url(url: &str) -> bool {
    url.parse::<ureq::http::Uri>()
        .map(|uri| matches!(uri.scheme_str(), Some("http" | "https")) && uri.authority().is_some())
        .unwrap_or(false)
}

/// `localhost`, a loopback address, or a private-network address (RFC 1918,
/// unique-local IPv6): where an unencrypted request stays on the user's own
/// network, e.g. Ollama or a gateway on the LAN. Link-local addresses (cloud
/// metadata services) are not included.
fn is_local_host(url: &str) -> bool {
    let Some(host) = url
        .parse::<ureq::http::Uri>()
        .ok()
        .and_then(|u| u.host().map(str::to_lowercase))
    else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback() || (ip.segments()[0] & 0xfe00) == 0xfc00,
        Err(_) => false,
    }
}

/// The models a provider key can call, as `(id, name)` pairs. Uses the
/// provider's own list endpoint; anything that is not an OpenAI-shaped
/// `{"data":[{"id":…}]}` (or `{"models":[…]}`) is reported as an error rather
/// than guessed at.
pub fn list_models(
    provider: &str,
    host: &str,
    api_key: &str,
) -> Result<Vec<(String, String)>, String> {
    let provider = provider.to_lowercase();
    let (url, auth) = models_url(&provider, host)?;
    let agent = models_agent();
    let mut request = agent.get(&url);
    if auth {
        request = request.header("Authorization", &format!("Bearer {api_key}"));
    }
    if provider.starts_with("opencode") {
        request = request.header("User-Agent", OPENCODE_USER_AGENT);
    }
    let response = request.call().map_err(|e| e.to_string())?;
    let body: Value = response
        .into_body()
        .read_json()
        .map_err(|e| format!("{url}: {e}"))?;
    let list = body
        .get("data")
        .or_else(|| body.get("models"))
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{url} did not return a model list"))?;
    let mut models: Vec<(String, String)> = list
        .iter()
        .filter_map(|item| {
            let id = item
                .get("id")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)?;
            let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
            Some((id.to_string(), name.to_string()))
        })
        .collect();
    models.sort();
    models.dedup();
    Ok(models)
}

/// Parse OpenRouter's key-info reply. A non-2xx is an error carrying the
/// provider's own message, never the key.
pub fn openrouter_key_reply(status: u16, body: &Value) -> Result<Value, String> {
    if !(200..300).contains(&status) {
        let said = body
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("no message");
        return Err(format!("OpenRouter answered {status}: {said}"));
    }
    let data = body.get("data").cloned().unwrap_or_else(|| body.clone());
    Ok(json!({
        "ok": true,
        "label": data.get("label").and_then(Value::as_str).unwrap_or("OpenRouter key"),
        "limit": data.get("limit").cloned().unwrap_or(Value::Null),
        "usage": data.get("usage").cloned().unwrap_or(Value::Null),
    }))
}

/// Ask OpenRouter who a key belongs to, without spending a request on a model.
/// `openrouter_key_at` takes its URL so a test can point it at a local server.
pub fn openrouter_key_at(url: &str, key: &str) -> Result<Value, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Add an OpenRouter key first".into());
    }
    let agent = models_agent();
    let mut reply = agent
        .get(url)
        .header("Authorization", &format!("Bearer {key}"))
        .call()
        .map_err(|e| {
            format!(
                "OpenRouter could not be reached: {}",
                e.to_string().replace(key, "***")
            )
        })?;
    let status = reply.status().as_u16();
    let body: Value = reply
        .body_mut()
        .read_json()
        .map_err(|e| format!("OpenRouter replied without JSON: {e}"))?;
    openrouter_key_reply(status, &body)
}

pub fn openrouter_key_check(key: &str) -> Result<Value, String> {
    openrouter_key_at(OPENROUTER_KEY_URL, key)
}

/// Parse OpenCode's model list as a key check: the list proves the key works,
/// and says whether the decision model JEV is on it. A non-2xx is an error
/// carrying OpenCode's own message, never the key.
pub fn opencode_key_reply(status: u16, body: &Value) -> Result<Value, String> {
    if !(200..300).contains(&status) {
        let said = body
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("no message");
        return Err(format!("OpenCode answered {status}: {said}"));
    }
    let list = body
        .get("data")
        .or_else(|| body.get("models"))
        .and_then(Value::as_array)
        .ok_or("OpenCode replied without a model list")?;
    let jev = list.iter().any(|model| {
        model
            .get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("jev-"))
    });
    Ok(json!({
        "ok": true,
        "label": format!(
            "{} models, decision model {}",
            list.len(),
            if jev { "JEV ready" } else { "JEV not listed" }
        ),
    }))
}

/// Ask OpenCode for its model list with a key, without spending a model call.
/// `opencode_key_at` takes its URL so a test can point it at a local server.
pub fn opencode_key_at(url: &str, key: &str) -> Result<Value, String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Add an OpenCode key first".into());
    }
    let agent = models_agent();
    let mut reply = agent
        .get(url)
        .header("Authorization", &format!("Bearer {key}"))
        .header("User-Agent", OPENCODE_USER_AGENT)
        .call()
        .map_err(|e| {
            format!(
                "OpenCode could not be reached: {}",
                e.to_string().replace(key, "***")
            )
        })?;
    let status = reply.status().as_u16();
    let body: Value = reply
        .body_mut()
        .read_json()
        .map_err(|e| format!("OpenCode replied without JSON: {e}"))?;
    opencode_key_reply(status, &body)
}

pub fn opencode_key_check(key: &str) -> Result<Value, String> {
    opencode_key_at(OPENCODE_MODELS_URL, key)
}

/// Where an OpenCode service talks: the configured endpoint if there is one
/// (a proxy, or a self-hosted gateway), else the default for this provider. An
/// endpoint given without a path is completed with `/chat/completions`.
fn opencode_url(host: &str, default: &str) -> String {
    let host = host.trim().trim_end_matches('/');
    if host.is_empty() {
        return default.into();
    }
    if host.ends_with("/chat/completions") {
        host.into()
    } else {
        format!("{host}/chat/completions")
    }
}

pub fn opencode_chat_url(host: &str) -> String {
    opencode_url(host, OPENCODE_CHAT_URL)
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
        // The key travels in the header, never the query string: URLs land in
        // proxy and server logs, headers do not (same contract as OpenAI and
        // Anthropic above).
        headers: vec![("x-goog-api-key".into(), api_key.to_string())],
        query: vec![],
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
        "openrouter" => "OpenRouter".into(),
        "anthropic" => "Anthropic".into(),
        "gemini" => "Gemini".into(),
        "opencode" | "opencode-go" => "OpenCode (pay-as-you-go credits)".into(),
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
    let mut request = match provider.as_str() {
        "ollama" => ollama_request(
            &settings.vlm_host,
            &settings.vlm_model,
            prompt,
            images,
            schema,
            num_predict,
        ),
        "openai" => openai_request(
            &settings.vlm_model,
            &settings.vlm_api_key,
            prompt,
            images,
            schema,
            num_predict,
        ),
        "openrouter" => openai_request_at(
            OPENROUTER_URL,
            false,
            &settings.vlm_model,
            &settings.vlm_api_key,
            prompt,
            images,
            schema,
            num_predict,
        ),
        "anthropic" => anthropic_request(settings, prompt, images, schema, num_predict),
        "gemini" => gemini_request(
            &settings.vlm_model,
            &settings.vlm_api_key,
            prompt,
            images,
            num_predict,
        ),
        "opencode" | "opencode-go" => {
            // Both spellings are the Console (credits) endpoint. `opencode-go`
            // is only an alias from older settings; it never reaches the Go
            // subscription base, whose allowance is for coding assistants.
            let mut request = openai_request_at(
                &opencode_chat_url(&settings.vlm_host),
                false,
                &settings.vlm_model,
                &settings.vlm_api_key,
                prompt,
                images,
                schema,
                num_predict,
            );
            request
                .headers
                .push(("User-Agent".into(), OPENCODE_USER_AGENT.into()));
            request
        }
        other => return Err(VlmError::UnknownProvider(other.to_string())),
    };
    apply_reasoning(
        &mut request,
        &provider,
        &settings.vlm_model,
        settings.vlm_reasoning,
    );
    Ok(request)
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
    serde_json::from_str(text)
        .or_else(|e| first_json(text).ok_or(e))
        .map_err(|e| VlmError::BadReply(e.to_string()))
}

/// The first complete JSON object in `text`, ignoring anything written around
/// it. A local model sometimes wraps the object in prose even when a schema
/// was requested; the bench reads that reply, so the app does too.
fn first_json(text: &str) -> Option<Value> {
    let mut from = text.find('{')?;
    loop {
        let mut values = serde_json::Deserializer::from_str(&text[from..]).into_iter::<Value>();
        if let Some(Ok(value)) = values.next() {
            if value.is_object() {
                return Some(value);
            }
        }
        from = text[from + 1..].find('{').map(|i| from + 1 + i)?;
    }
}

pub fn parse_openai_response(body: &Value) -> Result<Value, VlmError> {
    let choice = body
        .get("choices")
        .and_then(|c| c.get(0))
        .ok_or_else(|| VlmError::BadReply("no choices[0] in the reply".into()))?;
    let content = choice
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_str)
        .unwrap_or("");
    if content.trim().is_empty() {
        // A reasoning model can spend the whole budget thinking and send no
        // answer at all; say which happened instead of "EOF at line 1".
        let finish = choice
            .get("finish_reason")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        return Err(VlmError::BadReply(format!(
            "the model returned no answer (finish_reason {finish})"
        )));
    }
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
        // OpenRouter speaks the OpenAI envelope too.
        "openrouter" => parse_openai_response(body),
        "anthropic" => parse_anthropic_response(body),
        // The gateway speaks the OpenAI envelope.
        "opencode" | "opencode-go" => parse_openai_response(body),
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
    /// One token per service id, for `least_busy`; see [`HostPool`].
    services: HostPools,
    turn: std::sync::atomic::AtomicUsize,
}

impl VlmClient {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        VlmClient {
            agent: ureq::Agent::new_with_defaults(),
            gate: RateGate::new(clock),
            fatal: FatalTracker::default(),
            hosts: HostPools::default(),
            services: HostPools::default(),
            turn: Default::default(),
        }
    }

    /// Ask one of the enabled services, picked by `vlm_strategy`; on an
    /// error, each other enabled service gets one try before giving up.
    /// With no services at all, the single legacy provider is asked.
    pub fn call(
        &self,
        settings: &Settings,
        prompt: &str,
        images: &[String],
        schema: &Value,
        num_predict: i64,
    ) -> Result<Value, VlmError> {
        self.call_with(settings, prompt, images, schema, num_predict, None)
    }

    /// `call`, with an optional Ollama `repeat_penalty` for this ask only.
    #[allow(clippy::too_many_arguments)]
    pub fn call_with(
        &self,
        settings: &Settings,
        prompt: &str,
        images: &[String],
        schema: &Value,
        num_predict: i64,
        repeat_penalty: Option<f64>,
    ) -> Result<Value, VlmError> {
        let enabled: Vec<_> = settings.vlm_services.iter().filter(|s| s.enabled).collect();
        if enabled.is_empty() {
            // A configured list whose every row is off means the model is not
            // asked; the legacy single-provider fields must not resurrect a
            // service the user disabled.
            if !settings.vlm_services.is_empty() {
                return Err(VlmError::Misconfigured(
                    "No vision service is enabled".into(),
                ));
            }
            return self.call_one(
                settings,
                prompt,
                images,
                schema,
                num_predict,
                repeat_penalty,
            );
        }
        let ids: Vec<String> = enabled.iter().map(|s| s.id.clone()).collect();
        let pool = self.services.pool_for(ids.clone());
        let held = (settings.vlm_strategy == "least_busy" || settings.vlm_strategy.is_empty())
            .then(|| pool.acquire());
        let start = pick_service(&settings.vlm_strategy, &ids, held.as_deref(), &self.turn);
        let ask = |i: usize| {
            let one = for_service(settings, enabled[i]);
            self.call_one(&one, prompt, images, schema, num_predict, repeat_penalty)
        };
        let mut order = (start..ids.len()).chain(0..start);
        let first = ask(order.next().unwrap_or(0));
        // ponytail: only the first pick holds a least_busy token; failover
        // tries go unpooled rather than wait on a busy service.
        if let Some(id) = held {
            pool.release(id);
        }
        let mut last = match first {
            Err(VlmError::Stopped) | Ok(_) => return first,
            Err(e) => e,
        };
        for i in order {
            match ask(i) {
                Err(VlmError::Stopped) => return Err(VlmError::Stopped),
                Err(e) => last = e,
                ok => return ok,
            }
        }
        Err(last)
    }

    fn call_one(
        &self,
        settings: &Settings,
        prompt: &str,
        images: &[String],
        schema: &Value,
        num_predict: i64,
        repeat_penalty: Option<f64>,
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
            let mut req = ollama_request(
                &host,
                &settings.vlm_model,
                prompt,
                images,
                schema,
                num_predict,
            );
            apply_reasoning(
                &mut req,
                "ollama",
                &settings.vlm_model,
                settings.vlm_reasoning,
            );
            // Only for a retry: a penalty on the first ask costs the digits
            // that repeat in the visible text, and the first ask rarely loops.
            if let Some(penalty) = repeat_penalty {
                req.json["options"]["repeat_penalty"] = json!(penalty);
            }
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
pub const AIRCRAFT_PROMPT: &str = include_str!("prompts/aircraft.txt");
pub const PLAYER_PROMPT: &str = include_str!("prompts/player.txt");
pub const BURST_PROMPT: &str = include_str!("prompts/burst.txt");

/// What the caller already knows about the vehicle before asking.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VlmHints {
    /// A race or track preset: the car's own race number is the identity
    /// that matters, and a plate is left out of the question entirely.
    pub racing: bool,
    /// Entry-list "Make Model" strings the answer must be one of; empty when
    /// the shoot has no list.
    pub candidates: Vec<String>,
    /// What the crop shows, when it is not a car or motorcycle.
    pub subject: SubjectKind,
}

/// The kind of machine in the crop, so the model is never asked about a car
/// when the crop is an aircraft or a bicycle.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SubjectKind {
    /// A car or truck, or a motorcycle (`is_bike`).
    #[default]
    Road,
    Aircraft,
    Bicycle,
    /// A player on a field or court: the number is a jersey number.
    Player,
}

impl SubjectKind {
    /// From the detector's class name (`airplane`, `bicycle`, ...).
    pub fn from_class(class: &str) -> SubjectKind {
        match class {
            "airplane" => SubjectKind::Aircraft,
            "bicycle" => SubjectKind::Bicycle,
            "person" => SubjectKind::Player,
            _ => SubjectKind::Road,
        }
    }
}

fn candidate_line(hints: &VlmHints) -> String {
    if hints.candidates.is_empty() {
        String::new()
    } else {
        format!(
            "make/model must be one of: {}.",
            hints.candidates.join(", ")
        )
    }
}

/// Whether the question asks for a registration plate at all.
fn wants_plate(settings: &Settings, hints: &VlmHints) -> bool {
    settings.read_plates && !hints.racing
}

/// Add the shoot's known visual context instead of asking a local model to
/// decide whether incidental digits are a plate or a competition number.
pub fn vehicle_prompt(settings: &Settings, is_bike: bool, hints: &VlmHints) -> String {
    let swapped;
    let base = match hints.subject {
        SubjectKind::Aircraft => AIRCRAFT_PROMPT,
        SubjectKind::Player => PLAYER_PROMPT,
        SubjectKind::Bicycle => {
            swapped = BIKE_PROMPT.replace("motorcycle", "bicycle");
            swapped.as_str()
        }
        SubjectKind::Road if is_bike => BIKE_PROMPT,
        SubjectKind::Road => CAR_PROMPT,
    };
    let with_plate = wants_plate(settings, hints);
    let body = base
        .lines()
        .filter(|line| with_plate || !line.starts_with("- plate:"))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("{CANDIDATES}", &candidate_line(hints));
    let targets = if settings.read_numbers {
        "Read a competition number only from the subject; ignore unrelated digits."
    } else {
        "Set number to null."
    };
    // No shoot-preset context here. A preset sentence ("capture vehicle
    // identity, competition details, livery, and action") makes the model
    // spend its answer on livery and drop a windscreen number it had read, and
    // the bench reads the same crops better without it. The closing line asking
    // for one compact object keeps the reply short enough to finish.
    format!(
        "{}\n\nContext guides attention only; never infer unseen details. {targets}\nReply with one JSON object only.",
        body.trim()
    )
}

/// The reply's shape. `with_plate` is false for a race car, or when the
/// plate reader is off, so the model is never asked for one.
pub fn schema(with_plate: bool) -> Value {
    let mut properties = json!({
        "number": {"type": ["string", "null"]},
        "number_location": {
            "type": "string",
            "enum": ["door", "bonnet", "roof", "windscreen", "nose", "other", "none"],
        },
        "numbers": {
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "number": {"type": "string"},
                    "location": {
                        "type": "string",
                        "enum": ["door", "bonnet", "roof", "windscreen", "nose", "other"],
                    },
                },
                "required": ["number", "location"],
            },
        },
        "visible_text": {"type": "array", "items": {"type": "string"}},
        "action": {
            "type": "array",
            "items": {
                "type": "string",
                "enum": ["airborne", "dust", "tyre_smoke", "spray", "flames", "sparks", "contact", "wheel_lift"],
            },
        },
        "make": {"type": ["string", "null"]},
        "model": {"type": ["string", "null"]},
        "evidence": {"type": "string", "enum": ["badge", "script", "design", "none"]},
        "colour": {"type": ["string", "null"]},
        "is_competition": {"type": "boolean"},
        "confidence": {"type": "number"},
    });
    let mut required = vec![
        "number",
        "number_location",
        "visible_text",
        "action",
        "make",
        "model",
        "evidence",
        "colour",
        "is_competition",
        "confidence",
    ];
    if with_plate {
        properties["plate"] = json!({"type": ["string", "null"]});
        required.push("plate");
    }
    json!({"type": "object", "properties": properties, "required": required})
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VehicleDescription {
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub body_type: Option<String>,
    pub race_number: Option<String>,
    /// Every number the model saw, with the panel it was on. The race number
    /// is chosen from these (entry list, then the most prominent panel, then
    /// what the burst has already shown); `race_number` is its own answer.
    pub numbers: Vec<conrod_core::analysis::NumberCandidate>,
    /// How sure the model is of the race number, lowered when it was read
    /// off the nose board, where numbers are often incidental and can
    /// conflict with the official window number.
    pub number_confidence: f64,
    pub plate: Option<String>,
    pub team: Option<String>,
    pub driver: Option<String>,
    pub country: Option<String>,
    pub sponsors: Vec<String>,
    pub livery_text: Vec<String>,
    /// Visible action effects around the car: `airborne`, `dust`, `tyre_smoke`,
    /// `spray`, `flames`, `sparks`, `contact`, `wheel_lift`.
    pub action: Vec<String>,
    /// What the model based the make/model on: "badge", "script" or "design".
    pub model_evidence: Option<String>,
    /// The model was identified from the car's design alone, a suggestion.
    pub model_suggested: bool,
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

/// The field's plain-text reading, whatever type a reply put in it.
fn text(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if value.is_null() {
        return None;
    }
    let text = conrod_core::text::value_to_string(value).trim().to_string();
    if NULLISH.contains(&text.to_lowercase().as_str()) {
        None
    } else {
        Some(text)
    }
}

/// A registration plate as shown: spaces, dots and dashes dropped, upper
/// case, 2 to 7 letters and digits with at least one letter. Letters and
/// digits mixed is the common issue; an all-letter token must be a known
/// six-letter personalised format (`SRBGTR`), so a badge word still cannot
/// pass as a plate.
fn plate_text(value: Option<&Value>) -> Option<String> {
    let raw = text(value)?;
    let plate: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '.' | '-'))
        .collect::<String>()
        .to_uppercase();
    let letters = plate.bytes().any(|b| b.is_ascii_alphabetic());
    let digits = plate.bytes().any(|b| b.is_ascii_digit());
    let personalised = !digits && conrod_vision::plates::looks_like_plate(&plate);
    let valid = (2..=7).contains(&plate.chars().count())
        && plate.chars().all(|c| c.is_ascii_alphanumeric())
        && letters
        && (digits || personalised);
    valid.then_some(plate)
}

/// Port of `vlm._text_list`. A reply that gave the list as one
/// newline-separated string is split into its lines rather than refused.
fn text_list(value: Option<&Value>) -> Vec<String> {
    let split: Vec<Value>;
    let items: &[Value] = match value {
        Some(Value::Array(items)) => items,
        Some(Value::String(s)) => {
            split = s.lines().map(|l| Value::String(l.to_string())).collect();
            &split
        }
        _ => return vec![],
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
/// ponytail: ASCII digits only. A race number read off a car is always
/// ASCII; widen if a reply proves otherwise.
fn digits(value: Option<&Value>, settings: &Settings) -> Option<String> {
    let text = text(value)?;
    let token: String = text.chars().filter(char::is_ascii_digit).collect();
    if token.is_empty() {
        return None;
    }
    if !(settings.number_min_len..=settings.number_max_len).contains(&token.len()) {
        return None;
    }
    if let Ok(y) = token.parse::<u32>() {
        // ponytail: filters out a year the VLM misread as a race number
        // (e.g. a car's windshield date sticker). 1900-2099 covers every
        // plausible photo date for this app's lifetime; raise the ceiling
        // only once a real event is still shooting past 2099.
        if (1900..=2099).contains(&y) {
            return None;
        }
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

/// Whether a possibly-missing reply field carries anything: `null`, `false`,
/// `0`, `0.0`, `""` and an empty list/object are all absent; every other
/// value -- including the string `"false"` -- counts as present.
fn is_competition(value: Option<&Value>) -> bool {
    value.is_some_and(conrod_core::text::is_truthy)
}

/// Map a provider's parsed reply onto a [`VehicleDescription`]. Shared by
/// [`describe`] (which also reads a race number) and [`identify_burst`]
/// (which does not ask for one).
fn map_vehicle(
    parsed: &Value,
    settings: &Settings,
    with_number: bool,
    with_plate: bool,
) -> VehicleDescription {
    let evidence = parsed
        .get("evidence")
        .and_then(Value::as_str)
        .map(|e| e.trim().to_lowercase())
        .unwrap_or_default();
    let model = if evidence == "none" {
        None
    } else {
        text(parsed.get("model"))
    };
    // The nameplate is read more reliably than the badge; where the two
    // contradict each other and the nameplate belongs to exactly one
    // marque, the nameplate wins. See conrod-core's marques module.
    let make = text(parsed.get("make"));
    let make = conrod_core::marques::correct_make(make.as_deref(), model.as_deref());
    let confidence = number(parsed.get("confidence"));
    // A nose board can carry a different number from the official window one
    // (the benchmark's car 2: window 2, nose 1), so it is the weak spot.
    let number_location = text(parsed.get("number_location"))
        .map(|l| l.trim().to_lowercase())
        .unwrap_or_default();
    let weak_spot = number_location == "nose";
    let model_evidence = match evidence.as_str() {
        "badge" | "script" | "design" => Some(evidence.clone()),
        _ => None,
    };
    let visible = text_list(parsed.get("visible_text"));
    let action = if settings.read_action {
        action_list(parsed.get("action"))
    } else {
        Vec::new()
    };
    let race_number = if with_number {
        digits(parsed.get("number"), settings)
    } else {
        None
    };
    let mut numbers = if with_number {
        number_candidates(parsed.get("numbers"), settings)
    } else {
        Vec::new()
    };
    // The answer's own number belongs in the candidate list, so the tiebreak
    // always has something to choose from even on an older reply shape.
    if let Some(number) = &race_number {
        if !numbers.iter().any(|c| &c.number == number) {
            numbers.insert(
                0,
                conrod_core::analysis::NumberCandidate {
                    number: number.clone(),
                    location: number_location.clone(),
                },
            );
        }
    }
    VehicleDescription {
        make,
        model,
        colour: text(parsed.get("colour")),
        body_type: None,
        race_number,
        numbers,
        number_confidence: if weak_spot {
            confidence * 0.7
        } else {
            confidence
        },
        plate: if with_plate {
            plate_text(parsed.get("plate"))
        } else {
            None
        },
        team: None,
        driver: None,
        country: None,
        sponsors: visible.clone(),
        livery_text: visible,
        action,
        model_suggested: model_evidence.as_deref() == Some("design"),
        model_evidence,
        is_competition: is_competition(parsed.get("is_competition")),
        confidence,
    }
}

/// The numbers a reply listed, deduplicated by number, first sighting kept.
/// Each must pass the same [`digits`] gate as the answer's own number, so a
/// year sticker or over-long digit run cannot enter the tiebreak.
fn number_candidates(
    value: Option<&Value>,
    settings: &Settings,
) -> Vec<conrod_core::analysis::NumberCandidate> {
    let mut out: Vec<conrod_core::analysis::NumberCandidate> = Vec::new();
    for item in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(number) = digits(item.get("number"), settings) else {
            continue;
        };
        if out.iter().any(|c| c.number == number) {
            continue;
        }
        let location = text(item.get("location"))
            .map(|l| l.trim().to_lowercase())
            .unwrap_or_default();
        out.push(conrod_core::analysis::NumberCandidate { number, location });
    }
    out
}

/// The action effects the model reported, restricted to the schema's set so a
/// free-form answer cannot put prose where the UI expects a tag.
fn action_list(value: Option<&Value>) -> Vec<String> {
    const KNOWN: [&str; 8] = [
        "airborne",
        "dust",
        "tyre_smoke",
        "spray",
        "flames",
        "sparks",
        "contact",
        "wheel_lift",
    ];
    let mut out: Vec<String> = Vec::new();
    for item in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(word) = item.as_str() else {
            continue;
        };
        let key = word.trim().to_lowercase().replace([' ', '-'], "_");
        if KNOWN.contains(&key.as_str()) && !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

/// Downscale and JPEG-encode a crop for the model.
///
/// The resize uses `conrod_vision::imageops`'s Lanczos filter (checked
/// against Pillow's own output in that crate's tests, since the sharpness
/// measure's thresholds depend on matching it exactly). The JPEG *encoder*
/// here is the `image` crate's, not libjpeg, so the encoded bytes are not
/// byte-for-byte what Pillow would produce -- nothing downstream compares
/// them, only the request-building functions above are checked against
/// fixtures, and those take already-encoded strings.
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
    hints: &VlmHints,
) -> Result<VehicleDescription, VlmError> {
    describe_with_reply(client, image, settings, is_bike, hints).map(|(vehicle, _, _)| vehicle)
}

/// `describe`, plus the parsed reply and how many asks it took, for the eval
/// harness.
///
/// Even with a schema the reply runs long on a busy crop: a local model
/// repeats one livery item until the token budget cuts it off mid-string, and
/// a cut-off reply parses as nothing at all, losing the number, make and model
/// together. Such a reply gets a second ask with a light repetition penalty,
/// which breaks the loop; the penalty stays off the first ask because it costs
/// digits that repeat in the visible text.
pub fn describe_with_reply(
    client: &VlmClient,
    image: &Rgb,
    settings: &Settings,
    is_bike: bool,
    hints: &VlmHints,
) -> Result<(VehicleDescription, Value, u32), VlmError> {
    let payload_image = encode_for_model(image, settings.vlm_input_edge);
    let prompt = vehicle_prompt(settings, is_bike, hints);
    let with_plate = wants_plate(settings, hints);
    let reply_schema = schema(with_plate);
    let ask = |text: &str, penalty: Option<f64>| {
        client.call_with(
            settings,
            text,
            std::slice::from_ref(&payload_image),
            &reply_schema,
            600,
            penalty,
        )
    };
    let (parsed, attempts) = match ask(&prompt, None) {
        Ok(reply) => (reply, 1),
        Err(VlmError::BadReply(_)) => match ask(&prompt, Some(1.1)) {
            Ok(reply) => (reply, 2),
            Err(err) => {
                note_unless_fatal(client, settings, &err)?;
                return Err(err);
            }
        },
        Err(err) => {
            note_unless_fatal(client, settings, &err)?;
            return Err(err);
        }
    };
    let vehicle = map_vehicle(&parsed, settings, settings.read_numbers, with_plate);
    Ok((vehicle, parsed, attempts))
}

/// Count a failed ask against the run unless it was a Stop or a config refusal,
/// which are passed on untouched so the caller can end the run.
fn note_unless_fatal(
    client: &VlmClient,
    settings: &Settings,
    err: &VlmError,
) -> Result<(), VlmError> {
    if matches!(err, VlmError::Stopped | VlmError::Misconfigured(_)) {
        return Ok(());
    }
    note_failure(&client.fatal, &display_name(&settings.vlm_provider), err)
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
    match client.call(settings, &prompt, &encoded, &schema(false), 400) {
        Ok(parsed) => Ok(map_vehicle(&parsed, settings, false, false)),
        Err(VlmError::Stopped) => Err(VlmError::Stopped),
        Err(err) => {
            let who = display_name(&settings.vlm_provider);
            note_failure(&client.fatal, &who, &err)?;
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conrod_core::settings::VlmService;
    use std::io::{Read, Write};
    use std::sync::atomic::AtomicUsize;

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("s{i}")).collect()
    }

    #[test]
    fn round_robin_takes_turns() {
        let turn = AtomicUsize::new(0);
        let order: Vec<_> = (0..5)
            .map(|_| pick_service("round_robin", &ids(3), None, &turn))
            .collect();
        assert_eq!(order, [0, 1, 2, 0, 1]);
    }

    fn header(value: &str) -> Vec<(String, String)> {
        vec![("Retry-After".to_string(), value.to_string())]
    }

    #[test]
    fn retry_after_reads_plain_seconds_up_to_the_cap() {
        assert_eq!(retry_after(&header("120"), 0, 0.0), 120.0);
        assert_eq!(retry_after(&header("-5"), 0, 0.0), 0.0);
        assert_eq!(retry_after(&header("1e308"), 0, 0.0), MAX_RETRY_AFTER);
        assert_eq!(retry_after(&header("999999999"), 0, 0.0), MAX_RETRY_AFTER);
    }

    #[test]
    fn retry_after_ignores_a_value_that_is_not_a_finite_number() {
        for bad in ["inf", "-inf", "infinity", "NaN", "nan"] {
            let wait = retry_after(&header(bad), 2, 0.25);
            assert_eq!(wait, 4.25, "{bad} should fall back to the backoff guess");
        }
    }

    #[test]
    fn a_hostile_wait_cannot_panic_the_gate() {
        struct Quick;
        impl Clock for Quick {
            fn now(&self) -> f64 {
                0.0
            }
            fn sleep(&self, _seconds: f64) {}
        }
        let gate = RateGate::new(Arc::new(Quick));
        for seconds in [f64::INFINITY, f64::NAN, 1e308, -1.0] {
            gate.hold_off(seconds);
            gate.sleep_interruptible(seconds).unwrap();
        }
        gate.wait_turn().unwrap();
        // The real clock must clamp rather than hand `Duration` something it
        // cannot hold.
        RealClock::default().sleep(f64::NAN);
        RealClock::default().sleep(f64::NEG_INFINITY);
    }

    #[test]
    fn least_busy_never_double_books() {
        let pools = HostPools::default();
        let busy = Mutex::new(std::collections::HashSet::new());
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..20 {
                        let pool = pools.pool_for(ids(3));
                        let id = pool.acquire();
                        assert!(
                            busy.lock().unwrap().insert(id.clone()),
                            "{id} double-booked"
                        );
                        std::thread::yield_now();
                        busy.lock().unwrap().remove(&id);
                        pool.release(id);
                    }
                });
            }
        });
    }

    /// Answers one Ollama request with a canned reply.
    fn ollama_once() -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 4096];
            // Read the headers and the whole body before answering.
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                seen.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&seen).to_lowercase();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length:"))
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    if n == 0 || seen.len() >= end + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let body = br#"{"response":"{\"ok\":true}"}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
        });
        base
    }

    #[test]
    fn fails_over_to_the_next_service() {
        // A port nothing listens on: bound, then dropped.
        let dead = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            format!("http://{}", l.local_addr().unwrap())
        };
        let service = |id: &str, host: String| VlmService {
            id: id.into(),
            model: "m".into(),
            host,
            ..VlmService::default()
        };
        let settings = Settings {
            vlm_services: vec![service("dead", dead), service("live", ollama_once())],
            vlm_strategy: "round_robin".into(),
            vlm_timeout: 5.0,
            ..Settings::default()
        };
        let client = VlmClient::new(Arc::new(RealClock::default()));
        let answer = client
            .call(&settings, "p", &[], &json!({}), 8)
            .expect("the live service answers");
        assert_eq!(answer, json!({"ok": true}));
    }

    #[test]
    fn no_enabled_service_does_not_fall_back_to_the_legacy_model() {
        let service = |id: &str| VlmService {
            id: id.into(),
            provider: "ollama".into(),
            model: "qwen2.5vl:7b".into(),
            enabled: false,
            ..VlmService::default()
        };
        let settings = Settings {
            vlm_services: vec![service("off-1"), service("off-2")],
            vlm_provider: "ollama".into(),
            vlm_model: "qwen2.5vl:7b".into(),
            ..Settings::default()
        };
        let client = VlmClient::new(Arc::new(RealClock::default()));
        let answer = client.call(&settings, "p", &[], &json!({}), 8);
        assert!(
            matches!(answer, Err(VlmError::Misconfigured(_))),
            "{answer:?}"
        );
    }

    #[test]
    fn plates_are_cleaned_and_accept_mixed_or_six_letter_personalised() {
        let plate = |s: &str| plate_text(Some(&json!(s)));
        assert_eq!(plate("ab-12 cd"), Some("AB12CD".into()));
        assert_eq!(plate("1ABC.23"), Some("1ABC23".into()));
        assert_eq!(plate("SRB GTR"), Some("SRBGTR".into()), "personalised");
        assert_eq!(plate("ABCDEFG"), None, "seven letters is not a format");
        assert_eq!(plate("NISMO"), None, "a five-letter badge is not a plate");
        assert_eq!(plate("1234"), None, "digits only");
        assert_eq!(plate("A1"), Some("A1".into()));
        assert_eq!(plate("A12345678"), None, "too long");
        assert_eq!(plate("A"), None, "too short");
        assert_eq!(plate("null"), None);
    }

    #[test]
    fn visible_text_as_one_string_is_split_into_lines() {
        let list = text_list(Some(&json!("SHELL\nshell\n\nPirelli")));
        assert_eq!(list, vec!["SHELL".to_string(), "Pirelli".to_string()]);
        assert!(text_list(Some(&json!(["Shell", " SHELL "]))).len() == 1);
    }

    #[test]
    fn evidence_none_drops_the_model_and_design_suggests_it() {
        let settings = Settings::default();
        let none = json!({"make": "Ford", "model": "Mustang", "evidence": "none"});
        let d = map_vehicle(&none, &settings, true, true);
        assert_eq!(d.model, None);
        assert_eq!(d.model_evidence, None);
        assert!(!d.model_suggested);

        let design = json!({"make": "Ford", "model": "Mustang", "evidence": "design"});
        let d = map_vehicle(&design, &settings, true, true);
        assert_eq!(d.model.as_deref(), Some("Mustang"));
        assert_eq!(d.model_evidence.as_deref(), Some("design"));
        assert!(d.model_suggested);

        let badge = json!({"make": "Ford", "model": "Mustang", "evidence": "badge"});
        let d = map_vehicle(&badge, &settings, true, true);
        assert_eq!(d.model_evidence.as_deref(), Some("badge"));
        assert!(!d.model_suggested);
    }

    #[test]
    fn a_six_letter_personalised_plate_survives_the_vehicle_map() {
        let settings = Settings::default();
        let reply = json!({"plate": "SRB GTR", "evidence": "badge"});
        assert_eq!(
            map_vehicle(&reply, &settings, true, true).plate.as_deref(),
            Some("SRBGTR")
        );
    }

    #[test]
    fn racing_replies_never_carry_a_plate() {
        let settings = Settings::default();
        let reply = json!({"plate": "ABC123", "evidence": "none"});
        assert_eq!(map_vehicle(&reply, &settings, true, false).plate, None);
        let d = map_vehicle(&reply, &settings, true, true);
        assert_eq!(d.plate.as_deref(), Some("ABC123"));
    }

    #[test]
    fn action_effects_are_known_tags_only() {
        let mut settings = Settings::default();
        assert!(settings.read_action, "action is read by default");
        let reply = json!({
            "number": "7",
            "visible_text": [],
            "action": ["airborne", "tyre smoke", "FLAMES", "nonsense", "airborne"],
        });
        assert_eq!(
            map_vehicle(&reply, &settings, true, false).action,
            vec![
                "airborne".to_string(),
                "tyre_smoke".to_string(),
                "flames".to_string()
            ]
        );
        settings.read_action = false;
        assert!(map_vehicle(&reply, &settings, true, false)
            .action
            .is_empty());
        // A reply from before this field existed is not an error.
        assert!(map_vehicle(&json!({"number": "7"}), &settings, true, false)
            .action
            .is_empty());
    }

    #[test]
    fn every_visible_number_is_kept_with_its_panel() {
        use conrod_core::analysis::NumberCandidate;
        let settings = Settings::default();
        let reply = json!({
            "number": "7",
            "number_location": "windscreen",
            "numbers": [
                {"number": "7", "location": "windscreen"},
                {"number": "99", "location": "door"},
                {"number": "2023", "location": "other"},
            ],
            "confidence": 0.9,
        });
        let d = map_vehicle(&reply, &settings, true, false);
        assert_eq!(
            d.numbers,
            vec![
                NumberCandidate {
                    number: "7".into(),
                    location: "windscreen".into()
                },
                NumberCandidate {
                    number: "99".into(),
                    location: "door".into()
                },
            ],
            "a year sticker is not a race number"
        );
        // A reply from before this field existed still yields the answer's own
        // number, so the tiebreak has one candidate.
        let old = map_vehicle(&json!({"number": "5"}), &settings, true, false);
        assert_eq!(old.numbers.len(), 1);
        assert_eq!(old.numbers[0].number, "5");
    }

    #[test]
    fn the_prompt_asks_for_every_number_and_its_panel() {
        let settings = Settings::default();
        let prompt = vehicle_prompt(&settings, false, &VlmHints::default());
        assert!(prompt.contains("- numbers:"), "{prompt}");
        assert!(prompt.contains("\"bonnet\""), "{prompt}");
    }

    #[test]
    fn only_the_nose_number_lowers_the_number_confidence() {
        let settings = Settings::default();
        let door = json!({"number": "42", "number_location": "door", "confidence": 0.9});
        let window = json!({"number": "42", "number_location": "windscreen", "confidence": 0.9});
        let nose = json!({"number": "42", "number_location": "nose", "confidence": 0.9});
        assert_eq!(
            map_vehicle(&door, &settings, true, false).number_confidence,
            0.9
        );
        assert_eq!(
            map_vehicle(&window, &settings, true, false).number_confidence,
            0.9,
            "the window number is the official one, not a weak spot"
        );
        let d = map_vehicle(&nose, &settings, true, false);
        assert!((d.number_confidence - 0.63).abs() < 1e-9);
        assert_eq!(d.confidence, 0.9);
    }

    #[test]
    fn the_prompt_never_asks_about_a_car_when_the_crop_is_not_one() {
        let settings = Settings::default();
        let ask = |subject, is_bike| {
            vehicle_prompt(
                &settings,
                is_bike,
                &VlmHints {
                    subject,
                    ..VlmHints::default()
                },
            )
        };
        let plane = ask(SubjectKind::Aircraft, false);
        assert!(plane.contains("aircraft"), "{plane}");
        assert!(!plane.contains("car"), "{plane}");
        assert!(
            plane.contains("- plate:"),
            "registrations ride the plate line"
        );
        let cycle = ask(SubjectKind::Bicycle, false);
        assert!(cycle.contains("bicycle"), "{cycle}");
        assert!(!cycle.contains("motorcycle"), "{cycle}");
        assert!(ask(SubjectKind::Road, true).contains("motorcycle"));
        assert!(ask(SubjectKind::Road, false).contains("competition car"));
        assert_eq!(SubjectKind::from_class("airplane"), SubjectKind::Aircraft);
        assert_eq!(SubjectKind::from_class("bicycle"), SubjectKind::Bicycle);
        assert_eq!(SubjectKind::from_class("truck"), SubjectKind::Road);
    }

    #[test]
    fn candidate_line_lists_the_entry_list_or_nothing() {
        let none = VlmHints::default();
        assert_eq!(candidate_line(&none), "");
        let some = VlmHints {
            racing: false,
            candidates: vec!["Ford Mustang S650".into(), "Toyota GR Supra".into()],
            ..VlmHints::default()
        };
        assert_eq!(
            candidate_line(&some),
            "make/model must be one of: Ford Mustang S650, Toyota GR Supra."
        );
    }

    #[test]
    fn the_opencode_provider_speaks_openai_with_json_object() {
        let settings = Settings {
            vlm_provider: "opencode".into(),
            vlm_model: "qwen3.8-max".into(),
            vlm_api_key: "sk-test".into(),
            vlm_host: String::new(),
            ..Default::default()
        };
        let schema = json!({"type": "object", "properties": {"number": {"type": "string"}}});
        let request = build_request(&settings, "read the number", &["abc".into()], &schema, 100)
            .expect("opencode is a provider");
        assert_eq!(request.url, OPENCODE_CHAT_URL);
        assert!(request
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer sk-test"));
        assert_eq!(request.json["response_format"]["type"], "json_object");
        // The schema travels in the prompt: the gateway is not asked for a
        // strict schema it may not support, and the reply parser reads JSON out
        // of prose anyway.
        let text = request.json["messages"][0]["content"][0]["text"]
            .as_str()
            .expect("a text part");
        assert!(text.contains("read the number"));
        assert!(text.contains("\"number\""));
    }

    #[test]
    fn the_openrouter_provider_speaks_openai_with_a_bearer_key() {
        let settings = Settings {
            vlm_provider: "openrouter".into(),
            vlm_model: "qwen/qwen2.5-vl-72b-instruct".into(),
            vlm_api_key: "sk-or-test".into(),
            ..Default::default()
        };
        let schema = json!({"type": "object", "properties": {"number": {"type": "string"}}});
        let request = build_request(&settings, "read the number", &["abc".into()], &schema, 100)
            .expect("openrouter is a provider");
        assert_eq!(request.url, OPENROUTER_URL);
        assert!(request
            .headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Bearer sk-or-test"));
        // json_object, not a strict schema: OpenRouter models do not all accept one.
        assert_eq!(request.json["response_format"]["type"], "json_object");
        // The image travels as an OpenAI image_url data URI.
        assert_eq!(
            request.json["messages"][0]["content"][1]["type"],
            "image_url"
        );
        assert_eq!(
            parse_response(
                "openrouter",
                &json!({"choices": [{"message": {"content": "{\"number\": \"7\"}"}}]})
            )
            .unwrap()["number"],
            "7"
        );
    }

    #[test]
    fn an_openrouter_key_check_reports_the_account_and_never_the_key() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/key", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 2048];
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                seen.extend_from_slice(&buf[..n]);
                if n == 0 || String::from_utf8_lossy(&seen).contains("\r\n\r\n") {
                    break;
                }
            }
            let request = String::from_utf8_lossy(&seen).to_lowercase();
            assert!(request.starts_with("get "), "{request}");
            assert!(
                request.contains("authorization: bearer sk-or-test"),
                "{request}"
            );
            let body = br#"{"data":{"label":"conrod-test","limit":50,"usage":1}}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
        });
        let answer = openrouter_key_at(&url, " sk-or-test ").expect("the key is accepted");
        server.join().unwrap();
        assert_eq!(answer["ok"], true);
        assert_eq!(answer["label"], "conrod-test");

        assert!(
            openrouter_key_check("   ").is_err(),
            "a blank key is not sent"
        );
        let denied =
            openrouter_key_reply(401, &json!({"error": {"message": "No auth"}})).unwrap_err();
        assert!(denied.contains("401") && denied.contains("No auth"));
        assert!(!denied.contains("sk-"));
    }

    #[test]
    fn an_opencode_key_check_reads_the_model_list_and_spots_jev() {
        let body = json!({
            "data": [
                {"id": "jev-1.13", "name": "Jev 1.13"},
                {"id": "jev-1.13-free", "name": "Jev 1.13 Free"},
                {"id": "glm-5.3", "name": "GLM"}
            ]
        });
        let reply = opencode_key_reply(200, &body).unwrap();
        assert_eq!(reply["ok"], true);
        assert!(
            reply["label"].as_str().unwrap().contains("JEV ready"),
            "{:?}",
            reply["label"]
        );
        let no_jev = json!({"data": [{"id": "glm-5.3"}]});
        assert!(opencode_key_reply(200, &no_jev).unwrap()["label"]
            .as_str()
            .unwrap()
            .contains("JEV not listed"));
        let denied =
            opencode_key_reply(401, &json!({"error": {"message": "Invalid API key"}})).unwrap_err();
        assert!(denied.contains("401") && denied.contains("Invalid API key"));
        assert!(opencode_key_reply(200, &json!({"nope": true})).is_err());
        assert!(opencode_key_check("  ").is_err(), "a blank key is not sent");
    }

    #[test]
    fn model_lists_stay_on_allowlisted_hosts_and_time_out() {
        // A fixed provider ignores a webview-supplied host entirely.
        assert_eq!(
            models_url("openai", "http://attacker.example/v1").unwrap(),
            (OPENAI_MODELS_URL.to_string(), true)
        );
        assert_eq!(
            models_url("openrouter", "http://attacker.example/v1").unwrap(),
            (OPENROUTER_MODELS_URL.to_string(), true)
        );
        // Providers with no list endpoint are refused, whatever host is sent.
        assert!(models_url("gemini", "").is_err());
        assert!(models_url("ollama", "http://127.0.0.1:11434").is_err());
        // OpenCode's documented custom base URL survives, http(s) only.
        assert!(models_url("opencode", "file:///etc/passwd").is_err());
        assert!(models_url("opencode", "ftp://proxy.example/v1").is_err());
        assert!(models_url("opencode", "proxy.example/v1").is_err());
        assert!(models_url("opencode", r"\\server\share\models").is_err());
        // An unencrypted URL must stay on this machine or the private network.
        assert!(models_url("opencode", "http://attacker.example/v1").is_err());
        assert!(models_url("opencode", "http://169.254.169.254/v1").is_err());
        for local in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:8080/v1",
            "http://192.168.1.20/v1",
            "http://[::1]:8080/v1",
        ] {
            assert!(models_url("opencode", local).is_ok(), "{local}");
        }
        assert_eq!(
            models_url("opencode", "https://proxy.example/v1")
                .unwrap()
                .0,
            "https://proxy.example/v1/models"
        );
        // The request cannot hang the Settings screen.
        let timeouts = models_agent().config().timeouts();
        assert_eq!(timeouts.connect, Some(Duration::from_secs(10)));
        assert_eq!(timeouts.global, Some(Duration::from_secs(20)));
    }

    #[test]
    fn an_empty_openai_reply_names_the_finish_reason() {
        let body = json!({"choices": [{"finish_reason": "length", "message": {"content": ""}}]});
        let err = parse_openai_response(&body).expect_err("empty content is not an answer");
        assert!(
            format!("{err}").contains("finish_reason length"),
            "the error should say the budget ran out, not 'EOF at line 1': {err}"
        );
    }

    #[test]
    fn the_opencode_go_alias_bills_credits_and_never_the_subscription() {
        // Settings saved by an older build say `opencode-go`. That must now
        // spend Console credits, not the Go coding-assistant allowance.
        let settings = Settings {
            vlm_provider: "opencode-go".into(),
            vlm_model: "glm-5.3-flash".into(),
            vlm_api_key: "sk-test".into(),
            vlm_host: String::new(),
            ..Default::default()
        };
        let schema = json!({"type": "object", "properties": {"number": {"type": "string"}}});
        let request = build_request(&settings, "read the number", &["abc".into()], &schema, 100)
            .expect("opencode-go is still a settings alias");
        assert_eq!(
            request.url, OPENCODE_CHAT_URL,
            "the alias uses the Console credits base, never /inference/go"
        );
        let header = |name: &str| {
            request
                .headers
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| v.as_str())
        };
        assert_eq!(header("Authorization"), Some("Bearer sk-test"));
        assert_eq!(header("User-Agent"), Some(OPENCODE_USER_AGENT));
        assert_eq!(
            header("x-opencode-session"),
            None,
            "a session id is only for Go routing, which Conrod must not use"
        );
        // The model is the plain id, not the opencode-go/ prefix OpenCode's own
        // config uses.
        assert_eq!(request.json["model"], "glm-5.3-flash");
        assert_eq!(request.json["response_format"]["type"], "json_object");
        // Parsing is the OpenAI envelope, like the pay-as-you-go provider.
        let reply = json!({"choices": [{"message": {"content": "{\"number\":\"96\"}"}}]});
        assert_eq!(
            parse_response("opencode-go", &reply).unwrap()["number"],
            "96"
        );
        // The model list follows the same rule: the Console list, not Go's.
        assert_eq!(
            models_url("opencode-go", "").unwrap(),
            (OPENCODE_MODELS_URL.to_string(), true)
        );
        assert_eq!(display_name("opencode-go"), display_name("opencode"));
        assert!(display_name("opencode").contains("credits"));
    }

    #[test]
    fn an_opencode_endpoint_is_completed_with_the_chat_path() {
        assert_eq!(opencode_chat_url(""), OPENCODE_CHAT_URL);
        assert_eq!(
            opencode_chat_url("https://proxy.local/v1/"),
            "https://proxy.local/v1/chat/completions"
        );
        assert_eq!(
            opencode_chat_url("https://proxy.local/v1/chat/completions"),
            "https://proxy.local/v1/chat/completions"
        );
    }
}
