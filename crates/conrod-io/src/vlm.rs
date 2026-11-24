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
    /// The provider answered, but not with something `describe`
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

/// One note per process when the vision list holds the moments lane's own seat
/// (`in_process`). It is ignored rather than asked -- asking would fail every
/// crop with "unknown vision provider" -- but the user should still learn their
/// settings file needs a clean, so it is said once at debug level.
fn note_moments_seat(provider: &str) {
    static NOTE: std::sync::Once = std::sync::Once::new();
    NOTE.call_once(|| {
        eprintln!(
            "vlm: ignoring a '{provider}' entry in vlm_services (a moments service, not a vision provider)"
        );
    });
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
        // The service's own thinking switch, not the global one: the settings
        // card chooses it per entry as well as for the list.
        vlm_reasoning: service.reasoning,
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

/// Ollama's context window for a single-crop request. One 1568 px image is
/// about 2000 tokens (768 px on Qwen3-VL is ~1080 tokens), the system prompt
/// and schema ~500, and the reply at most 600: 4096 fits all of it while
/// halving KV-cache VRAM compared to 8192. Without this Ollama uses the
/// model's 128k default, whose KV cache alone is several GB and spills
/// qwen2.5vl:7b / qwen3-vl:8b off the GPU.
pub const OLLAMA_NUM_CTX: i64 = 4096;

/// Ollama's context window when a request sends multiple images (e.g. a burst).
pub const OLLAMA_MULTI_IMAGE_NUM_CTX: i64 = 8192;

/// Ollama's prompt/image evaluation batch size. Ollama injects
/// `--image-min-tokens 1024` for Qwen-VL models, so non-square crops round up
/// to ~1040–1078 image tokens; with the default 1024 batch size the trailing
/// 16–56 tokens spill into a second batch that stalls ROCm/CUDA prefill by
/// ~1 s per crop. 2048 keeps every single-crop image in one batch.
pub const OLLAMA_NUM_BATCH: i64 = 2048;

/// Short per-photo user prompt paired with [`vehicle_prompt`] as the system
/// message. Placing the ~450-token static instructions in `system` puts them
/// before the per-photo image tokens so Ollama and `llama-server` reuse the
/// KV cache across every crop in a shoot.
pub const VEHICLE_USER_PROMPT: &str = "Describe the subject in this photo.";

/// Add a provider's own switch for model thinking. `on` is the vision service
/// entry's `reasoning` flag (`vlm_reasoning` before the per-service field
/// existed). Only parameters the provider documents are sent:
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

#[allow(clippy::too_many_arguments)]
pub fn ollama_request(
    host: &str,
    model: &str,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> Result<ProviderRequest, VlmError> {
    ollama_request_with_system(host, model, None, prompt, images, schema, num_predict)
}

#[allow(clippy::too_many_arguments)]
pub fn ollama_request_with_system(
    host: &str,
    model: &str,
    system: Option<&str>,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> Result<ProviderRequest, VlmError> {
    let host = host.trim_end_matches('/');
    let url = format!("{host}/api/generate");
    guard_remote(&url, "vlm_host")?;
    let num_ctx = if images.len() > 1 {
        OLLAMA_MULTI_IMAGE_NUM_CTX
    } else {
        OLLAMA_NUM_CTX
    };
    let mut body = json!({
        "model": model,
        "prompt": prompt,
        "images": images,
        "stream": false,
        "format": schema,
        "keep_alive": "2m",
        "options": {
            "temperature": 0.0,
            "num_predict": num_predict,
            "num_ctx": num_ctx,
            "num_batch": OLLAMA_NUM_BATCH,
        },
    });
    if let Some(sys) = system.map(str::trim).filter(|s| !s.is_empty()) {
        body["system"] = json!(sys);
    }
    Ok(ProviderRequest {
        url,
        headers: vec![],
        query: vec![],
        json: body,
    })
}

pub fn openai_request(
    model: &str,
    api_key: &str,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> ProviderRequest {
    openai_request_at_with_system(
        OPENAI_URL,
        true,
        model,
        api_key,
        None,
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
    openai_request_at_with_system(
        url,
        strict,
        model,
        api_key,
        None,
        prompt,
        images,
        schema,
        num_predict,
    )
}

/// Like [`openai_request_at`], with an optional `system` prompt placed before
/// the user message so local `llama-server` and cloud providers can reuse the
/// static instruction KV-cache prefix across crops.
#[allow(clippy::too_many_arguments)]
pub fn openai_request_at_with_system(
    url: &str,
    strict: bool,
    model: &str,
    api_key: &str,
    system: Option<&str>,
    prompt: &str,
    images: &[String],
    schema: &Value,
    num_predict: i64,
) -> ProviderRequest {
    let sys = system.map(str::trim).filter(|s| !s.is_empty());
    let (system_text, user_text) = match sys {
        Some(sys) => {
            let sys_text = if strict {
                sys.to_string()
            } else {
                format!("{sys}\n\nReply with one JSON object matching this schema:\n{schema}")
            };
            (Some(sys_text), prompt.to_string())
        }
        None => {
            let user = if strict {
                prompt.to_string()
            } else {
                format!("{prompt}\n\nReply with one JSON object matching this schema:\n{schema}")
            };
            (None, user)
        }
    };
    let mut content = vec![json!({"type": "text", "text": user_text})];
    for image in images {
        content.push(json!({
            "type": "image_url",
            "image_url": {"url": format!("data:image/jpeg;base64,{image}")},
        }));
    }
    let mut messages = Vec::with_capacity(2);
    if let Some(sys_text) = system_text {
        messages.push(json!({"role": "system", "content": sys_text}));
    }
    messages.push(json!({"role": "user", "content": content}));
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
            "messages": messages,
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
            guard_remote(host, "vlm_host").map_err(|e| e.to_string())?;
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

/// The one rule every request that carries a user-supplied address obeys: an
/// https URL is always fine, and so is a plain-http URL that stays on the
/// user's own machine or private network (Ollama, a LAN gateway). Everything
/// else -- plain http to a remote host, `file:`, a UNC path, a bare host name
/// -- is refused with a `Misconfigured` error naming the setting to fix, and
/// refused *before* a key is attached or a socket is opened. A public
/// plain-http host would put the API key (and the photo) on the wire in clear
/// text, and a request to any host a setting names is an SSRF primitive.
pub fn guard_remote(url: &str, setting: &str) -> Result<(), VlmError> {
    let url = url.trim();
    if !is_http_url(url) {
        return Err(VlmError::Misconfigured(format!(
            "{setting} must be an http(s) address, not {url:?}"
        )));
    }
    if !url.to_ascii_lowercase().starts_with("https://") && !is_local_host(url) {
        return Err(VlmError::Misconfigured(format!(
            "{setting} is {url:?}: the key only goes to a secure or local address, so use https for a remote host, or http://127.0.0.1, http://localhost, or a private-network address"
        )));
    }
    Ok(())
}

/// The local Ollama models identify can hold on this machine, as
/// `(host, model)`: every enabled service on a loopback host (`localhost`,
/// `127.0.0.0/8`, `::1`), or the legacy single-provider fields when no service
/// list is configured. A cloud service, remote host, or separate LAN server
/// never leaks into this list.
pub fn local_ollama_models(settings: &Settings) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |provider: &str, host: &str, model: &str| {
        let host = if host.trim().is_empty() {
            conrod_core::settings::DEFAULT_VLM_HOST
        } else {
            host.trim()
        };
        let host = host.trim_end_matches('/').to_string();
        let model = model.trim();
        if provider.eq_ignore_ascii_case("ollama")
            && is_loopback_host(&host)
            && !model.is_empty()
            && !out.iter().any(|(h, m)| *h == host && m == model)
        {
            out.push((host, model.to_string()));
        }
    };
    if !settings.vlm_services.is_empty() {
        for service in settings.vlm_services.iter().filter(|s| s.enabled) {
            push(&service.provider, &service.host, &service.model);
        }
    } else {
        push(
            &settings.vlm_provider,
            &settings.vlm_host,
            &settings.vlm_model,
        );
    }
    out
}

/// Hooks registered by the engine so `provider: "builtin"` can lazily start and
/// release Conrod's managed local `llama-server` without `conrod-io` depending
/// on `conrod-engine`.
type BuiltinEndpointFn = Arc<dyn Fn() -> Result<String, String> + Send + Sync>;
type BuiltinReleaseFn = Arc<dyn Fn() + Send + Sync>;

/// Held while one request is in flight against the built-in server (dropped
/// to release it), so the engine's idle timer never unloads the server under
/// a slow answer.
pub type BuiltinLease = Box<dyn std::any::Any + Send>;
pub type BuiltinLeaseFn = Arc<dyn Fn() -> BuiltinLease + Send + Sync>;

struct BuiltinBackend {
    endpoint: BuiltinEndpointFn,
    release: BuiltinReleaseFn,
    lease: Option<BuiltinLeaseFn>,
}

fn builtin_backend() -> &'static Mutex<Option<BuiltinBackend>> {
    static BACKEND: std::sync::OnceLock<Mutex<Option<BuiltinBackend>>> = std::sync::OnceLock::new();
    BACKEND.get_or_init(|| Mutex::new(None))
}

/// Register the built-in local vision backend (endpoint resolver + VRAM release).
pub fn register_builtin(endpoint: BuiltinEndpointFn, release: BuiltinReleaseFn) {
    let mut backend = lock(builtin_backend());
    let lease = backend.as_ref().and_then(|b| b.lease.clone());
    *backend = Some(BuiltinBackend {
        endpoint,
        release,
        lease,
    });
}

/// Register the in-flight lease for the built-in backend. Call after
/// [`register_builtin`]; calling [`register_builtin`] again keeps it.
pub fn register_builtin_lease(lease: BuiltinLeaseFn) {
    if let Some(backend) = lock(builtin_backend()).as_mut() {
        backend.lease = Some(lease);
    }
}

/// A lease on the built-in server for the span of one request, if one is wired.
fn builtin_lease() -> Option<BuiltinLease> {
    let lease = lock(builtin_backend())
        .as_ref()
        .and_then(|b| b.lease.clone());
    lease.map(|lease| lease())
}

fn resolve_builtin_endpoint() -> Result<String, VlmError> {
    let endpoint = lock(builtin_backend())
        .as_ref()
        .map(|b| b.endpoint.clone())
        .ok_or_else(|| {
            VlmError::Misconfigured(
                "Built-in vision runtime is not initialized in this process".into(),
            )
        })?;
    // A server that cannot start fails every photo the same way: that is the
    // configuration (or the machine), not one frame, so it ends the run with
    // the plain reason instead of failing thousands of photos one by one.
    endpoint().map_err(VlmError::Misconfigured)
}

/// Unload the built-in vision server if one is registered.
pub fn release_builtin() -> bool {
    let release = lock(builtin_backend()).as_ref().map(|b| b.release.clone());
    if let Some(release) = release {
        release();
        true
    } else {
        false
    }
}

/// Whether the settings enable Conrod's built-in vision provider (`provider: "builtin"`).
pub fn uses_builtin_vision(settings: &Settings) -> bool {
    if !settings.vlm_services.is_empty() {
        settings
            .vlm_services
            .iter()
            .any(|s| s.enabled && s.provider.eq_ignore_ascii_case("builtin"))
    } else {
        settings.vlm_provider.eq_ignore_ascii_case("builtin")
    }
}

/// Ask every local Ollama endpoint identify uses to drop its model now
/// (`keep_alive: 0`), and release the built-in vision server if enabled, so
/// the next stage's model gets the whole card. This is the serial-by-stage
/// discipline: Ollama would otherwise keep the vision model resident for its
/// `keep_alive` window. Best effort and quick: a host that is down, remote or
/// refuses is skipped and never fails a run. Returns how many endpoints were
/// asked.
pub fn release_local_models(settings: &Settings) -> usize {
    let mut asked = 0;
    if uses_builtin_vision(settings) && release_builtin() {
        asked += 1;
    }
    for (host, model) in local_ollama_models(settings) {
        let url = format!("{host}/api/generate");
        // A plain-http host that is not local must never be contacted, even to
        // unload; the guard answers before a socket is opened.
        if guard_remote(&url, "vlm_host").is_err() {
            continue;
        }
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(2)))
            .build()
            .into();
        let body = json!({"model": model, "prompt": "", "stream": false, "keep_alive": 0});
        // The reply is deliberately ignored: an unload is a hint to the server,
        // and a model that stays resident still leaves the run correct.
        let _ = agent.post(&url).send_json(&body);
        asked += 1;
    }
    asked
}

/// `localhost`, a loopback address, or a private-network address (RFC 1918,
/// unique-local IPv6): where an unencrypted request stays on the user's own
/// network, e.g. Ollama or a gateway on the LAN. Link-local addresses (cloud
/// metadata services) are not included.
pub fn is_local_host(url: &str) -> bool {
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

/// `localhost` or a loopback address (`127.0.0.0/8`, `::1`): a server running
/// on *this* machine and sharing its GPU, excluding separate machines on the
/// private LAN.
pub fn is_loopback_host(url: &str) -> bool {
    let trimmed = url.trim();
    let host = trimmed
        .parse::<ureq::http::Uri>()
        .ok()
        .and_then(|u| u.host().map(str::to_lowercase))
        .or_else(|| {
            (!trimmed.is_empty() && !trimmed.contains("://") && !trimmed.contains('/'))
                .then(|| trimmed.to_ascii_lowercase())
        });
    let Some(host) = host else {
        return false;
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host == "localhost" || host.ends_with(".localhost") {
        return true;
    }
    match host.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback(),
        Ok(std::net::IpAddr::V6(ip)) => ip.is_loopback(),
        Err(_) => false,
    }
}

/// The models a provider key can call, with the provider's published price
/// where it has one. Uses the provider's own list endpoint; anything that is
/// not an OpenAI-shaped `{"data":[{"id":…}]}` (or `{"models":[…]}`) is reported
/// as an error rather than guessed at.
pub fn list_models(
    provider: &str,
    host: &str,
    api_key: &str,
) -> Result<Vec<ProviderModel>, String> {
    let provider = provider.to_lowercase();
    let (_, list) = fetch_model_items(&provider, host, api_key)?;
    Ok(parse_provider_models(&list))
}

/// One model a provider lists, and its price when the catalogue publishes one.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderModel {
    pub id: String,
    pub name: String,
    /// Omitted when the catalogue publishes no price for this model.
    pub pricing: Option<ModelPricing>,
}

impl ProviderModel {
    /// The wire shape the settings card reads: `id`, `name`, and a `pricing`
    /// object only when the provider published a price.
    pub fn to_json(&self) -> Value {
        let mut object = serde_json::Map::new();
        object.insert("id".into(), Value::String(self.id.clone()));
        object.insert("name".into(), Value::String(self.name.clone()));
        if let Some(pricing) = self.pricing {
            object.insert(
                "pricing".into(),
                json!({"prompt": pricing.prompt, "completion": pricing.completion}),
            );
        }
        Value::Object(object)
    }
}

/// A model's price in US dollars per million tokens: the settings card's
/// "est. per photo / per 1,000" is a token count times these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelPricing {
    pub prompt: f64,
    pub completion: f64,
}

/// A catalogue's per-token price as dollars per million. OpenAI-shaped
/// catalogues (OpenRouter, OpenCode) publish `pricing.prompt` and
/// `pricing.completion` as USD per token, as a number or a numeric string; a
/// catalogue that already says per-million is taken as written.
fn price_per_million(item: &Value, key: &str) -> Option<f64> {
    fn number(value: &Value) -> Option<f64> {
        value
            .as_f64()
            .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
    }
    if let Some(per_m) = item
        .pointer(&format!("/pricing/{key}_per_m"))
        .or_else(|| item.pointer(&format!("/pricing/{key}_per_million")))
        .and_then(number)
    {
        return (per_m.is_finite() && per_m >= 0.0).then_some(per_m);
    }
    let per_token = item.pointer(&format!("/pricing/{key}")).and_then(number)?;
    (per_token.is_finite() && per_token >= 0.0).then_some(per_token * 1_000_000.0)
}

/// One list entry's price, or `None` when it publishes no usable pair.
fn model_pricing(item: &Value) -> Option<ModelPricing> {
    Some(ModelPricing {
        prompt: price_per_million(item, "prompt")?,
        completion: price_per_million(item, "completion")?,
    })
}

/// The catalogue's entries as models, sorted by id, each with its price where
/// the provider publishes one.
pub fn parse_provider_models(list: &[Value]) -> Vec<ProviderModel> {
    let mut models: Vec<ProviderModel> = list
        .iter()
        .filter_map(|item| {
            let id = item
                .get("id")
                .or_else(|| item.get("name"))
                .and_then(Value::as_str)?;
            let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
            Some(ProviderModel {
                id: id.to_string(),
                name: name.to_string(),
                pricing: model_pricing(item),
            })
        })
        .collect();
    models.sort_by(|a, b| a.id.cmp(&b.id));
    models.dedup_by(|a, b| a.id == b.id);
    models
}

/// The raw entries of a provider's model list, and the URL they came from.
fn fetch_model_items(
    provider: &str,
    host: &str,
    api_key: &str,
) -> Result<(String, Vec<Value>), String> {
    let (url, auth) = models_url(provider, host)?;
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
    let list = model_items(&body)
        .ok_or_else(|| format!("{url} did not return a model list"))?
        .to_vec();
    Ok((url, list))
}

/// The entries of an OpenAI-shaped model list, or `{"models":[…]}`.
fn model_items(body: &Value) -> Option<&Vec<Value>> {
    body.get("data")
        .or_else(|| body.get("models"))
        .and_then(Value::as_array)
}

/// A model a service offers for judging photographs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionModel {
    pub id: String,
    pub name: String,
    /// `Some(true)` when the service's own metadata lists image input,
    /// `Some(false)` when it lists only text, `None` when it says nothing about
    /// input at all.
    pub image: Option<bool>,
    /// The metadata did not say whether it takes photos: offer it, but let the
    /// Test button be the truth.
    pub unverified: bool,
}

