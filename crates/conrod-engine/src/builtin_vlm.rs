//! Built-in Qwen3-VL-8B-Instruct vision server managed directly by Conrod.
//!
//! No external Ollama installation is needed: Conrod launches the bundled
//! `llama-server` from `<data>/tools/decider-runtime` against the pinned GGUF
//! weights (`Qwen3VL-8B-Instruct-Q4_K_M.gguf`) and F16 vision projector
//! (`mmproj-Qwen3VL-8B-Instruct-F16.gguf`) on a loopback ephemeral port,
//! exposes its OpenAI-compatible JSON-schema endpoint to `conrod_io::vlm`,
//! unloads automatically after an idle period, and shuts down cleanly on quit
//! or whenever a serial-card handoff needs the GPU back for the decision model.
//!
//! Lifecycle rules:
//!
//! - The child's stderr is drained on a background thread into a bounded ring
//!   buffer. llama.cpp logs hundreds of lines while it loads; an undrained
//!   Windows pipe holds 4 KiB and the child would block inside `write`.
//! - The service lock is never held while the model loads. A load is a
//!   `starting` state other callers wait on with a condvar, so the progress
//!   pump, the Models status poll and Quit never block behind it, and `unload`
//!   cancels a start in flight.
//! - A failed start is remembered for [`START_COOLDOWN`]: every caller in that
//!   window gets the same plain error at once instead of a fresh spawn and a
//!   90 s wait per photo. A new Identify run clears it.
//! - The child belongs to the app: on Windows it joins a kill-on-close Job
//!   Object, and a pid file lets the next launch sweep a server that survived
//!   a crash.

use crate::lock;
use conrod_core::models;
use std::collections::VecDeque;
use std::io::{BufRead, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Windows process creation flag: launch `llama-server.exe` without flashing a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Drop the child process (and return 100% of its VRAM) after this much quiet time.
const IDLE_UNLOAD: Duration = Duration::from_secs(60);

/// Maximum time to wait for `llama-server` to finish loading weights into VRAM.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(90);

/// After a failed start, every caller gets the same error for this long
/// instead of spawning (and waiting up to [`STARTUP_TIMEOUT`]) again.
const START_COOLDOWN: Duration = Duration::from_secs(60);

/// Lines of the child's stderr kept for error messages.
const STDERR_RING_LINES: usize = 64;

/// A stderr line longer than this is cut: a runaway line must not grow memory.
const STDERR_LINE_BYTES: usize = 4096;

/// Context window for Qwen3-VL single-crop reads. Fits a 1568 px image
/// (~2000 tokens), the ~500-token system prompt, and a 600-token reply with
/// headroom while halving KV-cache VRAM compared to 8192.
const DEFAULT_CTX_SIZE: u32 = 4096;

/// Logical and physical prompt/image prefill batch sizes. Keeps a full image
/// crop in a single batch and doubles llama-server's default 512 ubatch size
/// for faster vision prefill.
const DEFAULT_BATCH_SIZE: u32 = 2048;
const DEFAULT_UBATCH_SIZE: u32 = 1024;

/// Paths required to launch the built-in Qwen3-VL server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltinPaths {
    pub server: PathBuf,
    pub runtime_dir: PathBuf,
    pub model: PathBuf,
    pub mmproj: PathBuf,
}

impl BuiltinPaths {
    /// Discover the bundled runtime and Qwen3-VL GGUF pair on disk.
    pub fn probe() -> Result<Self, String> {
        let model = models::find(models::VISION).ok_or_else(|| {
            format!(
                "Built-in vision weights ({}) are not downloaded. Install them in Settings → AI models.",
                models::VISION
            )
        })?;
        let mmproj = models::find(models::VISION_MMPROJ).ok_or_else(|| {
            format!(
                "Built-in vision projector ({}) is not downloaded. Install it in Settings → AI models.",
                models::VISION_MMPROJ
            )
        })?;
        let server = models::vision_server().ok_or_else(|| {
            "Built-in llama.cpp runtime is not installed. Re-download the built-in vision model in Settings → AI models."
                .to_string()
        })?;
        let runtime_dir = server
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        Ok(BuiltinPaths {
            server,
            runtime_dir,
            model,
            mmproj,
        })
    }
}

