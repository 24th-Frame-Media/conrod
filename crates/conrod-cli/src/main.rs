//! conrod-cli selftest
//! conrod-cli cull <folder> [--profile motorsport|portrait|event|mix] [--limit N]
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

/// Install whichever models and tools are missing, showing progress as the app does.
fn install_models() -> i32 {
    let hub = TaskHub::new();
    let stop = std::sync::atomic::AtomicBool::new(false);
    let ids = conrod_engine::setup::everything();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let done = std::sync::atomic::AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            while !done.load(std::sync::atomic::Ordering::Relaxed) {
                for t in hub.snapshot().iter().filter(|t| t.state == State::Running) {
                    eprint!("\r{} {}%   ", t.label, t.done);
                }
                std::thread::sleep(Duration::from_millis(300));
            }
        });
        let result = conrod_engine::setup::ensure(&hub, &stop, &ids);
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        match result {
            Ok(()) => {
                eprintln!(
                    "
all models and tools are installed"
                );
                0
            }
            Err(e) => {
                eprintln!(
                    "
{e}"
                );
                1
            }
        }
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args
        .first()
        .is_some_and(|a| ["jobs", "identify", "write", "scan", "command"].contains(&a.as_str()))
    {
        let result = (|| -> Result<(), String> {
            let desktop =
                conrod_engine::desktop::Desktop::open(conrod_core::settings::data_root())?;
            let action = args[0].as_str();
            let (action, values) = match action {
                "jobs" => (action, json!({})),
                "identify" | "write" => (
                    action,
                    json!({"jobId": args.get(1).ok_or("Expected album id")?.parse::<i64>().map_err(|e| e.to_string())?, "dryRun": args.iter().any(|a| a == "--dry-run"), "embedInRaw": args.iter().any(|a| a == "--embed-in-raw")}),
                ),
                "scan" => (
                    action,
                    json!({"root": args.get(1).ok_or("Expected photo folder")?, "recursive": !args.iter().any(|a| a == "--no-recurse"), "stage": args.iter().position(|a| a == "--stage").and_then(|i| args.get(i+1)).map_or("cull", String::as_str)}),
                ),
                _ => (
                    args.get(1).ok_or("Expected action")?.as_str(),
                    serde_json::from_str(args.get(2).map_or("{}", String::as_str))
                        .map_err(|e| e.to_string())?,
                ),
            };
            println!("{}", desktop.dispatch(action, values)?);
            loop {
                let status = desktop.status();
                if status["activeJob"].is_null()
                    && status["operations"].as_array().is_none_or(Vec::is_empty)
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            if let Some(t) = desktop
                .hub
                .snapshot()
                .iter()
                .find(|t| t.state == State::Failed)
            {
                return Err(format!(
                    "{}: {}",
                    t.label,
                    t.error.as_deref().unwrap_or("failed")
                ));
            }
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    if args.first().map(String::as_str) == Some("install-models") {
        std::process::exit(install_models());
    }
    if args.first().map(String::as_str) == Some("selftest") {
        let (code, report) = conrod_engine::selftest::run();
        print!("{report}");
        std::process::exit(code);
    }
    if args.first().map(String::as_str) != Some("cull") || args.len() < 2 {
        eprintln!(
            "usage: conrod-cli selftest | conrod-cli install-models | conrod-cli cull <folder> [--profile motorsport|portrait|event|mix] [--limit N]"
        );
        std::process::exit(2);
    }
    let profile = args
        .iter()
        .position(|a| a == "--profile")
        .and_then(|i| args.get(i + 1))
        .map_or(ScanProfile::Motorsport, |p| ScanProfile::parse(p));
    // The first N frames in path order, for a quick benchmark.
    let limit = args
        .iter()
        .position(|a| a == "--limit")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse::<usize>().ok());
    let settings = Settings::load(&Settings::path());
    let hub = TaskHub::new();
    let stdout = Mutex::new(std::io::stdout());
    let started = Instant::now();
    let mut paths = conrod_engine::files(&PathBuf::from(&args[1]));
    paths.truncate(limit.unwrap_or(usize::MAX));
    let scan = conrod_engine::scan_files(paths, settings, profile, hub.clone(), move |result| {
        let Ok(f) = result else { return };
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
    });
    // Report from the hub, as the app's status area does, until nothing runs.
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let tasks = hub.snapshot();
        for t in tasks.iter().filter(|t| t.state == State::Failed) {
            if scan.is_finished() {
                eprintln!("{}: {}", t.label, t.error.clone().unwrap_or_default());
                std::process::exit(1);
            }
        }
        if scan.is_finished() && !tasks.iter().any(|t| t.label.starts_with("Culling")) {
            eprintln!("Scan ended before culling started");
            std::process::exit(1);
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
