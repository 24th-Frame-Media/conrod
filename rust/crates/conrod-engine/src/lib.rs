//! The cull lane over a folder: read, detect, measure, rate -- in parallel,
//! reporting through the TaskHub and handing each frame out as it finishes so
//! review can start while the scan is still going.
//!
//! Port of the cull half of `conrod/pipeline.py`'s frame loop. Identification
//! (plates, numbers, the vision model) is a separate, slower lane that never
//! holds this one up.

use conrod_core::framing;
use conrod_core::profile::ScanProfile;
use conrod_core::settings::Settings;
use conrod_core::tasks::TaskHub;
use conrod_io::raw;
use conrod_vision::detect::{self, DetectOptions, Detector, Device};
use conrod_vision::imageops::{Filter, Rgb};
use conrod_vision::sharpness;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub const EXTENSIONS: [&str; 4] = ["cr3", "cr2", "jpg", "jpeg"];
/// Long edge of the thumbnail handed to the UI with each frame.
pub const THUMB_EDGE: usize = 420;

/// Every frame under `root`, sorted.
pub fn files(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort();
    out
}

#[derive(Debug, Clone)]
pub struct Subject {
    pub class: &'static str,
    pub conf: f32,
    /// Frame pixels.
    pub bbox: [f64; 4],
    pub sharpness: f64,
    pub panning: bool,
    pub sharp_end: &'static str,
    /// Sharpness times the framing penalty: what the stars are read from.
    pub rating: f64,
    pub stars: u8,
    /// Why the cull would drop it, or empty.
    pub cull_reason: String,
}

#[derive(Debug, Clone)]
pub struct FrameResult {
    pub path: PathBuf,
    pub camera: String,
    pub taken: Option<f64>,
    pub size: (usize, usize),
    pub subjects: Vec<Subject>,
    /// The whole frame's own score, when there is no subject to judge.
    pub whole: Option<f64>,
    /// The frame's stars: its best subject's, else the whole frame's.
    pub stars: u8,
    pub thumb: Rgb,
}

/// detect.py's `cut`: native resolution, resized only to keep the longest
/// edge within the crop limits.
fn cut(frame: &Rgb, crop_box: [f64; 4], settings: &Settings) -> Rgb {
    let [x1, y1, x2, y2] = crop_box.map(|v| v as usize);
    let crop = frame.crop(x1, y1, x2, y2);
    let longest = crop.width.max(crop.height) as f64;
    let (lo, hi) = (settings.crop_min_edge as f64, settings.crop_max_edge as f64);
    let scale = if longest < lo {
        lo / longest
    } else if longest > hi {
        hi / longest
    } else {
        return crop;
    };
    let w = ((crop.width as f64 * scale) as usize).max(1);
    let h = ((crop.height as f64 * scale) as usize).max(1);
    crop.resize(w, h, Filter::Lanczos)
}

fn thumbnail(frame: &Rgb) -> Rgb {
    let scale = THUMB_EDGE as f64 / frame.width.max(frame.height) as f64;
    if scale >= 1.0 {
        return frame.clone();
    }
    let w = ((frame.width as f64 * scale).round() as usize).max(1);
    let h = ((frame.height as f64 * scale).round() as usize).max(1);
    // Box-shrink first: a 7000 px frame straight through Lanczos is slow and
    // the thumbnail does not need it.
    frame
        .resize_cv2_linear(w * 2, h * 2)
        .resize(w, h, Filter::Bilinear)
}

fn options(settings: &Settings, profile: ScanProfile) -> DetectOptions {
    let mut classes = Vec::new();
    if profile.wants_vehicles() {
        classes.extend(settings.vehicle_classes());
    }
    if profile.wants_people() && profile != ScanProfile::Motorsport {
        classes.push(detect::PERSON);
    }
    DetectOptions {
        conf: settings.detect_conf as f32,
        classes,
        min_box_fraction: settings.min_box_fraction,
        max_per_frame: settings.max_vehicles_per_frame.max(1) as usize,
        crop_padding: settings.crop_padding,
        dominant_subject_fraction: settings.dominant_subject_fraction,
        ..DetectOptions::default()
    }
}

