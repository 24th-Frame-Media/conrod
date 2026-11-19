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
use conrod_vision::imageops::{Filter, Gray, Rgb};
use conrod_vision::{exposure, expression, framing as vis_framing, level, sharpness};
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

/// Interchangeable model sessions that worker lanes borrow one at a time, so
/// lanes do not queue behind a single session. Every item is loaded from the
/// same files and keeps no state between calls.
pub struct Pool<T> {
    items: Vec<Mutex<T>>,
    turn: AtomicUsize,
}

impl<T> Pool<T> {
    pub(crate) fn new(items: Vec<T>) -> Self {
        assert!(!items.is_empty(), "a pool needs at least one item");
        Pool {
            items: items.into_iter().map(Mutex::new).collect(),
            turn: AtomicUsize::new(0),
        }
    }

    /// A free item if there is one, else wait on the next in turn.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        let start = self.turn.fetch_add(1, Ordering::Relaxed);
        for k in 0..self.items.len() {
            match self.items[(start + k) % self.items.len()].try_lock() {
                Ok(guard) => return guard,
                Err(std::sync::TryLockError::Poisoned(e)) => return e.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => {}
            }
        }
        lock(&self.items[start % self.items.len()])
    }
}

/// `n` face detectors (at least one); a failure past the first just makes
/// the pool smaller.
pub(crate) fn load_faces_pool(n: usize) -> Result<Pool<FaceDetector>, String> {
    let mut items = vec![load_faces()?];
    while items.len() < n {
        match load_faces() {
            Ok(d) => items.push(d),
            Err(_) => break,
        }
    }
    Ok(Pool::new(items))
}

/// `n` expression models, or `None` when they are not installed.
pub(crate) fn load_expression_pool(n: usize) -> Option<Pool<expression::ExpressionModel>> {
    let mut items = vec![load_expression()?];
    while items.len() < n {
        match load_expression() {
            Some(m) => items.push(m),
            None => break,
        }
    }
    Some(Pool::new(items))
}

/// Any error as the string the IPC layer sends.
pub(crate) fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Seconds since the Unix epoch, 0 if the clock reads before it.
pub(crate) fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

pub mod afsubject;
pub mod analyze;
pub mod bench;
#[cfg(test)]
mod blurtest;
pub mod catalog;
pub mod commands;
pub mod composition;
pub mod copies;
pub mod desktop;
#[cfg(test)]
mod edgetest;
pub mod edits;
#[cfg(test)]
mod evaltest;
pub mod housekeeping;
pub mod identify_live;
pub mod library;
mod looks;
pub mod operations;
#[cfg(test)]
mod pantest;
pub mod passes;
pub mod progress;
pub mod region_training;
pub mod rescore;
pub mod selftest;
pub mod setup;
#[cfg(test)]
mod sporttest;
pub mod taste;
#[cfg(test)]
mod testkit;
#[cfg(test)]
mod vlmeval;
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
    /// NIMA technical quality, 0..1; `None` when the model is off or absent.
    pub quality: Option<f64>,
    /// A face's OCEC `prob_open`, right eye then left; `None` where not judged.
    pub eyes_open: [Option<f32>; 2],
    /// Frame edges this subject's box runs off (0..=4). See `framing`.
    pub cut_sides: u8,
    /// The subject's box as a share of the frame.
    pub subject_frac: f32,
    /// Room ahead minus behind along the motion direction; `None` without it.
    pub lead_room: Option<f32>,
    /// The preset's framing score for this subject, 0..1. `None` when the
    /// preset has no framing policy.
    pub framing_score: Option<f32>,
    /// The camera's focus area is elsewhere, so this is not what the frame is
    /// of (see `afsubject`): it is stored as a bystander and drives nothing.
    pub bystander: bool,
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
    /// Measured exposure: the verdict is a flag, `dark_frame` the only cull.
    pub exposure: Option<exposure::Exposure>,
    /// EXIF exposure settings, so a regroup can find keep-together sets.
    pub exif: raw::Exposure,
    /// The lens as the file names it, when it names one.
    pub lens: Option<String>,
    /// The file's size on disk in bytes; 0 when it could not be read.
    pub file_size: i64,
    /// Tilt/convergence, for the presets where a crooked frame is worth
    /// flagging. `None` elsewhere.
    pub level: Option<level::Level>,
    /// Lowest eye-open and highest smile among the frame's faces.
    pub eye_open_min: Option<f32>,
    pub smile_max: Option<f32>,
    /// Rodeo/equestrian only: how much the animal is rearing under its rider.
    pub buck: Option<f64>,
    /// The whole-frame score of a subject-less frame before its in-focus-area
    /// rescue; `None` when the detector found a subject.
    pub whole_raw: Option<f64>,
    /// How much of a panning frame is sharp, 0..1; `None` when it is not a pan.
    pub pan_share: Option<f64>,
    /// The camera's focus area as fractions of the upright frame, read from the
    /// RAW's makernote; `None` for a body or file that records none.
    pub af: Option<conrod_io::af::AfArea>,
    /// The edge-blur rating inside that area when it is the frame's subject
    /// (see `afsubject`); `None` when a detection holds it or it is unusable.
    pub af_sharpness: Option<f64>,
}

fn crop_gray(frame: &Gray, crop_box: [f64; 4]) -> Gray {
    let [x1, y1, x2, y2] = crop_box.map(|v| v.max(0.0) as usize);
    let (x2, y2) = (x2.min(frame.width), y2.min(frame.height));
    let (x1, y1) = (x1.min(x2), y1.min(y2));
    let mut data = Vec::with_capacity((x2 - x1) * (y2 - y1));
    for y in y1..y2 {
        data.extend_from_slice(&frame.data[y * frame.width + x1..y * frame.width + x2]);
    }
    Gray::new(x2 - x1, y2 - y1, data)
}

/// Crop `grey` to `crop_box`, resizing only when upscaling below `crop_min_edge`
/// or when downscaling above `crop_max_edge` for a learned sharpness model
/// (`sharpness::measure` shrinks larger crops straight to `WORKING_EDGE`).
fn cut_gray(grey: &Gray, crop_box: [f64; 4], settings: &Settings, has_model: bool) -> Gray {
    let crop = crop_gray(grey, crop_box);
    let longest = crop.width.max(crop.height) as f64;
    if longest <= 0.0 {
        return crop;
    }
    let (lo, hi) = (settings.crop_min_edge as f64, settings.crop_max_edge as f64);
    let scale = if longest < lo {
        lo / longest
    } else if has_model && longest > hi {
        hi / longest
    } else {
        return crop;
    };
    let w = ((crop.width as f64 * scale) as usize).max(1);
    let h = ((crop.height as f64 * scale) as usize).max(1);
    crop.resize(w, h, Filter::Lanczos)
}

