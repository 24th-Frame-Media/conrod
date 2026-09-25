//! A small library in a temp directory, for the tests of the album operations.
use crate::desktop::Desktop;
use crate::lock;
use conrod_core::settings::Settings;
use rusqlite::{params, types::FromSql, Connection, Params};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, MutexGuard};

pub struct Lib {
    d: Option<Arc<Desktop>>,
    pub root: PathBuf,
    pub job: i64,
}

impl Lib {
    pub fn new(tag: &str) -> Lib {
        static N: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "conrod-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        let d = Desktop::open(root.clone()).unwrap();
        let job = conrod_store::create_job(
            &lock(&d.db),
            &root,
            Some("Test"),
            &json!(Settings::default()),
        )
        .unwrap();
        Lib {
            d: Some(d),
            root,
            job,
        }
    }

    pub fn d(&self) -> &Arc<Desktop> {
        self.d.as_ref().unwrap()
    }

    pub fn run(&self, action: &str, args: Value) -> Result<Value, String> {
        self.d().dispatch(action, args)
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        lock(&self.d().db)
    }

    pub fn sql(&self, sql: &str, args: impl Params) {
        self.db().execute(sql, args).unwrap();
    }

    pub fn one<T: FromSql>(&self, sql: &str, args: impl Params) -> T {
        self.db().query_row(sql, args, |r| r.get(0)).unwrap()
    }

    /// A scanned 6000x4000 frame of this album, in `burst`.
    pub fn frame(&self, name: &str, burst: Option<i64>) -> i64 {
        let db = self.db();
        db.execute(
            "INSERT INTO images(job_id,path,status,width,height,burst_key) VALUES(?,?,'done',6000,4000,?)",
            params![self.job, self.root.join(name).to_string_lossy(), burst],
        )
        .unwrap();
        db.last_insert_rowid()
    }

    /// A subject well inside the frame with this rating (and sharpness).
    pub fn detection(&self, image: i64, region: &str, rating: f64) -> i64 {
        let db = self.db();
        let cls = if region == "vehicle" { "car" } else { region };
        db.execute(
            "INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,sharpness,rating,region_type) VALUES(?,1000,1000,3000,2000,?,0.9,?,?,?)",
            params![image, cls, rating, rating, region],
        )
        .unwrap();
        db.last_insert_rowid()
    }

    /// Wait for the album's background operation to end.
    pub fn wait_idle(&self) {
        for _ in 0..400 {
            if lock(&self.d().operations).is_empty() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        panic!("the background operation did not finish");
    }

    /// The last task of the hub whose label starts with `label`.
    pub fn task(&self, label: &str) -> Value {
        let tasks = self.d().status()["tasks"].clone();
        tasks
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|t| t["label"].as_str().is_some_and(|l| l.starts_with(label)))
            .cloned()
            .unwrap_or_else(|| panic!("no task {label}: {tasks}"))
    }
}

impl Drop for Lib {
    fn drop(&mut self) {
        self.d.take();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
