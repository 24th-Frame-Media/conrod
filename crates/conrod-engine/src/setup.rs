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
use std::sync::Arc;

pub const SCAN: &[&str] = &[models::DETECTOR];
pub const FACES: &[&str] = &[models::FACES];
pub const IDENTIFY_PLATES: &[&str] = &[models::PLATE_DETECTOR, models::PLATE_READER];
pub const TEXT: &[&str] = &[models::OCR_DETECTOR, models::OCR_RECOGNISER];
pub const SIMILARITY: &[&str] = &[models::SIMILARITY];
pub const WRITE: &[&str] = &["exiftool"];

/// The rows of the Setup card: what a person would name it, and the assets it needs.
const GROUPS: [(&str, &[&str]); 6] = [
    ("Detector", SCAN),
    ("Faces", FACES),
    ("Similarity", SIMILARITY),
    ("Plates", IDENTIFY_PLATES),
    ("Text (OCR)", TEXT),
    ("ExifTool", WRITE),
];

fn present(asset: &Asset) -> bool {
    if asset.extract {
        models::exiftool().is_some() || exiftool_on_path()
    } else {
        models::find(&asset.name).is_some()
            || (asset.id() == models::OCR_DETECTOR && models::ocr_dir().is_some())
    }
}

/// Whether `exiftool` runs from PATH (a dev machine; a release bundles its own).
fn exiftool_on_path() -> bool {
    std::process::Command::new("exiftool")
        .arg("-ver")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn name_of(id: &str) -> &str {
    GROUPS
        .iter()
        .find(|(_, ids)| ids.contains(&id))
        .map_or(id, |(name, _)| name)
}

/// The Setup card: one row per group, ready when everything in it is on disk.
pub fn rows() -> Value {
    let manifest = assets::manifest();
    let ready = |ids: &[&str]| {
        ids.iter()
            .all(|id| manifest.iter().find(|a| a.id() == *id).is_none_or(present))
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
                json!({"name": name, "file": ids[0], "ready": ok})
            })
            .collect(),
    )
}

/// Install whichever of `ids` is missing, one task each, and say what failed.
pub fn ensure(hub: &TaskHub, stop: &AtomicBool, ids: &[&str]) -> Result<(), String> {
    let manifest = assets::manifest();
    let mut failures = Vec::new();
    for id in ids {
        let Some(asset) = manifest.iter().find(|a| a.id() == *id) else {
            continue;
        };
        if present(asset) {
            continue;
        }
        let task = hub.start(format!("Installing {}", name_of(id)), 100);
        let mut report = |done: u64, total: u64| task.progress(done * 100 / total.max(1), 100);
        match assets::install(asset, &asset.folder(), stop, &mut report) {
            Ok(_) => task.finish(),
            Err(e) => {
                task.fail(e.clone());
                failures.push(e);
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

/// Every asset the manifest lists, for "install what is missing".
pub fn everything() -> Vec<String> {
    assets::manifest()
        .iter()
        .map(|a| a.id().to_string())
        .collect()
}

/// The Setup card's button: install all that is missing in the background.
pub fn install_missing(d: &Arc<Desktop>) -> Result<Value, String> {
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
        let ids = everything();
        let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
        let _ = ensure(&desktop.hub, &flag, &ids);
        lock(&desktop.operations).remove(&operation);
    });
    Ok(json!({"operation": key}))
}
