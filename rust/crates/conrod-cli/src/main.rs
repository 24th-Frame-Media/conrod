//! conrod cull <folder> [--cpu] [--model path]
//!
//! The cull lane end to end: read each frame, find the vehicles, measure how
//! sharp each one is, and rate it -- one JSON line per frame on stdout, a
//! summary with the frame rate on stderr. No database yet; this is the lane
//! the app's scan will be built on, and what its speed is measured with.

use conrod_core::framing;
use conrod_io::raw;
use conrod_vision::detect::{DetectOptions, Detector, Device};
use conrod_vision::imageops::{Filter, Rgb};
use conrod_vision::sharpness;
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

const EXTENSIONS: [&str; 4] = ["cr3", "cr2", "jpg", "jpeg"];
const CROP_MIN_EDGE: usize = 320;
const CROP_MAX_EDGE: usize = 2048;

fn files(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        {
            out.push(path);
        }
    }
}

/// detect.py's `cut`: the padded box at native resolution, resized only to
/// keep the longest edge between the crop limits.
fn cut(frame: &Rgb, crop_box: [f64; 4]) -> Rgb {
    let [x1, y1, x2, y2] = crop_box.map(|v| v as usize);
    let crop = frame.crop(x1, y1, x2, y2);
    let longest = crop.width.max(crop.height);
    let scale = if longest < CROP_MIN_EDGE {
        CROP_MIN_EDGE as f64 / longest as f64
    } else if longest > CROP_MAX_EDGE {
        CROP_MAX_EDGE as f64 / longest as f64
    } else {
        return crop;
    };
    let w = ((crop.width as f64 * scale) as usize).max(1);
    let h = ((crop.height as f64 * scale) as usize).max(1);
    crop.resize(w, h, Filter::Lanczos)
}

fn cull(
    path: &Path,
    detector: &Mutex<Detector>,
    options: &DetectOptions,
) -> Result<serde_json::Value, String> {
    let frame = raw::read(path)?;
    let image = Rgb::decode_jpeg(&frame.preview, 1)?.orient(frame.orientation);
    let detections = detector.lock().unwrap().detect(&image, options)?;
    let (fw, fh) = (image.width, image.height);

    let mut vehicles = Vec::new();
    for det in &detections {
        let crop = cut(&image, det.crop_box);
        // The box inside the crop, scaled by whatever the crop was resized by.
        let span = det.crop_box[2] - det.crop_box[0];
        let scale = crop.width as f64 / span;
        let [cx, cy] = [det.crop_box[0], det.crop_box[1]];
        let inner = [
            (det.bbox[0] - cx) * scale,
            (det.bbox[1] - cy) * scale,
            (det.bbox[2] - cx) * scale,
            (det.bbox[3] - cy) * scale,
        ];
        let focus = sharpness::measure(&crop.to_gray(), Some(inner), None);
        let edges = framing::assess(Some(det.bbox), fw as i64, fh as i64);
        let rating = focus.score * edges.factor;
        vehicles.push(json!({
            "class": conrod_vision::detect::class_name(det.class_id),
            "conf": det.conf,
            "box": det.bbox,
            "sharpness": focus.score,
            "panning": focus.panning,
            "rating": rating,
            "stars": sharpness::stars_for(rating),
        }));
    }
    // A frame with nothing in it is judged as a whole.
    let whole = if detections.is_empty() {
        let focus = sharpness::measure(&image.to_gray(), None, None);
        focus
            .measured
            .then(|| json!({"sharpness": focus.score, "stars": sharpness::stars_for(focus.score)}))
    } else {
        None
    };
    Ok(json!({
        "path": path.display().to_string(),
        "camera": frame.camera("camera"),
        "taken": frame.taken(),
        "size": [fw, fh],
        "vehicles": vehicles,
        "whole": whole,
    }))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("cull") || args.len() < 2 {
        eprintln!("usage: conrod cull <folder> [--cpu] [--model path] [--limit n]");
        std::process::exit(2);
    }
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
    };
    let model = flag("--model").map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
            .join(".conrod/models/yolo11s-960.onnx")
    });
    let device = if args.iter().any(|a| a == "--cpu") {
        Device::Cpu
    } else {
        Device::Auto
    };

    let mut paths = Vec::new();
    files(Path::new(&args[1]), &mut paths);
    paths.sort();
    if let Some(limit) = flag("--limit").and_then(|v| v.parse().ok()) {
        paths.truncate(limit);
    }

    let detector = match Detector::load(&model, device) {
        Ok(d) => Mutex::new(d),
        Err(e) => {
            eprintln!("could not load the detector from {}: {e}", model.display());
            std::process::exit(1);
        }
    };
    let device = detector.lock().unwrap().device;
    let options = DetectOptions::default();
    // Each worker holds one decoded frame (~100 MB at 32 MP), so this bounds
    // memory as much as it spreads the decoding.
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    eprintln!(
        "{} frames, {workers} workers, detector on {device}",
        paths.len()
    );

    let start = Instant::now();
    let next = AtomicUsize::new(0);
    let stdout = Mutex::new(std::io::stdout());
    let failed = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                let Some(path) = paths.get(i) else { break };
                match cull(path, &detector, &options) {
                    Ok(line) => {
                        let _ = writeln!(stdout.lock().unwrap(), "{line}");
                    }
                    Err(e) => {
                        failed.fetch_add(1, Ordering::Relaxed);
                        eprintln!("{}: {e}", path.display());
                    }
                }
            });
        }
    });
    let secs = start.elapsed().as_secs_f64();
    eprintln!(
        "{} frames in {secs:.1} s: {:.1} frames/s, {} failed",
        paths.len(),
        paths.len() as f64 / secs,
        failed.load(Ordering::Relaxed)
    );
}
