//! The cull lane over a folder: read, detect, measure, rate -- in parallel,
//! reporting through the TaskHub and handing each frame out as it finishes so
//! review can start while the scan is still going.
//!
//! Port of the cull half of `conrod/pipeline.py`'s frame loop. Identification
//! (plates, numbers, the vision model) is a separate, slower lane that never
//! holds this one up.

use conrod_core::framing;
use conrod_core::profile::{ScanProfile, ShootPreset};
use conrod_core::settings::Settings;
use conrod_core::tasks::TaskHub;
use conrod_io::raw;
use conrod_vision::detect::{self, DetectOptions, Detector, Device};
use conrod_vision::faces::FaceDetector;
use conrod_vision::imageops::{Filter, Rgb};
use conrod_vision::sharpness;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Condvar;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Lock, recovering the data instead of panicking if a prior holder
/// panicked while holding it: a poisoned lock's data is still fine here,
/// there's no invariant a partial write could have broken.
pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

pub mod analyze;
pub mod bench;
pub mod catalog;
pub mod commands;
pub mod desktop;
pub mod edits;
pub mod housekeeping;
pub mod library;
pub mod operations;
pub mod passes;
pub mod region_training;
pub mod rescore;
pub mod selftest;
pub mod setup;
#[cfg(test)]
mod testkit;
pub mod training;
pub mod updating;
pub mod watching;

pub const EXTENSIONS: [&str; 4] = ["cr3", "cr2", "jpg", "jpeg"];
/// Long edge of the thumbnail handed to the UI with each frame.
pub const THUMB_EDGE: usize = 420;

/// Every frame under `root`, sorted.
pub fn files(root: &Path) -> Vec<PathBuf> {
    files_with_recursion(root, true)
}

