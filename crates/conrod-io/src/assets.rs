//! Installing what is missing: the models and ExifTool, from the pinned manifest.
//!
//! `scripts/assets.json` is both what CI bundles into a release and what the
//! app installs from when a file is absent (a developer checkout, or a library
//! whose model was deleted). Every download is hashed as it streams and is renamed
//! into place only if it equals the pin, so a truncated, swapped or tampered file
//! is never used.

use conrod_core::settings::data_root;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const MANIFEST: &str = include_str!("../../../scripts/assets.json");

#[derive(Debug, Clone, PartialEq)]
pub struct Asset {
    pub name: String,
    /// `models`, or the folder name of an extracted tool (`exiftool`).
    pub dest: String,
    pub extract: bool,
    pub sha256: String,
    pub size: u64,
    /// Upstream URLs, tried after the release mirror.
    pub sources: Vec<String>,
    /// Opt-in: not part of the release bundle or install everything (GB-sized,
    /// and only for a feature the user turns on). Fetched resumably.
    pub optional: bool,
}

impl Asset {
    /// What the rest of the app calls it: a model's file name, or the tool's folder name.
    pub fn id(&self) -> &str {
        if self.extract {
            &self.dest
        } else {
            &self.name
        }
    }

    /// Where it is installed: `<data>/models`, or `<data>/tools/<tool>`.
    pub fn folder(&self) -> PathBuf {
        if self.extract {
            data_root().join("tools").join(&self.dest)
        } else {
            data_root().join("models")
        }
    }

    /// Whether the manifest really pins this file: a full SHA-256 and a real
    /// size. A placeholder entry (all-zero hash, size 1) is NOT pinned: it has
    /// no trustworthy source yet and must never be fetched or called ready.
    pub fn pinned(&self) -> bool {
        self.size > 0
            && self.sha256.len() == 64
            && self.sha256.chars().all(|c| c.is_ascii_hexdigit())
            && self.sha256.chars().any(|c| c != '0')
    }
}

/// The committed manifest.
pub fn manifest() -> Vec<Asset> {
    parse(MANIFEST).0
}