/// Command-line arguments passed to `llama-server`. Pure so unit tests can pin them.
pub fn server_args(paths: &BuiltinPaths, port: u16) -> Vec<String> {
    vec![
        "--model".into(),
        paths.model.to_string_lossy().into_owned(),
        "--mmproj".into(),
        paths.mmproj.to_string_lossy().into_owned(),
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--ctx-size".into(),
        DEFAULT_CTX_SIZE.to_string(),
        "--batch-size".into(),
        DEFAULT_BATCH_SIZE.to_string(),
        "--ubatch-size".into(),
        DEFAULT_UBATCH_SIZE.to_string(),
        "--cache-type-k".into(),
        "q8_0".into(),
        "--cache-type-v".into(),
        "q8_0".into(),
        "--flash-attn".into(),
        "on".into(),
        "--gpu-layers".into(),
        "999".into(),
        "--parallel".into(),
        "1".into(),
        "--jinja".into(),
        "--reasoning".into(),
        "off".into(),
        "--sleep-idle-seconds".into(),
        "120".into(),
    ]
}

// ── the child's stderr ───────────────────────────────────────────────────

/// The last [`STDERR_RING_LINES`] lines a child wrote to stderr, shared between
/// the drain thread that fills it and the error messages that read it.
#[derive(Clone, Default)]
struct StderrRing {
    lines: Arc<Mutex<VecDeque<String>>>,
}

impl StderrRing {
    fn push(&self, line: String) {
        let mut lines = lock(&self.lines);
        if lines.len() == STDERR_RING_LINES {
            lines.pop_front();
        }
        lines.push_back(line);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        lock(&self.lines).len()
    }

    #[cfg(test)]
    fn last(&self) -> Option<String> {
        lock(&self.lines).back().cloned()
    }

    /// The last few non-empty lines as one line, for an error message.
    fn tail(&self) -> String {
        let lines = lock(&self.lines);
        let tail: Vec<&str> = lines
            .iter()
            .rev()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .take(6)
            .collect();
        tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
    }
}

/// Read `source` to its end on a background thread, keeping the tail in
/// `ring`. Returns once the pipe closes (the child exited), never earlier, so
/// the child can never block writing to a full pipe.
fn drain_lines<R: Read + Send + 'static>(
    source: R,
    ring: StderrRing,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut reader = std::io::BufReader::new(source);
        let mut line = Vec::new();
        loop {
            line.clear();
            // One line, bounded: bytes past the cap are consumed and dropped.
            let mut ended = false;
            loop {
                let (used, newline) = match reader.fill_buf() {
                    Ok([]) => {
                        ended = true;
                        break;
                    }
                    Ok(buf) => {
                        let newline = buf.iter().position(|b| *b == b'\n');
                        let upto = newline.map_or(buf.len(), |i| i + 1);
                        let room = STDERR_LINE_BYTES.saturating_sub(line.len());
                        line.extend_from_slice(&buf[..upto.min(room)]);
                        (upto, newline.is_some())
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        ended = true;
                        break;
                    }
                };
                reader.consume(used);
                if newline {
                    break;
                }
            }
            if !line.is_empty() {
                ring.push(String::from_utf8_lossy(&line).trim_end().to_string());
            }
            if ended {
                return;
            }
        }
    })
}

// ── the server process ───────────────────────────────────────────────────

/// What the service needs from a running server process. The real one is a
/// `llama-server` child; tests pass a fake so no model is ever loaded.
trait Proc: Send {
    /// `Some(description)` once the process has exited.
    fn exited(&mut self) -> Option<String>;
    /// The last lines it wrote to stderr.
    fn stderr_tail(&self) -> String;
    /// Kill it and wait for it.
    fn stop(&mut self);
}

struct ChildProc {
    child: Child,
    stderr: StderrRing,
    stopped: bool,
}

impl Proc for ChildProc {
    fn exited(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            _ => None,
        }
    }

    fn stderr_tail(&self) -> String {
        self.stderr.tail()
    }

    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        let _ = self.child.kill();
        let _ = self.child.wait();
        pidfile::clear_if(self.child.id());
    }
}

impl Drop for ChildProc {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A started, healthy server: the process and where it listens.
struct Started {
    proc: Box<dyn Proc>,
    base_url: String,
}

/// Lets `unload` cancel a start that is still loading the model.
struct StartCancel {
    epoch: u64,
    current: Arc<AtomicU64>,
}

impl StartCancel {
    fn cancelled(&self) -> bool {
        self.current.load(Ordering::Acquire) != self.epoch
    }
}

type StartFn = dyn Fn(&StartCancel) -> Result<Started, String> + Send + Sync;

struct Running {
    proc: Box<dyn Proc>,
    base_url: String,
    last_used: Instant,
}

impl Running {
    fn alive(&mut self) -> bool {
        self.proc.exited().is_none()
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.proc.stop();
    }
}

struct Failure {
    at: Instant,
    message: String,
}

#[derive(Default)]
struct State {
    running: Option<Running>,
    /// A start is in flight; other callers wait on [`Service::changed`].
    starting: bool,
    /// The last failed start, while it is still inside the cool-down.
    failure: Option<Failure>,
}

/// Process-wide lifecycle manager for Conrod's built-in Qwen3-VL server.
pub struct Service {
    state: Mutex<State>,
    changed: Condvar,
    /// Bumped by `unload`: a start that began under an older value gives up.
    epoch: Arc<AtomicU64>,
    /// Requests in flight (see [`Service::lease`]); the idle timer never
    /// unloads a server that is answering one.
    in_flight: AtomicUsize,
    /// Set by `shutdown`: the app is quitting and no new server may start.
    closed: AtomicBool,
    cooldown: Duration,
    starter: Arc<StartFn>,
}

impl Default for Service {
    fn default() -> Self {
        Service::with_starter(Arc::new(start_process), START_COOLDOWN)
    }
}

/// The singleton built-in vision service.
pub fn service() -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(Service::default)
}