/// What a service offers the moment lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionModels {
    /// Decision-capable, photo-capable models, in the service's own list order.
    /// A text-only decision model is never here.
    pub models: Vec<DecisionModel>,
    /// The service exposes no decision marker, so the screen keeps a free-text
    /// model field (OpenRouter) instead of pretending to enumerate.
    pub free_text: bool,
    /// A plain-words reason the list is empty or the known model is withheld.
    pub note: Option<String>,
}

/// The input modalities a model's metadata lists, when it lists any.
fn input_modalities(item: &Value) -> Option<Vec<String>> {
    let candidates = [
        item.get("input"),
        item.get("input_modalities"),
        item.pointer("/architecture/input_modalities"),
        item.pointer("/architecture/input"),
        item.pointer("/modalities/input"),
    ];
    for candidate in candidates.into_iter().flatten() {
        let parts: Vec<String> = match candidate {
            Value::Array(items) => items
                .iter()
                .filter_map(|part| {
                    part.as_str()
                        .or_else(|| part.get("type").and_then(Value::as_str))
                })
                .map(str::to_string)
                .collect(),
            Value::String(text) => text
                .split([',', '+', ';', ' '])
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect(),
            _ => Vec::new(),
        };
        if !parts.is_empty() {
            return Some(parts);
        }
    }
    None
}

