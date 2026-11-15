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
use crate::desktop::{rows, Desktop, Result};
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
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

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

/// The action: re-measure every stored subject of the album, as a background operation.
pub fn rescore(d: &Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    if !d.status()["activeJob"].is_null() {
        return Err("A scan is running. Stop it first.".into());
    }
    let cfg = settings(d, job)?;
    launch(d, job, "Re-measuring", move |d, stop, task| {
        run(d, job, &cfg, stop, task)
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
        run(d, job, &cfg, stop, task)?;
        task.detail(format!(
            "Rescanned the album; {unread} photos could not be read and were left as they were"
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
        "SELECT id, path FROM images WHERE job_id=? AND status='done' ORDER BY id",
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
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get() * 4 / 5)
        .clamp(2, 16);
    let pan_ok = ShootPreset::parse(&cfg.scan_profile).pan_compatible();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let n = next.fetch_add(1, Ordering::Relaxed);
                if n >= images.len() || stop.load(Ordering::Relaxed) {
                    return;
                }
                let image = &images[n];
                let id = image["id"].as_i64().unwrap_or_default();
                let path = std::path::Path::new(image["path"].as_str().unwrap_or_default());
                let empty = Vec::new();
                match measure_frame(path, by_image.get(&id).unwrap_or(&empty), cfg, pan_ok) {
                    Ok((found, whole)) => {
                        let db = lock(&d.db);
                        for (det, m) in found {
                            let _ = db.execute(
                                "UPDATE detections SET features=?,heuristic=?,panning=?,sharp_end=? WHERE id=?",
                                params![serde_json::to_string(&m.features).unwrap_or_default(), m.heuristic, m.panning, m.sharp_end, det],
                            );
                        }
                        if let Some(whole) = whole {
                            let _ = db.execute(
                                "UPDATE images SET sharpness=? WHERE id=?",
                                params![whole, id],
                            );
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
    Ok(unread.into_inner())
}

type Measures = (Vec<(i64, sharpness::Sharpness)>, Option<f64>);

/// What `cull_frame` would have measured for each stored subject of one frame
/// (subject id, measure), and for the frame as a whole.
fn measure_frame(
    path: &std::path::Path,
    subjects: &[&Value],
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
    let mut out = Vec::new();
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
            // As `cull_frame` scores a subject: the edge-blur estimate inside
            // the stored box on native pixels; the tile score stays the fallback
            // and still supplies the pan flag and the features.
            let region = [
                (bbox[0].max(0.0) as usize).min(fw),
                (bbox[1].max(0.0) as usize).min(fh),
                (bbox[2].max(0.0) as usize).min(fw),
                (bbox[3].max(0.0) as usize).min(fh),
            ];
            let blur = conrod_vision::edgeblur::measure(&grey, Some(region));
            if let Some(score) =
                sharpness::rating_from_blur(
                    blur.sigma_worst_at_reference(fw.max(fh)),
                    sharpness::BlurStrictness::parse(&cfg.blur_strictness),
                )
            {
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
            out.push((s["id"].as_i64().unwrap_or_default(), focus));
        }
    }
    let whole = sharpness::measure(&grey, None, None);
    let whole = whole.measured.then_some(whole.score);
    // As `cull_frame` rates a subject-less frame: on its in-focus area.
    let whole = if subjects.is_empty() {
        match (whole, sharpness::focal_score(&grey)) {
            (Some(whole), Some(focal)) => Some(whole.max(focal)),
            (whole, focal) => whole.or(focal),
        }
    } else {
        whole
    };
    Ok((out, whole))
}

/// A frame's subjects' ratings, for the rating of the frame itself.
#[derive(Default)]
struct Frame {
    whole: Option<f64>,
    best: HashMap<String, f64>,
    /// Person and vehicle box areas, for the main subject.
    boxes: Vec<(Subject, f64)>,
}

fn write(d: &Desktop, batch: &[(i64, Outcome)]) -> Result<()> {
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    for (id, o) in batch {
        tx.execute(
            // Shut eyes are OCEC's verdict, not a focus one: re-measuring keeps it.
            "UPDATE detections SET sharpness=?1,sharpness_verdict=?2,rating=?3,rating_verdict=?4,cull_reason=CASE WHEN region_type='face' AND COALESCE(CASE WHEN json_valid(attributes) THEN json_extract(attributes,'$.eyes_closed') END,0) THEN 'eyes closed' ELSE ?5 END WHERE id=?6",
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
) -> Result<()> {
    let models = load_models(d);
    let profile = ScanProfile::parse(&cfg.scan_profile);
    let found = rows(
        &lock(&d.reader),
        "SELECT d.id, d.image_id, d.x1, d.y1, d.x2, d.y2, COALESCE(d.region_type,'vehicle') AS region, d.features, d.heuristic, d.panning, d.rating, i.width, i.height, i.sharpness AS whole FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? ORDER BY d.id",
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
            return Ok(());
        }
        let region = row["region"].as_str().unwrap_or("vehicle");
        let frame = frames
            .entry(row["image_id"].as_i64().unwrap_or_default())
            .or_default();
        frame.whole = row["whole"].as_f64();
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
                let outcome = remeasure(
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
            tx.execute(
                "UPDATE images SET rating=? WHERE id=?",
                params![rating, image],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
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
        "Re-measured {} subjects: {changed} changed, {skipped} could not be (no stored features); {} keepers",
        found.len() - skipped,
        picked["picks"]
    ));
    Ok(())
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
            .starts_with("Re-measuring"));
        lib.wait_idle();
        let task = lib.task("Re-measuring");
        assert_eq!(task["state"], "done", "{task}");
        assert!(
            task["detail"].as_str().unwrap().contains("1 could not be"),
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
            task["detail"]
                .as_str()
                .unwrap()
                .contains("1 photos could not"),
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
            cull, crate::EYES_CLOSED,
            "a re-measure must not open shut eyes"
        );
        let cull: String = lib.one("SELECT cull_reason FROM detections WHERE id=?", [left]);
        assert_eq!(cull, "", "an open eye keeps its clean sheet");
    }
}