/// Wire `conrod_io::vlm`'s `provider: "builtin"` hooks to this service.
/// Safe to call more than once (idempotent). The first call also sweeps a
/// `llama-server` a crashed earlier run left behind.
pub fn register() {
    static SWEEP: std::sync::Once = std::sync::Once::new();
    SWEEP.call_once(pidfile::sweep_stale);
    service().closed.store(false, Ordering::Release);
    conrod_io::vlm::register_builtin(
        Arc::new(|| service().endpoint()),
        Arc::new(|| service().unload()),
    );
    conrod_io::vlm::register_builtin_lease(Arc::new(|| Box::new(service().lease())));
}

/// Held while one request is in flight against the built-in server.
pub struct Lease<'a> {
    service: &'a Service,
}

impl Drop for Lease<'_> {
    fn drop(&mut self) {
        self.service.in_flight.fetch_sub(1, Ordering::AcqRel);
        if let Some(r) = lock(&self.service.state).running.as_mut() {
            r.last_used = Instant::now();
        }
    }
}

impl Service {
    fn with_starter(starter: Arc<StartFn>, cooldown: Duration) -> Service {
        Service {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
            epoch: Arc::new(AtomicU64::new(0)),
            in_flight: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
            cooldown,
            starter,
        }
    }

    /// Whether the model weights, projector, and bundled `llama-server` are all on disk.
    pub fn installed() -> bool {
        BuiltinPaths::probe().is_ok()
    }

    /// Whether the built-in server child process is currently alive. Never
    /// waits for a model load: a server still starting is not running yet.
    pub fn running(&self) -> bool {
        let mut st = lock(&self.state);
        if let Some(r) = st.running.as_mut() {
            if r.alive() {
                return true;
            }
            st.running = None;
        }
        false
    }

    /// Mark one request in flight until the returned lease drops, so the idle
    /// timer cannot unload the server under a slow answer.
    pub fn lease(&self) -> Lease<'_> {
        self.in_flight.fetch_add(1, Ordering::AcqRel);
        Lease { service: self }
    }

    /// Forget the last failed start, so the next call tries again at once. An
    /// Identify run calls this first: the photographer may have fixed the cause.
    pub fn clear_failure(&self) {
        lock(&self.state).failure = None;
    }

    fn failure_text(&self, failure: &Failure) -> String {
        let left = self.cooldown.saturating_sub(failure.at.elapsed()).as_secs();
        format!(
            "{} Conrod will not try to start it again for {} s; fix the cause, then run Identify again.",
            failure.message.trim_end(),
            left.max(1)
        )
    }

    /// Ensure the built-in Qwen3-VL server is running and healthy, returning
    /// its loopback base URL (`http://127.0.0.1:<port>`).
    ///
    /// Only one caller starts the server; the rest wait for that start. The
    /// state lock is released for the whole load.
    pub fn endpoint(&self) -> Result<String, String> {
        let epoch;
        {
            let mut st = lock(&self.state);
            loop {
                if self.closed.load(Ordering::Acquire) {
                    return Err(
                        "Conrod is quitting; the built-in vision server was not started.".into(),
                    );
                }
                if let Some(r) = st.running.as_mut() {
                    if r.alive() {
                        r.last_used = Instant::now();
                        return Ok(r.base_url.clone());
                    }
                    st.running = None;
                }
                if let Some(f) = st.failure.as_ref() {
                    if f.at.elapsed() < self.cooldown {
                        return Err(self.failure_text(f));
                    }
                    st.failure = None;
                }
                if st.starting {
                    st = self.changed.wait(st).unwrap_or_else(|e| e.into_inner());
                    continue;
                }
                st.starting = true;
                epoch = self.epoch.load(Ordering::Acquire);
                break;
            }
        }

        let cancel = StartCancel {
            epoch,
            current: self.epoch.clone(),
        };
        // `starting` must be cleared even if the starter panics.
        let outcome =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.starter)(&cancel)))
                .unwrap_or_else(|_| Err("The built-in vision server start-up crashed.".into()));
        let cancelled = cancel.cancelled() || self.closed.load(Ordering::Acquire);
        let outcome = match outcome {
            Ok(mut started) if cancelled => {
                started.proc.stop();
                Err("The built-in vision server start was cancelled.".to_string())
            }
            other => other,
        };

        let mut st = lock(&self.state);
        st.starting = false;
        let result = match outcome {
            Ok(started) => {
                let url = started.base_url.clone();
                st.running = Some(Running {
                    proc: started.proc,
                    base_url: started.base_url,
                    last_used: Instant::now(),
                });
                Ok(url)
            }
            Err(message) if cancelled => Err(message),
            Err(message) => {
                let failure = Failure {
                    at: Instant::now(),
                    message,
                };
                let text = self.failure_text(&failure);
                st.failure = Some(failure);
                Err(text)
            }
        };
        drop(st);
        self.changed.notify_all();
        result
    }

    /// Terminate the built-in server immediately, returning 100% of its VRAM.
    /// A start still loading the model is cancelled too.
    pub fn unload(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        let taken = lock(&self.state).running.take();
        if let Some(mut r) = taken {
            r.proc.stop();
        }
        self.changed.notify_all();
    }

    /// The app is quitting: unload now and never start a server again.
    pub fn shutdown(&self) {
        self.closed.store(true, Ordering::Release);
        self.unload();
    }

    /// Periodic tick (called from the app loop): unload after an idle period
    /// so VRAM is not held indefinitely between scans. A server with a request
    /// in flight is never idle.
    pub fn tick(&self) {
        let taken = {
            let mut st = lock(&self.state);
            let busy = self.in_flight.load(Ordering::Acquire) > 0;
            let drop_now = st
                .running
                .as_mut()
                .is_some_and(|r| !r.alive() || (!busy && r.last_used.elapsed() > IDLE_UNLOAD));
            if drop_now {
                st.running.take()
            } else {
                None
            }
        };
        if let Some(mut r) = taken {
            r.proc.stop();
        }
    }
}

