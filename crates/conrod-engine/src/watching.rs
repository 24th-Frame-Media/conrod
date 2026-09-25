//! Watching a folder: the engine's half.
//!
//! `conrod_io::watch::Watcher` answers "which files are new and finished being
//! written". This decides what to do about them: add them to the album and resume
//! it, the way a card still copying while the shoot is packed up needs. A watch is
//! a resume that happens on its own; nothing here scans anything itself.

use crate::commands::WatchArgs;
use crate::desktop::Desktop;
use crate::lock;
use conrod_io::watch::{self, Watcher};
use rusqlite::OptionalExtension;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const DEFAULT_INTERVAL: f64 = 60.0;
/// Where a watch is remembered in `settings.json` (the Python app's key and field
/// names, so one library can be opened by either app).
const SETTING: &str = "watch";

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchStatus {
    pub active: bool,
    pub folder: Option<String>,
    pub job_id: Option<i64>,
    pub recursive: bool,
    pub interval: f64,
    /// Frames handed to the album since the watch was turned on.
    pub added: u64,
    /// Unix seconds of the last look at the folder.
    pub checked: Option<f64>,
    pub message: String,
}

/// What the desktop holds: the public status, and the handle to end the loop.
#[derive(Default)]
pub struct Watching {
    status: WatchStatus,
    running: Option<(Arc<AtomicBool>, std::thread::Thread)>,
}

impl Watching {
    fn stop(&mut self) {
        if let Some((flag, thread)) = self.running.take() {
            flag.store(true, Ordering::Relaxed);
            thread.unpark(); // interruptible: turning it off is immediate
        }
    }
}

pub fn status(d: &Desktop) -> Value {
    json!(lock(&d.watching).status)
}

