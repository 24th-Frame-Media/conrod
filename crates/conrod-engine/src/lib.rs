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
use conrod_vision::{expression, exposure, framing as vis_framing, level, sharpness};
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
pub mod progress;
pub mod region_training;
pub mod rescore;
pub mod selftest;
pub mod setup;
#[cfg(test)]
mod testkit;
#[cfg(test)]
mod blurtest;
#[cfg(test)]
mod edgetest;
#[cfg(test)]
mod evaltest;
#[cfg(test)]
mod vlmeval;
#[cfg(test)]
mod pantest;
#[cfg(test)]
mod sporttest;
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
    /// Tilt/convergence, for the presets where a crooked frame is worth
    /// flagging. `None` elsewhere.
    pub level: Option<level::Level>,
    /// Lowest eye-open and highest smile among the frame's faces.
    pub eye_open_min: Option<f32>,
    pub smile_max: Option<f32>,
    /// Rodeo/equestrian only: how much the animal is rearing under its rider.
    pub buck: Option<f64>,
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
pub(crate) fn load_expression() -> Option<Mutex<expression::ExpressionModel>> {
    use conrod_core::models;
    let landmarks = models::find(expression::LANDMARKS)?;
    let mut model = expression::ExpressionModel::load(&landmarks).ok()?;
    if let Some(blendshapes) = models::find(expression::BLENDSHAPES) {
        let _ = model.load_blendshapes(&blendshapes);
    }
    Some(Mutex::new(model))
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
    detector: &Mutex<FaceDetector>,
    expression: Option<&Mutex<expression::ExpressionModel>>,
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
        let found = lock(detector).detect(&crop)?;
        for face in found {
            let open = lock(detector).eyes_open(&crop, &face);
            // When the blendshape mesh agrees with the detector, its continuous
            // eye-open replaces the binary OCEC verdict.
            let judged = expression.and_then(|model| {
                let mut model = lock(model);
                model.read(&crop, &face).ok()
            });
            let closed = match judged {
                Some(ref read) if read.confidence >= expression::TRUST_CONFIDENCE => {
                    out.eye_open_min = Some(
                        out.eye_open_min
                            .map_or(read.eye_open_min(), |m: f32| m.min(read.eye_open_min())),
                    );
                    out.smile_max = Some(out.smile_max.map_or(read.smile, |m: f32| m.max(read.smile)));
                    read.eye_open_min() < expression::CLOSED_EYE_BELOW
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

#[allow(clippy::too_many_arguments)]
pub fn cull_frame(
    path: &Path,
    detector: &Mutex<Detector>,
    settings: &Settings,
    profile: ScanProfile,
    models: &std::collections::HashMap<String, conrod_core::ridge::SharpModel>,
    faces: Option<&Mutex<FaceDetector>>,
    // The blendshape expression model; `None` when its files are absent.
    expression: Option<&Mutex<expression::ExpressionModel>>,
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
        let mut detector = detector.lock().unwrap();
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
    let strictness = if settings.blur_explicit {
        settings.blur_strictness.clone()
    } else {
        policy.default_blur.to_string()
    };
    // Level only where a crooked frame is worth flagging, and framing only
    // where the preset says composition decides between frames.
    let level = policy.level.then(|| level::measure(&grey));
    let tilt = level
        .as_ref()
        .and_then(|l| l.tilt_deg.map(|t| t as f32));
    let framing_policy = framing_policy(preset);
    let boxes: Vec<[f64; 4]> = found.iter().map(|det| det.bbox).collect();

    let mut subjects = Vec::new();
    for det in &found {
        let kind = if det.class_id == detect::PERSON || detect::ANIMALS.contains(&det.class_id) {
            "person"
        } else {
            "vehicle"
        };
        let others: Vec<[f64; 4]> = boxes
            .iter()
            .copied()
            .filter(|b| *b != det.bbox)
            .collect();
        let measured = vis_framing::framing(
            [fw as f64, fh as f64],
            det.bbox,
            &others,
            None,
            tilt,
        );
        let framing_score =
            framing_policy.map(|policy| vis_framing::framing_score(&measured, &policy));
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
        // The subject's score is the edge-blur estimate inside the detector box
        // on native pixels; the tile measure above still supplies background,
        // bands, features and the held-pan flag. A loaded learned model keeps
        // priority, and an unmeasurable box keeps the tile score.
        if !focus.learned {
            let region = [
                (det.bbox[0].max(0.0) as usize).min(fw),
                (det.bbox[1].max(0.0) as usize).min(fh),
                (det.bbox[2].max(0.0) as usize).min(fw),
                (det.bbox[3].max(0.0) as usize).min(fh),
            ];
            let blur = conrod_vision::edgeblur::measure(&grey, Some(region));
            if let Some(score) =
                sharpness::rating_from_blur(
                    blur.sigma_worst_at_reference(fw.max(fh)),
                    sharpness::BlurStrictness::parse(&strictness),
                )
            {
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
        focus.panning = preset.pan_compatible()
            && subject_not_blurred
            && conrod_vision::streaks::background_streaks(&grey, det.bbox)
                .is_some_and(|s| s >= conrod_vision::streaks::PAN_COHERENCE);
        let verdict = sharpness::rating_for(rating, settings.sharp_at, settings.blurred_below);
        let stars = sharpness::stars_for(rating);
        let cull_reason = if policy.deliver_most {
            // A condition report keeps nearly everything: only a subject that
            // is clearly out of focus goes, and never one that is merely soft.
            if settings.cull_blurred && verdict == "blurred" {
                "too blurred".into()
            } else {
                String::new()
            }
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
                });
            }
        }
    }
    step("sharpness");
    let whole = {
        let focus = sharpness::measure(&grey, None, None);
        let measured = focus.measured.then_some(focus.score);
        if subjects.is_empty() {
            // Nothing to judge: rate the frame on its in-focus area, so a sharp
            // detail against a deliberately blurred background is not "blurred".
            //
            // Flat subject matter -- an empty wall, a worktop, a ceiling -- has
            // too little texture for either tile ratio, and a sharp interior
            // frame reads as soft. The edge blur of the frame's sharpest
            // direction asks only whether any edge is sharp, which is the same
            // question `focal_score` asks of the tiles.
            let edges = conrod_vision::edgeblur::measure(&grey, None);
            let edge = sharpness::rating_from_blur(
                conrod_vision::edgeblur::at_reference(edges.sigma_best, fw.max(fh)),
                sharpness::BlurStrictness::parse(&strictness),
            );
            let mut score = f64::NEG_INFINITY;
            for v in [measured, sharpness::focal_score(&grey), edge]
                .into_iter()
                .flatten()
            {
                score = score.max(v);
            }
            (score > f64::NEG_INFINITY).then_some(score)
        } else {
            measured
        }
    };
    let rated: Vec<_> = subjects
        .iter()
        .map(|s| (s.region_type, s.bbox, s.rating))
        .collect();
    let rating = frame_rating(preset.profile(), settings.include_people, &rated, whole);
    let stars = sharpness::stars_for(rating);
    // Exposure on the main subject, so a backlit rider's clipped whites are
    // told from a blown sky.
    let subject_box = subjects
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
        });
    let exposure = Some(exposure::measure(&image, subject_box));
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
        level,
        eye_open_min,
        smile_max,
        buck,
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
                match load_faces() {
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
        // Expression is optional: without the landmarker and blendshape files
        // the scan silently keeps the binary OCEC eye verdict.
        let expression = if faces.is_some() {
            load_expression()
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
                let (paths, settings, detectors, models, faces, expression, quality) =
                    (&paths, &settings, &detectors, &models, &faces, &expression, &quality);
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
}