/// Whether a model's metadata says it takes photographs: `Some(false)` is a
/// text-only model and is never offered.
fn image_capability(item: &Value) -> Option<bool> {
    input_modalities(item).map(|parts| {
        parts
            .iter()
            .any(|part| part.to_ascii_lowercase().contains("image"))
    })
}

/// One entry as a moment model, without requiring a decision marker.
fn decision_model(item: &Value, image: Option<bool>) -> Option<DecisionModel> {
    let id = item
        .get("id")
        .or_else(|| item.get("name"))
        .and_then(Value::as_str)?;
    let name = item.get("name").and_then(Value::as_str).unwrap_or(id);
    Some(DecisionModel {
        id: id.to_string(),
        name: name.to_string(),
        image,
        unverified: image.is_none(),
    })
}

/// One decision-typed entry as a moment model, or `None` when it is not a
/// decision model or its own metadata says it takes no images.
fn decision_entry(item: &Value) -> Option<DecisionModel> {
    let kind = item
        .get("type")
        .or_else(|| item.get("model_type"))
        .or_else(|| item.pointer("/metadata/type"))
        .and_then(Value::as_str)?;
    if !kind.to_ascii_lowercase().contains("decision") {
        return None;
    }
    let image = image_capability(item);
    if image == Some(false) {
        return None;
    }
    decision_model(item, image)
}

/// Whether an OpenRouter catalogue entry's id or name names a decision model.
/// Its endpoint publishes no decision type, so the marker in the name is the
/// only signal: `decision`, `decider`, or the JEV family's `jev`.
fn names_a_decider(item: &Value) -> bool {
    let marker = format!(
        "{} {}",
        item.get("id").and_then(Value::as_str).unwrap_or(""),
        item.get("name").and_then(Value::as_str).unwrap_or("")
    )
    .to_ascii_lowercase();
    ["decision", "decider", "jev"]
        .iter()
        .any(|part| marker.contains(part))
}

/// One OpenRouter entry as a moment model, or `None` when its name carries no
/// decision marker or its metadata says it takes only text. An entry that says
/// nothing about input is offered as unverified: the Test button is the truth.
fn openrouter_decision_entry(item: &Value) -> Option<DecisionModel> {
    if !names_a_decider(item) {
        return None;
    }
    let image = image_capability(item);
    if image == Some(false) {
        return None;
    }
    decision_model(item, image)
}

/// Parse a model-list reply for the moment lane. OpenCode marks each entry's
/// type and input modalities, so only its `decision` models that take photos
/// are offered. OpenRouter's catalogue describes chat models with no decision
/// type, so its decision-capable entries are the ones whose id or name carries
/// the marker such a model uses (`decision`, `decider`, `jev`); only the
/// photo-capable ones are offered. When nothing matches, its free-text field
/// and the known-good default stay, so the screen never dead-ends.
pub fn decision_models_reply(
    provider: &str,
    body: &Value,
    configured: &str,
) -> Result<DecisionModels, String> {
    match provider.to_lowercase().as_str() {
        "openrouter" => {
            let list = model_items(body);
            let models: Vec<DecisionModel> = list
                .map(|items| items.iter().filter_map(openrouter_decision_entry).collect())
                .unwrap_or_default();
            let configured = configured.trim();
            // A configured model the catalogue lists text-only is never
            // offered, and the screen is told why it has no listed default.
            let mut note = None;
            if !configured.is_empty() {
                if let Some(item) = list.and_then(|items| {
                    items
                        .iter()
                        .find(|item| item.get("id").and_then(Value::as_str) == Some(configured))
                }) {
                    if image_capability(item) == Some(false) {
                        note = Some(
                            "No photo-capable decision model available for this service".into(),
                        );
                    }
                }
            }
            let free_text = models.is_empty();
            Ok(DecisionModels {
                models,
                free_text,
                note,
            })
        }
        "opencode" | "opencode-go" => {
            let list = model_items(body).ok_or("OpenCode replied without a model list")?;
            Ok(DecisionModels {
                models: list.iter().filter_map(decision_entry).collect(),
                free_text: false,
                note: None,
            })
        }
        other => Err(format!(
            "'{other}' does not list decision models; type the model name"
        )),
    }
}

