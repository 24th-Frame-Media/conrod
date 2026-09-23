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
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    let part = into.join(format!("{}.part", asset.name));
    let mut failures = Vec::new();
    for url in urls {
        match download(url, &part, asset, stop, progress) {
            Ok(()) => return finish(asset, &part, into),
            Err(e) => {
                let _ = std::fs::remove_file(&part);
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

fn download(
    url: &str,
    part: &Path,
    asset: &Asset,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<(), String> {
    fetch(url, part, &asset.sha256, asset.size, stop, progress)
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
        std::fs::rename(part, &target).map_err(|e| format!("{}: {e}", target.display()))?;
        return Ok(target);
    }
    // Windows 10+ ships bsdtar, which reads zips; GNU tar on PATH would not, so
    // name the system one.
    let tar = std::env::var_os("SystemRoot")
        .map(|root| PathBuf::from(root).join("System32").join("tar.exe"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| PathBuf::from("tar"));
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
}
