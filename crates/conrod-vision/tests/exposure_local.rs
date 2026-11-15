//! The exposure measure against real embedded previews: interiors with blown
//! windows (a condition report) and an exterior job, printing every field so
//! the verdicts can be judged rather than assumed.
//!
//! Read-only, and ignored by default because it needs the photographer's
//! drives: `cargo test -p conrod-vision --release --test exposure_local --
//! --ignored --nocapture`. It also writes `target/exposure_check.json`, which
//! `tools/exposure_sheet.py` turns into a contact sheet of the frames with the
//! most blown window area.

use conrod_vision::exposure::{measure, Exposure};
use conrod_vision::imageops::Rgb;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Instant;

const INTERIORS: &str = r"D:\24th Frame Media\01_Clients\Client A\2026-10-12_24-Sample-Ave-Sampletown-Condition-Report\01_RAW";
const EXTERIORS: &str =
    r"D:\24th Frame Media\02_Projects\2026-10-06_Bathurst-1000\01_RAW\2026-10-08_Thursday";
const SHEET_JSON: &str = r"E:\code\photo\conrod\target\exposure_check.json";
/// Where the six worst previews, and the manifest the contact sheet reads, go.
const SHEET_DIR: &str = r"E:\code\photo\conrod\target\exposure_check";

/// Frames of a directory, sorted, taken evenly across it: a job's first thirty
/// frames are one room, the spread is what matters.
fn frames(dir: &Path, want: usize) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut all: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("cr3") || e.eq_ignore_ascii_case("cr2"))
        })
        .collect();
    all.sort();
    if all.len() <= want {
        return all;
    }
    let step = all.len() as f64 / want as f64;
    (0..want)
        .map(|i| all[(i as f64 * step) as usize].clone())
        .collect()
}

fn line(name: &str, e: &Exposure, measure_us: u128) -> String {
    let sky = match e.sky_clip {
        Some(s) => format!("{s:.3}"),
        None => "-".to_string(),
    };
    let subject = match e.subject_clip {
        Some(s) => format!("{s:.3}"),
        None => "-".to_string(),
    };
    format!(
        "{name:>4}  mean {:.3}  hi {:.3}  sh {:.3}  sky {sky:>5}  win {:.3}  subj {subject:>5}  {measure_us:>5}us  {}",
        e.mean, e.highlight_clip, e.shadow_clip, e.window_clip, e.verdict
    )
}

#[test]
#[ignore = "reads the photographer's RAW drives"]
fn real_previews_measure_as_expected() {
    let mut paths = frames(Path::new(INTERIORS), 30);
    paths.extend(frames(Path::new(EXTERIORS), 10));
    if paths.is_empty() {
        eprintln!("no RAW frames on the drives; skipping");
        return;
    }

    let (mut worst, mut totals, mut rows) = (
        Vec::new(),
        vec![(0usize, 0usize, 0usize, 0usize); 2],
        Vec::new(),
    );
    let (mut slowest, mut decode_us) = (0u128, 0u128);
    for (i, path) in paths.iter().enumerate() {
        let Ok(frame) = conrod_io::raw::read(path) else {
            eprintln!("{i:>4}  unreadable {}", path.display());
            continue;
        };
        let started = Instant::now();
        let Ok(image) =
            Rgb::decode_jpeg(&frame.preview, 1).map(|img| img.orient(frame.orientation))
        else {
            eprintln!("{i:>4}  no decodable preview {}", path.display());
            continue;
        };
        decode_us += started.elapsed().as_micros();

        let started = Instant::now();
        let e = measure(&image, None);
        let measure_us = started.elapsed().as_micros();
        slowest = slowest.max(measure_us);
        println!("{}", line(&i.to_string(), &e, measure_us));

        let group = if paths[i].starts_with(INTERIORS) {
            0
        } else {
            1
        };
        let tally = &mut totals[group];
        match e.verdict {
            "ok" => tally.0 += 1,
            "highlights" => tally.1 += 1,
            "shadows" => tally.2 += 1,
            _ => tally.3 += 1,
        }
        rows.push(json!({
            "path": path.display().to_string(),
            "mean": e.mean,
            "highlight_clip": e.highlight_clip,
            "shadow_clip": e.shadow_clip,
            "sky_clip": e.sky_clip,
            "window_clip": e.window_clip,
            "verdict": e.verdict,
            "measure_us": measure_us,
        }));
        worst.push((e.window_clip, path.display().to_string()));
    }

    worst.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("\nworst window_clip:");
    for (clip, path) in worst.iter().take(6) {
        println!("  {clip:.3}  {path}");
    }
    println!(
        "\ninteriors ok/highlights/shadows/dark = {:?}, exteriors = {:?}",
        totals[0], totals[1]
    );
    println!(
        "{} frames, measure mean {}us, slowest {slowest}us, decode mean {}us",
        rows.len(),
        rows.iter()
            .map(|r| r["measure_us"].as_u64().unwrap_or(0))
            .sum::<u64>()
            / rows.len().max(1) as u64,
        decode_us / rows.len().max(1) as u128
    );

    std::fs::write(SHEET_JSON, serde_json::to_string_pretty(&rows).unwrap())
        .expect("write the contact-sheet manifest");

    // The six worst frames' previews, plus a manifest, for the contact sheet:
    // the previews live inside the RAWs, which Pillow cannot open.
    let dir = Path::new(SHEET_DIR);
    std::fs::create_dir_all(dir).expect("make the contact-sheet folder");
    let mut sheet = Vec::new();
    for (rank, (clip, path)) in worst.iter().take(6).enumerate() {
        let Ok(frame) = conrod_io::raw::read(Path::new(path)) else {
            continue;
        };
        let file = dir.join(format!("{:02}.jpg", rank + 1));
        std::fs::write(&file, &frame.preview).expect("write a preview");
        sheet.push(json!({
            "image": file.display().to_string(),
            "label": format!("{}  window {clip:.3}", Path::new(path).file_name().unwrap().to_string_lossy()),
        }));
    }
    std::fs::write(
        dir.join("sheet.json"),
        serde_json::to_string_pretty(&sheet).unwrap(),
    )
    .expect("write the sheet manifest");

    assert!(!rows.is_empty());
}
