//! conrod cull <folder> [--profile motorsport|portrait|event|mix]
//!
//! The cull lane from the command line: one JSON line per frame on stdout,
//! progress and the frame rate on stderr.

use conrod_core::profile::ScanProfile;
use conrod_core::settings::Settings;
use conrod_core::tasks::{State, TaskHub};
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("cull") || args.len() < 2 {
        eprintln!("usage: conrod cull <folder> [--profile motorsport|portrait|event|mix]");
        std::process::exit(2);
    }
    let profile = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1))
        .map_or(ScanProfile::Motorsport, |p| ScanProfile::parse(p));
    let settings = Settings::load(&Settings::path());
    let hub = TaskHub::new();
    let stdout = Mutex::new(std::io::stdout());
    let started = Instant::now();
    conrod_engine::scan(
        PathBuf::from(&args[1]),
        settings,
        profile,
        hub.clone(),
        move |f| {
            let subjects: Vec<_> = f
            .subjects
            .iter()
            .map(|s| {
                json!({"class": s.class, "conf": s.conf, "box": s.bbox, "sharpness": s.sharpness,
                       "panning": s.panning, "rating": s.rating, "stars": s.stars, "cull": s.cull_reason})
            })
            .collect();
            let line = json!({"path": f.path, "camera": f.camera, "taken": f.taken, "size": [f.size.0, f.size.1],
                          "stars": f.stars, "whole": f.whole, "vehicles": subjects});
            let _ = writeln!(stdout.lock().unwrap(), "{line}");
        },
    );
    // Report from the hub, as the app's status area does, until nothing runs.
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let tasks = hub.snapshot();
        for t in tasks.iter().filter(|t| t.state == State::Failed) {
            if t.label.starts_with("Loading") || t.label.starts_with("Culling") {
                eprintln!("{}: {}", t.label, t.error.clone().unwrap_or_default());
                std::process::exit(1);
            }
        }
        if let Some(t) = tasks.iter().find(|t| t.label.starts_with("Culling")) {
            if t.state == State::Done {
                let secs = started.elapsed().as_secs_f64();
                eprintln!(
                    "{} frames in {secs:.1} s: {:.1} frames/s",
                    t.total,
                    t.total as f64 / secs
                );
                break;
            }
        }
    }
}
