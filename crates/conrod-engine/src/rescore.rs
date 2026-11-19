//! Re-measure an album with the sharpness models as they stand now.
//!
//! The scan keeps each subject's 18 focus features and its hand-built score
//! (`heuristic`), so a newly trained model can be applied to an old album without
//! touching a photograph: features through the model, then the framing penalty
//! and the cull decision exactly as `cull_frame` makes them. The frame size
//! and box are stored alongside the features, so the framing factor is
//! computed fresh rather than guessed back out of the old rating.
//!
//! What a model cannot change is left alone: the features themselves, and the
//! pan / background / sharp-end facts, which come from the hand-built measure.
//! Hand stars, rejects and reviews are in other columns and are never written.
use crate::afsubject;
use crate::desktop::{rows, Desktop, Result};
use crate::err;
use crate::library::require_job;
use crate::lock;
use crate::operations::{launch, settings};
use crate::passes::{pick_of_pass, region_name};
use conrod_core::{
    framing,
    profile::{ScanProfile, ShootPreset, Subject},
    ridge::SharpModel,
    settings::Settings,
    tasks::Task,
};
use conrod_vision::sharpness;
use rayon::prelude::*;
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// What the scan stored about one subject.
pub struct Stored<'a> {
    pub region: &'a str,
    /// Frame pixels, as detected.
    pub bbox: [f64; 4],
    pub frame: (i64, i64),
    pub features: &'a [f64],
    /// The hand-built focus score.
    pub heuristic: f64,
    pub panning: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub sharpness: f64,
    pub sharpness_verdict: &'static str,
    pub rating: f64,
    pub rating_verdict: &'static str,
    pub cull_reason: String,
}

/// The focus score, rating, verdicts and cull decision for one stored subject.
pub fn remeasure(s: &Stored, model: Option<&SharpModel>, cfg: &Settings) -> Outcome {
    let learned = model
        .filter(|_| !s.features.is_empty())
        .and_then(|m| sharpness::learned_score(m, s.features));
    let score = learned.unwrap_or(s.heuristic);
    let small = matches!(s.region, "face" | "eye");
    // Only vehicles take the edge penalty; faces and people/animals often touch
    // the frame edge intentionally.
    let edges = if s.region == "vehicle" {
        framing::assess(Some(s.bbox), s.frame.0, s.frame.1)
    } else {
        framing::Framing::default()
    };
    let rating = score * edges.factor;
    let verdict = sharpness::rating_for(rating, cfg.sharp_at, cfg.blurred_below);
    let stars = sharpness::stars_for(rating);
    let floor = cfg.auto_reject_below_stars as u8;
    let cull_reason = if small {
        if stars < floor || (cfg.cull_blurred && score < cfg.blurred_below) {
            format!("{} soft", s.region)
        } else {
            String::new()
        }
    } else if floor > stars {
        format!("{stars} star{}", if stars == 1 { "" } else { "s" })
    } else if cfg.cull_blurred && verdict == "poor" && !s.panning {
        if edges.cut_off() {
            framing::describe(&edges)
        } else {
            "too blurred".into()
        }
    } else {
        String::new()
    };
    Outcome {
        sharpness: score,
        sharpness_verdict: sharpness::verdict_for(score, cfg.sharp_at, cfg.blurred_below),
        rating,
        rating_verdict: verdict,
        cull_reason,
    }
}

/// The models trained so far, by region.
fn load_models(d: &Desktop) -> HashMap<&'static str, SharpModel> {
    ["vehicle", "person", "face", "eye"]
        .into_iter()
        .filter_map(|region| {
            let text = std::fs::read_to_string(crate::region_training::path(d, region)).ok()?;
            Some((region, serde_json::from_str(&text).ok()?))
        })
        .collect()
}

/// The action: re-read every photo, re-measure each stored subject and each
/// subject-less frame from the pixels, then derive ratings, verdicts and keepers.
///
/// It re-reads pixels on purpose: a threshold change (the pan sharp-share cap,
/// a new strictness) cannot be applied from stored scores alone, and a photographer
/// who asks for a re-score expects the numbers on screen to change. The stored
/// boxes are kept, so nothing is re-detected and no hand work is lost.
pub fn rescore(d: &Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    if !d.status()["activeJob"].is_null() {
        return Err("A scan is running. Stop it first.".into());
    }
    let cfg = settings(d, job)?;
    launch(d, job, "Re-scoring", move |d, stop, task| {
        let unread = remeasure_pixels(d, job, &cfg, stop, task)?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let (subjects, changed, skipped, keepers) = run(d, job, &cfg, stop, task)?;
        task.detail(format!(
            "Re-scored {subjects} subjects ({changed} changed, {skipped} kept as stored), {unread} photos unread, {keepers} keepers"
        ));
        Ok(())
    })
}

/// The action: read every photo of the album again and re-measure each stored
/// subject, as a background operation.
pub fn rescan(d: &Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    if !d.status()["activeJob"].is_null() {
        return Err("A scan is running. Stop it first.".into());
    }
    let cfg = settings(d, job)?;
    launch(d, job, "Rescanning", move |d, stop, task| {
        let unread = remeasure_pixels(d, job, &cfg, stop, task)?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let (subjects, changed, skipped, keepers) = run(d, job, &cfg, stop, task)?;
        task.detail(format!(
            "Rescanned the album: {subjects} subjects ({changed} changed, {skipped} kept as stored), {unread} photos unread, {keepers} keepers"
        ));
        Ok(())
    })
}

/// Re-measure the focus features of each stored subject from the image's pixels,
/// exactly as `cull_frame` measures a detection, but inside the boxes already
/// stored: nothing is re-detected, so no row is replaced and no hand work (stars,
/// rejects, reviews, numbers, attributes, people) can be lost. Only `features`,
/// `heuristic`, `panning` and `sharp_end` (and the whole-frame sharpness) are
/// written; `run` then derives the ratings, verdicts and cull reasons from them.
/// Returns how many photos could not be read (moved or deleted originals).
fn remeasure_pixels(
    d: &Desktop,
    job: i64,
    cfg: &Settings,
    stop: &AtomicBool,
    task: &Task,
) -> Result<usize> {
    let images = rows(
        &lock(&d.reader),
        "SELECT id, path, af_x1, af_y1, af_x2, af_y2 FROM images WHERE job_id=? AND status='done' AND COALESCE(copy_of,0)=0 ORDER BY id",
        [job],
    )?;
    let subjects = rows(
        &lock(&d.reader),
        "SELECT d.id, d.image_id, d.x1, d.y1, d.x2, d.y2, COALESCE(d.region_type,'vehicle') AS region FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?",
        [job],
    )?;
    let mut by_image: HashMap<i64, Vec<&Value>> = HashMap::new();
    for s in &subjects {
        by_image
            .entry(s["image_id"].as_i64().unwrap_or_default())
            .or_default()
            .push(s);
    }
    let total = images.len() as u64;
    task.progress(0, total);
    task.detail(format!("Reading {total} photos"));
    let (next, done, unread) = (
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    );
    // The album's own worker setting decides, not a private copy of the default.
    let workers = cfg.cull_workers() as usize;
    let pan_ok = ShootPreset::parse(&cfg.scan_profile).pan_compatible();
    let failed = AtomicBool::new(false);
    let first_error: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let n = next.fetch_add(1, Ordering::Relaxed);
                if n >= images.len() || stop.load(Ordering::Relaxed) || failed.load(Ordering::Relaxed)
                {
                    return;
                }
                let image = &images[n];
                let id = image["id"].as_i64().unwrap_or_default();
                let path = std::path::Path::new(image["path"].as_str().unwrap_or_default());
                let empty = Vec::new();
                match measure_frame(
                    path,
                    by_image.get(&id).unwrap_or(&empty),
                    af_box(image),
                    cfg,
                    pan_ok,
                ) {
                    Ok(m) => {
                        // One transaction per frame: a failure part-way leaves
                        // the frame as it was, not half re-measured.
                        let saved = (|| -> Result<()> {
                            let db = lock(&d.db);
                            let tx = db.unchecked_transaction().map_err(err)?;
                            for (det, focus) in &m.subjects {
                                let features =
                                    serde_json::to_string(&focus.features).map_err(err)?;
                                tx.execute(
                                    "UPDATE detections SET features=?,heuristic=?,panning=?,sharp_end=? WHERE id=?",
                                    params![features, focus.heuristic, focus.panning, focus.sharp_end, det],
                                )
                                .map_err(err)?;
                            }
                            // Written even when empty: a frame that is no longer a
                            // pan must not keep its old share, nor a frame that
                            // gained a subject its old whole-frame score.
                            tx.execute(
                                "UPDATE images SET sharpness=COALESCE(?,sharpness),sharpness_whole=?,pan_share=? WHERE id=?",
                                params![m.sharpness, m.whole_raw, m.pan_share, id],
                            )
                            .map_err(err)?;
                            if m.af_present {
                                write_af_call(&tx, id, m.af_sharpness)?;
                            }
                            tx.commit().map_err(err)
                        })();
                        if let Err(e) = saved {
                            lock(&first_error).get_or_insert(e);
                            failed.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                    Err(_) => {
                        unread.fetch_add(1, Ordering::Relaxed);
                    }
                }
                task.progress(done.fetch_add(1, Ordering::Relaxed) as u64 + 1, total);
            });
        }
    });
    if let Some(e) = first_error.into_inner().unwrap_or_else(|e| e.into_inner()) {
        return Err(format!("Could not save the re-measured scores: {e}"));
    }
    Ok(unread.into_inner())
}

