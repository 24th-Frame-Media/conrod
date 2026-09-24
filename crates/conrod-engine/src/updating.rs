//! Updating the app: the engine's half.
//!
//! `conrod_io::update` finds a newer release, downloads its installer and checks it
//! against the release's checksum. This runs that as a background task the status
//! popover already knows how to show, then asks the app to quit so the installer
//! can replace its files. It only ever runs because someone pressed the button.

use crate::desktop::Desktop;
use conrod_io::update::{self, Version};
use serde_json::{json, Value};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

const OPERATION: &str = "Updating Conrod";
/// The release notes shown beside the button.
const NOTES_CHARS: usize = 1200;

/// `CONROD_UPDATE_API` points the updater at a mirror (or a test server).
fn api() -> String {
    std::env::var("CONROD_UPDATE_API").unwrap_or_else(|_| update::REPO_API.into())
}

fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is semver")
}

/// Only a copy the installer put there can be updated by running a newer installer;
/// a portable unzip or a development build is told to download instead.
fn installable() -> bool {
    std::env::current_exe().is_ok_and(|exe| update::installed_by_installer(&exe))
}

/// Is there a newer release? Pre-releases count: until 1.0 they are the only ones.
pub fn check(force: bool) -> Value {
    let current = current();
    match update::latest_with_cache(&api(), &current, true, force) {
        Ok(found) => json!({
            "ok": true,
            "current": current.to_string(),
            "latest": found.as_ref().map_or_else(|| current.to_string(), |r| r.version.to_string()),
            "newer": found.is_some(),
            "tag": found.as_ref().map(|r| r.tag.clone()),
            "size": found.as_ref().map(|r| r.installer.size),
            "notes": found.as_ref().map(|r| r.notes.chars().take(NOTES_CHARS).collect::<String>()),
            "installable": installable(),
        }),
        Err(e) => json!({
            "ok": false,
            "current": current.to_string(),
            "error": e.to_string(),
        }),
    }
}

/// Download the newest installer and hand over to it. Returns at once; progress is
/// the "Updating Conrod" task, and the app quits when the installer is running.
pub fn install(d: &Arc<Desktop>) -> Result<Value, String> {
    if d.scanning() || !d.operations.lock().unwrap().is_empty() {
        return Err("Finish or stop current work before installing an update".into());
    }
    // Refuse before downloading 100 MB, not after.
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    if !update::installed_by_installer(&exe) {
        return Err(
            "This copy was not put here by the installer (a portable or development build), so it cannot update itself. Download the new version instead.".into(),
        );
    }
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut ops = d.operations.lock().unwrap();
        if ops.contains_key(OPERATION) {
            return Err("An update is already in progress".into());
        }
        ops.insert(OPERATION.into(), flag.clone());
    }
    let desktop = d.clone();
    std::thread::spawn(move || {
        let task = desktop.hub.start(OPERATION, 100);
        match run(&desktop, &exe, &flag, &task) {
            Ok(true) => {
                task.detail("restarting");
                task.finish();
                desktop.request_quit();
            }
            Ok(false) => task.finish(), // already up to date
            Err(e) => task.fail(e),
        }
        desktop.operations.lock().unwrap().remove(OPERATION);
    });
    Ok(json!({"operation": OPERATION}))
}

/// `Ok(true)`: the installer is running and the app should quit.
fn run(
    d: &Desktop,
    exe: &std::path::Path,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
) -> Result<bool, String> {
    task.detail("checking for the newest release");
    let Some(release) = update::latest(&api(), &current(), true)? else {
        task.detail("already up to date");
        return Ok(false);
    };
    task.detail(format!("downloading {}", release.version));
    let mut report = |done: u64, total: u64| task.progress(done * 100 / total.max(1), 100);
    let installer = update::download(&release, &d.root.join("updates"), stop, &mut report)?;
    task.detail(format!("installing {}", release.version));
    d.hub
        .note(format!("update: installing {}", release.version));
    update::apply(&installer, exe)?;
    Ok(true)
}
