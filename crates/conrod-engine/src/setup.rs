//! Making sure the models and tools a run needs are on disk, and installing what
//! is missing from the pinned manifest (`conrod_io::assets`), with progress in the
//! status area. Scan, identify and write call [`ensure`] before they load anything,
//! so a missing file is fetched rather than being an error the user has to fix.

use crate::desktop::Desktop;
use crate::lock;
use conrod_core::models;
use conrod_core::tasks::TaskHub;
use conrod_io::assets::{self, Asset};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

pub const SCAN: &[&str] = &[models::DETECTOR];
// ocec_l.onnx and face_recognition_sface_2021dec.onnx are pinned with upstream
// sources; the release mirror does not carry them yet (scripts/stage-assets.ps1).
pub const FACES: &[&str] = &[models::FACES, models::EYES, models::FACE_ID];
/// Extras a run is better with but works without: failing to install them is
/// shown in the status area and is not an error.
const OPTIONAL: &[&str] = &[models::EYES, models::FACE_ID];
pub const IDENTIFY_PLATES: &[&str] = &[models::PLATE_DETECTOR, models::PLATE_READER];
pub const TEXT: &[&str] = &[models::OCR_DETECTOR, models::OCR_RECOGNISER];
pub const SIMILARITY: &[&str] = &[models::SIMILARITY];
pub const WRITE: &[&str] = &["exiftool"];
/// NIMA technical quality. Opt-in: not in `GROUPS`/`everything()`, so a
/// fresh install or "install everything" never fetches it; only a settings
/// screen that turns quality scoring on should call [`ensure`] with this.
pub const QUALITY: &[&str] = &[models::QUALITY];
/// The built-in vision model: weights + vision projector, one Download button.
pub const VISION: &[&str] = &[models::VISION, models::VISION_MMPROJ];

/// The rows of the Setup card: what a person would name it, and the assets it needs.
const GROUPS: [(&str, &[&str]); 6] = [
    ("Detector", SCAN),
    ("Faces", FACES),
    ("Similarity", SIMILARITY),
    ("Plates", IDENTIFY_PLATES),
    ("Text (OCR)", TEXT),
    ("ExifTool", WRITE),
];

/// The stable row id the UI matches on (never a fuzzy name search).
pub fn group_id(name: &str) -> &'static str {
    match name {
        "Detector" => "detector",
        "Faces" => "faces",
        "Similarity" => "similarity",
        "Plates" => "plates",
        "Text (OCR)" => "text",
        "ExifTool" => "exiftool",
        _ => "other",
    }
}

/// Which manifest row an asset belongs to, for install progress the card can
/// attribute to the button that was clicked.
pub fn row_id_for(asset_id: &str) -> String {
    for (name, ids) in GROUPS {
        if ids.contains(&asset_id) {
            return group_id(name).to_string();
        }
    }
    if [
        models::DECIDER_INT8,
        models::DECIDER_INT4,
        models::DECIDER_MMPROJ,
        models::DECIDER_RUNTIME,
    ]
    .contains(&asset_id)
    {
        return "decider".into();
    }
    if VISION.contains(&asset_id) {
        return "builtin-vision".into();
    }
    if QUALITY.contains(&asset_id) {
        return "quality".into();
    }
    asset_id.to_string()
}

/// One install in flight (or the last one's failure), in real bytes. The card
/// polls this through `models_status` so its progress bar is the download's
/// own, not a timer. `error` stays until the next install starts.
#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub id: String,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}

fn tracker() -> &'static Mutex<Option<InstallProgress>> {
    static TRACKER: OnceLock<Mutex<Option<InstallProgress>>> = OnceLock::new();
    TRACKER.get_or_init(|| Mutex::new(None))
}

/// Start a fresh install: any previous failure is cleared.
pub fn progress_begin(id: &str, total: u64) {
    *lock(tracker()) = Some(InstallProgress {
        id: id.into(),
        done: 0,
        total,
        error: None,
    });
}

pub fn progress_note(id: &str, done: u64, total: u64) {
    let mut t = lock(tracker());
    match t.as_mut() {
        Some(p) if p.id == id && p.error.is_none() => {
            p.done = done;
            p.total = total;
        }
        _ => {
            *t = Some(InstallProgress {
                id: id.into(),
                done,
                total,
                error: None,
            })
        }
    }
}