// ── starting the real server ─────────────────────────────────────────────

/// Spawn `llama-server`, put it under the app's job, and wait for `/health`.
fn start_process(cancel: &StartCancel) -> Result<Started, String> {
    let paths = BuiltinPaths::probe()?;
    let port = pick_ephemeral_port()?;
    let base_url = format!("http://127.0.0.1:{port}");
    let args = server_args(&paths, port);

    let mut cmd = Command::new(&paths.server);
    cmd.args(&args)
        .current_dir(&paths.runtime_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "Could not start built-in vision server ({}): {e}.",
            paths.server.display()
        )
    })?;
    job::adopt(&child);
    pidfile::write(child.id(), &paths.server);
    let ring = StderrRing::default();
    if let Some(stderr) = child.stderr.take() {
        drain_lines(stderr, ring.clone());
    }
    let mut proc = ChildProc {
        child,
        stderr: ring,
        stopped: false,
    };
    match wait_until_healthy(&mut proc, port, STARTUP_TIMEOUT, cancel) {
        Ok(()) => Ok(Started {
            proc: Box::new(proc),
            base_url,
        }),
        Err(e) => {
            proc.stop();
            Err(e)
        }
    }
}

fn pick_ephemeral_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|e| format!("Could not reserve a local port for built-in vision: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Could not read local port for built-in vision: {e}"))?
        .port();
    drop(listener);
    Ok(port)
}

fn probe_health_ok(port: u16) -> bool {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_secs(1)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let req =
        format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut head = [0u8; 64];
    let Ok(n) = stream.read(&mut head) else {
        return false;
    };
    let line = std::str::from_utf8(&head[..n]).unwrap_or("");
    line.starts_with("HTTP/1.1 200") || line.starts_with("HTTP/1.0 200")
}

/// A start failure with what the server said last, when it said anything.
fn with_tail(message: String, tail: String) -> String {
    if tail.is_empty() {
        message
    } else {
        format!("{message} Server said: {tail}")
    }
}

fn wait_until_healthy(
    proc: &mut dyn Proc,
    port: u16,
    timeout: Duration,
    cancel: &StartCancel,
) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if cancel.cancelled() {
            return Err("The built-in vision server start was cancelled.".into());
        }
        if let Some(status) = proc.exited() {
            let message = format!("Built-in vision server failed to start ({status}).");
            return Err(with_tail(message, proc.stderr_tail()));
        }
        // The probe answers for whatever owns the port; a server of ours that
        // lost a race for it has exited by now, so check again before trusting it.
        if probe_health_ok(port) && proc.exited().is_none() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            let message =
                "Built-in vision server timed out while loading Qwen3-VL onto the GPU.".to_string();
            return Err(with_tail(message, proc.stderr_tail()));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

