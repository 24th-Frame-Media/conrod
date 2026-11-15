//! Keep-together sets over real folders.
//!
//! Ignored because it needs the photographer's originals, which are not in the
//! repository. Point it at one or more folders and it prints what was found:
//!
//! ```text
//! $env:CONROD_REAL_SET_DIR="D:\shoot\01_RAW;P:\shoot\02_RAW"
//! cargo test -p conrod-io --release --test sets_real -- --ignored --nocapture
//! ```

use conrod_core::sets::{find_sets, SetFrame, SetKind};
use conrod_io::raw;
use std::path::{Path, PathBuf};

/// Folders come from `CONROD_REAL_SET_DIR`, separated by `;`.
fn folders() -> Vec<PathBuf> {
    let Some(spec) = std::env::var_os("CONROD_REAL_SET_DIR") else {
        return Vec::new();
    };
    spec.to_string_lossy()
        .split(';')
        .map(str::trim)
        .filter(|s| s.len() > 1)
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
        .collect()
}

fn raw_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().and_then(|e| e.to_str()).is_some_and(|e| {
                    matches!(
                        e.to_ascii_lowercase().as_str(),
                        "cr3" | "cr2" | "jpg" | "jpeg"
                    )
                })
        })
        .collect();
    files.sort();
    files
}

fn frame(path: &Path) -> Option<SetFrame> {
    let meta = raw::read(path).ok()?;
    Some(SetFrame {
        taken: meta.taken(),
        camera: meta.camera("unknown"),
        exposure_bias: meta.exposure.exposure_bias,
        exposure_time: meta.exposure.exposure_time,
        f_number: meta.exposure.f_number,
        focal_length: meta.exposure.focal_length,
        focus_distance: meta.exposure.focus_distance,
        // The finder does not consult the crop yet; carried for the set
        // signature once the scan records dimensions.
        width: 0,
        height: 0,
    })
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[test]
#[ignore = "needs CONROD_REAL_SET_DIR pointing at a photographer's folder"]
fn report_sets_for_real_folders() {
    let folders = folders();
    if folders.is_empty() {
        eprintln!("CONROD_REAL_SET_DIR is unset or names no existing folder");
        return;
    }
    for dir in folders {
        let files = raw_files(&dir);
        if files.is_empty() {
            eprintln!("{}: no CR3/CR2/JPEG files", dir.display());
            continue;
        }
        let mut read: Vec<(SetFrame, PathBuf)> = Vec::new();
        let mut unreadable = 0;
        let mut present = [0usize; 5];
        for path in &files {
            match frame(path) {
                Some(f) => {
                    present[0] += usize::from(f.exposure_bias.is_some());
                    present[1] += usize::from(f.exposure_time.is_some());
                    present[2] += usize::from(f.f_number.is_some());
                    present[3] += usize::from(f.focal_length.is_some());
                    present[4] += usize::from(f.focus_distance.is_some());
                    read.push((f, path.clone()));
                }
                None => unreadable += 1,
            }
        }
        // Capture order, which is what the finder assumes.
        read.sort_by(|a, b| {
            a.0.taken
                .unwrap_or(f64::MAX)
                .total_cmp(&b.0.taken.unwrap_or(f64::MAX))
                .then_with(|| a.1.cmp(&b.1))
        });
        let paths: Vec<PathBuf> = read.iter().map(|(_, p)| p.clone()).collect();
        let frames: Vec<SetFrame> = read.into_iter().map(|(f, _)| f).collect();
        let sets = find_sets(&frames);
        let mut counts = [0usize; 3];
        let mut examples: Vec<(SetKind, usize, String, String)> = Vec::new();
        let mut i = 0;
        while i < sets.len() {
            let Some((key, kind)) = sets[i] else {
                i += 1;
                continue;
            };
            let start = i;
            while i < sets.len() && sets[i].is_some_and(|(k, _)| k == key) {
                i += 1;
            }
            counts[match kind {
                SetKind::Bracket => 0,
                SetKind::FocusStack => 1,
                SetKind::Interval => 2,
            }] += 1;
            if examples.len() < 12 {
                examples.push((kind, i - start, name(&paths[start]), name(&paths[i - 1])));
            }
        }
        eprintln!("\n{}", dir.display());
        if std::env::var_os("CONROD_REAL_SET_DUMP").is_some() {
            // Pair-by-pair against `exiftool -ExposureTime -FNumber
            // -ExposureCompensation -FocalLength`.
            for (path, frame) in paths.iter().zip(&frames).take(5) {
                eprintln!(
                    "  {} bias={:?} shutter={:?} f={:?} focal={:?} focus={:?} taken={:?}",
                    name(path),
                    frame.exposure_bias,
                    frame.exposure_time,
                    frame.f_number,
                    frame.focal_length,
                    frame.focus_distance,
                    frame.taken
                );
            }
        }
        eprintln!(
            "{} of {} files read ({unreadable} unreadable): bias {}, shutter {}, aperture {}, focal {}, focus distance {}",
            frames.len(),
            files.len(),
            present[0],
            present[1],
            present[2],
            present[3],
            present[4]
        );
        let in_sets = sets.iter().filter(|s| s.is_some()).count();
        eprintln!(
            "sets: {} bracket, {} focus stack, {} interval; {in_sets} of {} frames in a set",
            counts[0],
            counts[1],
            counts[2],
            frames.len()
        );
        for (kind, len, first, last) in examples {
            eprintln!("  {kind:?} x{len}: {first} .. {last}");
        }
    }
}