/// Keep the byte counts and record why the install failed.
pub fn progress_error(id: &str, error: &str) {
    let mut t = lock(tracker());
    match t.as_mut() {
        Some(p) if p.id == id => p.error = Some(error.into()),
        _ => {
            *t = Some(InstallProgress {
                id: id.into(),
                done: 0,
                total: 0,
                error: Some(error.into()),
            })
        }
    }
}

/// A finished, verified install leaves no running state behind.
pub fn progress_done() {
    *lock(tracker()) = None;
}

pub fn install_progress() -> Option<Value> {
    lock(tracker())
        .as_ref()
        .map(|p| json!({"id": p.id, "done": p.done, "total": p.total, "error": p.error}))
}

pub(crate) fn present(asset: &Asset) -> bool {
    if asset.id() == models::DECIDER_RUNTIME {
        models::decider_runtime().is_some()
    } else if asset.extract {
        models::exiftool().is_some() || exiftool_on_path()
    } else {
        models::find(&asset.name).is_some()
            || (asset.id() == models::OCR_DETECTOR && models::ocr_dir().is_some())
    }
}

/// Whether `exiftool` runs from PATH (a dev machine; a release bundles its own).
pub(crate) fn exiftool_on_path() -> bool {
    std::process::Command::new("exiftool")
        .arg("-ver")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub(crate) fn name_of(id: &str) -> &str {
    GROUPS
        .iter()
        .find(|(_, ids)| ids.contains(&id))
        .map_or(id, |(name, _)| name)
}

/// The Setup card: one row per group, ready when everything in it is on disk.
/// `id` is the stable handle the UI matches on; `downloaded` is disk truth;
/// `available` is false when an asset has no trustworthy pinned source yet.
pub fn rows() -> Value {
    let manifest = assets::manifest();
    let asset_for = |id: &str| manifest.iter().find(|a| a.id() == id);
    let ready = |ids: &[&str]| ids.iter().all(|id| asset_for(id).is_none_or(present));
    let available = |ids: &[&str]| {
        ids.iter()
            .all(|id| asset_for(id).is_none_or(|a| a.pinned()))
    };
    Value::Array(
        GROUPS
            .iter()
            .map(|(name, ids)| {
                let ok = if *name == "Text (OCR)" {
                    models::ocr_dir().is_some()
                } else {
                    ready(ids)
                };
                json!({
                    "id": group_id(name), "name": name, "file": ids[0],
                    "ready": ok, "downloaded": ok, "available": available(ids),
                })
            })
            .collect(),
    )
}

/// The built-in vision row: disk truth for both GGUFs, never a serving claim
/// from a cloud service. `downloaded` flips only when both verified files exist.
pub fn vision_row() -> Value {
    let weights = models::find(models::VISION).is_some();
    let mmproj = models::find(models::VISION_MMPROJ).is_some();
    let downloaded = weights && mmproj;
    let manifest = assets::manifest();
    let available = VISION.iter().all(|id| {
        manifest
            .iter()
            .find(|a| a.id() == *id)
            .is_some_and(|a| a.pinned())
    });
    let detail = if downloaded {
        if crate::builtin_vlm::service().running() {
            "Running on this PC (built-in).".to_string()
        } else if models::vision_server().is_some() {
            "On this PC (built-in).".to_string()
        } else {
            "On this PC (runtime installs automatically when enabled).".to_string()
        }
    } else if !available {
        "Not available yet: no pinned source to download it from.".to_string()
    } else {
        let bytes: u64 = manifest
            .iter()
            .filter(|a| VISION.contains(&a.id()) && !present(a))
            .map(|a| a.size)
            .sum();
        format!(
            "Not downloaded yet ({:.1} GB).",
            bytes as f64 / (1u64 << 30) as f64
        )
    };
    json!({
        "id": "builtin-vision", "name": "Vision model (built-in)", "file": models::VISION,
        "ready": downloaded, "downloaded": downloaded, "optional": true,
        "available": available, "detail": detail,
    })
}

/// The opt-in quality model: honest about its placeholder pin (not available).
pub fn quality_row() -> Value {
    let manifest = assets::manifest();
    let available = manifest
        .iter()
        .find(|a| a.id() == models::QUALITY)
        .is_some_and(|a| a.pinned());
    let downloaded = models::find(models::QUALITY).is_some();
    json!({
        "id": "quality", "name": "Quality", "file": models::QUALITY,
        "ready": downloaded, "downloaded": downloaded, "optional": true,
        "available": available,
        "detail": if available { "Opt-in sharpness tiebreak" } else { "Not available yet: no pinned source to download it from." },
    })
}

/// Install whichever of `ids` is missing, one task each, and say what failed.
/// An asset with no pinned source is skipped with a plain note, never fetched.
pub fn ensure(hub: &TaskHub, stop: &AtomicBool, ids: &[&str]) -> Result<(), String> {
    ensure_from(hub, stop, ids, &assets::manifest())
}

/// [`ensure`] against a given manifest (tests pin small local assets).
pub fn ensure_from(
    hub: &TaskHub,
    stop: &AtomicBool,
    ids: &[&str],
    manifest: &[Asset],
) -> Result<(), String> {
    let mut failures = Vec::new();
    for id in ids {
        let Some(asset) = manifest.iter().find(|a| a.id() == *id) else {
            continue;
        };
        if present(asset) {
            continue;
        }
        if !asset.pinned() {
            hub.note(format!(
                "{} is not available yet: no pinned source to download it from.",
                asset.name
            ));
            continue;
        }
        let row = row_id_for(id);
        let task = hub.start(format!("Installing {}", name_of(id)), 100);
        let mut report = |done: u64, total: u64| {
            task.progress(done * 100 / total.max(1), 100);
            progress_note(&row, done, total);
        };
        match assets::install(asset, &asset.folder(), stop, &mut report) {
            Ok(_) => task.finish(),
            Err(e) => {
                task.fail(e.clone());
                progress_error(&row, &e);
                if !OPTIONAL.contains(id) {
                    failures.push(e);
                }
            }
        }
        if stop.load(Ordering::Relaxed) {
            return Err("stopped".into());
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Every asset the manifest lists, for "install what is missing", minus the opt-in ones
/// (`optional` in the manifest: the multi-GB decision model installs from its own Settings card).
pub fn everything() -> Vec<String> {
    assets::manifest()
        .iter()
        .filter(|a| !a.optional)
        .map(|a| a.id().to_string())
        .filter(|id| !QUALITY.contains(&id.as_str()))
        .collect()
}

/// The Setup card's button: install all that is missing in the background.
pub fn install_missing(d: &Arc<Desktop>) -> Result<Value, String> {
    install_ids(d, everything())
}

/// Install exactly the named assets in the background (the card's per-model
/// Download), or everything missing when `ids` is empty. One install at a time.
pub fn install_ids(d: &Arc<Desktop>, ids: Vec<String>) -> Result<Value, String> {
    let mut ids = if ids.is_empty() { everything() } else { ids };
    // Installing the built-in Qwen3-VL pair also ensures the bundled llama.cpp
    // runtime (`llama-server.exe`) is present so it can run without Ollama.
    if ids.iter().any(|id| VISION.contains(&id.as_str()))
        && models::vision_server().is_none()
        && !ids.iter().any(|id| id == models::DECIDER_RUNTIME)
    {
        ids.push(models::DECIDER_RUNTIME.to_string());
    }
    let key = "Installing models".to_string();
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut ops = lock(&d.operations);
        if ops.contains_key(&key) {
            return Err("Models are already being installed".into());
        }
        ops.insert(key.clone(), flag.clone());
    }
    let desktop = d.clone();
    let operation = key.clone();
    std::thread::spawn(move || {
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        // Clear any previous failure so the card never reads a stale reason
        // as this install's outcome.
        if let Some(first) = ids.first() {
            progress_begin(&row_id_for(first), 0);
        }
        let result = ensure(&desktop.hub, &flag, &ids);
        match result {
            Ok(()) => progress_done(),
            // A failure keeps its plain reason in the tracker for the card.
            Err(e) if e == "stopped" => progress_done(),
            Err(_) => {}
        }
        lock(&desktop.operations).remove(&operation);
    });
    Ok(json!({"operation": key}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use conrod_io::assets::Asset;
    use sha2::{Digest, Sha256};
    use std::io::{Read, Write};
    use std::net::TcpListener;

    // These tests move `CONROD_HOME` / `CONROD_ASSETS_URL`, which every album
    // test reads ambiently (`~/.conrod` models). They must NOT run inside the
    // parallel suite: the gate runs them in their own serial pass
    // (`cargo test -p conrod-engine --lib setup::tests -- --test-threads=1`,
    // the main pass skips `setup::tests`). HOME_LOCK serialises them against
    // the decider row test, which moves the same variables.
    use crate::HOME_LOCK;

    struct Home {
        dir: std::path::PathBuf,
        old_home: Option<std::ffi::OsString>,
        old_mirror: Option<std::ffi::OsString>,
    }

    impl Home {
        fn new(tag: &str) -> Home {
            let dir =
                std::env::temp_dir().join(format!("conrod-setup-{tag}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("models")).unwrap();
            let old_home = std::env::var_os("CONROD_HOME");
            let old_mirror = std::env::var_os("CONROD_ASSETS_URL");
            std::env::set_var("CONROD_HOME", &dir);
            Home {
                dir,
                old_home,
                old_mirror,
            }
        }
        fn mirror(&self, base: &str) {
            std::env::set_var("CONROD_ASSETS_URL", base);
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            match self.old_home.take() {
                Some(v) => std::env::set_var("CONROD_HOME", v),
                None => std::env::remove_var("CONROD_HOME"),
            }
            match self.old_mirror.take() {
                Some(v) => std::env::set_var("CONROD_ASSETS_URL", v),
                None => std::env::remove_var("CONROD_ASSETS_URL"),
            }
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    /// A tiny HTTP server: answers each request from `routes` (path -> body) or 404.
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

    fn asset(name: &str, body: &[u8]) -> Asset {
        Asset {
            name: name.into(),
            dest: "models".into(),
            extract: false,
            sha256: format!("{:x}", Sha256::digest(body)),
            size: body.len() as u64,
            sources: vec![],
            optional: false,
        }
    }

    #[test]
    fn the_decision_model_is_opt_in_and_never_part_of_install_everything() {
        let manifest = assets::manifest();
        let decider_ids = [
            models::DECIDER_INT8,
            models::DECIDER_INT4,
            models::DECIDER_MMPROJ,
            models::DECIDER_RUNTIME,
        ];
        // Weights, vision projector, and the llama.cpp runtime (int4 is made locally).
        let decider: Vec<_> = manifest
            .iter()
            .filter(|a| a.optional && decider_ids.contains(&a.id()))
            .collect();
        assert_eq!(decider.len(), 3, "{decider:?}");
        assert!(decider
            .iter()
            .any(|a| a.id() == models::DECIDER_INT8 && a.size > 5_000_000_000));
        assert!(decider.iter().any(|a| a.id() == models::DECIDER_MMPROJ));
        assert!(decider
            .iter()
            .any(|a| a.id() == models::DECIDER_RUNTIME && a.extract));
        // The built-in vision pair is opt-in too, and never fetched by default.
        let vision: Vec<_> = manifest
            .iter()
            .filter(|a| a.optional && VISION.contains(&a.id()))
            .collect();
        assert_eq!(vision.len(), 2, "{vision:?}");
        let everything = everything();
        for a in decider.iter().chain(vision.iter()) {
            assert!(
                !everything.iter().any(|id| id == a.id()),
                "{} would be fetched by default",
                a.id()
            );
        }
        // The big files have upstream sources to resume from (no release mirror hosts them).
        assert!(decider.iter().all(|a| !a.sources.is_empty()));
        assert!(vision.iter().all(|a| !a.sources.is_empty()));
        // The old detector stays pinned and installable on request, but a fresh
        // install only fetches the current one.
        assert!(manifest
            .iter()
            .any(|a| a.id() == "yolo11s-960.onnx" && a.optional));
        assert!(!everything.iter().any(|id| id == "yolo11s-960.onnx"));
    }

    #[test]
    fn only_the_requested_asset_installs_and_the_row_flips_after_verify() {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = Home::new("only");
        let wanted = vec![7u8; 40_000];
        let other = vec![9u8; 40_000];
        let base = serve(
            vec![
                ("/wanted.onnx", wanted.clone()),
                ("/other.onnx", other.clone()),
            ],
            2,
        );
        home.mirror(&base);
        let manifest = vec![asset("wanted.onnx", &wanted), asset("other.onnx", &other)];
        let hub = TaskHub::new();
        let stop = AtomicBool::new(false);
        ensure_from(&hub, &stop, &["wanted.onnx"], &manifest).unwrap();
        assert!(home.dir.join("models/wanted.onnx").is_file());
        assert!(
            !home.dir.join("models/other.onnx").exists(),
            "installing one model must not fetch another"
        );
        assert!(present(&manifest[0]));
        assert!(!present(&manifest[1]));
        // The tracker reported the clicked row's real bytes; the background
        // wrapper (not `ensure` itself) clears it when the install ends.
        let p = install_progress().expect("progress for the clicked row");
        assert_eq!(p["id"], "wanted.onnx");
        assert_eq!(p["done"], 40_000);
        assert_eq!(p["total"], 40_000);
        assert_eq!(p["error"], Value::Null);
        progress_done();
        assert!(install_progress().is_none());
    }

    #[test]
    fn a_bad_hash_never_flips_the_row_and_keeps_the_reason() {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = Home::new("badhash");
        let good = vec![1u8; 8_000];
        let base = serve(vec![("/m.onnx", vec![2u8; 8_000])], 1);
        home.mirror(&base);
        let manifest = vec![asset("m.onnx", &good)];
        let hub = TaskHub::new();
        let stop = AtomicBool::new(false);
        progress_begin("m.onnx", 8_000);
        let err = ensure_from(&hub, &stop, &["m.onnx"], &manifest).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
        assert!(!home.dir.join("models/m.onnx").exists());
        assert!(!home.dir.join("models/m.onnx.part").exists());
        assert!(
            !present(&manifest[0]),
            "the row must not flip on a bad hash"
        );
        let p = install_progress().expect("failure kept for the card");
        assert_eq!(p["id"], "m.onnx");
        assert!(p["error"].as_str().unwrap().contains("does not match"));
        progress_done();
    }

    #[test]
    fn a_full_disk_is_refused_plainly_before_downloading() {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = Home::new("full");
        let mut big = asset("big.gguf", b"x");
        big.size = 1 << 40; // a terabyte: no volume on this machine has it
        let hub = TaskHub::new();
        let stop = AtomicBool::new(false);
        progress_begin("big.gguf", big.size);
        let err = ensure_from(&hub, &stop, &["big.gguf"], &[big]).unwrap_err();
        assert!(err.contains("Not enough disk space"), "{err}");
        assert!(err.contains("Free some space"), "{err}");
        assert!(!home.dir.join("models/big.gguf.part").exists());
        let p = install_progress().expect("failure kept for the card");
        assert!(p["error"]
            .as_str()
            .unwrap()
            .contains("Not enough disk space"));
        progress_done();
    }

    #[test]
    fn an_unpinned_asset_is_skipped_with_a_plain_note() {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = Home::new("unpinned");
        let mut placeholder = asset("nima.onnx", b"x");
        placeholder.sha256 = "0".repeat(64);
        placeholder.size = 1;
        let hub = TaskHub::new();
        let stop = AtomicBool::new(false);
        ensure_from(&hub, &stop, &["nima.onnx"], &[placeholder]).unwrap();
        assert!(!home.dir.join("models/nima.onnx").exists());
        assert!(
            hub.log().iter().any(|l| l.contains("not available yet")),
            "{:?}",
            hub.log()
        );
    }

    #[test]
    fn the_vision_and_quality_rows_tell_disk_truth_and_availability() {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let home = Home::new("rows");
        let row = vision_row();
        assert_eq!(row["id"], "builtin-vision");
        assert_eq!(row["downloaded"], false);
        assert_eq!(row["available"], true, "the Qwen pair is pinned upstream");
        assert!(row["detail"]
            .as_str()
            .unwrap()
            .contains("Not downloaded yet"));
        // Only one of the two files is NOT downloaded.
        std::fs::write(home.dir.join("models").join(models::VISION), b"x").unwrap();
        assert_eq!(vision_row()["downloaded"], false);
        std::fs::write(home.dir.join("models").join(models::VISION_MMPROJ), b"x").unwrap();
        let row = vision_row();
        assert_eq!(row["downloaded"], true);
        assert_eq!(row["ready"], true);
        // nima's placeholder pin reads as not available, never as a Download.
        let q = quality_row();
        assert_eq!(q["id"], "quality");
        assert_eq!(q["available"], false);
        assert_eq!(q["downloaded"], false);
        assert!(q["detail"].as_str().unwrap().contains("Not available yet"));
    }

    #[test]
    fn the_progress_tracker_keeps_bytes_and_the_last_failure() {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        progress_begin("decider", 100);
        progress_note("decider", 40, 100);
        let p = install_progress().unwrap();
        assert_eq!(p["done"], 40);
        assert_eq!(p["total"], 100);
        assert_eq!(p["error"], Value::Null);
        progress_error("decider", "the download does not match its checksum");
        let p = install_progress().unwrap();
        assert_eq!(p["done"], 40, "a failure keeps how far it got");
        assert!(p["error"].as_str().unwrap().contains("checksum"));
        progress_begin("builtin-vision", 200);
        assert_eq!(install_progress().unwrap()["error"], Value::Null);
        progress_done();
        assert!(install_progress().is_none());
    }
}