// ── tying the child to the app ───────────────────────────────────────────

/// A Windows Job Object with `KILL_ON_JOB_CLOSE`: the OS closes the handle when
/// Conrod dies for any reason, and that kills every process in the job.
mod job {
    use std::process::Child;

    #[cfg(windows)]
    pub fn adopt(child: &Child) {
        use std::os::windows::io::AsRawHandle;
        use std::sync::OnceLock;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };

        struct Job(isize);
        static JOB: OnceLock<Option<Job>> = OnceLock::new();
        let job = JOB.get_or_init(|| {
            // SAFETY: plain Win32 calls with a zeroed, correctly sized struct;
            // the handle is kept open for the life of the process on purpose.
            unsafe {
                let handle = CreateJobObjectW(None, windows::core::PCWSTR::null()).ok()?;
                let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
                info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
                .ok()?;
                Some(Job(handle.0 as isize))
            }
        });
        let Some(job) = job else {
            eprintln!(
                "conrod: could not create the vision server job object; the server is not tied to the app"
            );
            return;
        };
        // SAFETY: both handles are valid; the child handle is owned by `child`.
        let assigned = unsafe {
            AssignProcessToJobObject(
                HANDLE(job.0 as *mut std::ffi::c_void),
                HANDLE(child.as_raw_handle()),
            )
        };
        if let Err(e) = assigned {
            eprintln!("conrod: could not put the vision server in the app job: {e}");
        }
    }

    #[cfg(not(windows))]
    pub fn adopt(_child: &Child) {}
}

/// A pid file for the one server this app owns, so a crash that skipped the
/// job object (or a platform without one) cannot leave it running for ever.
mod pidfile {
    use super::*;

    fn path() -> PathBuf {
        conrod_core::settings::data_root().join("builtin-vlm.pid")
    }

    /// `<pid>\n<server path>`.
    pub(super) fn render(pid: u32, server: &Path) -> String {
        format!("{pid}\n{}\n", server.display())
    }

    pub(super) fn parse(text: &str) -> Option<(u32, PathBuf)> {
        let mut lines = text.lines();
        let pid = lines.next()?.trim().parse().ok()?;
        let server = lines.next()?.trim();
        (!server.is_empty()).then(|| (pid, PathBuf::from(server)))
    }

    /// The pid of the server this process started, 0 for none.
    static OUR_PID: AtomicU32 = AtomicU32::new(0);

    pub(super) fn write(pid: u32, server: &Path) {
        OUR_PID.store(pid, Ordering::Release);
        let _ = std::fs::write(path(), render(pid, server));
    }

    pub(super) fn clear_if(pid: u32) {
        if OUR_PID
            .compare_exchange(pid, 0, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            let _ = std::fs::remove_file(path());
        }
    }

    /// Kill a server a crashed earlier run left behind, if the pid still names
    /// that very executable.
    pub(super) fn sweep_stale() {
        let file = path();
        let Ok(text) = std::fs::read_to_string(&file) else {
            return;
        };
        let Some((pid, server)) = parse(&text) else {
            let _ = std::fs::remove_file(&file);
            return;
        };
        if pid == OUR_PID.load(Ordering::Acquire) {
            return;
        }
        let _ = std::fs::remove_file(&file);
        if kill_if_image_is(pid, &server) {
            eprintln!("conrod: stopped a leftover built-in vision server (pid {pid})");
        }
    }