/// detect.py's `cut`: native resolution, resized only to keep the longest
/// edge within the crop limits.
#[doc(hidden)]
pub fn cut(frame: &Rgb, crop_box: [f64; 4], settings: &Settings) -> Rgb {
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

/// Which framing terms the preset cares about, if any. Composition decides
/// between frames in sport (lead room) and in portraits and events (size and
/// centring); elsewhere the score is measured but does not rank.
pub(crate) fn framing_policy(preset: ShootPreset) -> Option<vis_framing::FramingPolicy> {
    use ShootPreset::*;
    match preset {
        Motorsport | MotorsportBurnouts | MotorsportTrack | MotorsportRally | MotorsportMeet
        | SportField | SportCourt | SportRodeo | SportEquestrian | SportCycling | SportAction
        | WildlifeBirds | Aviation | AutoDealer => Some(vis_framing::FramingPolicy::sport()),
        Portrait | PortraitGroup | PortraitCouple | PortraitPets | Wedding | Event | EventShows
        | EventParties => Some(vis_framing::FramingPolicy::portrait()),
        _ => None,
    }
}

/// The motion a frame's own pan measure supplies to framing: the axis the
/// background is smeared along (degrees clockwise from image-right) on a frame
/// the pan flag calls a pan, and nothing on any other frame. A single frame
/// cannot say which way along that line the subject travelled -- motion blur is
/// symmetric -- so the axis is passed as an axis, and lead room is measured on
/// the side with the room rather than on a guessed direction.
pub(crate) fn pan_motion(
    panning: bool,
    pan: Option<conrod_vision::streaks::Pan>,
) -> Option<vis_framing::Motion> {
    panning.then(|| vis_framing::Motion::AxisDeg(pan.map_or(0.0, |p| p.axis_deg)))
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
    if preset.wants_animals() {
        classes.extend(detect::ANIMALS);
    }
    // Aviation asks for the airplane class (COCO 4) through its policy; the
    // detector reports it as a vehicle region, which is what an aircraft is.
    classes.extend(preset.extra_vehicle_classes().iter().copied());
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

/// YuNet, and OCEC with it when installed: without OCEC eyes go unjudged.
pub(crate) fn load_faces() -> Result<FaceDetector, String> {
    use conrod_core::models;
    let mut detector = FaceDetector::load(&models::expected(models::FACES))?;
    if let Some(eyes) = models::find(models::EYES) {
        let _ = detector.load_eyes(&eyes);
    }
    Ok(detector)
}

/// The blendshape expression model, when its two ONNX files are installed.
/// Optional by design: a shoot with no expression models is scanned exactly as
/// before, on the binary OCEC eye verdict.
pub(crate) fn load_expression() -> Option<expression::ExpressionModel> {
    use conrod_core::models;
    let landmarks = models::find(expression::LANDMARKS)?;
    let mut model = expression::ExpressionModel::load(&landmarks).ok()?;
    if let Some(blendshapes) = models::find(expression::BLENDSHAPES) {
        let _ = model.load_blendshapes(&blendshapes);
    }
    Some(model)
}

/// The cull reason of a face whose eyes are shut.
pub const EYES_CLOSED: &str = "eyes closed";

/// Every eye OCEC judged is shut, and it judged at least one.
pub(crate) fn eyes_closed(open: [Option<f32>; 2]) -> bool {
    let judged: Vec<f32> = open.into_iter().flatten().collect();
    !judged.is_empty()
        && judged
            .iter()
            .all(|p| *p < conrod_vision::faces::EYES_CLOSED_BELOW)
}

/// The faces and eyes of one frame, and the expression extremes across them.
#[derive(Debug, Default)]
pub(crate) struct Faces {
    pub subjects: Vec<Subject>,
    /// The lowest eye-open of any face: a portrait is as good as its blink.
    pub eye_open_min: Option<f32>,
    pub smile_max: Option<f32>,
}

/// Faces and eyes found inside each person box, measured like any subject.
#[allow(clippy::too_many_arguments)]
pub(crate) fn face_subjects(
    image: &Rgb,
    grey: &Gray,
    people: impl IntoIterator<Item = [f64; 4]>,
    detector: &Pool<FaceDetector>,
    expression: Option<&Pool<expression::ExpressionModel>>,
    settings: &Settings,
    strictness: &str,
    models: &std::collections::HashMap<String, conrod_core::ridge::SharpModel>,
) -> Result<Faces, String> {
    let (fw, fh) = (image.width, image.height);
    let mut out = Faces::default();
    for bbox in people {
        let [x0, y0, x1, y1] = bbox.map(|v| v.max(0.0) as usize);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let crop = image.crop(x0, y0, x1.min(fw), y1.min(fh));
        let found = detector.lock().detect(&crop)?;
        for face in found {
            let open = detector.lock().eyes_open(&crop, &face);
            // When the blendshape mesh agrees with the detector, its continuous
            // eye-open replaces the binary OCEC verdict.
            let judged = expression.and_then(|model| {
                let mut model = model.lock();
                model.read(&crop, &face).ok()
            });
            let closed = match judged {
                Some(ref read) if read.confidence >= expression::TRUST_CONFIDENCE => {
                    out.eye_open_min = Some(
                        out.eye_open_min
                            .map_or(read.eye_open_min(), |m: f32| m.min(read.eye_open_min())),
                    );
                    out.smile_max =
                        Some(out.smile_max.map_or(read.smile, |m: f32| m.max(read.smile)));
                    // The mesh confirms or rescues the OCEC verdict, never
                    // invents a cull: on a turned head it reads a shut eye in
                    // a frame the photographer delivered, while OCEC said
                    // open. A face OCEC did not judge takes the mesh's word.
                    let ocec_closed = eyes_closed(open);
                    let ocec_judged = open.iter().any(Option::is_some);
                    read.eye_open_min() < expression::CLOSED_EYE_BELOW
                        && (!ocec_judged || ocec_closed)
                }
                _ => eyes_closed(open),
            };
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
                let patch = crop_gray(grey, [a as f64, b as f64, c as f64, d as f64]);
                let mut focus = sharpness::measure(&patch, None, models.get(kind));
                if !focus.measured {
                    continue;
                }
                if !focus.learned {
                    let edges = conrod_vision::edgeblur::measure(grey, Some([a, b, c, d]));
                    if let Some(score) = sharpness::rating_from_blur(
                        edges.sigma_worst_at_reference(fw.max(fh)),
                        sharpness::BlurStrictness::parse(strictness),
                    ) {
                        focus.score = score;
                    }
                }
                let stars = sharpness::stars_for(focus.score);
                let reason = if stars < (settings.auto_reject_below_stars as u8)
                    || (settings.cull_blurred && focus.score < settings.blurred_below)
                {
                    format!("{kind} soft")
                } else {
                    String::new()
                };
                let reason = if kind == "face" && closed {
                    EYES_CLOSED.to_string()
                } else {
                    reason
                };
                out.subjects.push(Subject {
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
                    // Quality scoring is only wired up for the frame's main
                    // subjects (see `cull_frame`), not each face/eye crop.
                    quality: None,
                    eyes_open: if kind == "face" { open } else { [None, None] },
                    cut_sides: 0,
                    subject_frac: 0.0,
                    lead_room: None,
                    framing_score: None,
                    bystander: false,
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
            let scores = subjects.iter().filter(|s| s.0 == name).map(|s| s.2);
            // A portrait is as sharp as its softer eye: one eye out of focus
            // is a miss the sharper one must not hide.
            if *kind == Kind::Eye {
                scores.min_by(f64::total_cmp)
            } else {
                scores.max_by(f64::total_cmp)
            }
        })
        .unwrap_or(0.0)
}

/// What a frame with no detected subject is rated on.
#[doc(hidden)]
pub struct BareFrame {
    /// The rating such a frame gets on its own: its in-focus area, so a sharp
    /// detail over a deliberately soft background is not "blurred".
    pub score: Option<f64>,
    /// The whole frame's score with no rescue: what the frame is rated on when
    /// its burst shows the detector simply missed the subject.
    pub whole: Option<f64>,
}

/// One function rates every subject-less frame, so the scan and a re-score
/// cannot disagree about it.
#[doc(hidden)]
pub fn bare_frame(grey: &Gray, strictness: &str) -> BareFrame {
    let focus = sharpness::measure(grey, None, None);
    // The raw whole-frame score is kept before any rescue: a burst where the
    // other frames have a subject rates the missed frame on this.
    let whole = focus.measured.then_some(focus.score);
    let focal = sharpness::focal_score(grey);
    let score = if focus.panning {
        // A pan is rated on the picture, and its cap is the whole point: the
        // car is sharp, the frame around it is not, and "how much of the frame
        // is sharp" decides the stars. The detail-shot rescue must not undo
        // that -- the car's own sharp detail is exactly what focal_score finds.
        sharpness::subjectless_score(&focus, None)
    } else {
        // Nothing to judge: rate the frame on its in-focus area, so a sharp
        // detail against a deliberately blurred background is not "blurred".
        //
        // Flat subject matter -- an empty wall, a worktop, a ceiling -- has
        // too little texture for either tile ratio, and a sharp interior
        // frame reads as soft. The edge blur of the frame's sharpest
        // direction asks only whether any edge is sharp, which is the same
        // question `focal_score` asks of the tiles.
        let edges = conrod_vision::edgeblur::measure(grey, None);
        let edge = sharpness::rating_from_blur(
            conrod_vision::edgeblur::at_reference(edges.sigma_best, grey.width.max(grey.height)),
            sharpness::BlurStrictness::parse(strictness),
        );
        let mut score = sharpness::subjectless_score(&focus, focal);
        if let Some(edge) = edge {
            score = Some(score.map_or(edge, |s| s.max(edge)));
        }
        score
    };
    BareFrame { score, whole }
}

/// What a subject's edge blur reads, in native pixels at
/// [`conrod_vision::edgeblur::REFERENCE_EDGE`].
#[doc(hidden)]
#[derive(Debug, Clone, Copy)]
pub struct SubjectBlur {
    /// The whole detector box.
    pub whole: f64,
    /// The box's centre 60% (20%..80% on each axis); NaN when too small to
    /// measure.
    pub centre: f64,
    /// The worse of the two, which is what a subject's score is derived from.
    pub worst: f64,
}

/// What a subject is scored on: its edge blur, in native pixels at
/// [`conrod_vision::edgeblur::REFERENCE_EDGE`], taken as the worse of the whole
/// detector box and the box's centre 60% (20%..80% of the box on each axis,
/// clamped to the frame).
///
/// The subject's body sits at the centre of its box; a sharp wall, road or
/// crowd behind the subject sits in the box's margins. Reading only the whole
/// box lets that background vouch for a blurred subject (a heavily blurred car
/// against a sharp sponsor banner measured 0.98 that way), so the centre is
/// measured too and the larger sigma -- the worse of the two -- is the answer.
/// This can only lower a subject whose centre is blurred: the centre of a sharp
/// subject is sharp too, so the subject is never penalised for the margin it
/// happens to sit against. A centre too small to measure (NaN, or a region that
/// clamps empty) keeps the whole-box value. A patch with no edges above the
/// noise floor (`EdgeBlur::flat`, contrast under
/// [`conrod_vision::edgeblur::MIN_CONTRAST`]) is unmeasured rather than sharp:
/// otherwise its relative floor pins sigma at the minimum and a completely
/// blurred subject is praised for it.
#[doc(hidden)]
pub fn subject_blur(grey: &Gray, bbox: [f64; 4], long_edge: usize) -> SubjectBlur {
    let (fw, fh) = (grey.width, grey.height);
    let px = |v: f64, edge: usize| (v.max(0.0) as usize).min(edge);
    let (x0, y0) = (px(bbox[0], fw), px(bbox[1], fh));
    let (x1, y1) = (px(bbox[2], fw).max(x0), px(bbox[3], fh).max(y0));
    let between = |a: usize, b: usize, t: f64| a + ((b - a) as f64 * t) as usize;
    // A patch with no edges above the noise floor (a completely blurred or
    // featureless subject) must read as *unmeasured*, not as perfectly sharp:
    // its relative edge floor otherwise pins sigma to the minimum.
    let edges = conrod_vision::edgeblur::measure(grey, Some([x0, y0, x1, y1]));
    let whole = if edges.flat() {
        f64::NAN
    } else {
        edges.sigma_worst_at_reference(long_edge)
    };
    let edges = conrod_vision::edgeblur::measure(
        grey,
        Some([
            between(x0, x1, 0.2),
            between(y0, y1, 0.2),
            between(x0, x1, 0.8),
            between(y0, y1, 0.8),
        ]),
    );
    let centre = if edges.flat() {
        f64::NAN
    } else {
        edges.sigma_worst_at_reference(long_edge)
    };
    // `f64::max` ignores a NaN, so the centre keeps the whole-box value when it
    // could not be measured, and an unmeasured whole box leaves the centre.
    let worst = if centre.is_nan() {
        whole
    } else {
        whole.max(centre)
    };
    SubjectBlur {
        whole,
        centre,
        worst,
    }
}

/// A pan's frame rating says how the picture reads: capped by how much of the
/// frame is sharp. The subjects keep their own ratings, which is what picks the
/// keeper of a burst.
pub(crate) fn pan_capped(rating: f64, panning: bool, share: Option<f64>) -> f64 {
    match share {
        Some(share) if panning => rating.min(sharpness::pan_share_cap(share)),
        _ => rating,
    }
}

/// A condition report keeps nearly everything: only a subject that is clearly
/// out of focus goes, and never one that is merely soft or a held pan.
/// `rating_word` is `rating_for`'s ("poor" is blurred), as `cullable` reads it.
fn deliver_most_reason(
    cull_blurred: bool,
    focus: &sharpness::Sharpness,
    rating_word: &str,
) -> String {
    if cull_blurred && sharpness::cullable(focus, rating_word) {
        "too blurred".into()
    } else {
        String::new()
    }
}

/// The file's own details, as opposed to what is in it: the lens the file
/// names (its EXIF tags first, the exposure walk as a fallback) and its size on
/// disk. A frame whose file cannot be stat'ed keeps a size of 0 rather than
/// failing a scan that has already read it.
fn source_details(path: &Path, frame: &raw::Frame) -> (Option<String>, i64) {
    let lens = ["LensModel", "LensID"]
        .into_iter()
        .find_map(|tag| frame.tags.get(tag).and_then(|v| v.as_str()))
        .map(str::trim)
        .filter(|lens| !lens.is_empty())
        .map(str::to_string)
        .or_else(|| frame.exposure.lens.clone());
    let file_size = std::fs::metadata(path).map_or(0, |meta| meta.len() as i64);
    (lens, file_size)
}

#[allow(clippy::too_many_arguments)]
pub fn cull_frame(
    path: &Path,
    detector: &Mutex<Detector>,
    settings: &Settings,
    profile: ScanProfile,
    models: &std::collections::HashMap<String, conrod_core::ridge::SharpModel>,
    faces: Option<&Pool<FaceDetector>>,
    // The blendshape expression model; `None` when its files are absent.
    expression: Option<&Pool<expression::ExpressionModel>>,
    quality: Option<&Mutex<conrod_vision::quality::QualityModel>>,
    step: impl Fn(&'static str),
) -> Result<FrameResult, String> {
    let preset = ShootPreset::parse(&settings.scan_profile);
    step("read");
    let frame = raw::read(path)?;
    let image = Rgb::decode_jpeg(&frame.preview, 1)?.orient(frame.orientation);
    // Resize outside the lock: only the network itself is serial.
    let input = detect::letterbox(&image);
    step("detect:wait");
    let found = {
        let mut detector = lock(detector);
        step("detect");
        detector.detect_letterboxed(input, &options(settings, profile))?
    };

    let (fw, fh) = (image.width, image.height);
    // Native-resolution luma, once: the subject's blur is measured on it.
    let grey = image.to_gray();
    step("sharpness");

    // The preset's own strictness applies until someone chooses one for the
    // album; this is the difference between a burnouts shoot and a portrait.
    let policy = preset.policy();
    let strictness = settings.effective_blur();
    // Level only where a crooked frame is worth flagging, and framing only
    // where the preset says composition decides between frames.
    let level = policy.level.then(|| level::measure(&grey));
    let tilt = level.as_ref().and_then(|l| l.tilt_deg.map(|t| t as f32));
    let framing_policy = framing_policy(preset);
    let boxes: Vec<[f64; 4]> = found.iter().map(|det| det.bbox).collect();
    // The camera's focus area, read from the makernote already in memory. A
    // focus grid holds only when the detector found nothing but small objects
    // (`afsubject::usable`): its union is a depth-of-field map, not a point.
    let af_area = frame.af_area((fw as u32, fh as u32));
    let af = af_area
        .map(|a| afsubject::FocusArea {
            area: [
                a.x1 * fw as f64,
                a.y1 * fh as f64,
                a.x2 * fw as f64,
                a.y2 * fh as f64,
            ],
            kind: a.kind,
        })
        .filter(|af| afsubject::usable(*af, (fw, fh), &boxes));
    // The area a detected subject's score may be checked against: usable, and
    // big enough that a body's single small cell is not a second opinion. The
    // same area still rescues the frame below.
    let af_opinion_area = af.and_then(|af| afsubject::opinion(af, (fw, fh), &boxes));

    let mut subjects = Vec::new();
    for det in &found {
        let kind = if det.class_id == detect::PERSON || detect::ANIMALS.contains(&det.class_id) {
            "person"
        } else {
            "vehicle"
        };
        let others: Vec<[f64; 4]> = boxes.iter().copied().filter(|b| *b != det.bbox).collect();
        let crop_grey = cut_gray(&grey, det.crop_box, settings, models.contains_key(kind));
        let scale = crop_grey.width as f64 / (det.crop_box[2] - det.crop_box[0]);
        let [cx, cy] = [det.crop_box[0], det.crop_box[1]];
        let inner = [
            (det.bbox[0] - cx) * scale,
            (det.bbox[1] - cy) * scale,
            (det.bbox[2] - cx) * scale,
            (det.bbox[3] - cy) * scale,
        ];
        let mut focus = sharpness::measure(&crop_grey, Some(inner), models.get(kind));
        // The subject's score is the edge-blur estimate on native pixels, taken
        // as the worse of its whole box and the box's centre 60%: a sharp wall
        // or banner behind the subject sits in the margins and must not vouch
        // for a blurred body. The tile measure above still supplies background,
        // bands, features and the held-pan flag. A loaded learned model keeps
        // priority, and an unmeasurable box keeps the tile score.
        if !focus.learned {
            let tile = focus.score;
            let blur = subject_blur(&grey, det.bbox, fw.max(fh));
            // The centre is consulted only when the tiles already suspect
            // softness: a texture-poor but sharp body reads soft at the centre
            // (its edge floor is region-relative), and asking it unasked demoted
            // a crisp frame from 1.000 to 0.797.
            let worst = if tile < 0.80 { blur.worst } else { blur.whole };
            let detection =
                sharpness::rating_from_blur(worst, sharpness::BlurStrictness::parse(&strictness));
            // The camera's focus area, when it sits on this subject and is big
            // enough to measure one with, is a second opinion that can only
            // lower the score (see `afsubject`).
            let af_opinion = af_opinion_area
                .filter(|af| afsubject::holds_middle(det.bbox, *af))
                .and_then(|af| afsubject::rate_box(&grey, af, &strictness));
            if let Some(score) = rescore::second_opinion(detection, af_opinion) {
                // The tile score is not used to overrule this one: it is measured
                // on the downscaled crop, so it is not comparable with the native
                // edge census. It only decides whether the centre was consulted.
                focus.score = score;
            }
        }
        let edges = framing::assess(Some(det.bbox), fw as i64, fh as i64);
        // Portraits and wildlife close-ups routinely touch the frame edge
        // without being miscropped; only vehicles take the edge penalty.
        let factor = if kind == "vehicle" { edges.factor } else { 1.0 };
        let rating = focus.score * factor;
        // A held pan is a *sharp subject over a streaked background*, not a
        // blurred subject: the camera moved with the subject, so the subject
        // stays sharp while everything else is one set of parallel streaks.
        // `conrod_vision::streaks` measures that on the ring around the box
        // (pan p10 0.826, static p90 0.787 on hand-labelled frames), and the
        // preset still says whether pans are expected for this shoot.
        //
        // "Sharp subject" is the album's own not-blurred verdict: a held pan
        // routinely scores 0.65-0.82 on the subject's blur alone (the wheels
        // and the far side still move), so requiring `sharp_at` would refuse
        // 8 of 48 labelled pans before the background is even looked at.
        // What the flag must never do is excuse a subject that is a blur miss,
        // and that is `blurred_below`.
        let subject_not_blurred =
            sharpness::verdict_for(focus.score, settings.sharp_at, settings.blurred_below)
                != "blurred";
        // One measurement answers both questions: the coherence the pan flag
        // reads, and the axis (the line the background is smeared along) that
        // lead room is measured on. A panning frame is the one place the frame
        // itself says where the subject is going, so framing gets that line
        // instead of `None` for it (an axis, not an arrow: see `pan_motion`).
        let pan = conrod_vision::streaks::background_pan(&grey, det.bbox);
        focus.panning = preset.pan_compatible()
            && subject_not_blurred
            && pan.is_some_and(|p| p.coherence >= conrod_vision::streaks::PAN_COHERENCE);
        let motion = pan_motion(focus.panning, pan);
        let measured =
            vis_framing::framing([fw as f64, fh as f64], det.bbox, &others, motion, tilt);
        let framing_score =
            framing_policy.map(|policy| vis_framing::framing_score(&measured, &policy));
        let verdict = sharpness::rating_for(rating, settings.sharp_at, settings.blurred_below);
        let stars = sharpness::stars_for(rating);
        let cull_reason = if policy.deliver_most {
            deliver_most_reason(settings.cull_blurred, &focus, verdict)
        } else if (settings.auto_reject_below_stars as u8) > stars {
            format!("{stars} star{}", if stars == 1 { "" } else { "s" })
        } else if settings.cull_blurred && sharpness::cullable(&focus, verdict) {
            if kind == "vehicle" && edges.cut_off() {
                framing::describe(&edges)
            } else {
                "too blurred".into()
            }
        } else {
            String::new()
        };
        // A failed read costs only the tiebreak, never the cull.
        let quality_score = quality.and_then(|q| {
            let crop = cut(&image, det.crop_box, settings);
            lock(q).score(&crop).ok()
        });
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
            region_type: kind,
            heuristic: if focus.learned {
                focus.heuristic
            } else {
                focus.score
            },
            quality: quality_score,
            eyes_open: [None, None],
            cut_sides: measured.cut_sides,
            subject_frac: measured.subject_frac,
            lead_room: measured.lead_room,
            framing_score,
            bystander: false,
        });
    }
    let mut eye_open_min = None;
    let mut smile_max = None;
    if let Some(detector) = faces {
        step("faces");
        let people = found
            .iter()
            .filter(|d| d.class_id == detect::PERSON)
            .map(|d| d.bbox);
        let read = face_subjects(
            &image,
            &grey,
            people,
            detector,
            expression,
            settings,
            &strictness,
            models,
        )?;
        subjects.extend(read.subjects);
        eye_open_min = read.eye_open_min;
        smile_max = read.smile_max;
    }
    // Rodeo and equestrian: the rider and the animal are one subject, and how
    // much the animal is rearing ranks the frames of a ride.
    let mut buck = None;
    if matches!(
        preset,
        ShootPreset::SportRodeo | ShootPreset::SportEquestrian
    ) {
        step("sharpness");
        let people: Vec<conrod_core::sport::Det> = found
            .iter()
            .filter(|d| d.class_id == detect::PERSON)
            .map(|d| (d.class_id, d.conf, d.bbox))
            .collect();
        let animals: Vec<conrod_core::sport::Det> = found
            .iter()
            .filter(|d| conrod_core::sport::ANIMALS.contains(&d.class_id))
            .map(|d| (d.class_id, d.conf, d.bbox))
            .collect();
        if let Some(rider) = conrod_core::sport::rider_on_animal(&people, &animals) {
            buck = Some(conrod_core::sport::buck_score(
                &rider,
                animals[rider.animal].2,
                people[rider.person].2,
            ));
        }
        if let Some(subject) = conrod_core::sport::rodeo_subject(&people, &animals) {
            let bbox = subject.bbox();
            let region = [
                (bbox[0].max(0.0) as usize).min(fw),
                (bbox[1].max(0.0) as usize).min(fh),
                (bbox[2].max(0.0) as usize).min(fw),
                (bbox[3].max(0.0) as usize).min(fh),
            ];
            if region[2] > region[0] + 8 && region[3] > region[1] + 8 {
                let blur = conrod_vision::edgeblur::measure(&grey, Some(region));
                let score = sharpness::rating_from_blur(
                    blur.sigma_worst_at_reference(fw.max(fh)),
                    sharpness::BlurStrictness::parse(&strictness),
                )
                .unwrap_or(0.0);
                let stars = sharpness::stars_for(score);
                let reason = if (settings.auto_reject_below_stars as u8) > stars {
                    format!("{stars} stars")
                } else {
                    String::new()
                };
                subjects.push(Subject {
                    class: match subject {
                        conrod_core::sport::RodeoSubject::Rider { .. } => "rider",
                        conrod_core::sport::RodeoSubject::Animal { .. } => "animal",
                        conrod_core::sport::RodeoSubject::Person { .. } => "person",
                    },
                    conf: 1.0,
                    bbox,
                    sharpness: score,
                    panning: false,
                    sharp_end: "subject",
                    rating: score,
                    stars,
                    cull_reason: reason,
                    features: Vec::new(),
                    heuristic: score,
                    region_type: "person",
                    quality: None,
                    eyes_open: [None, None],
                    cut_sides: 0,
                    subject_frac: 0.0,
                    lead_room: None,
                    framing_score: None,
                    bystander: false,
                });
            }
        }
    }
    step("sharpness");
    // A frame with no subject is rated by the same function a re-score rates it
    // by, so the two cannot differ; a frame with subjects keeps the plain
    // measured whole-frame score it always had.
    let bare = subjects.is_empty().then(|| bare_frame(&grey, &strictness));
    let whole = match &bare {
        Some(bare) => bare.score,
        None => {
            let focus = sharpness::measure(&grey, None, None);
            focus.measured.then_some(focus.score)
        }
    };
    // The frame as the detector and the whole-frame path read it, measured
    // before the camera's word can park any detection, so a rescue below has a
    // real floor to stand on.
    let detected_panning = subjects
        .iter()
        .any(|s| matches!(s.region_type, "vehicle" | "person") && s.panning);
    let share_measured = detected_panning
        .then(|| sharpness::frame_sharp_share(&grey))
        .flatten();
    let rated: Vec<_> = subjects
        .iter()
        .map(|s| (s.region_type, s.bbox, s.rating))
        .collect();
    let detected = pan_capped(
        frame_rating(preset.profile(), settings.include_people, &rated, whole),
        detected_panning,
        share_measured,
    );
    // Where the camera focused outside every detection, the focus area is the
    // subject: the detections are bystanders and the frame is rescued to its
    // score, never dragged down by it (see `afsubject::rescue`).
    let af_sharpness = afsubject::apply(&mut subjects, &grey, af, &strictness).subject();
    let rating = afsubject::rescue(detected, af_sharpness);
    // A pan's frame rating reads the picture, not the car; the subjects keep
    // their own ratings, which is what picks the keeper of a burst. A frame the
    // camera focused elsewhere is not a pan, and the share is the same
    // measurement the detected rating used.
    let panning = subjects
        .iter()
        .any(|s| !s.bystander && matches!(s.region_type, "vehicle" | "person") && s.panning);
    let pan_share = panning.then_some(share_measured).flatten();
    let stars = sharpness::stars_for(rating);
    // Exposure on the main subject, so a backlit rider's clipped whites are
    // told from a blown sky. A frame the focus area rates is of that area.
    let subject_box = match (af_sharpness, af) {
        (Some(_), Some(af)) => Some([
            af.area[0] / fw as f64,
            af.area[1] / fh as f64,
            af.area[2] / fw as f64,
            af.area[3] / fh as f64,
        ]),
        _ => subjects
            .iter()
            .filter(|s| s.region_type != "face" && s.region_type != "eye")
            .max_by(|a, b| {
                let area = |s: &Subject| (s.bbox[2] - s.bbox[0]) * (s.bbox[3] - s.bbox[1]);
                area(a).total_cmp(&area(b))
            })
            .map(|s| {
                [
                    s.bbox[0].max(0.0) / fw as f64,
                    s.bbox[1].max(0.0) / fh as f64,
                    (s.bbox[2].min(fw as f64)) / fw as f64,
                    (s.bbox[3].min(fh as f64)) / fh as f64,
                ]
            }),
    };
    let exposure = Some(exposure::measure(&image, subject_box));
    let (lens, file_size) = source_details(path, &frame);
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
        exposure,
        exif: frame.exposure.clone(),
        lens,
        file_size,
        level,
        eye_open_min,
        smile_max,
        buck,
        whole_raw: bare.as_ref().and_then(|bare| bare.whole),
        pan_share,
        af: af_area,
        af_sharpness,
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
    scan_files(paths, settings, profile, hub, None, move |result| {
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
    progress: Option<Arc<crate::progress::Progress>>,
    on_frame: impl Fn(ScanResult) + Send + Sync + 'static,
) -> Scan {
    scan_files_filtered(paths, settings, profile, hub, progress, |_| true, on_frame)
}

/// The cull lane. `progress`, when given, is told which stage and step every
/// worker is on, and gets the frame counts the status area cannot show.
pub(crate) fn scan_files_filtered(
    paths: Vec<PathBuf>,
    settings: Settings,
    profile: ScanProfile,
    hub: TaskHub,
    progress: Option<Arc<crate::progress::Progress>>,
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
        let detectors: Vec<Mutex<Detector>> = {
            let sessions = conrod_core::settings::clamp_workers(
                settings.detect_workers,
                conrod_core::settings::worker_caps().detect,
                1,
            ) as usize;
            let mut loaded = Vec::with_capacity(sessions);
            for _ in 0..sessions {
                match Detector::load(&detector_model(), Device::Auto) {
                    Ok(d) => loaded.push(Mutex::new(d)),
                    Err(e) => {
                        *lock(&failure) = Some(e.clone());
                        loading.fail(format!("could not load the detector: {e}"));
                        return;
                    }
                }
            }
            loading.finish();
            loaded
        };
        let device = lock(&detectors[0]).device;
        let detector_name = detector_model()
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let faces =
            if ShootPreset::parse(&settings.scan_profile).wants_faces(settings.include_people) {
                let task = hub.start("Loading face detector", 0);
                match load_faces_pool(detectors.len()) {
                    Ok(pool) => {
                        task.finish();
                        Some(pool)
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
        // Expression is optional: without the landmarker and blendshape files
        // the scan silently keeps the binary OCEC eye verdict.
        let expression = if faces.is_some() {
            load_expression_pool(detectors.len())
        } else {
            None
        };
        let models: std::collections::HashMap<_, _> = ["vehicle", "person", "face", "eye"]
            .into_iter()
            .filter_map(|region| load_region_model(region).map(|m| (region.to_string(), m)))
            .collect();
        // Opt-in and best-effort: an absent or unconvertible model just means
        // no quality score, never a failed scan.
        let quality = settings
            .use_quality_score
            .then(|| {
                let path = conrod_core::models::expected(conrod_core::models::QUALITY);
                path.is_file()
                    .then(|| conrod_vision::quality::QualityModel::load(&path, true).ok())
                    .flatten()
                    .map(Mutex::new)
            })
            .flatten();
        let task = hub.start(
            format!("Culling ({}, {device})", profile.label()),
            paths.len() as u64,
        );
        // Most of the machine, a fifth left for the UI. Past ~16 workers the
        // hyperthreads and E-cores add little: 10.1, 12.3, 13.8 and 14.4
        // frames/s at 8, 12, 16 and 20 workers on a 20-thread laptop. The
        // album's `workers` setting overrides it within that cap.
        let workers = settings.cull_workers() as usize;
        let workers = workers.min(paths.len().max(1));
        if let Some(p) = &progress {
            for stage in ["read", "detect", "sharpness", "faces"] {
                p.total(stage, paths.len() as u64);
            }
            p.phase("detect");
            p.describe("detect", detector_name.clone(), device);
            p.describe("read", "camera preview", "CPU");
            p.describe(
                "sharpness",
                if models.is_empty() {
                    "edge blur"
                } else {
                    "edge blur + learned ridge"
                },
                "CPU",
            );
            p.describe(
                "faces",
                if faces.is_some() {
                    "YuNet 2023mar + OCEC"
                } else {
                    "off for this shoot"
                },
                "CPU",
            );
            p.describe("group", "burst gaps + subject framing", "CPU");
            p.describe("pick", "keepers by pass", "CPU");
        }
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        std::thread::scope(|s| {
            for lane in 0..workers {
                // Borrows, not moves: every worker shares the same pools. `lane`
                // is copied so its borrow does not outlive the loop iteration.
                let (paths, settings, detectors, models, faces, expression, quality) = (
                    &paths,
                    &settings,
                    &detectors,
                    &models,
                    &faces,
                    &expression,
                    &quality,
                );
                let (progress, on_frame, should_scan, task) =
                    (&progress, &on_frame, &should_scan, &task);
                let (pausing, flag, next, done, hub) = (&pausing, &flag, &next, &done, &hub);
                s.spawn(move || loop {
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
                    let file = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    let step = |step: &'static str| {
                        let Some(p) = progress.as_ref() else { return };
                        match step {
                            "detect:wait" => {
                                p.waiting("read", lane, "detect", "lock", step, &file);
                            }
                            "detect" => p.busy("read", lane, "detect", step, &file),
                            "sharpness" => p.busy("read", lane, "sharpness", step, &file),
                            "faces" => p.busy("read", lane, "faces", step, &file),
                            _ => p.busy("read", lane, "read", step, &file),
                        }
                    };
                    let detector = &detectors[lane % detectors.len()];
                    match cull_frame(
                        path,
                        detector,
                        settings,
                        profile,
                        models,
                        faces.as_ref(),
                        expression.as_ref(),
                        quality.as_ref(),
                        step,
                    ) {
                        Ok(result) => on_frame(Ok(result)),
                        Err(e) => {
                            on_frame(Err((path.clone(), e.clone())));
                            let t = hub.start(format!("Skipped {}", path.display()), 0);
                            t.fail(e);
                        }
                    }
                    if let Some(p) = progress.as_ref() {
                        for stage in ["read", "detect", "sharpness", "faces"] {
                            p.advance(stage, 1);
                        }
                        p.idle("read", lane);
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    task.progress(n as u64, paths.len() as u64);
                });
            }
        });
        if flag.load(Ordering::Relaxed) {
            task.cancel();
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
    fn a_condition_report_culls_only_clearly_blurred_subjects() {
        let focus = sharpness::Sharpness::default();
        assert_eq!(deliver_most_reason(true, &focus, "poor"), "too blurred");
        assert_eq!(
            deliver_most_reason(true, &focus, "fair"),
            "",
            "soft is kept"
        );
        assert_eq!(deliver_most_reason(true, &focus, "good"), "");
        assert_eq!(
            deliver_most_reason(false, &focus, "poor"),
            "",
            "culling off"
        );
        let held = sharpness::Sharpness {
            panning: true,
            ..focus
        };
        assert_eq!(
            deliver_most_reason(true, &held, "poor"),
            "",
            "a held pan stays"
        );
    }

    #[test]
    fn a_scanned_frame_carries_the_files_size_and_a_bare_jpeg_names_no_lens() {
        let dir = std::env::temp_dir().join(format!("conrod-frame-details-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bare.jpg");
        // A real file on disk, encoded the way a scan's thumbnail is: the size
        // comes from its metadata, not from anything the JPEG carries.
        let pixels: Vec<u8> = (0..8 * 8 * 3).map(|i| (i * 7) as u8).collect();
        crate::desktop::save_jpeg(&Rgb::new(8, 8, pixels), &path).unwrap();
        let frame = raw::read(&path).expect("a written JPEG reads back");
        assert_eq!(frame.exposure.lens, None, "a synthetic JPEG names no lens");
        assert_eq!(frame.exposure.iso, None);
        let (lens, file_size) = source_details(&path, &frame);
        assert_eq!(lens, None);
        assert_eq!(file_size, std::fs::metadata(&path).unwrap().len() as i64);
        assert!(file_size > 0, "the frame records the file it came from");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Deterministic value noise, for a synthetic background.
    fn hash(x: usize, y: usize) -> u32 {
        let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 13;
        h = h.wrapping_mul(0xC2B2_AE35);
        h ^ (h >> 16)
    }

    /// A smooth background of blurred value noise with one small checkerboard
    /// patch left sharp: the detail shot a whole-frame score calls blurred. A
    /// straight gradient cannot stand in for it: every one of its tiles is a
    /// coherent ramp, which the pan measure reads as a pan.
    fn detail_shot(edge: usize, fraction: f64) -> Gray {
        let noise = |x: usize, y: usize| 20.0 + 215.0 * f64::from(hash(x, y) % 256) / 255.0;
        let mut cur: Vec<f64> = (0..edge * edge)
            .map(|i| noise((i % edge) / 2, (i / edge) / 2))
            .collect();
        // Two 7x7 box passes: out of focus, cheaply.
        for _ in 0..2 {
            let mut next = cur.clone();
            for y in 0..edge {
                for x in 0..edge {
                    let mut sum = 0.0;
                    for dy in 0..7 {
                        for dx in 0..7 {
                            let px = (x + dx).saturating_sub(3).min(edge - 1);
                            let py = (y + dy).saturating_sub(3).min(edge - 1);
                            sum += cur[py * edge + px];
                        }
                    }
                    next[y * edge + x] = sum / 49.0;
                }
            }
            cur = next;
        }
        let mut data: Vec<u8> = cur.iter().map(|v| v.clamp(0.0, 255.0) as u8).collect();
        let side = (edge as f64 * fraction.sqrt()) as usize;
        let corner = (edge - side) / 2;
        for y in corner..corner + side {
            for x in corner..corner + side {
                let on = ((x - corner) / 6 + (y - corner) / 6).is_multiple_of(2);
                data[y * edge + x] = if on { 235 } else { 20 };
            }
        }
        Gray::new(edge, edge, data)
    }

    #[test]
    fn a_pan_rating_is_capped_by_how_much_of_the_frame_is_sharp() {
        assert_eq!(
            pan_capped(0.95, false, Some(0.1)),
            0.95,
            "a still frame is not capped"
        );
        assert_eq!(
            pan_capped(0.95, true, None),
            0.95,
            "a pan with no share is not capped"
        );
        let capped = pan_capped(0.95, true, Some(0.1));
        assert_eq!(capped, sharpness::pan_share_cap(0.1));
        assert_eq!(sharpness::stars_for(capped), 3);
        assert_eq!(
            pan_capped(0.60, true, Some(0.1)),
            0.60,
            "a cap never raises a rating"
        );
    }

    #[test]
    fn a_bare_frame_keeps_its_sharp_detail_but_reads_soft_as_a_whole() {
        let shot = detail_shot(768, 0.06);
        let bare = bare_frame(&shot, "critical");
        let score = bare.score.expect("a measurable frame");
        let whole = bare.whole.expect("a measurable frame");
        assert!(
            score >= whole,
            "the in-focus area is never the harder judge: {score} vs {whole}"
        );
        assert!(
            score >= sharpness::KEEPER_MIN_RATING,
            "a sharp detail clears the keeper bar: {score}"
        );
        assert!(
            whole < score,
            "the whole frame reads softer than its detail: {whole} vs {score}"
        );
    }

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

    #[test]
    fn a_panning_frame_gets_lead_room_and_a_still_one_gets_none() {
        const FRAME: [f64; 2] = [6000.0, 4000.0];
        // The pan flag only fires on a measured pan; its axis is the line the
        // background is smeared along (0 = horizontal).
        let pan = conrod_vision::streaks::Pan {
            coherence: 0.91,
            axis_deg: 0.0,
            consistency: 0.97,
        };
        let motion = pan_motion(true, Some(pan));
        assert_eq!(motion, Some(vis_framing::Motion::AxisDeg(0.0)));

        // The car left of centre: the room ahead is the larger share, so lead
        // room is positive. Placed right of centre -- as a car travelling the
        // other way would be -- it is positive again, mirrored: a single frame
        // may not call either one "room behind".
        let car = [1000.0, 1000.0, 3000.0, 3000.0];
        let mirror = [3000.0, 1000.0, 5000.0, 3000.0];
        let left = vis_framing::framing(FRAME, car, &[], motion, None);
        let right = vis_framing::framing(FRAME, mirror, &[], motion, None);
        let (l, r) = (
            left.lead_room.expect("panned"),
            right.lead_room.expect("panned"),
        );
        assert!(l > 0.20 && r > 0.20, "left {l:.3}, right {r:.3}");
        assert!(
            (l - r).abs() < 1e-6,
            "mirrored pans read the same: {l} vs {r}"
        );
        let policy = vis_framing::FramingPolicy::sport();
        assert!(
            (vis_framing::framing_score(&left, &policy)
                - vis_framing::framing_score(&right, &policy))
            .abs()
                < 1e-6
        );

        // A still frame: no motion, so lead room is absent (not zero), and the
        // sport score is the one without a lead term.
        let still = pan_motion(false, Some(pan));
        assert_eq!(still, None);
        let held = vis_framing::framing(FRAME, car, &[], still, None);
        assert_eq!(held.lead_room, None);
        assert_eq!(
            vis_framing::framing_score(&held, &policy),
            vis_framing::framing_score(
                &held,
                &vis_framing::FramingPolicy {
                    lead_weight: 0.0,
                    ..policy
                }
            )
        );
    }

    #[test]
    fn a_portrait_is_as_sharp_as_its_softer_eye() {
        let portrait = [
            ("face", [0.0, 0.0, 10.0, 10.0], 0.99),
            ("eye", [1.0, 1.0, 3.0, 3.0], 0.99),
            ("eye", [5.0, 1.0, 7.0, 3.0], 0.91),
            ("person", [0.0, 0.0, 100.0, 100.0], 0.88),
        ];
        assert_eq!(
            frame_rating(ScanProfile::Portrait, false, &portrait, Some(0.5)),
            0.91,
            "one soft eye is a miss the sharp one must not hide"
        );
        // Every other kind still takes its sharpest subject.
        let people = [
            ("person", [0.0, 0.0, 10.0, 10.0], 0.91),
            ("person", [0.0, 0.0, 50.0, 50.0], 0.99),
        ];
        assert_eq!(
            frame_rating(ScanProfile::Portrait, false, &people, None),
            0.99
        );
    }

    /// The frame the two subject-blur tests share.
    const SUBJECT_FRAME: (usize, usize) = (4000, 1800);
    const CAR_BOX: [f64; 4] = [1000.0, 400.0, 2600.0, 1400.0];
    const CAR: [usize; 4] = [1100, 450, 2500, 1350];

    /// Hash-driven two-level blocks of `cell` px: edges lie far apart, which is
    /// the one pattern `edgeblur` reads honestly. A fine periodic checkerboard
    /// is not: the re-blur barely attenuates a pattern that repeats every 16 px,
    /// so the ratio reads it as blurred.
    fn blocks(w: usize, h: usize, cell: usize, lo: f32, hi: f32, seed: usize) -> Vec<f32> {
        (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let on = hash(x / cell + seed, y / cell).is_multiple_of(2);
                if on {
                    hi
                } else {
                    lo
                }
            })
            .collect()
    }

    /// A separable box blur of radius `r`, edge-clamped, sliding window.
    fn box_blur(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
        let win = (2 * r + 1) as f32;
        let mut tmp = vec![0f32; w * h];
        for y in 0..h {
            let row = &src[y * w..(y + 1) * w];
            let mut acc: f32 = (0..=r).map(|i| row[i.min(w - 1)]).sum::<f32>()
                + (1..=r).map(|_| row[0]).sum::<f32>();
            for x in 0..w {
                tmp[y * w + x] = acc / win;
                acc += row[(x + r + 1).min(w - 1)] - row[x.saturating_sub(r)];
            }
        }
        let mut out = vec![0f32; w * h];
        for x in 0..w {
            let mut acc: f32 = (0..=r).map(|i| tmp[i.min(h - 1) * w + x]).sum::<f32>()
                + (1..=r).map(|_| tmp[x]).sum::<f32>();
            for y in 0..h {
                out[y * w + x] = acc / win;
                acc += tmp[(y + r + 1).min(h - 1) * w + x] - tmp[y.saturating_sub(r) * w + x];
            }
        }
        out
    }

    /// The frame the two subject-blur tests share: a bright sharp background
    /// with a dark car of coarse blocks in the middle of `CAR_BOX`, its own
    /// 1400x900 rectangle inside the detection box so the margins are
    /// background. `bg_blur`/`car_blur` are box-blur passes (radius 4, about
    /// 2.6 px of sigma per pass).
    fn subject_frame(bg_blur: usize, car_blur: usize) -> Gray {
        let (w, h) = SUBJECT_FRAME;
        let mut data = blocks(w, h, 32, 25.0, 235.0, 1);
        for _ in 0..bg_blur {
            data = box_blur(&data, w, h, 4);
        }
        let [x0, y0, x1, y1] = CAR;
        let (cw, ch) = (x1 - x0, y1 - y0);
        let mut car = blocks(cw, ch, 60, 12.0, 112.0, 2);
        for _ in 0..car_blur {
            car = box_blur(&car, cw, ch, 4);
        }
        for y in 0..ch {
            for x in 0..cw {
                data[(y0 + y) * w + x0 + x] = car[y * cw + x];
            }
        }
        Gray::new(
            w,
            h,
            data.iter()
                .map(|v| v.round().clamp(0.0, 255.0) as u8)
                .collect(),
        )
    }

    #[test]
    fn a_blurred_subject_is_not_rescued_by_a_sharp_background_in_its_box() {
        let strict = sharpness::BlurStrictness::Strict;
        let long = SUBJECT_FRAME.0.max(SUBJECT_FRAME.1);
        let frame = subject_frame(0, 3);
        // The old reading, inside the whole box: the sharp banner in the
        // margins is what it sees, so a heavily blurred car passes as sharp.
        let whole = conrod_vision::edgeblur::measure(&frame, Some([1000, 400, 2600, 1400]))
            .sigma_worst_at_reference(long);
        let old = sharpness::rating_from_blur(whole, strict).unwrap();
        assert!(
            sharpness::stars_for(old) >= 4,
            "the sharp background should read sharp on its own: sigma {whole:.3} -> {old:.3}"
        );
        // The worse of the box and its centre sees the car.
        let blur = subject_blur(&frame, CAR_BOX, long);
        assert!(
            blur.centre.is_finite() && blur.centre > blur.whole,
            "the centre must be the blurred part: whole {:.3}, centre {:.3}",
            blur.whole,
            blur.centre
        );
        let score = sharpness::rating_from_blur(blur.worst, strict).unwrap();
        assert!(
            score < sharpness::KEEPER_MIN_RATING,
            "a blurred subject must not be a keeper: sigma {:.3} -> {score:.3}",
            blur.worst
        );
        // The same frame with a sharp car: the change is not a blanket penalty.
        let sharp = subject_blur(&subject_frame(0, 0), CAR_BOX, long);
        let score = sharpness::rating_from_blur(sharp.worst, strict).unwrap();
        assert!(
            sharpness::stars_for(score) >= 4,
            "a sharp car on the same background stays above four stars: sigma {:.3} -> {score:.3}",
            sharp.worst
        );
    }

    #[test]
    fn a_sharp_subject_over_a_blurred_background_stays_sharp() {
        let strict = sharpness::BlurStrictness::Strict;
        let long = SUBJECT_FRAME.0.max(SUBJECT_FRAME.1);
        let frame = subject_frame(2, 0);
        // The margins really are blurred and still measurable: a strip of the
        // box outside the car measures as a blur miss.
        let strip = conrod_vision::edgeblur::measure(&frame, Some([1000, 400, 1100, 1400]))
            .sigma_worst_at_reference(long);
        let margin = sharpness::rating_from_blur(strip, strict).unwrap();
        assert!(
            margin < sharpness::KEEPER_MIN_RATING,
            "the background in the margins should read blurred: sigma {strip:.3} -> {margin:.3}"
        );
        // The centre is the sharp subject, so the worse of the two stays sharp.
        let blur = subject_blur(&frame, CAR_BOX, long);
        let centre = sharpness::rating_from_blur(blur.centre, strict).unwrap();
        assert!(
            sharpness::stars_for(centre) >= 4,
            "the centre must read sharp on its own: sigma {:.3} -> {centre:.3}",
            blur.centre
        );
        let score = sharpness::rating_from_blur(blur.worst, strict).unwrap();
        assert!(
            sharpness::stars_for(score) >= 4,
            "a sharp subject over a blurred background stays above four stars: sigma {:.3} -> {score:.3}",
            blur.worst
        );
    }
}