/// Turn folder monitoring on or off.
pub fn set(d: &Arc<Desktop>, a: &WatchArgs) -> Result<Value, String> {
    if !a.active {
        {
            let mut w = lock(&d.watching);
            w.stop();
            w.status = WatchStatus::default();
        }
        forget(d);
        return Ok(status(d));
    }
    let job = a
        .job_id
        .ok_or("A watch continues an album, so it needs one")?;
    // The folder defaults to the album's own, so the UI need only say which album.
    let root: Option<String> = d
        .reader
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .query_row("SELECT root FROM jobs WHERE id=?", [job], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    if root.is_none() {
        return Err("That album no longer exists".into());
    }
    let folder = a
        .path
        .clone()
        .or(root)
        .ok_or("That album no longer exists")?;
    // Canonical, as a scan stores it: `D:\Work` and `d:/work` must compare equal
    // against what the album holds.
    let folder = dunce::canonicalize(&folder).map_err(|_| "Not a folder".to_string())?;
    if !folder.is_dir() {
        return Err("Not a folder".into());
    }
    if a.interval
        .is_some_and(|n| !n.is_finite() || !(0.0..=86400.0).contains(&n))
    {
        return Err("Watch interval must be between 0 and 86400 seconds".into());
    }
    let interval = a
        .interval
        .unwrap_or(DEFAULT_INTERVAL)
        .max(watch::MIN_INTERVAL.as_secs_f64());
    let recursive = a.recursive.unwrap_or(true);
    start(d, folder.clone(), job, recursive, interval);
    {
        let mut settings = lock(&d.settings);
        // Remembered so a watch survives closing the app: the copy is still
        // running and Conrod is not.
        settings.extra.insert(
            SETTING.into(),
            json!({"path": folder, "job_id": job, "recursive": recursive, "interval": interval}),
        );
        settings
            .save(&d.root.join("settings.json"))
            .map_err(|e| e.to_string())?;
    }
    Ok(status(d))
}

/// Resume the watch the last session left on. An album that is gone ends it.
pub fn restore(d: &Arc<Desktop>) {
    let saved = lock(&d.settings).extra.get(SETTING).cloned();
    let Some(saved) = saved else { return };
    let (Some(path), Some(job)) = (saved["path"].as_str(), saved["job_id"].as_i64()) else {
        return forget(d);
    };
    if !album_exists(d, job) {
        return forget(d);
    }
    start(
        d,
        PathBuf::from(path),
        job,
        saved["recursive"].as_bool().unwrap_or(true),
        saved["interval"]
            .as_f64()
            .filter(|n| n.is_finite())
            .unwrap_or(DEFAULT_INTERVAL)
            .clamp(watch::MIN_INTERVAL.as_secs_f64(), 86400.0),
    );
}

fn start(d: &Arc<Desktop>, folder: PathBuf, job: i64, recursive: bool, interval: f64) {
    let stop = Arc::new(AtomicBool::new(false));
    let handle = {
        let (d, stop, folder) = (d.clone(), stop.clone(), folder.clone());
        std::thread::spawn(move || run(d, Watcher::new(folder, recursive), job, interval, stop))
    };
    let mut w = lock(&d.watching);
    w.stop(); // one loop per folder: two would both try to resume the same album
    w.running = Some((stop, handle.thread().clone()));
    w.status = WatchStatus {
        active: true,
        folder: Some(folder.to_string_lossy().into_owned()),
        job_id: Some(job),
        recursive,
        interval,
        message: "waiting".into(),
        ..WatchStatus::default()
    };
    d.hub
        .note(format!("watch: monitoring {}", folder.display()));
}

/// Take a watch out of `settings.json` as well as out of memory. Otherwise it comes
/// straight back on the next launch, rescanning an album that is long gone.
fn forget(d: &Desktop) {
    let mut settings = lock(&d.settings);
    if settings.extra.remove(SETTING).is_some() {
        let _ = settings.save(&d.root.join("settings.json"));
    }
}

fn album_exists(d: &Desktop, job: i64) -> bool {
    match d
        .reader
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .query_row("SELECT 1 FROM jobs WHERE id=?", [job], |_| Ok(()))
        .optional()
    {
        Ok(found) => found.is_some(),
        // Not knowing is not proof it is gone, and ending a watch on a locked
        // database would be worse than one wasted pass.
        Err(_) => true,
    }
}

/// The frames an album already holds, in the watcher's spelling.
fn known(d: &Desktop, job: i64) -> HashSet<String> {
    let db = lock(&d.reader);
    let Ok(mut stmt) = db.prepare("SELECT path FROM images WHERE job_id=?") else {
        return HashSet::new();
    };
    stmt.query_map([job], |r| r.get::<_, String>(0))
        .map(|rows| {
            rows.flatten()
                .map(|p| watch::key(std::path::Path::new(&p)))
                .collect()
        })
        .unwrap_or_default()
}

/// Apply `change` to the public status, but only while `stop` is still the current
/// watch: a loop that was turned off must not write over its successor.
fn update(d: &Desktop, stop: &Arc<AtomicBool>, change: impl FnOnce(&mut WatchStatus)) {
    let mut w = lock(&d.watching);
    if w.running
        .as_ref()
        .is_some_and(|(flag, _)| Arc::ptr_eq(flag, stop))
    {
        change(&mut w.status);
    }
}

fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn run(d: Arc<Desktop>, mut watcher: Watcher, job: i64, interval: f64, stop: Arc<AtomicBool>) {
    let started = Instant::now();
    let mut owed = false;
    loop {
        std::thread::park_timeout(Duration::from_secs_f64(interval));
        if stop.load(Ordering::Relaxed) {
            return;
        }
        if d.scanning() {
            continue; // a scan is using the folder
        }
        // Deleted albums were the whole of a real runaway in the Python app: with
        // the job gone every frame looked new on every pass, and a fresh full scan
        // started once a minute for ever. A watch with nothing to add to is
        // finished, not idle.
        if !album_exists(&d, job) {
            let mut w = lock(&d.watching);
            if w.running
                .as_ref()
                .is_some_and(|(flag, _)| Arc::ptr_eq(flag, &stop))
            {
                w.running = None;
                w.status = WatchStatus {
                    message: "The album this was watching has been deleted, so watching stopped"
                        .into(),
                    ..WatchStatus::default()
                };
                drop(w);
                d.hub
                    .note("watch: the album was deleted, so watching stopped");
                forget(&d);
            }
            return;
        }
        let found = watcher.poll(&known(&d, job), started.elapsed());
        update(&d, &stop, |s| s.checked = Some(now()));
        let n = found.len();
        if n > 0 {
            if let Err(e) = add(&d, job, &found) {
                d.hub.note(format!("watch: {e}"));
                update(&d, &stop, |s| s.message = e);
                continue; // not in the album, so offered again next pass
            }
            owed = true;
        }
        // Frames that are in the album but not yet scanned stay owed a resume until
        // one starts: the album may have been busy when they arrived.
        if owed {
            match d.resume_album(job) {
                Ok(()) => {
                    owed = false;
                    d.hub.note("watch: new frames, continuing the album");
                    update(&d, &stop, |s| {
                        s.added += n as u64;
                        s.message = format!("{n} new frame{}", if n == 1 { "" } else { "s" });
                    });
                }
                Err(e) => {
                    d.hub
                        .note(format!("watch: could not continue the album: {e}"));
                    update(&d, &stop, |s| s.added += n as u64);
                }
            }
        }
    }
}

fn add(d: &Desktop, job: i64, found: &[PathBuf]) -> Result<(), String> {
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    conrod_store::add_images(&tx, job, found).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}