pub fn cull_frame(
    path: &Path,
    detector: &Mutex<Detector>,
    settings: &Settings,
    profile: ScanProfile,
    model: Option<&conrod_core::ridge::SharpModel>,
) -> Result<FrameResult, String> {
    let frame = raw::read(path)?;
    let image = Rgb::decode_jpeg(&frame.preview, 1)?.orient(frame.orientation);
    let found = detector
        .lock()
        .unwrap()
        .detect(&image, &options(settings, profile))?;
    let (fw, fh) = (image.width, image.height);

    let mut subjects = Vec::new();
    for det in &found {
        let crop = cut(&image, det.crop_box, settings);
        let scale = crop.width as f64 / (det.crop_box[2] - det.crop_box[0]);
        let [cx, cy] = [det.crop_box[0], det.crop_box[1]];
        let inner = [
            (det.bbox[0] - cx) * scale,
            (det.bbox[1] - cy) * scale,
            (det.bbox[2] - cx) * scale,
            (det.bbox[3] - cy) * scale,
        ];
        let mut focus = sharpness::measure(&crop.to_gray(), Some(inner), model);
        if !profile.pan_compatible() {
            focus.panning = false;
        }
        let edges = framing::assess(Some(det.bbox), fw as i64, fh as i64);
        let rating = focus.score * edges.factor;
        let verdict = sharpness::rating_for(rating, settings.sharp_at, settings.blurred_below);
        let stars = sharpness::stars_for(rating);
        let cull_reason = if (settings.auto_reject_below_stars as u8) > stars {
            format!("{stars} star{}", if stars == 1 { "" } else { "s" })
        } else if settings.cull_blurred && sharpness::cullable(&focus, verdict) {
            if edges.cut_off() {
                framing::describe(&edges)
            } else {
                "too blurred".into()
            }
        } else {
            String::new()
        };
        subjects.push(Subject {
            class: detect::class_name(det.class_id),
            conf: det.conf,
            bbox: det.bbox,
            sharpness: focus.score,
            panning: focus.panning,
            sharp_end: focus.sharp_end,
            rating,
            stars,
            cull_reason,
        });
    }
    let whole = if found.is_empty() {
        let focus = sharpness::measure(&image.to_gray(), None, None);
        focus.measured.then_some(focus.score)
    } else {
        None
    };
    let stars = subjects
        .iter()
        .map(|s| s.stars)
        .max()
        .or(whole.map(sharpness::stars_for))
        .unwrap_or(1);
    Ok(FrameResult {
        path: path.to_path_buf(),
        camera: frame.camera("camera"),
        taken: frame.taken(),
        size: (fw, fh),
        subjects,
        whole,
        stars,
        thumb: thumbnail(&image),
    })
}

/// Where the detector model lives, per the data directory.
pub fn detector_model() -> PathBuf {
    conrod_core::settings::data_root().join("models/yolo11s-960.onnx")
}

pub struct Scan {
    pub stop: Arc<AtomicBool>,
}

/// Cull every frame under `root` on background threads. `on_frame` is called
/// from worker threads as each frame completes, in no particular order.
pub fn scan(
    root: PathBuf,
    settings: Settings,
    profile: ScanProfile,
    hub: TaskHub,
    on_frame: impl Fn(FrameResult) + Send + Sync + 'static,
) -> Scan {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::spawn(move || {
        let finding = hub.start("Finding photos", 0);
        let paths = files(&root);
        finding.finish();
        let loading = hub.start("Loading the detector", 0);
        let detector = match Detector::load(&detector_model(), Device::Auto) {
            Ok(d) => {
                loading.finish();
                Mutex::new(d)
            }
            Err(e) => {
                loading.fail(format!("could not load the detector: {e}"));
                return;
            }
        };
        let device = detector.lock().unwrap().device;
        let model = load_sharp_model();
        let task = hub.start(
            format!("Culling ({}, {device})", profile.label()),
            paths.len() as u64,
        );
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let workers = std::thread::available_parallelism()
            .map_or(4, |n| n.get())
            .min(8);
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(i) else { break };
                    match cull_frame(path, &detector, &settings, profile, model.as_ref()) {
                        Ok(result) => on_frame(result),
                        Err(e) => {
                            let t = hub.start(format!("Skipped {}", path.display()), 0);
                            t.fail(e);
                        }
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    task.progress(n as u64, paths.len() as u64);
                });
            }
        });
        if flag.load(Ordering::Relaxed) {
            task.fail("stopped");
        } else {
            task.finish();
        }
    });
    Scan { stop }
}

/// The photographer's learned sharpness model, if one is saved and current.
pub fn load_sharp_model() -> Option<conrod_core::ridge::SharpModel> {
    let path = conrod_core::settings::data_root().join("models/sharpness.json");
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}