/// Store what the AF area means for one frame: its rating when it is the
/// subject (NULL otherwise), and the bystander marks that follow from it. Only
/// marks this rule made (`af_bystander` 1) are ever taken back, and a detection
/// a person has marked or unmarked (`af_bystander` 0, or a bystander set before
/// this rule existed) is never touched, so hand work survives a re-measure.
fn write_af_call(tx: &rusqlite::Connection, image: i64, subject: Option<f64>) -> Result<()> {
    tx.execute(
        "UPDATE images SET af_sharpness=? WHERE id=?",
        params![subject, image],
    )
    .map_err(err)?;
    if subject.is_some() {
        tx.execute(
            "UPDATE detections SET bystander=1,af_bystander=1,cull_reason=NULL \
             WHERE image_id=? AND af_bystander IS NULL AND COALESCE(bystander,0)=0",
            [image],
        )
        .map_err(err)?;
    } else {
        tx.execute(
            "UPDATE detections SET bystander=0,af_bystander=NULL \
             WHERE image_id=? AND af_bystander=1",
            [image],
        )
        .map_err(err)?;
    }
    Ok(())
}

/// Read the camera's AF area against each visible frame's stored detections
/// and keep the verdict: `images.af_sharpness` and the bystander marks (see
/// `afsubject`).
///
/// Geometry decides first, from the boxes already in the database: a frame
/// whose AF area is held by a detection, or is a zone covering much of the
/// frame, needs no pixels. A frame whose AF centre is outside every detection
/// (or that has none) is read, and the box measured on the upright preview's
/// native luma exactly as `cull_frame` measures a detected subject, rated with
/// the album's effective strictness. This is the backfill of an album scanned
/// before the scan did it itself.
///
/// Returns `(measured, unread)`: how many frames took a score, and how many
/// could not be read (moved or deleted originals). A frame it cannot read keeps
/// the score it already had -- the old score of that same AF box is still a
/// better answer than none -- while a box that measures to nothing is NULLed.
pub(crate) fn measure_af_frames(
    d: &Desktop,
    job: i64,
    cfg: &Settings,
    task: &Task,
) -> Result<(usize, usize)> {
    let images = rows(
        &lock(&d.reader),
        "SELECT id,path,width,height,af_x1,af_y1,af_x2,af_y2 FROM images WHERE job_id=? \
         AND status='done' AND COALESCE(copy_of,0)=0 AND af_x1 IS NOT NULL \
         AND af_y1 IS NOT NULL AND af_x2 IS NOT NULL AND af_y2 IS NOT NULL ORDER BY id",
        [job],
    )?;
    let stored = rows(
        &lock(&d.reader),
        "SELECT d.image_id, d.x1, d.y1, d.x2, d.y2 FROM detections d JOIN images i ON i.id=d.image_id \
         WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle') IN ('vehicle','person')",
        [job],
    )?;
    let mut boxes: HashMap<i64, Vec<[f64; 4]>> = HashMap::new();
    for s in &stored {
        boxes
            .entry(s["image_id"].as_i64().unwrap_or_default())
            .or_default()
            .push(["x1", "y1", "x2", "y2"].map(|k| s[k].as_f64().unwrap_or_default()));
    }
    // (id, frame, whether the geometry makes the AF area a candidate subject).
    let mut to_read = Vec::new();
    let mut not_subject = Vec::new();
    for row in &images {
        let id = row["id"].as_i64().unwrap_or_default();
        let (w, h) = (
            row["width"].as_f64().unwrap_or_default(),
            row["height"].as_f64().unwrap_or_default(),
        );
        let frac =
            ["af_x1", "af_y1", "af_x2", "af_y2"].map(|key| row[key].as_f64().unwrap_or_default());
        let area = [frac[0] * w, frac[1] * h, frac[2] * w, frac[3] * h];
        let held = boxes.get(&id).map_or(&[][..], Vec::as_slice);
        // A grid's union is stored like a point's box; the makernote says which
        // it is, so the gate can be applied here as at scan time.
        let path = std::path::Path::new(row["path"].as_str().unwrap_or_default());
        let kind = frame_kind(path);
        let candidate = w > 0.0
            && h > 0.0
            && afsubject::geometry(
                afsubject::FocusArea { area, kind },
                (w as usize, h as usize),
                held,
            ) == Some(true);
        if candidate {
            to_read.push((id, row, frac));
        } else {
            not_subject.push(id);
        }
    }
    let total = to_read.len() as u64;
    task.progress(0, total);
    task.detail(format!(
        "Measuring {total} frames inside the focus area (off every detected subject)"
    ));
    let strictness = cfg.effective_blur();
    let done = AtomicUsize::new(0);
    let results: Vec<(i64, Result<Option<f64>>)> = to_read
        .par_iter()
        .map(|(id, row, area)| {
            let path = std::path::Path::new(row["path"].as_str().unwrap_or_default());
            let result = af_rating(path, *area, &strictness);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            task.progress(n as u64, total);
            (*id, result)
        })
        .collect();
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    let (mut measured, mut unread) = (0, 0);
    for id in &not_subject {
        write_af_call(&tx, *id, None)?;
    }
    for (id, result) in &results {
        match result {
            Ok(Some(score)) => {
                measured += 1;
                write_af_call(&tx, *id, Some(*score))?;
            }
            Ok(None) => write_af_call(&tx, *id, None)?,
            Err(_) => unread += 1,
        }
    }
    tx.commit().map_err(err)?;
    Ok((measured, unread))
}

/// Whether a frame's makernote names a focus grid rather than one point. Only
/// the record's kind is needed here, not its box: the box is already stored.
/// A file that cannot be read again is taken as a point, so its stored box
/// behaves as it did before grids were read at all.
fn frame_kind(path: &std::path::Path) -> conrod_io::af::AreaKind {
    use conrod_io::af::AreaKind;
    conrod_io::raw::af_header(path)
        .ok()
        .and_then(|h| h.af)
        .map_or(AreaKind::Point, |af| {
            if af.names_grid() {
                AreaKind::Grid
            } else {
                AreaKind::Point
            }
        })
}

/// One AF box's edge-blur rating, on the upright preview's native luma. The box
/// is fractions of the upright frame, the space `af::read` returns.
fn af_rating(path: &std::path::Path, area: [f64; 4], strictness: &str) -> Result<Option<f64>> {
    let frame = conrod_io::raw::read(path)?;
    let image =
        conrod_vision::imageops::Rgb::decode_jpeg(&frame.preview, 1)?.orient(frame.orientation);
    let grey = image.to_gray();
    let (fw, fh) = (grey.width as f64, grey.height as f64);
    let px = [
        area[0].clamp(0.0, 1.0) * fw,
        area[1].clamp(0.0, 1.0) * fh,
        area[2].clamp(0.0, 1.0) * fw,
        area[3].clamp(0.0, 1.0) * fh,
    ];
    Ok(afsubject::rate_box(&grey, px, strictness))
}