/// Ask a service which of its models can judge photographs. Uses the same
/// list request the Test button makes, so no extra cost beyond that one call.
pub fn decision_models(
    provider: &str,
    host: &str,
    api_key: &str,
    configured: &str,
) -> Result<DecisionModels, String> {
    let provider = provider.to_lowercase();
    match provider.as_str() {
        // OpenRouter's catalogue has no decision type, so the live list is
        // matched by name; the free-text field is the fallback when nothing
        // matches, and the configured model's modality is checked for it.
        "openrouter" => {
            let (_, list) = fetch_model_items(&provider, host, api_key)?;
            let body = json!({ "data": list });
            decision_models_reply(&provider, &body, configured)
        }
        "opencode" | "opencode-go" => {
            let (_, list) = fetch_model_items(&provider, host, api_key)?;
            let body = json!({ "data": list });
            decision_models_reply(&provider, &body, configured)
        }
        other => Err(format!(
            "'{other}' does not list decision models; type the model name"
        )),
    }
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
    guard_remote(url, "OpenRouter endpoint").map_err(|e| e.to_string())?;
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
    guard_remote(url, "OpenCode endpoint").map_err(|e| e.to_string())?;
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

/// The chat URL for an OpenCode service, guarded like every other address a
/// setting names: https, or plain http on the user's own machine or LAN only.
pub fn opencode_chat_url(host: &str) -> Result<String, VlmError> {
    let url = opencode_url(host, OPENCODE_CHAT_URL);
    guard_remote(&url, "vlm_host")?;
    Ok(url)
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
    anthropic_request_with_system(settings, None, prompt, images, schema, num_predict)
}

pub fn anthropic_request_with_system(
    settings: &Settings,
    system: Option<&str>,
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
    let mut body = json!({
        "model": settings.vlm_model,
        "max_tokens": num_predict,
        "tools": [{
            "name": "describe_vehicle",
            "description": "Record the extracted vehicle fields.",
            "input_schema": schema,
        }],
        "tool_choice": {"type": "tool", "name": "describe_vehicle"},
        "messages": [{"role": "user", "content": content}],
    });
    if let Some(sys) = system.map(str::trim).filter(|s| !s.is_empty()) {
        body["system"] = json!(sys);
    }
    ProviderRequest {
        url: ANTHROPIC_URL.into(),
        headers,
        query: vec![],
        json: body,
    }
}

pub fn gemini_request(
    model: &str,
    api_key: &str,
    prompt: &str,
    images: &[String],
    num_predict: i64,
) -> ProviderRequest {
    gemini_request_with_system(model, api_key, None, prompt, images, num_predict)
}

pub fn gemini_request_with_system(
    model: &str,
    api_key: &str,
    system: Option<&str>,
    prompt: &str,
    images: &[String],
    num_predict: i64,
) -> ProviderRequest {
    let mut parts = vec![json!({"text": prompt})];
    for image in images {
        parts.push(json!({"inline_data": {"mime_type": "image/jpeg", "data": image}}));
    }
    let mut body = json!({
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
    });
    if let Some(sys) = system.map(str::trim).filter(|s| !s.is_empty()) {
        body["systemInstruction"] = json!({"parts": [{"text": sys}]});
    }
    ProviderRequest {
        url: GEMINI_URL.replace("{model}", model),
        // The key travels in the header, never the query string: URLs land in
        // proxy and server logs, headers do not (same contract as OpenAI and
        // Anthropic above).
        headers: vec![("x-goog-api-key".into(), api_key.to_string())],
        query: vec![],
        json: body,
    }
}

/// How each provider is spelled when shown to someone.
pub fn display_name(provider: &str) -> String {
    match provider.to_lowercase().as_str() {
        "builtin" => "This PC (built-in)".into(),
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
    build_request_with_system(settings, None, prompt, images, schema, num_predict)
}

/// Like [`build_request`], carrying an optional `system` instruction prompt
/// placed before the per-photo user turn for KV-cache prefix reuse.
pub fn build_request_with_system(
    settings: &Settings,
    system: Option<&str>,
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
        "ollama" => ollama_request_with_system(
            &settings.vlm_host,
            &settings.vlm_model,
            system,
            prompt,
            images,
            schema,
            num_predict,
        )?,
        "openai" => openai_request_at_with_system(
            OPENAI_URL,
            true,
            &settings.vlm_model,
            &settings.vlm_api_key,
            system,
            prompt,
            images,
            schema,
            num_predict,
        ),
        "openrouter" => openai_request_at_with_system(
            OPENROUTER_URL,
            false,
            &settings.vlm_model,
            &settings.vlm_api_key,
            system,
            prompt,
            images,
            schema,
            num_predict,
        ),
        "anthropic" => {
            anthropic_request_with_system(settings, system, prompt, images, schema, num_predict)
        }
        "gemini" => gemini_request_with_system(
            &settings.vlm_model,
            &settings.vlm_api_key,
            system,
            prompt,
            images,
            num_predict,
        ),
        "opencode" | "opencode-go" => {
            // Both spellings are the Console (credits) endpoint. `opencode-go`
            // is only an alias from older settings; it never reaches the Go
            // subscription base, whose allowance is for coding assistants.
            let mut request = openai_request_at_with_system(
                &opencode_chat_url(&settings.vlm_host)?,
                false,
                &settings.vlm_model,
                &settings.vlm_api_key,
                system,
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
        "builtin" | "openai" => parse_openai_response(body),
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
        self.call_with_system(
            settings,
            None,
            prompt,
            images,
            schema,
            num_predict,
            repeat_penalty,
        )
    }

    /// `call_with`, carrying an optional `system` prompt placed before the
    /// per-photo user turn so local and cloud backends can reuse the cached
    /// static instruction prefix across crops.
    #[allow(clippy::too_many_arguments)]
    pub fn call_with_system(
        &self,
        settings: &Settings,
        system: Option<&str>,
        prompt: &str,
        images: &[String],
        schema: &Value,
        num_predict: i64,
        repeat_penalty: Option<f64>,
    ) -> Result<Value, VlmError> {
        let enabled: Vec<_> = settings
            .vlm_services
            .iter()
            .filter(|s| s.enabled)
            .filter(|s| {
                if conrod_core::settings::is_vision_provider(&s.provider) {
                    return true;
                }
                note_moments_seat(&s.provider);
                false
            })
            .collect();
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
                system,
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
            self.call_one(
                &one,
                system,
                prompt,
                images,
                schema,
                num_predict,
                repeat_penalty,
            )
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

    #[allow(clippy::too_many_arguments)]
    fn call_one(
        &self,
        settings: &Settings,
        system: Option<&str>,
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
        if !conrod_core::settings::is_vision_provider(&provider) {
            // The legacy single-provider field can only be the moments seat if
            // nothing was migrated; there is no vision service to ask, and
            // naming it an unknown provider would blame the wrong thing.
            note_moments_seat(&provider);
            return Err(VlmError::Misconfigured(
                "No vision service is enabled".into(),
            ));
        }
        let resp = if provider == "builtin" {
            let base = resolve_builtin_endpoint()?;
            let _lease = builtin_lease();
            let url = format!("{}/v1/chat/completions", base.trim_end_matches('/'));
            let model = if settings.vlm_model.is_empty() {
                "qwen3-vl:8b"
            } else {
                &settings.vlm_model
            };
            let mut req = openai_request_at_with_system(
                &url,
                true,
                model,
                "",
                system,
                prompt,
                images,
                schema,
                num_predict,
            );
            req.headers.clear();
            if let Some(penalty) = repeat_penalty {
                req.json["repeat_penalty"] = json!(penalty);
            }
            self.send_once(
                &req,
                Duration::from_secs_f64(settings.vlm_timeout),
                Some(Duration::from_secs(10)),
            )?
        } else if provider == "ollama" {
            // Deliberately not through the rate gate: nothing meters a
            // program talking to itself, and Ollama blocks while it loads a
            // model rather than refusing -- there is nothing here worth
            // waiting out.
            let pool = self.hosts.pool_for(settings.ollama_hosts());
            let host = pool.acquire();
            let mut req = match ollama_request_with_system(
                &host,
                &settings.vlm_model,
                system,
                prompt,
                images,
                schema,
                num_predict,
            ) {
                Ok(req) => req,
                // The host was refused before a socket was opened: hand the
                // pool token back instead of leaking it on the error path.
                Err(e) => {
                    pool.release(host);
                    return Err(e);
                }
            };
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
            let req =
                build_request_with_system(settings, system, prompt, images, schema, num_predict)?;
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
    /// The resolved shoot type's one-line attention guidance. `None` keeps
    /// the long-standing prompt exactly as it was; an `auto` album that
    /// accepted a suggestion adds it so the read is finetuned for that type.
    pub context: Option<String>,
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

// ── prompt budget ────────────────────────────────────────────────────────
// A local server (Ollama, the built-in llama-server) runs a 4096-token context
// that must hold the image, the system prompt, the entry-list candidates, the
// shoot guidance and the reply. The first two are fixed; the last two are user
// data of any length. Past the window Ollama drops the front of the prompt
// silently (the instructions go first) and llama-server refuses the request, so
// the two variable parts are cut to what is left, and the cut is reported.

/// Context tokens a local vision server holds for one single-crop request.
const LOCAL_CONTEXT_TOKENS: usize = OLLAMA_NUM_CTX as usize;
/// Tokens reserved for the reply (`num_predict` is 600 for a vehicle read).
const REPLY_TOKENS: usize = 600;
/// Spare tokens for the chat template and the short user message.
const PROMPT_HEADROOM_TOKENS: usize = 128;
/// Pixels per image-token side for Qwen3-VL (32 px patches after merging).
const IMAGE_PATCH_PX: usize = 32;
/// The most tokens shoot guidance may take, however much room is left.
const GUIDANCE_MAX_TOKENS: usize = 80;
/// The fixed closing lines every prompt ends with (~170 characters).
const PROMPT_TAIL_TOKENS: usize = 60;
/// Characters per token assumed when estimating: English prose is ~4, but
/// entry-list names, digits and punctuation tokenise at about 2-3.
const CHARS_PER_TOKEN: usize = 3;

/// A conservative token estimate for prompt text: never fewer than the real
/// count for ordinary Latin text.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(CHARS_PER_TOKEN)
}

/// Whether any enabled vision service runs with a small local context window.
/// A cloud model's window is large enough that the entry list is never cut.
fn context_limited(settings: &Settings) -> bool {
    let local = |provider: &str| {
        matches!(
            provider.to_ascii_lowercase().as_str(),
            "" | "ollama" | "builtin"
        )
    };
    if settings.vlm_services.is_empty() {
        local(&settings.vlm_provider)
    } else {
        settings
            .vlm_services
            .iter()
            .any(|s| s.enabled && local(&s.provider))
    }
}

/// Image tokens a square crop at the model's input edge can cost.
fn image_tokens(edge: u32) -> usize {
    let side = (edge.max(1) as usize).div_ceil(IMAGE_PATCH_PX);
    side * side
}

/// The first `max_chars` characters of `text`, cut at a word when it can be.
fn clip_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let cut: String = text.chars().take(max_chars).collect();
    match cut.rfind(char::is_whitespace) {
        Some(at) if at > max_chars / 2 => cut[..at].trim_end().to_string(),
        _ => cut.trim_end().to_string(),
    }
}

/// The two variable prompt parts after fitting them to the context window.
struct FittedHints {
    candidates: Vec<String>,
    /// Whether the candidate list lost entries (it then stops saying "must").
    partial: bool,
    guidance: Option<String>,
    /// What was cut, in one plain line; `None` when everything fit.
    trimmed: Option<String>,
}

fn fit_hints(settings: &Settings, base_tokens: usize, hints: &VlmHints) -> FittedHints {
    let guidance = hints
        .context
        .as_deref()
        .map(str::trim)
        .filter(|context| !context.is_empty())
        .map(str::to_string);
    let mut fitted = FittedHints {
        candidates: hints.candidates.clone(),
        partial: false,
        guidance,
        trimmed: None,
    };
    if !context_limited(settings) {
        return fitted;
    }
    let budget = LOCAL_CONTEXT_TOKENS
        .saturating_sub(image_tokens(settings.vlm_input_edge))
        .saturating_sub(REPLY_TOKENS)
        .saturating_sub(PROMPT_HEADROOM_TOKENS)
        .saturating_sub(base_tokens);
    let mut notes = Vec::new();
    let mut left = budget;
    if let Some(guidance) = fitted.guidance.take() {
        // The line is "Shoot guidance: <text>\n".
        let overhead = "Shoot guidance: \n".len().div_ceil(CHARS_PER_TOKEN);
        let room = left.min(GUIDANCE_MAX_TOKENS).saturating_sub(overhead);
        if estimate_tokens(&guidance) <= room {
            left = left.saturating_sub(estimate_tokens(&guidance) + overhead);
            fitted.guidance = Some(guidance);
        } else if room > 0 {
            let clipped = clip_chars(&guidance, room * CHARS_PER_TOKEN);
            left = left.saturating_sub(estimate_tokens(&clipped) + overhead);
            fitted.guidance = Some(clipped);
            notes.push("shoot guidance shortened".to_string());
        } else {
            notes.push("shoot guidance dropped".to_string());
        }
    }
    if !fitted.candidates.is_empty() {
        let overhead = "make/model is likely one of: ."
            .len()
            .div_ceil(CHARS_PER_TOKEN);
        let mut chars = left.saturating_sub(overhead) * CHARS_PER_TOKEN;
        let total = fitted.candidates.len();
        let mut kept = Vec::new();
        for candidate in &fitted.candidates {
            let cost = candidate.chars().count() + 2; // ", "
            if cost > chars {
                break;
            }
            chars -= cost;
            kept.push(candidate.clone());
        }
        if kept.len() < total {
            notes.push(format!(
                "entry list cut to {} of {total} make/models",
                kept.len()
            ));
            fitted.partial = true;
            fitted.candidates = kept;
        }
    }
    if !notes.is_empty() {
        fitted.trimmed = Some(format!(
            "prompt trimmed to fit the {LOCAL_CONTEXT_TOKENS}-token context (images at {} px): {}",
            settings.vlm_input_edge,
            notes.join(", ")
        ));
    }
    fitted
}

/// A built prompt and what, if anything, was cut from it to fit the context.
pub struct PromptFit {
    pub prompt: String,
    /// One plain line naming what was cut; `None` when the prompt fits whole.
    pub trimmed: Option<String>,
}

/// Say once per distinct message that a prompt was cut, on stderr.
fn note_prompt_trimmed(trimmed: &str) {
    static SEEN: std::sync::OnceLock<Mutex<std::collections::HashSet<String>>> =
        std::sync::OnceLock::new();
    let seen = SEEN.get_or_init(Default::default);
    if lock(seen).insert(trimmed.to_string()) {
        eprintln!("conrod: {trimmed}");
    }
}

/// Whether the question asks for a registration plate at all.
fn wants_plate(settings: &Settings, hints: &VlmHints) -> bool {
    settings.read_plates && !hints.racing
}

/// Add the shoot's known visual context instead of asking a local model to
/// decide whether incidental digits are a plate or a competition number.
pub fn vehicle_prompt(settings: &Settings, is_bike: bool, hints: &VlmHints) -> String {
    vehicle_prompt_fit(settings, is_bike, hints).prompt
}

/// [`vehicle_prompt`], with the entry-list candidates and the shoot guidance cut
/// to fit a local server's context window next to the image and the reply, and
/// a note of what was cut.
pub fn vehicle_prompt_fit(settings: &Settings, is_bike: bool, hints: &VlmHints) -> PromptFit {
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
        .join("\n");
    let base_tokens = estimate_tokens(&body) + PROMPT_TAIL_TOKENS;
    let fitted = fit_hints(settings, base_tokens, hints);
    let body = body.replace(
        "{CANDIDATES}",
        &if fitted.candidates.is_empty() {
            String::new()
        } else if fitted.partial {
            format!(
                "make/model is likely one of: {}.",
                fitted.candidates.join(", ")
            )
        } else {
            candidate_line(&VlmHints {
                candidates: fitted.candidates.clone(),
                ..VlmHints::default()
            })
        },
    );
    let targets = if settings.read_numbers {
        "Read a competition number only from the subject; ignore unrelated digits."
    } else {
        "Set number to null."
    };
    // No shoot-preset context by default. A preset sentence ("capture vehicle
    // identity, competition details, livery, and action") makes the model
    // spend its answer on livery and drop a windscreen number it had read, and
    // the bench reads the same crops better without it. An `auto` album that
    // accepted a suggested type opts in, once, with that type's guidance.
    let guidance = fitted
        .guidance
        .as_deref()
        .map(|context| format!("Shoot guidance: {context}\n"))
        .unwrap_or_default();
    let prompt = format!(
        "{guidance}{}\n\nContext guides attention only; never infer unseen details. {targets}\nReply with one JSON object only.",
        body.trim()
    );
    PromptFit {
        prompt,
        trimmed: fitted.trimmed,
    }
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

/// Map a provider's parsed reply onto a [`VehicleDescription`], for [`describe`].
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
    let PromptFit {
        prompt: system_prompt,
        trimmed,
    } = vehicle_prompt_fit(settings, is_bike, hints);
    if let Some(trimmed) = trimmed {
        note_prompt_trimmed(&trimmed);
    }
    let with_plate = wants_plate(settings, hints);
    let reply_schema = schema(with_plate);
    let ask = |sys: &str, penalty: Option<f64>| {
        client.call_with_system(
            settings,
            Some(sys),
            VEHICLE_USER_PROMPT,
            std::slice::from_ref(&payload_image),
            &reply_schema,
            600,
            penalty,
        )
    };
    let (parsed, attempts) = match ask(&system_prompt, None) {
        Ok(reply) => (reply, 1),
        Err(VlmError::BadReply(_)) => match ask(&system_prompt, Some(1.1)) {
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
    fn a_moments_local_service_is_never_asked_for_a_car_read() {
        // The built-in reader's seat belongs to the moments lane; an early AI
        // card wrote it into vlm_services too, and every car read then failed
        // with "unknown vision provider 'in_process'".
        let seat = VlmService {
            id: "vision-local-1".into(),
            provider: "in_process".into(),
            model: "Built-in car reader".into(),
            enabled: true,
            ..VlmService::default()
        };
        let live = VlmService {
            id: "live".into(),
            provider: "ollama".into(),
            model: "m".into(),
            host: ollama_once(),
            enabled: true,
            ..VlmService::default()
        };
        let settings = Settings {
            vlm_services: vec![seat.clone(), live],
            vlm_strategy: "round_robin".into(),
            vlm_timeout: 5.0,
            ..Settings::default()
        };
        let client = VlmClient::new(Arc::new(RealClock::default()));
        let answer = client.call(&settings, "p", &[], &json!({}), 8);
        assert_eq!(
            answer.expect("the live vision service answers"),
            json!({"ok": true}),
            "the seat is skipped, not asked"
        );
        // Alone it is not a vision service at all: the read says so plainly
        // instead of naming an unknown provider.
        let only = Settings {
            vlm_services: vec![seat],
            ..settings
        };
        let err = client.call(&only, "p", &[], &json!({}), 8).unwrap_err();
        assert!(
            matches!(err, VlmError::Misconfigured(_)),
            "not UnknownProvider: {err:?}"
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

    fn long_entry_list(n: usize) -> Vec<String> {
        (0..n)
            .map(|i| format!("Make{i} Model-{i} GT3 RS"))
            .collect()
    }

    #[test]
    fn a_long_entry_list_is_cut_to_fit_the_local_context() {
        for provider in ["ollama", "builtin"] {
            let settings = Settings {
                vlm_provider: provider.into(),
                ..Settings::default()
            };
            let hints = VlmHints {
                candidates: long_entry_list(400),
                context: Some("Wedding photography. ".repeat(60)),
                ..VlmHints::default()
            };
            let fit = vehicle_prompt_fit(&settings, false, &hints);
            let used =
                estimate_tokens(&fit.prompt) + image_tokens(settings.vlm_input_edge) + REPLY_TOKENS;
            assert!(
                used <= LOCAL_CONTEXT_TOKENS,
                "{provider}: {used} tokens of {LOCAL_CONTEXT_TOKENS}"
            );
            let note = fit.trimmed.expect("a cut is reported");
            assert!(note.contains("entry list cut to"), "{note}");
            assert!(note.contains("of 400"), "{note}");
            assert!(note.contains("shoot guidance shortened"), "{note}");
            assert!(
                fit.prompt.contains("make/model is likely one of"),
                "a partial list no longer says 'must'"
            );
            assert!(
                fit.prompt.contains("Make0 Model-0"),
                "the list's head stays"
            );
            assert!(!fit.prompt.contains("Make399 "), "its tail is cut");
        }
    }

    #[test]
    fn a_short_entry_list_and_a_cloud_model_are_never_cut() {
        let short = VlmHints {
            candidates: long_entry_list(5),
            context: Some("Rally.".into()),
            ..VlmHints::default()
        };
        let local = Settings {
            vlm_provider: "ollama".into(),
            ..Settings::default()
        };
        let fit = vehicle_prompt_fit(&local, false, &short);
        assert_eq!(fit.trimmed, None);
        assert!(fit.prompt.contains("make/model must be one of: Make0"));
        assert!(fit.prompt.contains("Shoot guidance: Rally."));

        let cloud = Settings {
            vlm_provider: "anthropic".into(),
            ..Settings::default()
        };
        let long = VlmHints {
            candidates: long_entry_list(400),
            ..VlmHints::default()
        };
        let fit = vehicle_prompt_fit(&cloud, false, &long);
        assert_eq!(fit.trimmed, None, "a cloud window holds the whole list");
        assert!(fit.prompt.contains("Make399 "));
    }

    #[test]
    fn a_big_image_edge_leaves_less_room_for_the_entry_list() {
        let hints = VlmHints {
            candidates: long_entry_list(60),
            ..VlmHints::default()
        };
        let kept = |edge: u32| {
            let settings = Settings {
                vlm_provider: "ollama".into(),
                vlm_input_edge: edge,
                ..Settings::default()
            };
            vehicle_prompt_fit(&settings, false, &hints)
                .prompt
                .matches("GT3 RS")
                .count()
        };
        assert!(kept(1568) < 60, "a 1568 px crop leaves little room");
        assert!(kept(1024) > kept(1568), "a smaller crop leaves more");
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
    fn opencode_decision_models_enumerate_every_image_capable_decision_entry() {
        // OpenCode marks each entry's type and input; four decision models were
        // live when this was written, and anything added later must appear
        // without a code change.
        let body = json!({"data": [
            {"id": "jev-1.13", "name": "JEV", "type": "decision",
             "input": ["text", "image/png", "image/jpeg"]},
            {"id": "jev-1.13-free", "type": "decision",
             "input": ["text", "image/png", "image/jpeg"]},
            {"id": "gemini-decision-1", "type": "decision",
             "input": ["text", "image/png", "image/jpeg"]},
            {"id": "gpt-6-luna-decision", "type": "decision",
             "input": ["text", "image/png", "image/jpeg"]},
            {"id": "text-decider", "type": "decision", "input": ["text"]},
            {"id": "mystery-decision", "type": "decision"},
            {"id": "vision-model", "type": "vision",
             "input": ["text", "image/png"]}
        ]});
        let list = decision_models_reply("opencode", &body, "").unwrap();
        assert!(!list.free_text, "OpenCode enumerates");
        let ids: Vec<&str> = list.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "jev-1.13",
                "jev-1.13-free",
                "gemini-decision-1",
                "gpt-6-luna-decision",
                "mystery-decision"
            ],
            "every photo-capable decision model; a text-only one and a vision one are not offered"
        );
        assert!(list
            .models
            .iter()
            .all(|m| m.unverified == (m.id == "mystery-decision")));
        assert_eq!(
            list.models
                .iter()
                .find(|m| m.id == "mystery-decision")
                .unwrap()
                .image,
            None,
            "no modality metadata: offered as unverified, not filtered silently"
        );
        assert!(list.models.iter().all(|m| m.id != "text-decider"));
    }

    #[test]
    fn openrouter_enumerates_decision_named_models_that_take_photos() {
        // Its catalogue has no decision type, so the name is the marker:
        // `decision`, `decider`, or JEV's `jev`.
        let body = json!({"data": [
            {"id": "typesafe/jev-router", "name": "JEV Router",
             "architecture": {"input_modalities": ["text", "image"]}},
            {"id": "acme/decision-lite", "name": "Decision Lite",
             "architecture": {"input_modalities": ["image", "text"]}},
            {"id": "mystery/jev-mini", "name": "JEV Mini"},
            {"id": "perplexity/pplx-decider-flash-v1.3:free", "name": "PPLX Decider",
             "architecture": {"input_modalities": ["text"]}},
            {"id": "google/gemini-4-neon",
             "architecture": {"input_modalities": ["text", "image"]}}
        ]});
        let list = decision_models_reply("openrouter", &body, "").unwrap();
        assert!(!list.free_text, "a match means the list is offered");
        let ids: Vec<&str> = list.models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "typesafe/jev-router",
                "acme/decision-lite",
                "mystery/jev-mini"
            ],
            "decision-named models only, in the catalogue's order; a text-only \
             decider and a plain vision model are not offered"
        );
        let mystery = list
            .models
            .iter()
            .find(|m| m.id == "mystery/jev-mini")
            .unwrap();
        assert_eq!(mystery.image, None, "no modality metadata");
        assert!(mystery.unverified, "offered, but the Test button decides");
        assert!(list.models.iter().all(|m| m.id != "google/gemini-4-neon"));
        assert_eq!(
            list.models[0].image,
            Some(true),
            "an image-capable entry is verified"
        );
    }

    #[test]
    fn openrouter_with_no_decision_named_model_keeps_the_free_text_fallback() {
        // Nothing matches: the free-text field stays, so the screen never
        // dead-ends on an empty list.
        let plain = json!({"data": [
            {"id": "google/gemini-4-neon",
             "architecture": {"input_modalities": ["text", "image"]}}
        ]});
        let list = decision_models_reply("openrouter", &plain, "").unwrap();
        assert!(list.free_text);
        assert!(list.models.is_empty());

        // An empty catalogue is the same fallback, not an error.
        let empty = decision_models_reply("openrouter", &json!({"data": []}), "").unwrap();
        assert!(empty.free_text);
        assert!(empty.models.is_empty());

        // A configured model the catalogue calls text-only is withheld, with
        // a plain-words reason.
        let text_only = json!({"data": [
            {"id": "perplexity/pplx-decider-flash-v1.3:free",
             "architecture": {"input_modalities": ["text"]}}
        ]});
        let pplx = decision_models_reply(
            "openrouter",
            &text_only,
            "perplexity/pplx-decider-flash-v1.3:free",
        )
        .unwrap();
        assert!(pplx.free_text, "still free text");
        assert!(
            pplx.models.is_empty(),
            "a text-only known model is hidden, not offered as a broken default"
        );
        assert!(pplx
            .note
            .as_deref()
            .unwrap_or("")
            .contains("No photo-capable decision model"));

        // A provider without any decision list says so rather than guessing.
        assert!(decision_models_reply("ollama", &plain, "").is_err());
    }

    #[test]
    fn provider_models_carry_published_prices_per_million() {
        // OpenRouter publishes pricing.prompt/completion as USD per token;
        // the card wants dollars per million. Absent stays absent.
        let body = json!({"data": [
            {"id": "b/model", "name": "B Model",
             "pricing": {"prompt": "0.000003", "completion": "0.000015"}},
            {"id": "a/free", "pricing": {"prompt": "0", "completion": 0}},
            {"id": "c/unknown", "name": "Unknown Price"}
        ]});
        let models = parse_provider_models(body["data"].as_array().unwrap());
        let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["a/free", "b/model", "c/unknown"]);
        assert_eq!(
            models[1].pricing,
            Some(ModelPricing {
                prompt: 3.0,
                completion: 15.0
            })
        );
        assert_eq!(models[1].name, "B Model");
        assert_eq!(models[2].pricing, None, "no published price");
        let absent = models[2].to_json();
        assert!(
            absent.get("pricing").is_none(),
            "the field is omitted, never null: {absent}"
        );
    }

    #[test]
    fn the_thinking_switch_is_the_services_own_flag() {
        let schema = json!({"type": "object"});
        let service = |reasoning: bool| conrod_core::settings::VlmService {
            id: "s".into(),
            provider: "ollama".into(),
            model: "qwen2.5vl:7b".into(),
            host: "http://127.0.0.1:11434".into(),
            reasoning,
            ..conrod_core::settings::VlmService::default()
        };
        // The old one global switch: on. The entry still decides.
        let settings = Settings {
            vlm_reasoning: true,
            ..Settings::default()
        };
        let quiet = for_service(&settings, &service(false));
        let request = build_request(&quiet, "describe it", &[], &schema, 500).unwrap();
        assert_eq!(
            request.json["think"], false,
            "the service's own flag overrides the global one"
        );
        let loud = for_service(&settings, &service(true));
        let request = build_request(&loud, "describe it", &[], &schema, 500).unwrap();
        assert_eq!(request.json["think"], true);
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
        assert_eq!(opencode_chat_url("").unwrap(), OPENCODE_CHAT_URL);
        assert_eq!(
            opencode_chat_url("https://proxy.local/v1/").unwrap(),
            "https://proxy.local/v1/chat/completions"
        );
        assert_eq!(
            opencode_chat_url("https://proxy.local/v1/chat/completions").unwrap(),
            "https://proxy.local/v1/chat/completions"
        );
    }

    #[test]
    fn a_plain_http_remote_chat_host_is_refused_but_https_and_local_are_allowed() {
        assert!(
            matches!(
                opencode_chat_url("http://evil.example/v1"),
                Err(VlmError::Misconfigured(_))
            ),
            "a public plain-http host must be refused before the key is attached"
        );
        assert!(opencode_chat_url("https://proxy.example/v1").is_ok());
        for local in [
            "http://127.0.0.1:11434",
            "http://localhost:11434",
            "http://192.168.1.9:11434",
            "http://10.0.0.5:11434",
            "http://172.20.3.4:11434",
        ] {
            assert!(opencode_chat_url(local).is_ok(), "{local}");
        }
        assert!(matches!(
            opencode_chat_url("file:///etc/passwd"),
            Err(VlmError::Misconfigured(_))
        ));
        assert!(matches!(
            opencode_chat_url("http://169.254.169.254/latest"),
            Err(VlmError::Misconfigured(_))
        ));
    }

    #[test]
    fn the_vision_request_refuses_a_plain_http_remote_host() {
        let cloud = Settings {
            vlm_provider: "opencode".into(),
            vlm_model: "jev-1".into(),
            vlm_api_key: "sk-test".into(),
            vlm_host: "http://evil.example".into(),
            ..Default::default()
        };
        let err = build_request(&cloud, "x", &[], &json!({}), 10).unwrap_err();
        assert!(
            matches!(err, VlmError::Misconfigured(ref m) if m.contains("vlm_host")),
            "{err}"
        );

        let ollama = Settings {
            vlm_provider: "ollama".into(),
            vlm_host: "http://evil.example:11434".into(),
            ..Default::default()
        };
        let err = build_request(&ollama, "x", &[], &json!({}), 10).unwrap_err();
        assert!(matches!(err, VlmError::Misconfigured(_)), "{err}");
    }

    fn capture_one_request() -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                seen.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&seen).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            let l = l.to_ascii_lowercase();
                            l.strip_prefix("content-length:")
                                .map(|v| v.trim().to_string())
                        })
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    if n == 0 || seen.len() >= end + 4 + len {
                        break;
                    }
                } else if n == 0 {
                    break;
                }
            }
            let body = String::from_utf8_lossy(&seen).to_string();
            let body = match body.find("\r\n\r\n") {
                Some(end) => body[end + 4..].to_string(),
                None => body,
            };
            let reply = br#"{"response":"","done":true}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(reply);
            body
        });
        (base, server)
    }

    #[test]
    fn releasing_the_vision_model_sends_keep_alive_zero_to_local_ollama_only() {
        let (base, server) = capture_one_request();
        let settings = Settings {
            vlm_services: vec![
                VlmService {
                    id: "local".into(),
                    provider: "ollama".into(),
                    model: "qwen3-vl:8b".into(),
                    host: base.clone(),
                    enabled: true,
                    ..VlmService::default()
                },
                VlmService {
                    id: "remote".into(),
                    provider: "ollama".into(),
                    model: "qwen3-vl:8b".into(),
                    host: "https://box.example:11434".into(),
                    enabled: true,
                    ..VlmService::default()
                },
            ],
            ..Settings::default()
        };
        assert_eq!(
            local_ollama_models(&settings)
                .into_iter()
                .map(|(_, model)| model)
                .collect::<Vec<_>>(),
            ["qwen3-vl:8b"],
            "only the local server is asked"
        );
        assert_eq!(release_local_models(&settings), 1);
        let sent: Value = serde_json::from_str(server.join().unwrap().trim()).unwrap();
        assert_eq!(sent["model"], "qwen3-vl:8b");
        assert_eq!(sent["keep_alive"], 0, "the model is released now");
        assert_eq!(sent["stream"], false);
    }

    #[test]
    fn a_legacy_local_ollama_vision_is_released_and_a_cloud_one_is_not() {
        let legacy = Settings {
            vlm_provider: "ollama".into(),
            vlm_model: "qwen3-vl:8b".into(),
            vlm_host: "http://127.0.0.1:11434/".into(),
            vlm_services: Vec::new(),
            ..Settings::default()
        };
        assert_eq!(
            local_ollama_models(&legacy),
            [(
                "http://127.0.0.1:11434".to_string(),
                "qwen3-vl:8b".to_string()
            )]
        );
        let cloud = Settings {
            vlm_provider: "openrouter".into(),
            vlm_model: "qwen/qwen3-vl".into(),
            ..legacy.clone()
        };
        assert!(local_ollama_models(&cloud).is_empty());
    }

    #[test]
    fn loopback_host_excludes_lan_ips_and_local_ollama_models_skips_lan_servers() {
        for loopback in [
            "http://localhost:11434",
            "http://sub.localhost:11434",
            "http://127.0.0.1:11434",
            "http://127.0.1.5:11434",
            "http://[::1]:11434",
            "https://[::1]:11434",
        ] {
            assert!(is_loopback_host(loopback), "{loopback} should be loopback");
            assert!(is_local_host(loopback), "{loopback} should also be local");
        }
        for bare in ["localhost", "127.0.0.1", "::1", "[::1]"] {
            assert!(is_loopback_host(bare), "{bare} should be loopback");
        }
        for lan in [
            "http://192.168.1.50:11434",
            "http://10.0.0.5:11434",
            "http://172.20.3.4:11434",
            "http://[fd00::1]:11434",
        ] {
            assert!(
                !is_loopback_host(lan),
                "{lan} is on the LAN, not loopback on this machine"
            );
            assert!(is_local_host(lan), "{lan} stays local for guard_remote");
        }
        for remote in ["https://box.example:11434", "http://169.254.169.254", ""] {
            assert!(!is_loopback_host(remote), "{remote}");
        }

        let settings = Settings {
            vlm_services: vec![
                VlmService {
                    id: "lan".into(),
                    provider: "ollama".into(),
                    model: "qwen3-vl:8b".into(),
                    host: "http://192.168.1.50:11434".into(),
                    enabled: true,
                    ..VlmService::default()
                },
                VlmService {
                    id: "this-box".into(),
                    provider: "ollama".into(),
                    model: "qwen3-vl:8b".into(),
                    host: "http://[::1]:11434/".into(),
                    enabled: true,
                    ..VlmService::default()
                },
            ],
            ..Settings::default()
        };
        assert_eq!(
            local_ollama_models(&settings),
            [("http://[::1]:11434".to_string(), "qwen3-vl:8b".to_string())],
            "a separate LAN Ollama server is not unloaded by release_local_models"
        );
    }

    #[test]
    fn builtin_vision_provider_uses_registered_endpoint_and_release_hook() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::atomic::{AtomicBool, Ordering};

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let body = r#"{"choices":[{"message":{"content":"{\"make\":\"Porsche\",\"model\":\"911 GT3\",\"colour\":\"silver\",\"body_type\":\"coupe\",\"team\":null,\"sponsors\":[],\"race_number\":\"7\",\"visible_text\":[],\"action\":[],\"notes\":null}"}}]}"#;
            let reply = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        });

        let released = Arc::new(AtomicBool::new(false));
        let rel = released.clone();
        register_builtin(
            Arc::new(move || Ok(base.clone())),
            Arc::new(move || rel.store(true, Ordering::SeqCst)),
        );

        let settings = Settings {
            vlm_services: vec![VlmService {
                id: "vlm-builtin".into(),
                provider: "builtin".into(),
                model: "qwen3-vl:8b".into(),
                enabled: true,
                ..VlmService::default()
            }],
            ..Settings::default()
        };
        assert!(uses_builtin_vision(&settings));
        let client = VlmClient::new(Arc::new(RealClock::default()));
        let schema =
            json!({"type": "object", "properties": {"make": {"type": ["string", "null"]}}});
        let out = client
            .call(&settings, "describe", &[], &schema, 128)
            .expect("builtin provider call succeeds against registered loopback server");
        assert_eq!(out["make"], "Porsche");
        assert_eq!(out["race_number"], "7");
        server.join().unwrap();

        assert_eq!(release_local_models(&settings), 1);
        assert!(released.load(Ordering::SeqCst));
    }

    #[test]
    fn describe_with_reply_sends_static_prompt_as_system_with_batch_and_ctx_on_ollama() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                seen.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&seen).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            let l = l.to_ascii_lowercase();
                            l.strip_prefix("content-length:")
                                .map(|v| v.trim().to_string())
                        })
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    if n == 0 || seen.len() >= end + 4 + len {
                        break;
                    }
                } else if n == 0 {
                    break;
                }
            }
            let raw = String::from_utf8_lossy(&seen).to_string();
            let body = match raw.find("\r\n\r\n") {
                Some(end) => raw[end + 4..].to_string(),
                None => raw,
            };
            let reply = br#"{"response":"{\"number\":\"7\",\"number_location\":\"door\",\"numbers\":[],\"visible_text\":[],\"action\":[],\"make\":\"Porsche\",\"model\":\"911\",\"evidence\":\"badge\",\"colour\":\"white\",\"is_competition\":true,\"confidence\":0.9}"}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(reply);
            body
        });

        let settings = Settings {
            vlm_provider: "ollama".into(),
            vlm_host: base,
            vlm_model: "qwen3-vl:8b-instruct-q4_K_M".into(),
            ..Settings::default()
        };
        let img = Rgb {
            width: 4,
            height: 4,
            data: vec![128; 4 * 4 * 3],
        };
        let client = VlmClient::new(Arc::new(RealClock::default()));
        let (vehicle, _, attempts) =
            describe_with_reply(&client, &img, &settings, false, &VlmHints::default()).unwrap();
        assert_eq!(attempts, 1);
        assert_eq!(vehicle.race_number.as_deref(), Some("7"));

        let sent: Value = serde_json::from_str(server.join().unwrap().trim()).unwrap();
        assert_eq!(sent["prompt"], json!(VEHICLE_USER_PROMPT));
        assert!(sent["system"]
            .as_str()
            .unwrap()
            .contains("You are reading one competition car"));
        assert_eq!(sent["options"]["num_ctx"], json!(4096));
        assert_eq!(sent["options"]["num_batch"], json!(2048));
    }
}
