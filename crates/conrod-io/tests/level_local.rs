//! The tilt and keystone measure on the photographer's own frames: the two
//! shoots the research named, read from their RAW previews only.
//!
//! Local-only and read-only: `cargo test --release -p conrod-io -- --ignored`.
//! `CONROD_LEVEL_DIR` measures one folder instead of the two shoots. Prints one
//! line per frame and writes `target/level_check.csv` plus the measured planes
//! as `target/level_frames/NNN.pgm`, which the contact sheet script
//! (tools/level_check.py) reads. Nothing is written outside `target/` and no
//! RAW is modified.

use conrod_io::raw;
use conrod_vision::imageops::{Gray, Rgb};
use conrod_vision::level::{measure, Level};
use std::path::PathBuf;
use std::time::Instant;

/// Frames per shoot: enough to see a distribution, few enough to stay quick.
const PER_SHOOT: usize = 20;

const SHOOTS: [&str; 2] = [
    r"D:\24th Frame Media\01_Clients\Client A\2026-10-12_24-Sample-Ave-Sampletown-Condition-Report\01_RAW",
    r"P:\Client Work\Client A\2026\09\30",
];

fn out_dir() -> PathBuf {
    std::env::var_os("CONROD_LEVEL_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"E:\code\photo\conrod\target"))
}

/// Every RAW in a shoot, evenly thinned to `PER_SHOOT`: neighbours in a shoot
/// are often the same corner of the same room.
fn sample(dir: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        eprintln!("no such shoot: {dir}");
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("cr3") || e.eq_ignore_ascii_case("cr2"))
        })
        .collect();
    files.sort();
    if files.len() <= PER_SHOOT {
        return files;
    }
    let step = files.len() as f64 / PER_SHOOT as f64;
    (0..PER_SHOOT)
        .map(|i| files[(i as f64 * step) as usize].clone())
        .collect()
}

fn pgm(gray: &Gray) -> Vec<u8> {
    let mut out = format!("P5\n{} {}\n255\n", gray.width, gray.height).into_bytes();
    out.extend_from_slice(&gray.data);
    out
}

fn shoots() -> Vec<String> {
    match std::env::var("CONROD_LEVEL_DIR") {
        Ok(dir) if !dir.is_empty() => vec![dir],
        _ => SHOOTS.iter().map(|s| s.to_string()).collect(),
    }
}

#[test]
#[ignore = "real photos: cargo test --release -p conrod-io -- --ignored"]
fn tilt_and_keystone_on_real_frames() {
    let out = out_dir();
    let frames_dir = out.join("level_frames");
    let _ = std::fs::create_dir_all(&frames_dir);
    let mut csv = String::from("index,file,tilt_deg,converge_deg,lines,confidence,verdict,ms\n");
    let mut rows: Vec<(usize, String, f64, f64, Level, f64)> = Vec::new();

    let mut index = 0;
    let mut total_ms = 0.0;
    for dir in shoots() {
        for file in sample(&dir) {
            let started = Instant::now();
            let Ok(frame) = raw::read(&file) else {
                eprintln!("{}: unreadable", file.display());
                continue;
            };
            let Ok(rgb) = Rgb::decode_jpeg(&frame.preview, 1) else {
                eprintln!("{}: preview did not decode", file.display());
                continue;
            };
            let gray = rgb.orient(frame.orientation).to_gray();
            let read_ms = started.elapsed().as_secs_f64() * 1000.0;

            let started = Instant::now();
            let level = measure(&gray);
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            total_ms += ms;

            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            println!(
                "{name}: tilt {:+.2} converge {} lines {} confidence {:.2} {} ({ms:.1} ms measure, {read_ms:.0} ms preview) {}x{}",
                level.tilt_deg.unwrap_or(f64::NAN),
                level
                    .verticals_converge_deg
                    .map_or("-".to_string(), |c| format!("{c:.2}")),
                level.lines,
                level.confidence,
                level.verdict,
                gray.width,
                gray.height,
            );
            csv.push_str(&format!(
                "{index},{name},{:.3},{:.3},{},{:.4},{},{:.2}\n",
                level.tilt_deg.unwrap_or(f64::NAN),
                level.verticals_converge_deg.unwrap_or(f64::NAN),
                level.lines,
                level.confidence,
                level.verdict,
                ms,
            ));
            let _ = std::fs::write(frames_dir.join(format!("{index:03}.pgm")), pgm(&gray.thumbnail(768)));
            rows.push((
                index,
                name,
                level.tilt_deg.unwrap_or(f64::NAN),
                level.verticals_converge_deg.unwrap_or(f64::NAN),
                level,
                ms,
            ));
            index += 1;
        }
    }

    // The distribution, so the thresholds can be read against it.
    let mut tilts: Vec<f64> = rows
        .iter()
        .map(|r| r.2.abs())
        .filter(|t| t.is_finite())
        .collect();
    tilts.sort_by(f64::total_cmp);
    let mut converge: Vec<f64> = rows
        .iter()
        .map(|r| r.3)
        .filter(|c| c.is_finite())
        .collect();
    converge.sort_by(f64::total_cmp);
    let pct = |v: &[f64], p: f64| -> f64 {
        if v.is_empty() {
            return f64::NAN;
        }
        v[(((v.len() - 1) as f64) * p).round() as usize]
    };
    let verdicts = ["level", "tilted", "keystone", "unknown"];
    let counts: Vec<(&str, usize)> = verdicts
        .iter()
        .map(|v| (*v, rows.iter().filter(|r| r.4.verdict == *v).count()))
        .collect();
    println!(
        "{} frames: |tilt| median {:.2}, p90 {:.2}, max {:.2}; converge median {:.2}, p90 {:.2}, max {:.2}; mean measure {:.1} ms",
        rows.len(),
        pct(&tilts, 0.5),
        pct(&tilts, 0.9),
        tilts.last().copied().unwrap_or(f64::NAN),
        pct(&converge, 0.5),
        pct(&converge, 0.9),
        converge.last().copied().unwrap_or(f64::NAN),
        total_ms / rows.len().max(1) as f64,
    );
    println!("verdicts: {counts:?}");
    let path = out.join("level_check.csv");
    std::fs::write(&path, csv).unwrap();
    println!("wrote {} and {}", path.display(), frames_dir.display());

    assert!(rows.len() >= 20, "only {} frames read", rows.len());
}