/// What `cull_frame` would have measured for one frame.
struct Measures {
    /// Each stored subject and its fresh measure.
    subjects: Vec<(i64, sharpness::Sharpness)>,
    /// The score `images.sharpness` takes: a bare frame's rescued score, else
    /// the measured whole frame.
    sharpness: Option<f64>,
    /// The bare frame's raw whole-frame score; `None` when it has subjects.
    whole_raw: Option<f64>,
    /// How much of a panning frame is sharp; `None` when it is not a pan.
    pan_share: Option<f64>,
    /// Whether the frame has a stored AF box at all.
    af_present: bool,
    /// The AF box's rating when it is the frame's subject; `None` when a
    /// detection holds it or it is unusable.
    af_sharpness: Option<f64>,
}

/// A frame's stored autofocus area, as fractions of the upright frame (the
/// space `conrod_io::af::read` returns); `None` when it has no box or only part
/// of one.
fn af_box(row: &Value) -> Option<[f64; 4]> {
    let c = |k: &str| row[k].as_f64();
    Some([c("af_x1")?, c("af_y1")?, c("af_x2")?, c("af_y2")?])
}

/// The focus area is a second opinion on a subject's sharpness, never a veto
/// against the detection box: it may lower the score (on a motion-smeared car
/// it reads worse) but it may not raise it, because it can also land on a sharp
/// corner of a subject the detection box found soft.
#[doc(hidden)]
pub fn second_opinion(detection: Option<f64>, af: Option<f64>) -> Option<f64> {
    match (detection, af) {
        (Some(detection), Some(af)) => Some(detection.min(af)),
        (detection, af) => detection.or(af),
    }
}

/// What `cull_frame` would have measured for each stored subject of one frame
/// (subject id, measure), and for the frame as a whole.
///
/// `af` is the frame's AF box in fractions; its kind comes from the makernote
/// (the scan does not persist it, a grid's box is still the box). `afsubject`
/// says what it means: when its centre lies inside a subject's detection box it
/// is a second opinion on that subject's sharpness that can only lower the
/// score; when it lies outside every detection it is the subject itself. A
/// grid speaks only when every detection is small. `cull_frame` makes the same
/// call at scan time, so a scan and a re-score agree.
fn measure_frame(
    path: &std::path::Path,
    subjects: &[&Value],
    af: Option<[f64; 4]>,
    cfg: &Settings,
    pan_ok: bool,
) -> Result<Measures> {
    let frame = conrod_io::raw::read(path)?;
    let image =
        conrod_vision::imageops::Rgb::decode_jpeg(&frame.preview, 1)?.orient(frame.orientation);
    let opts = conrod_vision::detect::DetectOptions {
        crop_padding: cfg.crop_padding,
        dominant_subject_fraction: cfg.dominant_subject_fraction,
        ..Default::default()
    };
    let grey = image.to_gray();
    let (fw, fh) = (image.width, image.height);
    // The detections that count for the AF rule, as the scan builds them.
    let boxes: Vec<[f64; 4]> = subjects
        .iter()
        .filter(|s| {
            matches!(
                s["region"].as_str().unwrap_or("vehicle"),
                "vehicle" | "person"
            )
        })
        .map(|s| ["x1", "y1", "x2", "y2"].map(|k| s[k].as_f64().unwrap_or_default()))
        .collect();
    // The AF box is fractions of the upright frame; the edge census works in
    // the preview's pixels. The record kind comes from the file itself: a grid
    // whose union is stored is only trusted when everything the detector found
    // is small (`afsubject::usable`), exactly as at scan time.
    let kind = frame
        .af
        .as_ref()
        .map_or(conrod_io::af::AreaKind::Point, |af| {
            if af.names_grid() {
                conrod_io::af::AreaKind::Grid
            } else {
                conrod_io::af::AreaKind::Point
            }
        });
    let af_present = af.is_some();
    let af = af
        .map(|[x1, y1, x2, y2]| afsubject::FocusArea {
            area: [
                x1 * fw as f64,
                y1 * fh as f64,
                x2 * fw as f64,
                y2 * fh as f64,
            ],
            kind,
        })
        .filter(|af| afsubject::usable(*af, (fw, fh), &boxes));
    let af_px = af.map(|af| af.area);
    let mut out = Vec::new();
    let mut panning = false;
    for s in subjects {
        let c = |k: &str| s[k].as_f64().unwrap_or_default();
        let bbox = [c("x1"), c("y1"), c("x2"), c("y2")];
        let focus = if matches!(s["region"].as_str(), Some("face" | "eye")) {
            let [a, b, x, y] = bbox.map(|v| v.max(0.0) as usize);
            if x <= a || y <= b {
                continue;
            }
            let mut focus = sharpness::measure(
                &image
                    .crop(a, b, x.min(image.width), y.min(image.height))
                    .to_gray(),
                None,
                None,
            );
            // A held pan is a vehicle or person over a streaked background; a
            // face crop has no background of its own to judge.
            focus.panning = false;
            focus
        } else {
            let crop_box =
                conrod_vision::detect::expand_box(bbox, image.width, image.height, &opts);
            let crop = crate::cut(&image, crop_box, cfg);
            let scale = crop.width as f64 / (crop_box[2] - crop_box[0]);
            let inner = [
                (bbox[0] - crop_box[0]) * scale,
                (bbox[1] - crop_box[1]) * scale,
                (bbox[2] - crop_box[0]) * scale,
                (bbox[3] - crop_box[1]) * scale,
            ];
            let mut focus = sharpness::measure(&crop.to_gray(), Some(inner), None);
            // As `cull_frame` scores a subject: the edge-blur estimate on native
            // pixels, the worse of the whole box and the box's centre 60%, so a
            // sharp background in the margins cannot vouch for a blurred body.
            // The tile score stays the fallback and still supplies the pan flag
            // and the features. As in `cull_frame`, the crop's tile score only
            // decides whether the centre was consulted; it never overrules the
            // edge census.
            let tile = focus.score;
            let blur = crate::subject_blur(&grey, bbox, fw.max(fh));
            let worst = if tile < 0.80 { blur.worst } else { blur.whole };
            let strictness = sharpness::BlurStrictness::parse(&cfg.effective_blur());
            let detection = sharpness::rating_from_blur(worst, strictness);
            // The camera's focus area is the better subject box when it sits on
            // this subject: its whole-box edge blur needs no centre-60% gate
            // (the AF box is already the subject, and the detection box's
            // margins are exactly what let a sharp background vouch for a
            // smeared car). It is a second opinion only: on a heavily blurred
            // car it reads worse and lowers the score, but it can also read
            // sharper than the detection box, and must not vouch for a frame the
            // detection box found soft (B1000_4218 read 0.984 that way).
            let af = af_px
                .filter(|af| afsubject::holds_middle(bbox, *af))
                .and_then(|af| afsubject::rate_box(&grey, af, &cfg.effective_blur()));
            if let Some(score) = second_opinion(detection, af) {
                focus.score = score;
            }
            // As `cull_frame` decides it: the subject not a blur miss, a
            // streaked background, and a preset that expects pans.
            focus.panning = pan_ok
                && sharpness::verdict_for(focus.score, cfg.sharp_at, cfg.blurred_below)
                    != "blurred"
                && conrod_vision::streaks::background_streaks(&grey, bbox)
                    .is_some_and(|s| s >= conrod_vision::streaks::PAN_COHERENCE);
            focus
        };
        if focus.measured {
            // As the scan stores it: without a model the hand-built score is the score.
            let mut focus = focus;
            focus.heuristic = focus.score;
            if focus.panning
                && matches!(
                    s["region"].as_str().unwrap_or("vehicle"),
                    "vehicle" | "person"
                )
            {
                panning = true;
            }
            out.push((s["id"].as_i64().unwrap_or_default(), focus));
        }
    }
    // A frame with no subject is rated by the same function the scan uses, so
    // the two cannot disagree; one with subjects keeps the plain measured
    // whole-frame score it always had.
    let (sharpness, whole_raw) = if subjects.is_empty() {
        let bare = crate::bare_frame(&grey, &cfg.effective_blur());
        (bare.score, bare.whole)
    } else {
        let whole = sharpness::measure(&grey, None, None);
        (whole.measured.then_some(whole.score), None)
    };
    // Where the camera focused outside every detection, the focus area is the
    // subject (the scan's rule, `afsubject::decide`): no pan, and the frame is
    // rated on it.
    let af_sharpness = afsubject::decide(&grey, af, &boxes, &cfg.effective_blur()).subject();
    let panning = panning && af_sharpness.is_none();
    let pan_share = panning
        .then(|| sharpness::frame_sharp_share(&grey))
        .flatten();
    Ok(Measures {
        subjects: out,
        sharpness,
        whole_raw,
        pan_share,
        af_present,
        af_sharpness,
    })
}