/// (assets, release base URL).
fn parse(text: &str) -> (Vec<Asset>, String) {
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default();
    let release = root["release"].as_str().unwrap_or_default().to_string();
    let assets = root["assets"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    Some(Asset {
                        name: a["name"].as_str()?.to_string(),
                        dest: a["dest"].as_str()?.to_string(),
                        extract: a["extract"].as_bool().unwrap_or(false),
                        optional: a["optional"].as_bool().unwrap_or(false),
                        sha256: a["sha256"].as_str()?.to_lowercase(),
                        size: a["size"].as_u64().unwrap_or(0),
                        sources: a["sources"]
                            .as_array()
                            .map(|s| {
                                s.iter()
                                    .filter_map(|u| u.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (assets, release)
}

/// Where to try, in order: a mirror named by `CONROD_ASSETS_URL` (for a private
/// mirror or a test), the release the manifest names, then the upstream sources.
fn urls(asset: &Asset, release: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(mirror) = std::env::var("CONROD_ASSETS_URL")
        .ok()
        .filter(|m| !m.is_empty())
    {
        out.push(format!("{}/{}", mirror.trim_end_matches('/'), asset.name));
    }
    if !release.is_empty() {
        out.push(format!("{}/{}", release.trim_end_matches('/'), asset.name));
    }
    out.extend(asset.sources.iter().cloned());
    out
}

/// Download, verify and install one asset into `into`; returns where it went.
/// `progress` gets (bytes so far, expected bytes). `stop` aborts between reads.
pub fn install(
    asset: &Asset,
    into: &Path,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, String> {
    let (_, release) = parse(MANIFEST);
    install_from(asset, &urls(asset, &release), into, stop, progress)
}

fn install_from(
    asset: &Asset,
    urls: &[String],
    into: &Path,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, String> {
    // The volume query needs an existing directory to name the volume.
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    install_with_free(asset, urls, into, stop, progress, free_bytes(into))
}

/// Like [install_from], but the volume's free space is given (tests refuse a
/// download on a nearly-full disk without needing one).
fn install_with_free(
    asset: &Asset,
    urls: &[String],
    into: &Path,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
    free: Option<u64>,
) -> Result<PathBuf, String> {
    if !asset.pinned() {
        return Err(format!(
            "{} is not available yet: its checksum is not pinned, so there is nothing trustworthy to download.",
            asset.name
        ));
    }
    if urls.is_empty() {
        return Err(format!(
            "{} is not available yet: no download source is pinned for it.",
            asset.name
        ));
    }
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    // Refuse before writing a byte when the volume cannot hold the file (an
    // extracted tool needs room for the archive AND its unpacked contents).
    let need = asset.size.saturating_mul(if asset.extract { 2 } else { 1 });
    check_space(into, &asset.name, need, free)?;
    let part = into.join(format!("{}.part", asset.name));
    let mut failures = Vec::new();
    for url in urls {
        match download(url, &part, asset, stop, progress) {
            Ok(()) => return finish(asset, &part, into),
            Err(e) => {
                // A big download keeps what it has so the next try resumes it.
                if !resumable(asset) {
                    let _ = std::fs::remove_file(&part);
                }
                if stop.load(Ordering::Relaxed) {
                    return Err("stopped".into());
                }
                failures.push(format!("{url}: {e}"));
            }
        }
    }
    Err(format!(
        "could not install {}: {}",
        asset.name,
        failures.join("; ")
    ))
}

/// Headroom on top of the file itself: filesystem slack, and the brief moment
/// where a renamed file and its directory entry both exist.
const SPACE_HEADROOM: u64 = 64 << 20;

/// Free bytes on the volume holding `path`, when the OS can say. Windows is
/// the app's platform; elsewhere the check is skipped rather than guessed.
pub fn free_bytes(path: &Path) -> Option<u64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        extern "system" {
            fn GetDiskFreeSpaceExW(
                directory: *const u16,
                free_to_caller: *mut u64,
                total: *mut u64,
                total_free: *mut u64,
            ) -> i32;
        }
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut free = 0u64;
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that lives across the
        // call; the out pointer is a valid local; the two unused outs are null.
        let ok = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        (ok != 0).then_some(free)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        None
    }
}

/// Plain-language refusal when the destination volume is too full. `free` of
/// `None` means the OS could not tell us, so the download is allowed to try.
fn check_space(into: &Path, name: &str, need: u64, free: Option<u64>) -> Result<(), String> {
    let Some(free) = free else {
        return Ok(());
    };
    if free >= need.saturating_add(SPACE_HEADROOM) {
        return Ok(());
    }
    let gb = |b: u64| format!("{:.1} GB", b as f64 / (1u64 << 30) as f64);
    let mb = |b: u64| format!("{} MB", b / (1u64 << 20));
    let volume = into
        .components()
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .unwrap_or_else(|| "the disk".into());
    Err(format!(
        "Not enough disk space for {name}: it needs {} free on {volume}, but only {} is left. Free some space and try again.",
        gb(need),
        if free < (1u64 << 30) { mb(free) } else { gb(free) },
    ))
}

fn download(
    url: &str,
    part: &Path,
    asset: &Asset,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    if resumable(asset) {
        fetch_resumable(url, part, &asset.sha256, asset.size, stop, progress)
    } else {
        fetch(url, part, &asset.sha256, asset.size, stop, progress)
    }
}

/// Files this big are worth resuming after an interruption.
const RESUME_FROM: u64 = 256 << 20;

fn resumable(asset: &Asset) -> bool {
    asset.size >= RESUME_FROM
}

/// Like [etch], but continues a partial part from where it stopped (HTTP Range)
/// and leaves it in place on an interruption. A checksum mismatch removes it.
pub fn fetch_resumable(
    url: &str,
    part: &Path,
    sha256: &str,
    size_hint: u64,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
    let resume = have > 0 && have < size_hint;
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(6 * 60 * 60)))
        .build()
        .new_agent();
    let mut request = agent.get(url);
    if resume {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let mut response = request.call().map_err(|e| e.to_string())?;
    let partial = resume && response.status().as_u16() == 206;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut file = if partial {
        let mut old = std::fs::File::open(part).map_err(|e| e.to_string())?;
        let mut buffer = vec![0u8; 1 << 20];
        loop {
            let n = old.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
            done += n as u64;
        }
        std::fs::OpenOptions::new()
            .append(true)
            .open(part)
            .map_err(|e| e.to_string())?
    } else {
        std::fs::File::create(part).map_err(|e| e.to_string())?
    };
    let total = if partial {
        done + response
            .body()
            .content_length()
            .unwrap_or(size_hint.saturating_sub(done))
    } else {
        response.body().content_length().unwrap_or(size_hint)
    }
    .max(1);
    let mut reader = response.body_mut().as_reader();
    let mut buffer = vec![0u8; 256 * 1024];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".into());
        }
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress(done, total);
    }
    file.flush().map_err(|e| e.to_string())?;
    let got = format!("{:x}", hasher.finalize());
    if got == sha256 {
        Ok(())
    } else {
        let _ = std::fs::remove_file(part);
        Err(format!(
            "the download does not match the checksum (got {got}, expected {sha256})"
        ))
    }
}

/// Stream `url` into `part`, hashing as it goes, and succeed only if the SHA-256
/// equals `sha256` (lower-case hex). `size_hint` is used for progress when the
/// server does not say how long the body is. The caller removes `part` on failure.
pub fn fetch(
    url: &str,
    part: &Path,
    sha256: &str,
    size_hint: u64,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(30 * 60)))
        .build()
        .new_agent();
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    let total = response.body().content_length().unwrap_or(size_hint).max(1);
    let mut reader = response.body_mut().as_reader();
    let mut file = std::fs::File::create(part).map_err(|e| e.to_string())?;
    let (mut hasher, mut done) = (Sha256::new(), 0u64);
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".into());
        }
        let n = reader.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        progress(done, total);
    }
    file.flush().map_err(|e| e.to_string())?;
    let got = format!("{:x}", hasher.finalize());
    if got == sha256 {
        Ok(())
    } else {
        Err(format!(
            "the download does not match its checksum (got {got}, expected {sha256})"
        ))
    }
}

