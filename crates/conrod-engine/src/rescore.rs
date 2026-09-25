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
    profile::{ScanProfile, Subject},
    ridge::SharpModel,
    settings::Settings,
    tasks::Task,
};
use conrod_vision::sharpness;
use rusqlite::params;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
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
    // A face is judged on its own; a vehicle or person also by how much of it is in frame.
    let edges = if small {
        framing::Framing::default()
    } else {
        framing::assess(Some(s.bbox), s.frame.0, s.frame.1)
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
            "UPDATE detections SET sharpness=?,sharpness_verdict=?,rating=?,rating_verdict=?,cull_reason=? WHERE id=?",
            params![o.sharpness, o.sharpness_verdict, o.rating, o.rating_verdict, o.cull_reason, id],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
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
                    frame
                        .best
                        .entry(region.into())
                        .and_modify(|b| *b = b.max(rating))
                        .or_insert(rating);
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
                frame
                    .best
                    .entry(region.into())
                    .and_modify(|b| *b = b.max(outcome.rating))
                    .or_insert(outcome.rating);
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
    let picked = pick_of_pass(&lock(&d.db), job, profile, cfg.include_people)?;
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
            assert_eq!(scanned.features.len(), 18);
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
    fn rescore_refuses_an_unknown_album() {
        let lib = Lib::new("rescore-none");
        assert!(lib
            .run("rescore", json!({"jobId": 77}))
            .unwrap_err()
            .contains("No such album"));
    }
}