pub fn files_with_recursion(root: &Path, recursive: bool) -> Vec<PathBuf> {
    fn walk(
        dir: &Path,
        recursive: bool,
        out: &mut Vec<PathBuf>,
        seen: &mut std::collections::HashSet<PathBuf>,
    ) {
        let Ok(canonical) = dir.canonicalize() else {
            return;
        };
        if !seen.insert(canonical) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // The listing already says which entries are folders; `is_dir()`
            // is a stat each, 11 s over the 9,500 files of one shoot on USB.
            let is_dir = match entry.file_type() {
                Ok(t) if !t.is_symlink() => t.is_dir(),
                _ => path.is_dir(),
            };
            if is_dir {
                if recursive {
                    walk(&path, recursive, out, seen);
                }
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
    walk(
        root,
        recursive,
        &mut out,
        &mut std::collections::HashSet::new(),
    );
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
    pub features: Vec<f64>,
    pub heuristic: f64,
    pub region_type: &'static str,
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
    pub rating: f64,
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

fn options(settings: &Settings, _profile: ScanProfile) -> DetectOptions {
    let preset = ShootPreset::parse(&settings.scan_profile);
    let mut classes = Vec::new();
    if preset.wants_vehicles() {
        classes.extend(settings.vehicle_classes());
    }
    if preset.wants_people() {
        classes.push(detect::PERSON);
    }
    if preset.wants_pets() {
        classes.extend(detect::PETS);
    }
    DetectOptions {
        conf: settings.detect_conf as f32,
        classes,
        min_box_fraction: preset.min_box_fraction(settings.min_box_fraction),
        max_per_frame: preset.max_subjects(settings.max_vehicles_per_frame.max(1) as usize),
        crop_padding: settings.crop_padding,
        dominant_subject_fraction: settings.dominant_subject_fraction,
        ..DetectOptions::default()
    }
}

/// Faces and eyes found inside each person box, measured like any subject.
pub(crate) fn face_subjects(
    image: &Rgb,
    people: impl IntoIterator<Item = [f64; 4]>,
    detector: &Mutex<FaceDetector>,
    settings: &Settings,
    models: &std::collections::HashMap<String, conrod_core::ridge::SharpModel>,
) -> Result<Vec<Subject>, String> {
    let (fw, fh) = (image.width, image.height);
    let mut out = Vec::new();
    for bbox in people {
        let [x0, y0, x1, y1] = bbox.map(|v| v.max(0.0) as usize);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let crop = image.crop(x0, y0, x1.min(fw), y1.min(fh));
        for face in lock(detector).detect(&crop)? {
            let face_box = [
                face.bbox[0] + x0 as f64,
                face.bbox[1] + y0 as f64,
                face.bbox[2] + x0 as f64,
                face.bbox[3] + y0 as f64,
            ];
            let mut regions = vec![("face", face_box)];
            let side = (face.bbox[2] - face.bbox[0]) / 3.0;
            for (x, y) in face.eyes {
                let (x, y) = (x + x0 as f64, y + y0 as f64);
                regions.push((
                    "eye",
                    [
                        x - side / 2.0,
                        y - side / 2.0,
                        x + side / 2.0,
                        y + side / 2.0,
                    ],
                ));
            }
            for (kind, bbox) in regions {
                let [a, b, c, d] = [
                    bbox[0].clamp(0.0, fw as f64),
                    bbox[1].clamp(0.0, fh as f64),
                    bbox[2].clamp(0.0, fw as f64),
                    bbox[3].clamp(0.0, fh as f64),
                ]
                .map(|v| v as usize);
                if c <= a + 24 || d <= b + 24 {
                    continue;
                }
                let focus =
                    sharpness::measure(&image.crop(a, b, c, d).to_gray(), None, models.get(kind));
                if !focus.measured {
                    continue;
                }
                let stars = sharpness::stars_for(focus.score);
                let reason = if stars < (settings.auto_reject_below_stars as u8)
                    || (settings.cull_blurred && focus.score < settings.blurred_below)
                {
                    format!("{kind} soft")
                } else {
                    String::new()
                };
                out.push(Subject {
                    class: kind,
                    conf: face.score,
                    bbox: [a as f64, b as f64, c as f64, d as f64],
                    sharpness: focus.score,
                    panning: false,
                    sharp_end: focus.sharp_end,
                    rating: focus.score,
                    stars,
                    cull_reason: reason,
                    features: focus.features,
                    heuristic: if focus.learned {
                        focus.heuristic
                    } else {
                        focus.score
                    },
                    region_type: kind,
                });
            }
        }
    }
    Ok(out)
}

/// The frame's rating: the best subject of the first kind its priority finds.
/// Subjects are `(region_type, bbox, rating)`.
pub(crate) fn frame_rating(
    profile: ScanProfile,
    include_people: bool,
    subjects: &[(&str, [f64; 4], f64)],
    whole: Option<f64>,
) -> f64 {
    use conrod_core::profile::{main_subject, Subject as Kind};
    let kind_of = |region: &str| match region {
        "person" => Some(Kind::Person),
        "vehicle" => Some(Kind::Vehicle),
        _ => None,
    };
    let main = main_subject(subjects.iter().filter_map(|(region, [x0, y0, x1, y1], _)| {
        kind_of(region).map(|k| (k, (x1 - x0) * (y1 - y0)))
    }));
    profile
        .frame_priority(include_people, main)
        .iter()
        .find_map(|kind| {
            if *kind == Kind::WholeFrame {
                return whole;
            }
            let name = crate::passes::region_name(*kind);
            subjects
                .iter()
                .filter(|s| s.0 == name)
                .map(|s| s.2)
                .max_by(f64::total_cmp)
        })
        .unwrap_or(0.0)
}

pub fn cull_frame(
    path: &Path,
    detector: &Mutex<Detector>,
    settings: &Settings,
    profile: ScanProfile,
    models: &std::collections::HashMap<String, conrod_core::ridge::SharpModel>,
    faces: Option<&Mutex<FaceDetector>>,
) -> Result<FrameResult, String> {
    let preset = ShootPreset::parse(&settings.scan_profile);
    let frame = raw::read(path)?;
    let image = Rgb::decode_jpeg(&frame.preview, 1)?.orient(frame.orientation);
    // Resize outside the lock: only the network itself is serial.
    let input = detect::letterbox(&image);
    let found = detector
        .lock()
        .unwrap()
        .detect_letterboxed(input, &options(settings, profile))?;

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
        let kind = if det.class_id == detect::PERSON || detect::PETS.contains(&det.class_id) {
            "person"
        } else {
            "vehicle"
        };
        let mut focus = sharpness::measure(&crop.to_gray(), Some(inner), models.get(kind));
        if !preset.pan_compatible() {
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
            features: focus.features,
            region_type: if det.class_id == detect::PERSON || detect::PETS.contains(&det.class_id) {
                "person"
            } else {
                "vehicle"
            },
            heuristic: if focus.learned {
                focus.heuristic
            } else {
                focus.score
            },
        });
    }
    if let Some(detector) = faces {
        let people = found
            .iter()
            .filter(|d| d.class_id == detect::PERSON)
            .map(|d| d.bbox);
        subjects.extend(face_subjects(&image, people, detector, settings, models)?);
    }
    let whole = {
        let focus = sharpness::measure(&image.to_gray(), None, None);
        focus.measured.then_some(focus.score)
    };
    let rated: Vec<_> = subjects
        .iter()
        .map(|s| (s.region_type, s.bbox, s.rating))
        .collect();
    let rating = frame_rating(preset.profile(), settings.include_people, &rated, whole);
    let stars = sharpness::stars_for(rating);
    Ok(FrameResult {
        path: path.to_path_buf(),
        camera: frame.camera("camera"),
        taken: frame.taken(),
        size: (fw, fh),
        subjects,
        whole,
        stars,
        thumb: thumbnail(&image),
        rating,
    })
}

/// Where the detector model lives, per the data directory.
pub fn detector_model() -> PathBuf {
    conrod_core::models::expected(conrod_core::models::DETECTOR)
}

pub struct Scan {
    pub stop: Arc<AtomicBool>,
    pub finished: Arc<AtomicBool>,
    pub error: Arc<Mutex<Option<String>>>,
    pause: Arc<(Mutex<bool>, Condvar)>,
}

impl Scan {
    pub fn pause(&self, paused: bool) {
        *lock(&self.pause.0) = paused;
        self.pause.1.notify_all();
    }
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Relaxed);
        self.pause.1.notify_all();
    }
    pub fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }
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
    let finding = hub.start("Finding photos", 0);
    let paths = files(&root);
    finding.finish();
    scan_files(paths, settings, profile, hub, move |result| {
        if let Ok(frame) = result {
            on_frame(frame);
        }
    })
}