/// The verified download becomes the model file, or is unpacked (a tool).
fn finish(asset: &Asset, part: &Path, into: &Path) -> Result<PathBuf, String> {
    if !asset.extract {
        let target = into.join(&asset.name);
        // Windows' rename refuses an existing destination; a stale or partial
        // file of the same name must not block the verified one.
        if target.exists() {
            std::fs::remove_file(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
        std::fs::rename(part, &target).map_err(|e| {
            let _ = std::fs::remove_file(part);
            format!("{}: {e}", target.display())
        })?;
        return Ok(target);
    }
    // Windows 10+ ships bsdtar, which reads zips; GNU tar on PATH would not, so
    // name the system one specifically rather than falling back to whatever
    // "tar" resolves to on PATH (fail closed instead of unpacking with an
    // unknown, possibly wrong, tool).
    let tar = std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32").join("tar.exe"))
        .filter(|p| p.is_file())
        .ok_or_else(|| "System32\\tar.exe not found; cannot unpack downloaded asset".to_string())?;
    let status = std::process::Command::new(tar)
        .arg("-xf")
        .arg(part)
        .arg("-C")
        .arg(into)
        .output()
        .map_err(|e| format!("unpacking {}: {e}", asset.name))?;
    let _ = std::fs::remove_file(part);
    if status.status.success() {
        Ok(into.to_path_buf())
    } else {
        Err(format!(
            "unpacking {}: {}",
            asset.name,
            String::from_utf8_lossy(&status.stderr).trim()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// A tiny HTTP server: answers each request from `routes` (path -> body) or 404,
    /// for exactly `requests` connections.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>, requests: usize) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for _ in 0..requests {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 2048];
                let n = stream.read(&mut request).unwrap_or(0);
                let path = String::from_utf8_lossy(&request[..n])
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                let reply = match routes.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => {
                        let mut r = format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        )
                        .into_bytes();
                        r.extend_from_slice(body);
                        r
                    }
                    None => {
                        b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                            .to_vec()
                    }
                };
                let _ = stream.write_all(&reply);
            }
        });
        base
    }

    fn asset(body: &[u8]) -> Asset {
        Asset {
            name: "model.onnx".into(),
            dest: "models".into(),
            extract: false,
            sha256: format!("{:x}", Sha256::digest(body)),
            size: body.len() as u64,
            sources: vec![],
            optional: false,
        }
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("conrod-assets-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_committed_manifest_is_complete_and_pinned() {
        let assets = manifest();
        assert!(assets.len() >= 8, "{} assets", assets.len());
        for a in &assets {
            assert_eq!(a.sha256.len(), 64, "{}", a.name);
            assert!(
                a.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{}",
                a.name
            );
            assert!(a.size > 0, "{}", a.name);
        }
        assert!(assets.iter().any(|a| a.extract && a.dest == "exiftool"));
        assert!(assets.iter().any(|a| a.name == "yolo11s-960.onnx"));
    }

    #[test]
    fn the_default_detector_is_pinned_and_installed_by_default() {
        let assets = manifest();
        let detector = assets
            .iter()
            .find(|a| a.name == conrod_core::models::DETECTOR)
            .expect("the default detector is pinned in the manifest");
        assert!(!detector.optional, "{} is opt-in", detector.name);
        assert_eq!(detector.dest, "models");
        let (_, release) = parse(MANIFEST);
        assert!(
            urls(detector, &release)
                .iter()
                .any(|u| u.ends_with(detector.name.as_str())),
            "{} has no source to install from",
            detector.name
        );
    }

    #[test]
    fn a_verified_download_is_installed_and_reports_progress() {
        let body = vec![7u8; 200_000];
        let base = serve(vec![("/model.onnx", body.clone())], 1);
        let into = scratch("ok");
        let mut last = (0, 0);
        let path = install_from(
            &asset(&body),
            &[format!("{base}/model.onnx")],
            &into,
            &AtomicBool::new(false),
            &mut |d, t| last = (d, t),
        )
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(last, (200_000, 200_000));
        assert!(!into.join("model.onnx.part").exists());
        std::fs::remove_dir_all(&into).unwrap();
    }

    #[test]
    fn a_swapped_file_is_refused_and_the_next_source_is_tried() {
        let good = vec![1u8; 4096];
        let base = serve(vec![("/bad", vec![9u8; 4096]), ("/good", good.clone())], 3);
        let into = scratch("swap");
        let path = install_from(
            &asset(&good),
            &[
                format!("{base}/missing"),
                format!("{base}/bad"),
                format!("{base}/good"),
            ],
            &into,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), good);
        std::fs::remove_dir_all(&into).unwrap();
    }

    #[test]
    fn nothing_is_installed_when_every_source_fails_the_check() {
        let base = serve(vec![("/m", vec![9u8; 100])], 1);
        let into = scratch("none");
        let err = install_from(
            &asset(&[1u8; 100]),
            &[format!("{base}/m")],
            &into,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap_err();
        assert!(err.contains("does not match its checksum"), "{err}");
        assert!(!into.join("model.onnx").exists() && !into.join("model.onnx.part").exists());
        std::fs::remove_dir_all(&into).unwrap();
    }

    #[test]
    fn a_nearly_full_disk_is_refused_before_a_byte_is_written() {
        let body = vec![3u8; 500_000];
        let mut big = asset(&body);
        big.size = 100 << 20; // the file claims 100 MB; the volume has 66 MB
        let into = scratch("full");
        let err = install_with_free(
            &big,
            &["http://127.0.0.1:1/never".into()],
            &into,
            &AtomicBool::new(false),
            &mut |_, _| {},
            Some(66 << 20), // the real E: drive one night: 66 MB free
        )
        .unwrap_err();
        assert!(err.contains("Not enough disk space"), "{err}");
        assert!(err.contains("model.onnx"), "{err}");
        assert!(err.contains("Free some space"), "{err}");
        assert!(!into.join("model.onnx.part").exists());
        std::fs::remove_dir_all(&into).unwrap();
    }

    #[test]
    fn an_unpinned_placeholder_is_not_available_and_never_fetched() {
        let mut a = asset(b"x");
        a.sha256 = "0".repeat(64); // the nima TODO entry's shape
        a.size = 1;
        assert!(!a.pinned());
        let into = scratch("unpinned");
        let err = install_with_free(
            &a,
            &["http://127.0.0.1:1/never".into()],
            &into,
            &AtomicBool::new(false),
            &mut |_, _| {},
            None,
        )
        .unwrap_err();
        assert!(err.contains("not available yet"), "{err}");
        // Refused before anything was created: no directory, no part file.
        assert!(!into.exists());
    }

    #[test]
    fn a_pinned_asset_with_no_source_is_not_available() {
        let into = scratch("nosource");
        let err = install_with_free(
            &asset(b"x"),
            &[],
            &into,
            &AtomicBool::new(false),
            &mut |_, _| {},
            None,
        )
        .unwrap_err();
        assert!(err.contains("not available yet"), "{err}");
        assert!(err.contains("no download source"), "{err}");
        assert!(!into.exists());
    }

    #[test]
    fn a_verified_download_replaces_a_stale_file_of_the_same_name() {
        let body = vec![4u8; 4096];
        let base = serve(vec![("/model.onnx", body.clone())], 1);
        let into = scratch("replace");
        std::fs::create_dir_all(&into).unwrap();
        std::fs::write(into.join("model.onnx"), b"stale").unwrap();
        let path = install_from(
            &asset(&body),
            &[format!("{base}/model.onnx")],
            &into,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(path).unwrap(), body);
        std::fs::remove_dir_all(&into).unwrap();
    }

    #[test]
    fn the_builtin_vision_model_is_pinned_opt_in_and_resumable() {
        let assets = manifest();
        for name in [
            conrod_core::models::VISION,
            conrod_core::models::VISION_MMPROJ,
        ] {
            let a = assets.iter().find(|a| a.name == name).expect(name);
            assert!(a.pinned(), "{name} is not pinned");
            assert!(a.optional, "{name} must be opt-in");
            assert!(a.size >= 256 << 20, "{name} should resume");
            assert!(
                a.sources
                    .iter()
                    .any(|u| u.starts_with("https://huggingface.co/Qwen/")),
                "{name} needs its official upstream source"
            );
        }
        // The nima placeholder stays honestly unpinned until the owner stages it.
        let nima = assets
            .iter()
            .find(|a| a.name == conrod_core::models::QUALITY)
            .expect("nima entry");
        assert!(!nima.pinned(), "nima must not pretend to be pinned");
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;
    use std::net::TcpListener;

    /// Serves `body` for one connection, honouring `Range: bytes=N-` like a CDN.
    fn serve_ranged(body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let n = stream.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..n]).to_lowercase();
            let from = text
                .split("range: bytes=")
                .nth(1)
                .and_then(|r| r.split('-').next())
                .and_then(|n| n.trim().parse::<usize>().ok());
            let reply = match from {
                Some(from) => {
                    let rest = &body[from..];
                    let mut r = format!(
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{}/{}\r\nConnection: close\r\n\r\n",
                        rest.len(),
                        body.len() - 1,
                        body.len()
                    )
                    .into_bytes();
                    r.extend_from_slice(rest);
                    r
                }
                None => {
                    let mut r = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .into_bytes();
                    r.extend_from_slice(&body);
                    r
                }
            };
            let _ = stream.write_all(&reply);
        });
        base
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("conrod-resume-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Real network: the pinned llama.cpp runtime installs (hash-checked, unpacked) from GitHub.
    #[test]
    #[ignore = "downloads 38 MB from github.com"]
    fn the_decision_runtime_installs_from_its_upstream_source() {
        let asset = manifest()
            .into_iter()
            .find(|a| a.name.starts_with("llama-b12359-bin-win-vulkan"))
            .unwrap();
        let into = scratch("runtime");
        install_from(
            &asset,
            &asset.sources,
            &into,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap();
        for dll in [
            "llama.dll",
            "mtmd.dll",
            "ggml.dll",
            "ggml-base.dll",
            "ggml-vulkan.dll",
        ] {
            assert!(into.join(dll).is_file(), "{dll}");
        }
        std::fs::remove_dir_all(&into).unwrap();
    }

    #[test]
    fn an_interrupted_download_continues_where_it_stopped() {
        let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        let base = serve_ranged(body.clone());
        let dir = scratch("ok");
        let part = dir.join("m.gguf.part");
        std::fs::write(&part, &body[..120_000]).unwrap();
        let sha = format!("{:x}", Sha256::digest(&body));
        let mut seen = Vec::new();
        fetch_resumable(
            &format!("{base}/m"),
            &part,
            &sha,
            body.len() as u64,
            &AtomicBool::new(false),
            &mut |d, t| seen.push((d, t)),
        )
        .unwrap();
        assert_eq!(std::fs::read(&part).unwrap(), body);
        // Progress starts past what was already on disk and ends at the full size.
        assert!(seen.first().unwrap().0 > 120_000);
        assert_eq!(*seen.last().unwrap(), (300_000, 300_000));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_resumed_file_that_fails_the_checksum_is_discarded() {
        let body = vec![5u8; 50_000];
        let base = serve_ranged(body.clone());
        let dir = scratch("bad");
        let part = dir.join("m.gguf.part");
        std::fs::write(&part, vec![9u8; 10_000]).unwrap(); // not a prefix of the real file
        let sha = format!("{:x}", Sha256::digest(&body));
        let err = fetch_resumable(
            &format!("{base}/m"),
            &part,
            &sha,
            body.len() as u64,
            &AtomicBool::new(false),
            &mut |_, _| {},
        )
        .unwrap_err();
        assert!(err.contains("does not match"), "{err}");
        assert!(!part.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