/// A frame's subjects' ratings, for the rating of the frame itself.
#[derive(Default)]
struct Frame {
    whole: Option<f64>,
    best: HashMap<String, f64>,
    /// Person and vehicle box areas, for the main subject.
    boxes: Vec<(Subject, f64)>,
    /// The stored sharp share of a panning frame, for the frame rating's cap.
    pan_share: Option<f64>,
    /// Whether any vehicle or person of the frame is a held pan.
    panning: bool,
}

fn write(d: &Desktop, batch: &[(i64, Outcome)]) -> Result<()> {
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    for (id, o) in batch {
        tx.execute(
            // Shut eyes are OCEC's verdict, not a focus one: re-measuring keeps it.
            "UPDATE detections SET sharpness=?1,sharpness_verdict=?2,rating=?3,rating_verdict=?4,cull_reason=CASE WHEN region_type='face' AND COALESCE(bystander,0)=0 AND COALESCE(CASE WHEN json_valid(attributes) THEN json_extract(attributes,'$.eyes_closed') END,0) THEN 'eyes closed' ELSE ?5 END WHERE id=?6",
            params![o.sharpness, o.sharpness_verdict, o.rating, o.rating_verdict, o.cull_reason, id],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

/// The frame's best score per region: the sharpest subject, except the eyes,
/// where a portrait is as sharp as its softer eye (as `frame_rating` reads it).
fn keep(best: &mut HashMap<String, f64>, region: &str, rating: f64) {
    best.entry(region.into())
        .and_modify(|b| {
            *b = if region == "eye" {
                b.min(rating)
            } else {
                b.max(rating)
            }
        })
        .or_insert(rating);
}

pub(crate) fn run(
    d: &Desktop,
    job: i64,
    cfg: &Settings,
    stop: &AtomicBool,
    task: &Task,
) -> Result<(usize, usize, usize, usize)> {
    let models = load_models(d);
    let profile = ScanProfile::parse(&cfg.scan_profile);
    let found = rows(
        &lock(&d.reader),
        "SELECT d.id, d.image_id, d.x1, d.y1, d.x2, d.y2, COALESCE(d.region_type,'vehicle') AS region, d.features, d.heuristic, d.panning, d.rating, COALESCE(d.bystander,0) AS bystander, i.width, i.height, i.sharpness AS whole, i.pan_share FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? ORDER BY d.id",
        [job],
    )?;
    let total = found.len() as u64;
    task.progress(0, total);
    task.detail(format!("Re-measuring {total} subjects"));
    let (mut changed, mut skipped) = (0, 0);
    let mut frames: HashMap<i64, Frame> = HashMap::new();
    let mut batch = Vec::new();
    for (n, row) in found.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            return Ok((0, 0, 0, 0));
        }
        let region = row["region"].as_str().unwrap_or("vehicle");
        // A bystander (the camera focused elsewhere, or a person said so) is
        // measured like any subject but drives neither the frame's rating nor
        // the cull: a frame with only bystanders is rated by `rate_missed_frames`.
        let bystander = row["bystander"].as_i64() == Some(1);
        let mut scratch = Frame::default();
        let frame = if bystander {
            &mut scratch
        } else {
            frames
                .entry(row["image_id"].as_i64().unwrap_or_default())
                .or_default()
        };
        frame.whole = row["whole"].as_f64();
        frame.pan_share = row["pan_share"].as_f64();
        if matches!(region, "vehicle" | "person") && row["panning"].as_i64() == Some(1) {
            frame.panning = true;
        }
        let kind = match region {
            "person" => Some(Subject::Person),
            "vehicle" => Some(Subject::Vehicle),
            _ => None,
        };
        if let Some(kind) = kind {
            let corner = |k: &str| row[k].as_f64().unwrap_or_default();
            let area = (corner("x2") - corner("x1")) * (corner("y2") - corner("y1"));
            frame.boxes.push((kind, area));
        }
        let old = row["rating"].as_f64();
        // A detection stored without its hand-built score (a legacy scan,
        // from before that score was recorded) cannot be re-measured without
        // its crop: kept as it was.
        match row["heuristic"].as_f64().filter(|h| *h >= 0.0) {
            None => {
                skipped += 1;
                if let Some(rating) = old {
                    keep(&mut frame.best, region, rating);
                }
            }
            Some(heuristic) => {
                let features: Vec<f64> = row["features"]
                    .as_str()
                    .and_then(|s| serde_json::from_str(s).ok())
                    .unwrap_or_default();
                let corner = |k: &str| row[k].as_f64().unwrap_or_default();
                let mut outcome = remeasure(
                    &Stored {
                        region,
                        bbox: [corner("x1"), corner("y1"), corner("x2"), corner("y2")],
                        frame: (
                            row["width"].as_i64().unwrap_or_default(),
                            row["height"].as_i64().unwrap_or_default(),
                        ),
                        features: &features,
                        heuristic,
                        panning: row["panning"].as_i64() == Some(1),
                    },
                    models.get(region),
                    cfg,
                );
                if bystander {
                    outcome.cull_reason.clear();
                }
                if old.is_none_or(|o| (o - outcome.rating).abs() > 1e-9) {
                    changed += 1;
                }
                keep(&mut frame.best, region, outcome.rating);
                batch.push((row["id"].as_i64().unwrap_or_default(), outcome));
            }
        }
        if batch.len() >= 200 {
            write(d, &batch)?;
            batch.clear();
        }
        task.progress(n as u64 + 1, total);
    }
    write(d, &batch)?;

    // The frame's own rating, from the subject the profile rates it by.
    {
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        for (image, frame) in &frames {
            let main = conrod_core::profile::main_subject(frame.boxes.iter().copied());
            let rating = profile
                .frame_priority(cfg.include_people, main)
                .iter()
                .find_map(|kind| match kind {
                    Subject::WholeFrame => frame.whole,
                    kind => frame.best.get(region_name(*kind)).copied(),
                })
                .unwrap_or(0.0);
            // The frame's rating says how the picture reads; a pan's is capped
            // by how much of the frame is sharp. The subjects keep their own
            // ratings, which is what picks the keeper.
            let rating = crate::pan_capped(rating, frame.panning, frame.pan_share);
            tx.execute(
                "UPDATE images SET rating=? WHERE id=?",
                params![rating, image],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        // A frame the detector found nothing in has no subject to rate it: in a
        // burst where the other frames have one it is a shot the detector
        // missed, and takes its whole-frame score rather than the almost
        // always-perfect score of its sharpest patch; alone, its in-focus area
        // is the rating, which `remeasure_pixels` has just refreshed.
        crate::passes::rate_missed_frames(&db, job)?;
    }
    // The stars have moved, so the keeper of a pass may have moved with them.
    let picked = pick_of_pass(
        &lock(&d.db),
        job,
        profile,
        cfg.include_people,
        cfg.use_quality_score,
    )?;
    task.detail(format!(
        "Re-measured {} subjects: {changed} changed, {skipped} kept as stored",
        found.len() - skipped
    ));
    Ok((
        found.len() - skipped,
        changed,
        skipped,
        picked["picks"].as_u64().unwrap_or_default() as usize,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;
    use conrod_core::ridge;
    use conrod_vision::imageops::Gray;
    use serde_json::json;

    fn cfg() -> Settings {
        Settings::default()
    }

    #[test]
    fn a_frame_with_no_subject_is_rated_by_its_own_sharpness_on_a_rescore() {
        let lib = Lib::new("rescore-bare");
        let bare = lib.frame("bare.jpg", Some(1));
        // The scan stored 1.0 for a pan the detector missed; the measure has since
        // changed. No file on disk, so the pixel pass leaves the stored sharpness
        // alone and the rating must still follow it.
        lib.sql(
            "UPDATE images SET sharpness=0.66, rating=1.0 WHERE id=?",
            [bare],
        );
        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        let rating: f64 = lib.one("SELECT rating FROM images WHERE id=?", [bare]);
        assert!((rating - 0.66).abs() < 1e-9, "rating {rating}");
        assert_eq!(lib.task("Re-scoring")["state"], "done");
    }

    #[test]
    fn a_pan_frame_rating_is_capped_by_the_stored_share() {
        let lib = Lib::new("rescore-pan-cap");
        let frame = lib.frame("pan.jpg", Some(1));
        let car = lib.detection(frame, "vehicle", 0.95);
        // A stored heuristic so the subject is re-rated, a pan flag so the frame
        // is capped, and a share the scan would have stored for a small car.
        with_features(&lib, car, 0.95, 0.1);
        lib.sql(
            "UPDATE detections SET panning=1,rating=0.95 WHERE id=?",
            [car],
        );
        lib.sql(
            "UPDATE images SET sharpness=0.95,rating=0.95,pan_share=0.1 WHERE id=?",
            [frame],
        );

        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();

        let rating: f64 = lib.one("SELECT rating FROM images WHERE id=?", [frame]);
        assert!(
            rating <= sharpness::pan_share_cap(0.1) + 1e-9,
            "the frame rating is capped by the pan's sharp share: {rating}"
        );
        let subject: f64 = lib.one("SELECT rating FROM detections WHERE id=?", [car]);
        assert_eq!(
            subject, 0.95,
            "the detection keeps the car's own rating: the keeper of the burst is chosen on it"
        );

        // The same frame not held as a pan keeps its subject's rating.
        lib.sql("UPDATE detections SET panning=0 WHERE id=?", [car]);
        lib.sql("UPDATE images SET rating=0.95 WHERE id=?", [frame]);
        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        let rating: f64 = lib.one("SELECT rating FROM images WHERE id=?", [frame]);
        assert!(
            (rating - 0.95).abs() < 1e-9,
            "a still frame is not capped: {rating}"
        );
    }

    fn stored<'a>(region: &'a str, heuristic: f64, features: &'a [f64]) -> Stored<'a> {
        Stored {
            region,
            bbox: [1000.0, 1000.0, 3000.0, 2000.0],
            frame: (6000, 4000),
            features,
            heuristic,
            panning: false,
        }
    }

    /// Everything zero but the first weight: stars = 3 * first feature + 0.5.
    fn model() -> SharpModel {
        let mut weights = vec![0.0; 18];
        weights[0] = 3.0;
        SharpModel {
            version: ridge::FEATURE_VERSION,
            trained_on: 40,
            mean: vec![0.0; 18],
            spread: vec![1.0; 18],
            weights,
            intercept: 0.5,
        }
    }

    #[test]
    fn without_a_model_the_hand_built_score_stands_and_the_frame_edge_costs_rating() {
        let inside = remeasure(&stored("vehicle", 0.9, &[]), None, &cfg());
        assert_eq!((inside.sharpness, inside.rating), (0.9, 0.9));
        assert_eq!(inside.rating_verdict, "good");
        assert_eq!(inside.cull_reason, "");
        let mut clipped = stored("vehicle", 0.9, &[]);
        clipped.bbox = [0.0, 1000.0, 3000.0, 2000.0];
        let clipped = remeasure(&clipped, None, &cfg());
        assert!((clipped.rating - 0.9 * 0.85).abs() < 1e-12);
        assert_eq!(clipped.sharpness, 0.9, "the focus score is not marked down");
    }

    #[test]
    fn the_cull_decision_follows_the_new_rating() {
        let mut lenient = cfg();
        lenient.auto_reject_below_stars = 0;
        lenient.cull_blurred = true;
        let blurred = remeasure(&stored("vehicle", 0.2, &[]), None, &lenient);
        assert_eq!(blurred.cull_reason, "too blurred");
        assert_eq!(blurred.rating_verdict, "poor");
        let mut pan = stored("vehicle", 0.2, &[]);
        pan.panning = true;
        assert_eq!(
            remeasure(&pan, None, &lenient).cull_reason,
            "",
            "a held pan is never culled"
        );
        let mut edge = stored("vehicle", 0.4, &[]);
        edge.bbox = [0.0, 0.0, 3000.0, 2000.0];
        assert_eq!(
            remeasure(&edge, None, &lenient).cull_reason,
            "cut off on 2 edges"
        );
        let mut strict = cfg();
        strict.auto_reject_below_stars = 3;
        assert_eq!(
            remeasure(&stored("vehicle", 0.3, &[]), None, &strict).cull_reason,
            "1 star"
        );
        assert_eq!(
            remeasure(&stored("face", 0.3, &[]), None, &strict).cull_reason,
            "face soft"
        );
        assert_eq!(
            remeasure(&stored("eye", 0.99, &[]), None, &strict).cull_reason,
            ""
        );
        // A face is not marked down for its box touching the edge.
        let mut face = stored("face", 0.9, &[]);
        face.bbox = [0.0, 0.0, 6000.0, 4000.0];
        assert_eq!(remeasure(&face, None, &cfg()).rating, 0.9);
    }

    #[test]
    fn a_model_of_another_version_or_a_missing_one_falls_back_to_the_heuristic() {
        let features = vec![0.9; 18];
        let learned = remeasure(&stored("vehicle", 0.4, &features), Some(&model()), &cfg());
        // 3*0.9+0.5 = 3.2 stars, seven tenths of the way from 0.728 to 0.825.
        assert!((learned.sharpness - 0.7959).abs() < 1e-9, "{learned:?}");
        let mut old = model();
        old.version += 1;
        assert_eq!(
            remeasure(&stored("vehicle", 0.4, &features), Some(&old), &cfg()).sharpness,
            0.4
        );
        assert_eq!(
            remeasure(&stored("vehicle", 0.4, &[]), Some(&model()), &cfg()).sharpness,
            0.4
        );
    }

    /// Guards the copy of the stars-to-score mapping: what the scan would have
    /// stored for a real measured subject must be what a re-measure gives.
    #[test]
    fn a_learned_model_gives_what_the_scan_would_have_given() {
        let (w, h) = (400usize, 300usize);
        let mut seed = 12345u32;
        let data: Vec<u8> = (0..w * h)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 24) as u8
            })
            .collect();
        let bbox = [50.0, 50.0, 350.0, 250.0];
        for m in [model(), {
            let mut m = model();
            m.weights[0] = 5.0;
            m.intercept = 0.0;
            m
        }] {
            let scanned = sharpness::measure(&Gray::new(w, h, data.clone()), Some(bbox), Some(&m));
            assert!(scanned.learned, "the model applied to a measured subject");
            assert_eq!(scanned.features.len(), sharpness::FEATURE_NAMES.len());
            let again = remeasure(
                &Stored {
                    region: "vehicle",
                    bbox: [0.0; 4],
                    frame: (0, 0),
                    features: &scanned.features,
                    heuristic: scanned.heuristic,
                    panning: scanned.panning,
                },
                Some(&m),
                &cfg(),
            );
            assert!(
                (again.sharpness - scanned.score).abs() < 1e-12,
                "{again:?} vs {scanned:?}"
            );
        }
    }

    fn with_features(lib: &Lib, det: i64, heuristic: f64, first: f64) {
        let mut features = vec![0.0; 18];
        features[0] = first;
        lib.sql(
            "UPDATE detections SET features=?,heuristic=?,panning=0 WHERE id=?",
            params![serde_json::to_string(&features).unwrap(), heuristic, det],
        );
    }

    #[test]
    fn rescore_brings_a_new_model_to_an_old_album_and_spares_hand_work() {
        let lib = Lib::new("rescore");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(2)),
        );
        let (low, high) = (
            lib.detection(f1, "vehicle", 0.30),
            lib.detection(f2, "vehicle", 0.35),
        );
        let (no_features, starred) = (
            lib.detection(f3, "vehicle", 0.66),
            lib.detection(f3, "vehicle", 0.10),
        );
        with_features(&lib, low, 0.30, 0.1);
        with_features(&lib, high, 0.35, 1.0);
        with_features(&lib, starred, 0.10, 0.1);
        lib.sql(
            "UPDATE detections SET stars=5,reviewed=1,rejected=1 WHERE id=?",
            [starred],
        );
        lib.sql(
            "UPDATE images SET rating=0.35, sharpness=0.5 WHERE id=?",
            [f2],
        );
        std::fs::create_dir_all(lib.root.join("models")).unwrap();
        std::fs::write(
            lib.root.join("models/sharpness.json"),
            serde_json::to_string(&model()).unwrap(),
        )
        .unwrap();

        let started = lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        assert!(started["operation"]
            .as_str()
            .unwrap()
            .starts_with("Re-scoring"));
        lib.wait_idle();
        let task = lib.task("Re-scoring");
        assert_eq!(task["state"], "done", "{task}");
        assert!(
            task["detail"]
                .as_str()
                .unwrap()
                .contains("1 kept as stored"),
            "{task}"
        );

        let rating =
            |det: i64| -> f64 { lib.one("SELECT rating FROM detections WHERE id=?", [det]) };
        // 3*1.0+0.5 = 3.5 stars -> 0.825; 3*0.1+0.5 = 0.8 stars -> 0.24.
        assert!((rating(high) - 0.825).abs() < 1e-9, "{}", rating(high));
        assert!((rating(low) - 0.24).abs() < 1e-9, "{}", rating(low));
        assert_eq!(
            rating(no_features),
            0.66,
            "no stored features: left as it was"
        );
        let verdict: String = lib.one("SELECT rating_verdict FROM detections WHERE id=?", [high]);
        assert_eq!(verdict, "good");
        // The hand star, reject and review are untouched.
        let (stars, rejected, reviewed): (i64, i64, i64) = lib
            .db()
            .query_row(
                "SELECT stars,rejected,reviewed FROM detections WHERE id=?",
                [starred],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((stars, rejected, reviewed), (5, 1, 1));
        // The frame's own rating follows its subject; keepers were re-picked.
        let frame: f64 = lib.one("SELECT rating FROM images WHERE id=?", [f2]);
        assert!((frame - 0.825).abs() < 1e-9);
        let frame: f64 = lib.one("SELECT rating FROM images WHERE id=?", [f3]);
        assert_eq!(frame, 0.66);
        let keeper: i64 = lib.one("SELECT burst_pick FROM detections WHERE id=?", [high]);
        assert_eq!(keeper, 1);
        // Reading the stored features again changes nothing.
        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        assert!((rating(high) - 0.825).abs() < 1e-9);
    }

    #[test]
    fn rescan_reads_pixels_again_and_spares_hand_work() {
        let lib = Lib::new("rescan");
        let (w, h) = (1200usize, 800usize);
        let mut seed = 7u32;
        let data: Vec<u8> = (0..w * h * 3)
            .map(|_| {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 24) as u8
            })
            .collect();
        crate::desktop::save_jpeg(
            &conrod_vision::imageops::Rgb::new(w, h, data),
            &lib.root.join("real.jpg"),
        )
        .unwrap();
        let (real, gone) = (lib.frame("real.jpg", None), lib.frame("gone.jpg", None));
        let (car, lost) = (
            lib.detection(real, "vehicle", 0.5),
            lib.detection(gone, "vehicle", 0.5),
        );
        for det in [car, lost] {
            lib.sql(
                "UPDATE detections SET x1=200,y1=150,x2=1000,y2=650,features='[]',heuristic=0.123,panning=1,sharp_end='left' WHERE id=?",
                [det],
            );
        }
        lib.sql(
            "UPDATE detections SET stars=5,reviewed=1,rejected=1,number='42',number_source='manual',attributes='{\"make\":\"Ford\"}' WHERE id=?",
            [car],
        );

        let started = lib.run("rescan", json!({"jobId": lib.job})).unwrap();
        assert!(started["operation"]
            .as_str()
            .unwrap()
            .starts_with("Rescanning"));
        lib.wait_idle();
        let task = lib.task("Rescanning");
        assert_eq!(task["state"], "done", "{task}");
        assert!(
            task["detail"].as_str().unwrap().contains("1 photos unread"),
            "{task}"
        );

        // Recomputed from the pixels.
        let features: String = lib.one("SELECT features FROM detections WHERE id=?", [car]);
        let features: Vec<f64> = serde_json::from_str(&features).unwrap();
        assert_eq!(features.len(), sharpness::FEATURE_NAMES.len());
        let heuristic: f64 = lib.one("SELECT heuristic FROM detections WHERE id=?", [car]);
        let rating: f64 = lib.one("SELECT rating FROM detections WHERE id=?", [car]);
        assert_ne!(heuristic, 0.123);
        assert!(
            (rating - heuristic).abs() < 1e-9,
            "no model: the heuristic is the rating"
        );
        // Hand work and the box stand.
        let kept: (i64, i64, i64, String, String, f64) = lib
            .db()
            .query_row(
                "SELECT stars,reviewed,rejected,number,attributes,x2 FROM detections WHERE id=?",
                [car],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            kept,
            (5, 1, 1, "42".into(), "{\"make\":\"Ford\"}".into(), 1000.0)
        );
        // A missing original is left exactly as it was.
        let (heuristic, sharp_end): (f64, String) = lib
            .db()
            .query_row(
                "SELECT heuristic,sharp_end FROM detections WHERE id=?",
                [lost],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((heuristic, sharp_end.as_str()), (0.123, "left"));
    }

    #[test]
    fn rescore_refuses_an_unknown_album() {
        let lib = Lib::new("rescore-none");
        assert!(lib
            .run("rescore", json!({"jobId": 77}))
            .unwrap_err()
            .contains("No such album"));
    }

    #[test]
    fn a_portrait_frame_takes_its_softer_eye_and_keeps_a_shut_face_shut() {
        let lib = Lib::new("rescore-eyes");
        let frame = lib.frame("1.jpg", Some(1));
        let left = lib.detection(frame, "eye", 0.95);
        let right = lib.detection(frame, "eye", 0.90);
        let face = lib.detection(frame, "face", 0.94);
        for (det, score) in [(left, 0.95), (right, 0.90), (face, 0.94)] {
            with_features(&lib, det, score, 0.1);
            lib.sql("UPDATE detections SET cull_reason='' WHERE id=?", [det]);
        }
        lib.sql(
            "UPDATE detections SET attributes=? WHERE id=?",
            params![
                json!({"eyes_open": [0.1, 0.2], "eyes_closed": true}).to_string(),
                face
            ],
        );
        lib.sql(
            "UPDATE jobs SET settings_json=? WHERE id=?",
            params![json!({"scan_profile": "portrait"}).to_string(), lib.job],
        );

        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();

        let rating: f64 = lib.one("SELECT rating FROM images WHERE id=?", [frame]);
        assert!(
            (rating - 0.90).abs() < 1e-9,
            "the softer eye decides the portrait: {rating}"
        );
        let cull: String = lib.one("SELECT cull_reason FROM detections WHERE id=?", [face]);
        assert_eq!(
            cull,
            crate::EYES_CLOSED,
            "a re-measure must not open shut eyes"
        );
        let cull: String = lib.one("SELECT cull_reason FROM detections WHERE id=?", [left]);
        assert_eq!(cull, "", "an open eye keeps its clean sheet");
    }

    #[test]
    fn a_missed_frame_is_measured_inside_its_af_box_and_a_detected_one_is_not() {
        let lib = Lib::new("af-measure");
        // A readable fixture: deterministic value noise at several block
        // scales, so edges run in every direction (as a real subject's do) and
        // the native-pixel sigma, normalised to the 6000 px reference the way
        // `cull_frame` does it, reads as sharp.
        let (w, h) = (2400usize, 1600usize);
        let hash = |x: usize, y: usize| {
            let mut v = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
            v ^= v >> 13;
            v = v.wrapping_mul(0xC2B2_AE35);
            v ^ (v >> 16)
        };
        let mut data = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let mut level = 0.0f64;
                for (block, amp) in [(16usize, 90.0), (8, 60.0), (4, 40.0), (2, 25.0)] {
                    let n = hash(x / block, y / block) % 256;
                    level += amp * (f64::from(n) / 255.0 - 0.5);
                }
                let v = (128.0 + level).clamp(0.0, 255.0) as u8;
                data.extend([v, v, v]);
            }
        }
        crate::desktop::save_jpeg(
            &conrod_vision::imageops::Rgb::new(w, h, data),
            &lib.root.join("af.jpg"),
        )
        .unwrap();
        let bare = lib.frame("af.jpg", None);
        lib.sql(
            "UPDATE images SET af_x1=0.25,af_y1=0.25,af_x2=0.75,af_y2=0.75 WHERE id=?",
            [bare],
        );
        // A frame with a detection already has a better box: left alone.
        let seen = lib.frame("seen.jpg", None);
        lib.detection(seen, "vehicle", 0.9);
        lib.sql(
            "UPDATE images SET af_x1=0.25,af_y1=0.25,af_x2=0.75,af_y2=0.75 WHERE id=?",
            [seen],
        );
        // A stored box whose original is gone cannot be read.
        let gone = lib.frame("gone.jpg", None);
        lib.sql(
            "UPDATE images SET af_x1=0.25,af_y1=0.25,af_x2=0.75,af_y2=0.75 WHERE id=?",
            [gone],
        );

        let task = lib.d().hub.start("Measuring", 0);
        let (measured, unread) = measure_af_frames(lib.d(), lib.job, &cfg(), &task).unwrap();
        task.finish();

        assert_eq!((measured, unread), (1, 1));
        let score: Option<f64> = lib.one("SELECT af_sharpness FROM images WHERE id=?", [bare]);
        assert!(score.is_some_and(|s| s > 0.5), "a sharp AF box: {score:?}");
        let kept: Option<f64> = lib.one("SELECT af_sharpness FROM images WHERE id=?", [seen]);
        assert_eq!(kept, None, "a detected frame is never AF-measured");
    }

    /// A readable fixture: deterministic value noise at several block scales, so
    /// edges run in every direction (as a real subject's do), with one rectangle
    /// box-blurred so its edge census reads as a smeared subject while the rest
    /// of the frame stays sharp. `blurred` is in pixels; an empty rect blurs
    /// nothing.
    fn noise_data(w: usize, h: usize, blurred: [usize; 4]) -> Vec<u8> {
        let hash = |x: usize, y: usize| {
            let mut v = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
            v ^= v >> 13;
            v = v.wrapping_mul(0xC2B2_AE35);
            v ^ (v >> 16)
        };
        let mut grey = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut level = 0.0f64;
                for (block, amp) in [(16usize, 90.0), (8, 60.0), (4, 40.0), (2, 25.0)] {
                    let n = hash(x / block, y / block) % 256;
                    level += amp * (f64::from(n) / 255.0 - 0.5);
                }
                grey.push((128.0 + level).clamp(0.0, 255.0) as u8);
            }
        }
        // A 5x5 box blur over the rectangle: a defocused, smeared subject.
        let [x0, y0, x1, y1] = blurred;
        let src = grey.clone();
        for y in y0..y1.min(h) {
            for x in x0..x1.min(w) {
                let (mut sum, mut n) = (0u32, 0u32);
                for dy in -2i32..=2 {
                    for dx in -2i32..=2 {
                        let sx = (x as i32 + dx).clamp(0, w as i32 - 1) as usize;
                        let sy = (y as i32 + dy).clamp(0, h as i32 - 1) as usize;
                        sum += u32::from(src[sy * w + sx]);
                        n += 1;
                    }
                }
                grey[y * w + x] = (sum / n) as u8;
            }
        }
        grey.iter().flat_map(|&v| [v, v, v]).collect()
    }

    /// The pixels an AF box's fractions cover, as the engine reads them.
    fn af_pixels(area: [f64; 4], w: usize, h: usize) -> [usize; 4] {
        [
            (area[0] * w as f64) as usize,
            (area[1] * h as f64) as usize,
            (area[2] * w as f64) as usize,
            (area[3] * h as f64) as usize,
        ]
    }

    /// What a re-score measures for one AF box: the whole-box edge blur, rated
    /// with the album's own strictness.
    fn af_score(lib: &Lib, name: &str, area: [f64; 4]) -> f64 {
        let frame = conrod_io::raw::read(&lib.root.join(name)).unwrap();
        let image = conrod_vision::imageops::Rgb::decode_jpeg(&frame.preview, 1)
            .unwrap()
            .orient(frame.orientation);
        let grey = image.to_gray();
        let (fw, fh) = (grey.width, grey.height);
        let rect = [
            area[0] * fw as f64,
            area[1] * fh as f64,
            area[2] * fw as f64,
            area[3] * fh as f64,
        ];
        let blur = crate::subject_blur(&grey, rect, fw.max(fh));
        let strictness = settings(lib.d(), lib.job).unwrap().effective_blur();
        sharpness::rating_from_blur(blur.whole, sharpness::BlurStrictness::parse(&strictness))
            .expect("the AF box measures")
    }

    fn save_frame(lib: &Lib, name: &str, w: usize, h: usize, data: &[u8]) {
        crate::desktop::save_jpeg(
            &conrod_vision::imageops::Rgb::new(w, h, data.to_vec()),
            &lib.root.join(name),
        )
        .unwrap();
    }

    #[test]
    fn a_detected_subject_is_scored_inside_the_af_box_that_sits_on_it() {
        let lib = Lib::new("af-subject");
        let (w, h) = (2400usize, 1600usize);
        // The blurred patch sits in the detection box's margins, so the
        // detection-box measure would read the sharp noise around it.
        let af = [0.72, 0.62, 0.84, 0.74];
        let data = noise_data(w, h, af_pixels(af, w, h));
        save_frame(&lib, "af.jpg", w, h, &data);
        save_frame(&lib, "plain.jpg", w, h, &data);
        let framed = lib.frame("af.jpg", Some(1));
        let plain = lib.frame("plain.jpg", Some(1));
        let (with_af, control) = (
            lib.detection(framed, "vehicle", 0.95),
            lib.detection(plain, "vehicle", 0.95),
        );
        for det in [with_af, control] {
            lib.sql(
                "UPDATE detections SET x1=200,y1=200,x2=2200,y2=1400,heuristic=0.95 WHERE id=?",
                [det],
            );
        }
        lib.sql(
            "UPDATE images SET af_x1=?,af_y1=?,af_x2=?,af_y2=? WHERE id=?",
            params![af[0], af[1], af[2], af[3], framed],
        );

        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();

        let af_rating: f64 = lib.one("SELECT rating FROM detections WHERE id=?", [with_af]);
        let control_rating: f64 = lib.one("SELECT rating FROM detections WHERE id=?", [control]);
        let expected = af_score(&lib, "af.jpg", af);
        assert!(
            (af_rating - expected).abs() < 1e-9,
            "the stored rating follows the AF box: {af_rating} vs {expected}"
        );
        assert!(
            expected < sharpness::KEEPER_MIN_RATING,
            "the fixture's AF box is the worse measure: {expected}"
        );
        assert!(
            control_rating >= sharpness::KEEPER_MIN_RATING && control_rating > expected + 0.05,
            "the same pixels scored on the detection box would have kept their stars: {control_rating} vs {expected}"
        );
    }

    #[test]
    fn the_focus_area_is_a_second_opinion_and_can_only_lower_a_score() {
        // A smeared car: the AF box reads worse, and that is the score.
        assert_eq!(second_opinion(Some(0.92), Some(0.77)), Some(0.77));
        // The AF box landed on a sharp corner of a soft subject: no rescue.
        assert_eq!(second_opinion(Some(0.86), Some(0.98)), Some(0.86));
        // One measure only.
        assert_eq!(second_opinion(Some(0.86), None), Some(0.86));
        assert_eq!(second_opinion(None, Some(0.77)), Some(0.77));
        assert_eq!(second_opinion(None, None), None);
    }

    /// A 2400x1600 frame of sharp noise with a smeared "car" at `car` (pixels)
    /// and one detection on it, stored as an album frame with the given AF box.
    /// Returns (frame, detection).
    fn smeared_car_frame(
        lib: &Lib,
        name: &str,
        car: [usize; 4],
        af: [f64; 4],
        mode: i64,
    ) -> (i64, i64) {
        let (w, h) = (2400usize, 1600usize);
        save_frame(lib, name, w, h, &noise_data(w, h, car));
        let frame = lib.frame(name, Some(1));
        lib.sql(
            "UPDATE images SET width=?,height=?,af_x1=?,af_y1=?,af_x2=?,af_y2=?,af_mode=? WHERE id=?",
            params![w as i64, h as i64, af[0], af[1], af[2], af[3], mode, frame],
        );
        let det = lib.detection(frame, "vehicle", 0.95);
        lib.sql(
            "UPDATE detections SET x1=?,y1=?,x2=?,y2=?,heuristic=0.95,rating=0.95,cull_reason='2 stars' WHERE id=?",
            params![car[0] as f64, car[1] as f64, car[2] as f64, car[3] as f64, det],
        );
        (frame, det)
    }

    fn rescore_and_wait(lib: &Lib) {
        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
    }

    #[test]
    fn an_af_box_off_every_subject_is_the_subject_and_the_detection_a_bystander() {
        let lib = Lib::new("af-off");
        // The smeared car sits top-left; the camera focused on sharp ground to
        // the right, a 3% patch outside every detection.
        let af = [0.70, 0.55, 0.85, 0.70];
        let (frame, det) = smeared_car_frame(&lib, "af.jpg", [100, 100, 900, 700], af, 9);

        rescore_and_wait(&lib);

        let expected = af_score(&lib, "af.jpg", af);
        let af_sharp: Option<f64> = lib.one("SELECT af_sharpness FROM images WHERE id=?", [frame]);
        assert_eq!(af_sharp, Some(expected), "the AF area is measured");
        let rating: f64 = lib.one("SELECT rating FROM images WHERE id=?", [frame]);
        assert!(
            (rating - expected).abs() < 1e-9,
            "the frame is rated on the AF area, not the car: {rating} vs {expected}"
        );
        let (bystander, auto): (i64, i64) = lib
            .db()
            .query_row(
                "SELECT bystander,af_bystander FROM detections WHERE id=?",
                [det],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((bystander, auto), (1, 1));
        let cull: Option<String> = lib.one("SELECT cull_reason FROM detections WHERE id=?", [det]);
        assert!(
            cull.unwrap_or_default().is_empty(),
            "a bystander drives no cull"
        );
        // And it no longer competes: the frame is its own candidate.
        let picked: Option<i64> = lib.one("SELECT burst_pick FROM images WHERE id=?", [frame]);
        let det_pick: Option<i64> = lib.one("SELECT burst_pick FROM detections WHERE id=?", [det]);
        assert_eq!(det_pick, None, "a bystander is never the keeper");
        assert_eq!(
            picked.unwrap_or_default() == 1,
            expected >= sharpness::KEEPER_MIN_RATING,
            "the frame itself is the keeper candidate when its AF area clears the bar"
        );
    }

    #[test]
    fn a_hand_decision_about_a_detection_outlives_the_focus_area() {
        let lib = Lib::new("af-hand");
        let af = [0.70, 0.55, 0.85, 0.70];
        let (_, kept) = smeared_car_frame(&lib, "a.jpg", [100, 100, 900, 700], af, 9);
        // The photographer said this car is not a bystander.
        lib.sql(
            "UPDATE detections SET bystander=0,af_bystander=0,reviewed=1 WHERE id=?",
            [kept],
        );
        let (_, marked) = smeared_car_frame(&lib, "b.jpg", [100, 100, 900, 700], af, 9);
        // ... and this one is, set by hand before the camera's word counted.
        lib.sql("UPDATE detections SET bystander=1 WHERE id=?", [marked]);

        rescore_and_wait(&lib);

        let state = |det: i64| -> (i64, Option<i64>) {
            lib.db()
                .query_row(
                    "SELECT COALESCE(bystander,0),af_bystander FROM detections WHERE id=?",
                    [det],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap()
        };
        assert_eq!(state(kept), (0, Some(0)), "an unmarked car stays unmarked");
        assert_eq!(state(marked), (1, None), "a hand mark is not the camera's");
    }

    #[test]
    fn the_marks_the_focus_area_made_are_taken_back_when_it_lands_on_a_subject() {
        let lib = Lib::new("af-undo");
        // The focus area is now inside the car.
        let af = [0.10, 0.10, 0.20, 0.20];
        let (frame, det) = smeared_car_frame(&lib, "u.jpg", [100, 100, 900, 700], af, 9);
        lib.sql(
            "UPDATE detections SET bystander=1,af_bystander=1,cull_reason=NULL WHERE id=?",
            [det],
        );
        lib.sql("UPDATE images SET af_sharpness=0.9 WHERE id=?", [frame]);

        rescore_and_wait(&lib);

        let (bystander, auto): (i64, Option<i64>) = lib
            .db()
            .query_row(
                "SELECT COALESCE(bystander,0),af_bystander FROM detections WHERE id=?",
                [det],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((bystander, auto), (0, None));
        let af_sharp: Option<f64> = lib.one("SELECT af_sharpness FROM images WHERE id=?", [frame]);
        assert_eq!(af_sharp, None);
    }

    #[test]
    fn a_large_af_zone_is_not_the_subject() {
        let lib = Lib::new("af-zone");
        // Mode 22, whole-area AF: 60% of the frame, centred off the car.
        let zone = [0.30, 0.20, 0.95, 0.90];
        let (frame, det) = smeared_car_frame(&lib, "z.jpg", [100, 100, 500, 400], zone, 22);

        rescore_and_wait(&lib);

        let bystander: Option<i64> = lib.one("SELECT bystander FROM detections WHERE id=?", [det]);
        assert_eq!(
            bystander.unwrap_or_default(),
            0,
            "the car stays the subject"
        );
        let af_sharp: Option<f64> = lib.one("SELECT af_sharpness FROM images WHERE id=?", [frame]);
        assert_eq!(af_sharp, None, "the zone is not measured as a subject");
    }

    #[test]
    fn the_backfill_marks_bystanders_without_reading_a_frame_the_area_does_not_decide() {
        let lib = Lib::new("af-backfill");
        let af = [0.70, 0.55, 0.85, 0.70];
        let (frame, det) = smeared_car_frame(&lib, "bf.jpg", [100, 100, 900, 700], af, 9);
        // A frame whose AF centre is inside its car needs no pixels at all:
        // its original is not even on disk.
        let (inside, inside_det) = smeared_car_frame(
            &lib,
            "gone.jpg",
            [100, 100, 900, 700],
            [0.1, 0.1, 0.2, 0.2],
            9,
        );
        std::fs::remove_file(lib.root.join("gone.jpg")).unwrap();

        let task = lib.d().hub.start("Measuring", 0);
        let (measured, unread) = measure_af_frames(lib.d(), lib.job, &cfg(), &task).unwrap();
        task.finish();

        assert_eq!((measured, unread), (1, 0));
        let bystander: i64 = lib.one(
            "SELECT COALESCE(bystander,0) FROM detections WHERE id=?",
            [det],
        );
        assert_eq!(bystander, 1);
        let kept: i64 = lib.one(
            "SELECT COALESCE(bystander,0) FROM detections WHERE id=?",
            [inside_det],
        );
        assert_eq!(kept, 0);
        let cull: Option<String> = lib.one("SELECT cull_reason FROM detections WHERE id=?", [det]);
        assert!(cull.unwrap_or_default().is_empty());
        let none: Option<f64> = lib.one("SELECT af_sharpness FROM images WHERE id=?", [inside]);
        assert_eq!(none, None);
        let score: Option<f64> = lib.one("SELECT af_sharpness FROM images WHERE id=?", [frame]);
        assert!(score.is_some());
    }

    #[test]
    fn a_frame_without_an_af_box_keeps_the_detection_box_measure() {
        let lib = Lib::new("af-none");
        let (w, h) = (2400usize, 1600usize);
        let data = noise_data(w, h, [0; 4]);
        save_frame(&lib, "plain.jpg", w, h, &data);
        let frame = lib.frame("plain.jpg", Some(1));
        let car = lib.detection(frame, "vehicle", 0.95);
        lib.sql(
            "UPDATE detections SET x1=400,y1=400,x2=2000,y2=1200,heuristic=0.95 WHERE id=?",
            [car],
        );

        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        let rating: f64 = lib.one("SELECT rating FROM detections WHERE id=?", [car]);
        assert!(
            rating >= sharpness::KEEPER_MIN_RATING,
            "no AF box: the detection box is measured as before: {rating}"
        );
        let af: Option<f64> = lib.one("SELECT af_x1 FROM images WHERE id=?", [frame]);
        assert_eq!(af, None, "nothing invented an AF box");
        // A second re-score of the same pixels keeps the same number.
        lib.run("rescore", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        let again: f64 = lib.one("SELECT rating FROM detections WHERE id=?", [car]);
        assert_eq!(again, rating);
    }
}
