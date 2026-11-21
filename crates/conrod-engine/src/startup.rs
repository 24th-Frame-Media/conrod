//! What the engine does between launch and "ready", told step by step so the splash
//! can show it instead of a fixed line.
//!
//! Only work that really runs at start-up is listed. The detector, face, similarity and
//! quality models are *not* loaded here: they load on the first scan, so the splash never
//! pretends to load them. What does run: opening the library (database and updates), putting
//! albums a crash left half-scanned back to "stopped", checking which model files are on disk
//! (and starting the download of any that are missing), finding ExifTool, and reading the
//! graphics card's memory.
//!
//! The shell sets a [`Tracker::set_sink`] that forwards each snapshot as a `startup-progress`
//! event. Steps 1 and 2 run before the window can listen, so the same snapshot is also
//! served by the `startup_status` command and the splash reads it on mount.

use crate::desktop::Desktop;
use conrod_core::models;
use conrod_io::assets::Asset;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// How many steps a normal start-up runs; the bar is `done / TOTAL`.
pub const TOTAL: usize = 5;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ModelRef {
    pub name: String,
    pub file: String,
    pub bytes: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Step {
    pub step: String,
    pub label: String,
    pub detail: Option<String>,
    pub model: Option<ModelRef>,
    pub error: Option<String>,
    pub ms: u64,
}

type Sink = Arc<dyn Fn(Value) + Send + Sync>;

#[derive(Default)]
struct Inner {
    current: Option<(Step, Instant)>,
    completed: Vec<Step>,
    ready: bool,
    sink: Option<Sink>,
}

pub struct Tracker {
    inner: Mutex<Inner>,
    total: usize,
}

impl Tracker {
    pub fn new(total: usize) -> Self {
        Tracker {
            inner: Mutex::new(Inner::default()),
            total,
        }
    }

    /// Forward every change to `sink` (and send the current state straight away).
    pub fn set_sink(&self, sink: impl Fn(Value) + Send + Sync + 'static) {
        let sink: Sink = Arc::new(sink);
        self.inner.lock().unwrap_or_else(|e| e.into_inner()).sink = Some(sink.clone());
        sink(self.snapshot());
    }

    /// Where start-up is now: the running step, the finished ones, `done / total`.
    pub fn snapshot(&self) -> Value {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let done = inner.completed.len().min(self.total);
        let (step, label, detail, model, error) = match &inner.current {
            Some((s, _)) => (
                s.step.as_str(),
                s.label.as_str(),
                s.detail.clone(),
                s.model.clone(),
                s.error.clone(),
            ),
            None if inner.ready => ("ready", "Ready", None, None, None),
            // Between two steps: keep showing the one that just ended.
            None => match inner.completed.last() {
                Some(s) => (
                    s.step.as_str(),
                    s.label.as_str(),
                    s.detail.clone(),
                    s.model.clone(),
                    s.error.clone(),
                ),
                None => ("starting", "Starting", None, None, None),
            },
        };
        json!({
            "step": step,
            "label": label,
            "detail": detail,
            "model": model,
            "error": error,
            "done": if inner.ready { self.total } else { done },
            "total": self.total,
            "ready": inner.ready,
            "completed": inner.completed,
        })
    }

    fn change(&self, edit: impl FnOnce(&mut Inner)) {
        let sink = {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            edit(&mut inner);
            inner.sink.clone()
        };
        if let Some(sink) = sink {
            sink(self.snapshot());
        }
    }

    /// Start a step. A step still running is closed as it stands.
    pub fn begin(&self, step: &str, label: &str, model: Option<ModelRef>) {
        self.change(|inner| {
            close(inner, None, None, None);
            inner.current = Some((
                Step {
                    step: step.into(),
                    label: label.into(),
                    detail: None,
                    model,
                    error: None,
                    ms: 0,
                },
                Instant::now(),
            ));
        });
    }

    /// End the running step, optionally rewording it by what was found.
    pub fn finish(&self, label: Option<&str>, detail: Option<String>, model: Option<ModelRef>) {
        self.change(|inner| close(inner, label, detail, model));
    }

    /// End the running step with a problem. Shown amber, never stops the start.
    pub fn fail(&self, error: impl Into<String>) {
        let error = error.into();
        self.change(|inner| {
            if let Some((step, _)) = inner.current.as_mut() {
                step.error = Some(error);
            }
            close(inner, None, None, None);
        });
    }

    /// Everything that runs at start-up has run.
    pub fn ready(&self) {
        self.change(|inner| {
            close(inner, None, None, None);
            inner.ready = true;
        });
    }
}

fn close(inner: &mut Inner, label: Option<&str>, detail: Option<String>, model: Option<ModelRef>) {
    if let Some((mut step, began)) = inner.current.take() {
        if let Some(label) = label {
            step.label = label.into();
        }
        if detail.is_some() {
            step.detail = detail;
        }
        if model.is_some() {
            step.model = model;
        }
        step.ms = began.elapsed().as_millis() as u64;
        inner.completed.push(step);
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

/// The part of start-up that follows opening the library: the model check, ExifTool, the
/// graphics card. The shell runs it on its own thread once the window exists. `install`
/// starts the download of whatever is missing (tests pass `false`).
pub fn run(d: &Arc<Desktop>, install: bool) {
    let t = &d.startup;
    let manifest = conrod_io::assets::manifest();

    t.begin("models", "Checking models", None);
    let wanted: Vec<&Asset> = manifest
        .iter()
        .filter(|a| !a.optional && !a.extract && !crate::setup::QUALITY.contains(&a.id()))
        .collect();
    let missing: Vec<&Asset> = wanted
        .iter()
        .copied()
        .filter(|a| !crate::setup::present(a))
        .collect();
    let biggest = missing.iter().max_by_key(|a| a.size).map(|a| ModelRef {
        name: crate::setup::name_of(a.id()).to_string(),
        file: a.name.clone(),
        bytes: Some(a.size),
    });
    if missing.is_empty() {
        t.finish(
            None,
            Some(format!(
                "All {} are on disk",
                plural(wanted.len(), "model", "models")
            )),
            None,
        );
    } else {
        t.finish(
            Some(
                format!(
                    "{} downloading",
                    plural(missing.len(), "model needs", "models need")
                )
                .as_str(),
            ),
            Some("Starting the download in the background".into()),
            biggest,
        );
    }

    t.begin("exiftool", "Looking for ExifTool", None);
    let exiftool_missing = if models::exiftool().is_some() {
        t.finish(
            Some("ExifTool found"),
            Some("Bundled with Conrod".into()),
            None,
        );
        false
    } else if crate::setup::exiftool_on_path() {
        t.finish(
            Some("ExifTool found"),
            Some("Using the copy already installed".into()),
            None,
        );
        false
    } else {
        let size = manifest.iter().find(|a| a.extract).map(|a| a.size);
        t.finish(
            Some("ExifTool is missing"),
            Some("Downloading it in the background".into()),
            Some(ModelRef {
                name: "ExifTool".into(),
                file: "exiftool".into(),
                bytes: size,
            }),
        );
        true
    };
    if install && (!missing.is_empty() || exiftool_missing) {
        let _ = crate::setup::install_missing(d);
    }

    t.begin("gpu", "Checking the graphics card", None);
    match conrod_decider::vram::query() {
        Some(g) => t.finish(
            None,
            Some(format!(
                "{}, {:.0} GB",
                g.name,
                g.total as f64 / f64::from(1u32 << 30)
            )),
            None,
        ),
        None => t.finish(None, Some("No dedicated graphics card found".into()), None),
    }
    t.ready();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_are_reported_in_order_and_the_snapshot_matches() {
        let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
        let t = Tracker::new(2);
        let log = seen.clone();
        t.set_sink(move |v| log.lock().unwrap().push(v));
        t.begin("a", "First", None);
        t.finish(None, Some("done".into()), None);
        t.begin(
            "b",
            "Second",
            Some(ModelRef {
                name: "Detector".into(),
                file: "d.onnx".into(),
                bytes: Some(7),
            }),
        );
        t.fail("nope");
        t.ready();
        let seen = seen.lock().unwrap();
        let steps: Vec<_> = seen.iter().map(|v| v["step"].as_str().unwrap()).collect();
        assert_eq!(steps, ["starting", "a", "a", "b", "b", "ready"]);
        let now = t.snapshot();
        assert_eq!(now["ready"], json!(true));
        assert_eq!(
            (now["done"].clone(), now["total"].clone()),
            (json!(2), json!(2))
        );
        assert_eq!(now["completed"][0]["step"], "a");
        assert_eq!(now["completed"][0]["detail"], "done");
        assert_eq!(now["completed"][1]["error"], "nope");
        assert_eq!(now["completed"][1]["model"]["file"], "d.onnx");
    }

    #[test]
    fn a_new_library_reports_its_steps_and_the_command_reads_them_back() {
        let lib = crate::testkit::Lib::new("startup");
        let before = lib.run("startup_status", json!({})).unwrap();
        let ids: Vec<_> = before["completed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["step"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, ["library", "albums"]);
        assert_eq!(before["ready"], json!(false));
        assert_eq!(before["done"], json!(2));
        run(lib.d(), false);
        let after = lib.run("startup_status", json!({})).unwrap();
        let ids: Vec<_> = after["completed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["step"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, ["library", "albums", "models", "exiftool", "gpu"]);
        assert_eq!(after["ready"], json!(true));
        assert_eq!(after["done"], json!(TOTAL));
        assert_eq!(after["total"], json!(TOTAL));
    }

    #[test]
    fn albums_a_crash_left_open_are_named() {
        let root =
            std::env::temp_dir().join(format!("conrod-startup-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        {
            let d = Desktop::open(root.clone()).unwrap();
            let db = crate::lock(&d.db);
            let job = conrod_store::create_job(
                &db,
                &root,
                Some("Half"),
                &json!(conrod_core::settings::Settings::default()),
            )
            .unwrap();
            db.execute("UPDATE jobs SET status='scanning' WHERE id=?", [job])
                .unwrap();
        }
        let d = Desktop::open(root.clone()).unwrap();
        let s = d.startup.snapshot();
        let albums = &s["completed"][1];
        assert_eq!(albums["label"], "Unfinished scans ready to resume");
        assert!(albums["detail"].as_str().unwrap().starts_with("1 album"));
        drop(d);
        let _ = std::fs::remove_dir_all(&root);
    }
}
