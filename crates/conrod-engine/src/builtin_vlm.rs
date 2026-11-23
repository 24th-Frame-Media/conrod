//! Built-in Qwen3-VL-8B-Instruct vision server managed directly by Conrod.
//!
//! No external Ollama installation is needed: Conrod launches the bundled
//! `llama-server` from `<data>/tools/decider-runtime` against the pinned GGUF
//! weights (`Qwen3VL-8B-Instruct-Q4_K_M.gguf`) and F16 vision projector
//! (`mmproj-Qwen3VL-8B-Instruct-F16.gguf`) on a loopback ephemeral port,
//! exposes its OpenAI-compatible JSON-schema endpoint to `conrod_io::vlm`,
//! unloads automatically after an idle period, and shuts down cleanly on quit
//! or whenever a serial-card handoff needs the GPU back for the decision model.

use crate::lock;
use conrod_core::models;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
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

/// Context window for Qwen3-VL single-crop / multi-frame reads.
const DEFAULT_CTX_SIZE: u32 = 8192;

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

struct Running {
    child: Child,
    base_url: String,
    last_used: Instant,
}

impl Running {
    fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Process-wide lifecycle manager for Conrod's built-in Qwen3-VL server.
#[derive(Default)]
pub struct Service {
    state: Mutex<Option<Running>>,
}

/// The singleton built-in vision service.
pub fn service() -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(Service::default)
}

/// Wire `conrod_io::vlm`'s `provider: "builtin"` hooks to this service.
/// Safe to call more than once (idempotent).
pub fn register() {
    conrod_io::vlm::register_builtin(
        Arc::new(|| service().endpoint()),
        Arc::new(|| service().unload()),
    );
}

impl Service {
    /// Whether the model weights, projector, and bundled `llama-server` are all on disk.
    pub fn installed() -> bool {
        BuiltinPaths::probe().is_ok()
    }

    /// Whether the built-in server child process is currently alive.
    pub fn running(&self) -> bool {
        let mut guard = lock(&self.state);
        if let Some(r) = guard.as_mut() {
            if r.alive() {
                return true;
            }
            *guard = None;
        }
        false
    }

    /// Ensure the built-in Qwen3-VL server is running and healthy, returning
    /// its loopback base URL (`http://127.0.0.1:<port>`).
    pub fn endpoint(&self) -> Result<String, String> {
        let mut guard = lock(&self.state);
        if let Some(r) = guard.as_mut() {
            if r.alive() {
                r.last_used = Instant::now();
                return Ok(r.base_url.clone());
            }
            *guard = None;
        }

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
                "Could not start built-in vision server ({}): {e}",
                paths.server.display()
            )
        })?;

        if let Err(e) = wait_until_healthy(&mut child, port, STARTUP_TIMEOUT) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }

        *guard = Some(Running {
            child,
            base_url: base_url.clone(),
            last_used: Instant::now(),
        });
        Ok(base_url)
    }

    /// Terminate the built-in server immediately, returning 100% of its VRAM.
    pub fn unload(&self) {
        if let Some(mut r) = lock(&self.state).take() {
            r.stop();
        }
    }

    /// Periodic tick (called from the app loop): unload after an idle period
    /// so VRAM is not held indefinitely between scans.
    pub fn tick(&self) {
        let mut guard = lock(&self.state);
        let drop_now = guard
            .as_mut()
            .is_some_and(|r| !r.alive() || r.last_used.elapsed() > IDLE_UNLOAD);
        if drop_now {
            if let Some(mut r) = guard.take() {
                r.stop();
            }
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

fn read_stderr_tail(child: &mut Child) -> String {
    let Some(mut err) = child.stderr.take() else {
        return String::new();
    };
    let mut buf = String::new();
    let _ = err.read_to_string(&mut buf);
    let tail: Vec<&str> = buf
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(6)
        .collect();
    tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
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

fn wait_until_healthy(child: &mut Child, port: u16, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let tail = read_stderr_tail(child);
            return Err(if tail.is_empty() {
                format!("Built-in vision server exited early ({status}).")
            } else {
                format!("Built-in vision server failed to start ({status}): {tail}")
            });
        }
        if probe_health_ok(port) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(
                "Built-in vision server timed out while loading Qwen3-VL onto the GPU.".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(has_pair("--ctx-size", "8192"));
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
}