pub type ScanResult = Result<FrameResult, (PathBuf, String)>;

pub fn scan_files(
    paths: Vec<PathBuf>,
    settings: Settings,
    profile: ScanProfile,
    hub: TaskHub,
    on_frame: impl Fn(ScanResult) + Send + Sync + 'static,
) -> Scan {
    scan_files_filtered(paths, settings, profile, hub, |_| true, on_frame)
}

pub(crate) fn scan_files_filtered(
    paths: Vec<PathBuf>,
    settings: Settings,
    profile: ScanProfile,
    hub: TaskHub,
    should_scan: impl Fn(&Path) -> bool + Send + Sync + 'static,
    on_frame: impl Fn(ScanResult) + Send + Sync + 'static,
) -> Scan {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let finished = Arc::new(AtomicBool::new(false));
    let complete = finished.clone();
    let error = Arc::new(Mutex::new(None));
    let failure = error.clone();
    let pause = Arc::new((Mutex::new(false), Condvar::new()));
    let pausing = pause.clone();
    let loading = hub.start("Loading the detector", 0);
    std::thread::spawn(move || {
        struct Completion(Arc<AtomicBool>);
        impl Drop for Completion {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let _completion = Completion(complete);
        if paths.is_empty() || flag.load(Ordering::Relaxed) {
            loading.finish();
            return;
        }
        let mut needs = setup::SCAN.to_vec();
        if ShootPreset::parse(&settings.scan_profile).wants_faces(settings.include_people) {
            needs.extend(setup::FACES);
        }
        if let Err(e) = setup::ensure(&hub, &flag, &needs) {
            *lock(&failure) = Some(e.clone());
            loading.fail(format!("could not install the models: {e}"));
            return;
        }
        let detector = match Detector::load(&detector_model(), Device::Auto) {
            Ok(d) => {
                loading.finish();
                Mutex::new(d)
            }
            Err(e) => {
                *lock(&failure) = Some(e.clone());
                loading.fail(format!("could not load the detector: {e}"));
                return;
            }
        };
        let device = lock(&detector).device;
        let faces = if ShootPreset::parse(&settings.scan_profile)
            .wants_faces(settings.include_people)
        {
            let task = hub.start("Loading face detector", 0);
            match FaceDetector::load(&conrod_core::models::expected(conrod_core::models::FACES)) {
                Ok(detector) => {
                    task.finish();
                    Some(Mutex::new(detector))
                }
                Err(e) => {
                    *lock(&failure) = Some(e.clone());
                    task.fail(e);
                    return;
                }
            }
        } else {
            None
        };
        let models: std::collections::HashMap<_, _> = ["vehicle", "person", "face", "eye"]
            .into_iter()
            .filter_map(|region| load_region_model(region).map(|m| (region.to_string(), m)))
            .collect();
        let task = hub.start(
            format!("Culling ({}, {device})", profile.label()),
            paths.len() as u64,
        );
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        // Most of the machine, a fifth left for the UI. Past ~16 workers the
        // hyperthreads and E-cores add little: 10.1, 12.3, 13.8 and 14.4
        // frames/s at 8, 12, 16 and 20 workers on a 20-thread laptop.
        let workers = std::thread::available_parallelism()
            .map_or(4, |n| n.get() * 4 / 5)
            .clamp(2, 16);
        std::thread::scope(|s| {
            for _ in 0..workers {
                s.spawn(|| loop {
                    let mut paused = lock(&pausing.0);
                    while *paused && !flag.load(Ordering::Relaxed) {
                        task.paused(true);
                        paused = pausing
                            .1
                            .wait_timeout(paused, std::time::Duration::from_millis(100))
                            .unwrap()
                            .0;
                    }
                    drop(paused);
                    task.paused(false);
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(i) else { break };
                    if !should_scan(path) {
                        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                        task.progress(n as u64, paths.len() as u64);
                        continue;
                    }
                    match cull_frame(path, &detector, &settings, profile, &models, faces.as_ref()) {
                        Ok(result) => on_frame(Ok(result)),
                        Err(e) => {
                            on_frame(Err((path.clone(), e.clone())));
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
    Scan {
        stop,
        finished,
        error,
        pause,
    }
}

/// The photographer's learned sharpness model, if one is saved and current.
pub fn load_sharp_model() -> Option<conrod_core::ridge::SharpModel> {
    load_region_model("vehicle")
}

pub fn load_region_model(region: &str) -> Option<conrod_core::ridge::SharpModel> {
    let name = if region == "vehicle" {
        "sharpness.json".to_string()
    } else {
        format!("sharpness-{region}.json")
    };
    let path = conrod_core::settings::data_root().join("models").join(name);
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_finds_frames_in_subfolders_only_by_extension_sorted() {
        let root = std::env::temp_dir().join(format!("conrod-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("b/deep")).unwrap();
        for name in [
            "z.CR3",
            "a.jpg",
            "a.xmp",
            "b/m.cr2",
            "b/deep/n.JPEG",
            "b/notes.txt",
        ] {
            std::fs::write(root.join(name), b"").unwrap();
        }
        let found: Vec<_> = files(&root)
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(found, ["a.jpg", "b/deep/n.JPEG", "b/m.cr2", "z.CR3"]);
    }
}