    #[cfg(windows)]
    fn kill_if_image_is(pid: u32, server: &Path) -> bool {
        use windows::core::PWSTR;
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_FORMAT,
            PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
        };
        let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
        // SAFETY: the handle is opened, used and closed in this block, and the
        // name buffer is a local array whose length is passed alongside it.
        unsafe {
            let Ok(handle) = OpenProcess(
                PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                false,
                pid,
            ) else {
                return false;
            };
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let named = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(buf.as_mut_ptr()),
                &mut len,
            );
            let killed = named.is_ok() && {
                let image = String::from_utf16_lossy(&buf[..len as usize]);
                norm(Path::new(&image)) == norm(server) && TerminateProcess(handle, 1).is_ok()
            };
            let _ = CloseHandle(handle);
            killed
        }
    }

    #[cfg(not(windows))]
    fn kill_if_image_is(_pid: u32, _server: &Path) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn server_args_bind_loopback_with_qwen3_vl_flags() {
        let paths = BuiltinPaths {
            server: PathBuf::from("C:/runtime/llama-server.exe"),
            runtime_dir: PathBuf::from("C:/runtime"),
            model: PathBuf::from("C:/models/Qwen3VL-8B-Instruct-Q4_K_M.gguf"),
            mmproj: PathBuf::from("C:/models/mmproj-Qwen3VL-8B-Instruct-F16.gguf"),
        };
        let args = server_args(&paths, 41234);
        let has_pair = |k: &str, v: &str| args.windows(2).any(|w| w[0] == k && w[1] == v);
        assert!(has_pair("--host", "127.0.0.1"));
        assert!(has_pair("--port", "41234"));
        assert!(has_pair(
            "--model",
            "C:/models/Qwen3VL-8B-Instruct-Q4_K_M.gguf"
        ));
        assert!(has_pair(
            "--mmproj",
            "C:/models/mmproj-Qwen3VL-8B-Instruct-F16.gguf"
        ));
        assert!(has_pair("--ctx-size", "4096"));
        assert!(has_pair("--batch-size", "2048"));
        assert!(has_pair("--ubatch-size", "1024"));
        assert!(has_pair("--cache-type-k", "q8_0"));
        assert!(has_pair("--cache-type-v", "q8_0"));
        assert!(has_pair("--gpu-layers", "999"));
        assert!(has_pair("--parallel", "1"));
        assert!(has_pair("--reasoning", "off"));
        assert!(args.iter().any(|a| a == "--jinja"));
    }

    #[test]
    fn probe_reports_missing_files_plainly() {
        let _guard = crate::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("conrod-builtin-vlm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("models")).unwrap();
        let old = std::env::var_os("CONROD_HOME");
        std::env::set_var("CONROD_HOME", &dir);

        let err = BuiltinPaths::probe().unwrap_err();
        assert!(err.contains(models::VISION), "{err}");

        std::fs::write(dir.join("models").join(models::VISION), b"x").unwrap();
        let err = BuiltinPaths::probe().unwrap_err();
        assert!(err.contains(models::VISION_MMPROJ), "{err}");

        std::fs::write(dir.join("models").join(models::VISION_MMPROJ), b"x").unwrap();
        let err = BuiltinPaths::probe().unwrap_err();
        assert!(err.contains("runtime"), "{err}");

        let rt = dir.join("tools").join(models::DECIDER_RUNTIME);
        std::fs::create_dir_all(&rt).unwrap();
        std::fs::write(rt.join("llama.dll"), b"x").unwrap();
        #[cfg(windows)]
        std::fs::write(rt.join("llama-server.exe"), b"x").unwrap();
        #[cfg(not(windows))]
        std::fs::write(rt.join("llama-server"), b"x").unwrap();
        assert!(BuiltinPaths::probe().is_ok());

        match old {
            Some(v) => std::env::set_var("CONROD_HOME", v),
            None => std::env::remove_var("CONROD_HOME"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── stderr drain ─────────────────────────────────────────────────────

    #[test]
    fn a_flood_of_stderr_keeps_only_a_bounded_tail() {
        // 5 000 lines of ~100 bytes (~500 KB) and one unterminated final line.
        let mut flood = Vec::new();
        for i in 0..5000 {
            flood.extend_from_slice(format!("line {i:05} {}\n", "x".repeat(90)).as_bytes());
        }
        flood.extend_from_slice("final line without a newline".as_bytes());
        let ring = StderrRing::default();
        drain_lines(std::io::Cursor::new(flood), ring.clone())
            .join()
            .unwrap();
        assert_eq!(ring.len(), STDERR_RING_LINES, "the ring is bounded");
        assert_eq!(ring.last().as_deref(), Some("final line without a newline"));
        assert!(ring.tail().contains("final line"), "{}", ring.tail());
        assert!(!ring.tail().contains("line 00001"), "old lines fall out");
    }

    #[test]
    fn a_runaway_line_is_cut_instead_of_growing_memory() {
        let ring = StderrRing::default();
        let mut data = vec![b'a'; 100_000];
        data.extend_from_slice(b"\nnext\n");
        drain_lines(std::io::Cursor::new(data), ring.clone())
            .join()
            .unwrap();
        let lines = lock(&ring.lines);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].len() <= STDERR_LINE_BYTES);
        assert_eq!(lines[1], "next");
    }

    /// A real child writes far more than a pipe holds to stderr. With the drain
    /// it finishes; without one it would block in `write` for ever.
    #[test]
    fn a_child_writing_over_64_kib_of_stderr_is_never_blocked() {
        #[cfg(windows)]
        let mut cmd = {
            let mut c = Command::new("cmd");
            c.args([
                "/C",
                "for /L %i in (1,1,3000) do @echo 0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789 1>&2",
            ]);
            c.creation_flags(CREATE_NO_WINDOW);
            c
        };
        #[cfg(not(windows))]
        let mut cmd = {
            let mut c = Command::new("sh");
            c.args([
                "-c",
                "i=0; while [ $i -lt 3000 ]; do echo 0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789 >&2; i=$((i+1)); done",
            ]);
            c
        };
        let mut child = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn the chatty child");
        let ring = StderrRing::default();
        let drain = drain_lines(child.stderr.take().unwrap(), ring.clone());
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("the child blocked on a full stderr pipe");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        drain.join().unwrap();
        assert_eq!(ring.len(), STDERR_RING_LINES);
        assert!(ring.tail().contains("0123456789"));
    }

    // ── service lifecycle (fake server processes) ──────────────────────────

    struct FakeProc {
        alive: Arc<AtomicBool>,
        stopped: Arc<AtomicBool>,
    }

    impl Proc for FakeProc {
        fn exited(&mut self) -> Option<String> {
            (!self.alive.load(Ordering::Acquire)).then(|| "exit status: 1".to_string())
        }
        fn stderr_tail(&self) -> String {
            String::new()
        }
        fn stop(&mut self) {
            self.alive.store(false, Ordering::Release);
            self.stopped.store(true, Ordering::Release);
        }
    }

    fn fake_started(stopped: &Arc<AtomicBool>) -> Started {
        Started {
            proc: Box::new(FakeProc {
                alive: Arc::new(AtomicBool::new(true)),
                stopped: stopped.clone(),
            }),
            base_url: "http://127.0.0.1:1".into(),
        }
    }

    #[test]
    fn a_failed_start_backs_off_instead_of_respawning_per_photo() {
        let calls = Arc::new(AtomicUsize::new(0));
        let n = calls.clone();
        let svc = Service::with_starter(
            Arc::new(move |_| {
                n.fetch_add(1, Ordering::SeqCst);
                Err("Built-in vision server failed to start (exit status: 1). Server said: out of memory".to_string())
            }),
            Duration::from_secs(30),
        );
        let first = svc.endpoint().unwrap_err();
        assert!(first.contains("out of memory"), "{first}");
        assert!(first.contains("will not try"), "{first}");
        for _ in 0..50 {
            let again = svc.endpoint().unwrap_err();
            assert!(again.contains("out of memory"), "{again}");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1, "one spawn for 51 photos");

        // A new Identify run forgets the failure and tries once more.
        svc.clear_failure();
        let _ = svc.endpoint().unwrap_err();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_back_off_ends_after_the_cool_down() {
        let calls = Arc::new(AtomicUsize::new(0));
        let n = calls.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let ok = stopped.clone();
        let svc = Service::with_starter(
            Arc::new(move |_| {
                if n.fetch_add(1, Ordering::SeqCst) == 0 {
                    Err("boom".to_string())
                } else {
                    Ok(fake_started(&ok))
                }
            }),
            Duration::from_millis(50),
        );
        assert!(svc.endpoint().is_err());
        std::thread::sleep(Duration::from_millis(80));
        assert_eq!(svc.endpoint().unwrap(), "http://127.0.0.1:1");
        assert!(svc.running());
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_service_lock_is_not_held_while_the_model_loads() {
        let (entered_tx, entered_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let entered_tx = Mutex::new(entered_tx);
        let stopped = Arc::new(AtomicBool::new(false));
        let ok = stopped.clone();
        let svc = Arc::new(Service::with_starter(
            Arc::new(move |cancel| {
                lock(&entered_tx).send(()).unwrap();
                // "Loading": wait until released or cancelled.
                loop {
                    if cancel.cancelled() {
                        return Err("The built-in vision server start was cancelled.".into());
                    }
                    if lock(&release_rx)
                        .recv_timeout(Duration::from_millis(20))
                        .is_ok()
                    {
                        return Ok(fake_started(&ok));
                    }
                }
            }),
            Duration::from_secs(30),
        ));

        let loader = {
            let svc = svc.clone();
            std::thread::spawn(move || svc.endpoint())
        };
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();

        // While the load is in flight the pump's calls return at once.
        let (done_tx, done_rx) = mpsc::channel();
        {
            let svc = svc.clone();
            std::thread::spawn(move || {
                assert!(!svc.running());
                svc.tick();
                done_tx.send(()).unwrap();
            });
        }
        done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("running() and tick() must not wait for the model load");

        // A second caller waits for the same start; it does not spawn another.
        let waiter = {
            let svc = svc.clone();
            std::thread::spawn(move || svc.endpoint())
        };
        std::thread::sleep(Duration::from_millis(60));
        release_tx.send(()).unwrap();
        assert_eq!(loader.join().unwrap().unwrap(), "http://127.0.0.1:1");
        assert_eq!(waiter.join().unwrap().unwrap(), "http://127.0.0.1:1");
        assert!(svc.running());
    }

    #[test]
    fn unload_cancels_a_start_that_is_still_loading() {
        let (entered_tx, entered_rx) = mpsc::channel::<()>();
        let entered_tx = Mutex::new(entered_tx);
        let svc = Arc::new(Service::with_starter(
            Arc::new(move |cancel| {
                lock(&entered_tx).send(()).unwrap();
                let until = Instant::now() + Duration::from_secs(20);
                while !cancel.cancelled() && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err("The built-in vision server start was cancelled.".into())
            }),
            Duration::from_secs(30),
        ));
        let loader = {
            let svc = svc.clone();
            std::thread::spawn(move || svc.endpoint())
        };
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let begun = Instant::now();
        svc.unload();
        let err = loader.join().unwrap().unwrap_err();
        assert!(
            begun.elapsed() < Duration::from_secs(5),
            "Quit never waits for a load"
        );
        assert!(err.contains("cancelled"), "{err}");
        // A cancelled start is not a failed one: no cool-down is recorded.
        assert!(lock(&svc.state).failure.is_none());
    }

    #[test]
    fn a_server_that_finishes_loading_after_a_cancel_is_stopped() {
        let stopped = Arc::new(AtomicBool::new(false));
        let ok = stopped.clone();
        let svc = Arc::new(Service::with_starter(
            Arc::new(move |cancel| {
                while !cancel.cancelled() {
                    std::thread::sleep(Duration::from_millis(5));
                }
                // The load completed anyway, a moment too late.
                Ok(fake_started(&ok))
            }),
            Duration::from_secs(30),
        ));
        let loader = {
            let svc = svc.clone();
            std::thread::spawn(move || svc.endpoint())
        };
        std::thread::sleep(Duration::from_millis(40));
        svc.unload();
        assert!(loader.join().unwrap().is_err());
        assert!(stopped.load(Ordering::Acquire), "the late server is killed");
        assert!(!svc.running());
    }

    #[test]
    fn shutdown_refuses_to_start_anything_new() {
        let calls = Arc::new(AtomicUsize::new(0));
        let n = calls.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let svc = Service::with_starter(
            Arc::new(move |_| {
                n.fetch_add(1, Ordering::SeqCst);
                Ok(fake_started(&stopped))
            }),
            Duration::from_secs(30),
        );
        svc.endpoint().unwrap();
        svc.shutdown();
        assert!(!svc.running());
        let err = svc.endpoint().unwrap_err();
        assert!(err.contains("quitting"), "{err}");
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn the_idle_timer_never_unloads_a_server_with_a_request_in_flight() {
        let stopped = Arc::new(AtomicBool::new(false));
        let ok = stopped.clone();
        let svc = Service::with_starter(
            Arc::new(move |_| Ok(fake_started(&ok))),
            Duration::from_secs(30),
        );
        svc.endpoint().unwrap();
        let long_ago = Instant::now() - IDLE_UNLOAD - Duration::from_secs(5);
        lock(&svc.state).running.as_mut().unwrap().last_used = long_ago;
        let lease = svc.lease();
        svc.tick();
        assert!(svc.running(), "a slow answer keeps the server");
        drop(lease);
        // The lease's release counts as use, so the idle clock restarts.
        svc.tick();
        assert!(svc.running());
        lock(&svc.state).running.as_mut().unwrap().last_used = long_ago;
        svc.tick();
        assert!(!svc.running());
        assert!(stopped.load(Ordering::Acquire));
    }

    #[test]
    fn a_crashed_server_is_replaced_on_the_next_call() {
        let flags = Arc::new(Mutex::new(Vec::<Arc<AtomicBool>>::new()));
        let seen = flags.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let svc = Service::with_starter(
            Arc::new(move |_| {
                let flag = Arc::new(AtomicBool::new(true));
                lock(&seen).push(flag.clone());
                Ok(Started {
                    proc: Box::new(FakeProc {
                        alive: flag,
                        stopped: stopped.clone(),
                    }),
                    base_url: "http://127.0.0.1:1".into(),
                })
            }),
            Duration::from_secs(30),
        );
        svc.endpoint().unwrap();
        lock(&flags)[0].store(false, Ordering::Release);
        assert!(!svc.running());
        svc.endpoint().unwrap();
        assert_eq!(lock(&flags).len(), 2);
    }

    #[test]
    fn the_pid_file_round_trips_and_rejects_junk() {
        let text = pidfile::render(4242, Path::new("C:/rt/llama-server.exe"));
        assert_eq!(
            pidfile::parse(&text),
            Some((4242, PathBuf::from("C:/rt/llama-server.exe")))
        );
        assert_eq!(pidfile::parse(""), None);
        assert_eq!(pidfile::parse("abc\nC:/x"), None);
        assert_eq!(pidfile::parse("12\n"), None);
    }
}
