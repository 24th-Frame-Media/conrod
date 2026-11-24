//! Run one pipeline stage again on an album that is already scanned, on demand,
//! without redoing the whole album.
//!
//! This is a thin dispatch over the operations the scan itself uses, not a second
//! implementation of them: `detect` re-finds boxes through `cull_frame` and merges
//! them into the stored rows (so a reviewed or read subject keeps its markings),
//! `rate` is `rescore`'s pixel re-measure and derive, `identify` is the existing
//! incremental identify run, `moments` re-judges through the moment lane, and
//! `pick` is `pick_of_pass`. No stage runs another: each leaves the album ready
//! for the photographer to ask for the next one by hand.
//!
//! One rerun at a time, and never beside a scan: the guard refuses while a scan
//! is active, while the album waits in the scan queue, while the album already
//! has a background operation, or while any other stage rerun is running.

use crate::commands::RerunStage;
use crate::desktop::{insert_subject, rows, write_frame_measurements, Desktop, Result};
use crate::err;
use crate::library::require_job;
use crate::lock;
use crate::operations::{launch, settings};
use conrod_core::{models, profile::ScanProfile, settings::Settings};
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// The operation-name prefix every stage rerun shares, so the busy guard and the
/// review screen can recognise a rerun whichever stage it is.
pub const KIND: &str = "Rerunning";

/// A box overlap at or above this counts as the same subject found again: enough
/// that a re-detected car keeps its old row (and its readings), loose enough that
/// a slightly tighter or wider box still matches.
const MATCH_IOU: f64 = 0.5;

pub fn rerun_stage(
    d: &Arc<Desktop>,
    job: i64,
    stage: RerunStage,
    full: bool,
    scope: Option<&str>,
) -> Result<Value> {
    rerun_stage_only(d, job, stage, full, scope, None)
}

pub fn rerun_stage_only(
    d: &Arc<Desktop>,
    job: i64,
    stage: RerunStage,
    full: bool,
    scope: Option<&str>,
    only_stage: Option<&'static str>,
) -> Result<Value> {
    require_job(d, job)?;
    let status = d.status();
    if !status["activeJob"].is_null() {
        return Err("A scan is running. Stop it first.".into());
    }
    if status["queue"]
        .as_array()
        .is_some_and(|q| q.iter().any(|j| j.as_i64() == Some(job)))
    {
        return Err(
            "This album is waiting to scan. Let it finish or take it out of the line first.".into(),
        );
    }
    {
        let ops = lock(&d.operations);
        if ops.keys().any(|k| k.ends_with(&format!(":{job}"))) {
            return Err("This album already has a background operation".into());
        }
        if ops.keys().any(|k| k.starts_with(&format!("{KIND} "))) {
            return Err("A stage is already running again. Wait for it to finish.".into());
        }
    }
    let cfg = settings(d, job)?;
    match stage {
        RerunStage::Detect => rerun_detect(d, job, cfg),
        RerunStage::Rate => rerun_rate(d, job, cfg),
        // The existing identify run: incremental by default (only the subjects
        // nobody has read), `full` reads every unaccepted subject again. When
        // `only_stage` is set (`identify:ocr` or `identify:vision`), only that
        // sub-stage runs.
        RerunStage::Identify => crate::operations::identify_stage(
            d,
            job,
            scope,
            !full,
            &format!("{KIND} identify"),
            only_stage,
        ),
        RerunStage::Moments => {
            let rejudge = only_stage.is_none() || full;
            if !rejudge {
                // An incremental Decision rerun on a judged album would ask
                // nothing and still report success. Say so plainly instead,
                // and say how to judge everything again.
                let lanes = cfg.lanes();
                let scope = crate::operations::run_scope(&cfg.identify_scope, None);
                if crate::moments::unjudged_work(d, job, &lanes, scope)? == 0 {
                    return Ok(nothing_new_to_judge());
                }
            }
            rerun_moments(d, job, cfg, rejudge)
        }
        RerunStage::Pick => rerun_pick(d, job, cfg),
    }
}

/// The answer to an incremental Decision rerun that has nothing to ask: no
/// operation starts, and the caller can tell this from a finished run.
/// `{"ok": true, "nothing_new": true, "judged": 0, "message": "..."}`; sending
/// `stage_rerun` again with `"full": true` judges every frame again.
pub(crate) fn nothing_new_to_judge() -> Value {
    json!({
        "ok": true,
        "nothing_new": true,
        "stage": conrod_core::progress::IDENTIFY_DECISION,
        "judged": 0,
        "message": "Nothing new to judge: every frame already has a decision. Run the stage with full set to judge them all again.",
    })
}

/// `rate`: re-read the pixels and re-derive every rating, verdict and cull, then
/// re-pick — exactly `rescore`, under the rerun name. Hand stars, rejects and
/// reviews live in other columns and are never written, so a photographer-rated
/// frame is never demoted; the sharp-frame rules apply as they do at scan.
fn rerun_rate(d: &Arc<Desktop>, job: i64, cfg: Settings) -> Result<Value> {
    launch(d, job, &format!("{KIND} rate"), move |d, stop, task| {
        let unread = crate::rescore::remeasure_pixels(d, job, &cfg, stop, task)?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let (subjects, changed, skipped, keepers) = crate::rescore::run(d, job, &cfg, stop, task)?;
        task.detail(format!(
            "Re-scored {subjects} subjects ({changed} changed, {skipped} kept as stored), {unread} photos unread, {keepers} keepers"
        ));
        Ok(())
    })
}

/// `moments`: ask the judge again about every keeper frame (or every frame in the
/// everything scope), then re-pick so the new decisions rank. A full `moments`
/// rerun rejudges every frame (`rejudge = true`), while a per-stage
/// `identify:decision` rerun respects `full` (incremental by default). A failed
/// answer leaves a frame's old verdict in place. Returns the counts the screen
/// toasts.
fn rerun_moments(d: &Arc<Desktop>, job: i64, cfg: Settings, rejudge: bool) -> Result<Value> {
    let lanes = cfg.lanes();
    let scope = crate::operations::run_scope(&cfg.identify_scope, None);
    launch(d, job, &format!("{KIND} moments"), move |d, stop, task| {
        // A rejudge is a new decision pass: its own frames and answers.
        crate::decider::service().begin_run();
        let judged = crate::moments::run(d, job, stop, task, &cfg, &lanes, scope, rejudge)?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        // How many frames now carry a moment the judge actually saw.
        let found: i64 = lock(&d.reader)
            .query_row(
                "SELECT count(*) FROM images WHERE job_id=? AND COALESCE(copy_of,0)=0 \
                 AND moments IS NOT NULL AND json_valid(moments) \
                 AND json_array_length(json_extract(moments,'$.readings')) > 0",
                [job],
                |r| r.get(0),
            )
            .map_err(err)?;
        let picked = crate::passes::pick_of_pass(&lock(&d.db), job, &lanes, cfg.use_quality_score)?;
        let keepers = picked["picks"].as_u64().unwrap_or_default();
        task.detail(format!(
            "Judged {judged} frames · {found} moments found · {keepers} keepers"
        ));
        Ok(())
    })
}

/// `pick`: recompute the keeper of every pass. A hand pick or hand star always
/// wins (`pick_of_pass` ranks `by_hand` first and the grid treats a hand pick as
/// the effective keeper), so this only moves the engine's own suggestions.
fn rerun_pick(d: &Arc<Desktop>, job: i64, cfg: Settings) -> Result<Value> {
    let lanes = cfg.lanes();
    launch(d, job, &format!("{KIND} pick"), move |d, stop, task| {
        // One whole-album pass: a pause holds it at its edge.
        if !d.stage_control(job).park_with("pick", stop, None) {
            return Ok(());
        }
        let stats = crate::passes::pick_of_pass(&lock(&d.db), job, &lanes, cfg.use_quality_score)?;
        task.detail(format!(
            "Picked {} keepers from {} frames across {} passes",
            stats["picks"], stats["considered"], stats["passes"]
        ));
        Ok(())
    })
}

/// `detect`: run the detector again over every visible frame at the album's own
/// settings, and merge what it finds into the stored detections. A subject found
/// again keeps its row — and its reviewed flag, reading, stars, reject and name;
/// a genuinely new subject is inserted; a stored subject the detector no longer
/// finds is dropped only when nobody has marked or read it. The frame is then
/// re-rated and regrouped so the new boxes anchor the bursts correctly.
fn rerun_detect(d: &Arc<Desktop>, job: i64, cfg: Settings) -> Result<Value> {
    launch(d, job, &format!("{KIND} detect"), move |d, stop, task| {
        let frames = rows(
            &lock(&d.reader),
            "SELECT id,path FROM images WHERE job_id=? AND status='done' \
             AND COALESCE(rejected,0)=0 AND COALESCE(copy_of,0)=0 ORDER BY id",
            [job],
        )?;
        if frames.is_empty() {
            task.detail("No scanned frames to look at again");
            return Ok(());
        }
        let profile = ScanProfile::parse(&cfg.scan_profile);
        crate::setup::ensure(&d.hub, stop, crate::setup::SCAN)?;
        let lanes = usize::try_from(cfg.cull_workers())
            .unwrap_or(1)
            .clamp(1, 16)
            .min(frames.len());
        // The detector and face sessions share the card with the decision model.
        let _gpu = crate::gpu::stage();
        let mut detectors = Vec::with_capacity(lanes);
        for _ in 0..lanes {
            detectors.push(Mutex::new(conrod_vision::detect::Detector::load(
                &crate::detector_model(),
                conrod_vision::detect::Device::Auto,
            )?));
        }
        let faces = cfg
            .lanes()
            .faces
            .then(|| crate::load_faces_pool(lanes).ok())
            .flatten();
        let expression = faces
            .is_some()
            .then(|| crate::load_expression_pool(lanes))
            .flatten();
        let models: HashMap<_, _> = ["vehicle", "person", "face", "eye"]
            .into_iter()
            .filter_map(|region| crate::load_region_model(region).map(|m| (region.to_string(), m)))
            .collect();
        let quality = cfg
            .use_quality_score
            .then(|| {
                let path = models::expected(models::QUALITY);
                path.is_file()
                    .then(|| conrod_vision::quality::QualityModel::load(&path, true).ok())
                    .flatten()
                    .map(Mutex::new)
            })
            .flatten();
        let total = frames.len() as u64;
        task.progress(0, total);
        task.detail(format!("Looking at {total} frames again for subjects"));
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        let matched = AtomicUsize::new(0);
        let added = AtomicUsize::new(0);
        let removed = AtomicUsize::new(0);
        let kept = AtomicUsize::new(0);
        let unread = AtomicUsize::new(0);
        let first_error = Mutex::new(None::<String>);
        std::thread::scope(|scope| {
            for detector in detectors.iter().take(lanes) {
                let (frames, cfg, profile, models, faces, expression, quality) = (
                    &frames,
                    &cfg,
                    profile,
                    &models,
                    &faces,
                    &expression,
                    &quality,
                );
                let (next, done, matched, added, removed, kept, unread) =
                    (&next, &done, &matched, &added, &removed, &kept, &unread);
                let first_error = &first_error;
                let (task, stop) = (task, stop);
                scope.spawn(move || loop {
                    let n = next.fetch_add(1, Ordering::Relaxed);
                    let Some(frame) = frames.get(n) else { break };
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                    let image = frame["id"].as_i64().unwrap_or_default();
                    let path = std::path::Path::new(frame["path"].as_str().unwrap_or_default());
                    match crate::cull_frame(
                        path,
                        detector,
                        cfg,
                        profile,
                        models,
                        faces.as_ref(),
                        expression.as_ref(),
                        quality.as_ref(),
                        |_| {},
                    ) {
                        Ok(result) => match merge_detected_frame(d, job, image, &result, cfg) {
                            Ok(counts) => {
                                matched.fetch_add(counts.matched, Ordering::Relaxed);
                                added.fetch_add(counts.added, Ordering::Relaxed);
                                removed.fetch_add(counts.removed, Ordering::Relaxed);
                                kept.fetch_add(counts.kept, Ordering::Relaxed);
                            }
                            Err(e) => {
                                let mut slot = lock(first_error);
                                if slot.is_none() {
                                    *slot = Some(e);
                                }
                            }
                        },
                        Err(_) => {
                            unread.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                    task.progress(done.fetch_add(1, Ordering::Relaxed) as u64 + 1, total);
                });
            }
        });
        if let Some(e) = first_error.into_inner().unwrap_or_else(|e| e.into_inner()) {
            return Err(e);
        }
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let (matched, added, removed, kept, unread) = (
            matched.load(Ordering::Relaxed),
            added.load(Ordering::Relaxed),
            removed.load(Ordering::Relaxed),
            kept.load(Ordering::Relaxed),
            unread.load(Ordering::Relaxed),
        );
        let rated = crate::passes::rate_missed_frames(&lock(&d.db), job)?;
        let counts = d.regroup(job)?;
        task.detail(format!(
            "Found subjects again: {matched} kept their markings, {added} new, {removed} gone, {kept} marked boxes kept ({unread} unread) · \
             {rated} missed frames re-rated · {} bursts · {} keepers",
            counts["bursts"], counts["keepers"]
        ));
        Ok(())
    })
}

/// What one frame's merge did, for the run's counts.
#[derive(Default)]
struct MergeCounts {
    matched: usize,
    added: usize,
    removed: usize,
    kept: usize,
}

/// The stored subject columns the merge reads: the box and region to match on,
/// and the markings that decide whether an unmatched row may be dropped.
struct StoredDet {
    id: i64,
    bbox: [f64; 4],
    region: String,
    /// A person marked this bystander (the camera did not): never overridden.
    manual_bystander: bool,
    /// Reviewed, read, starred, rejected or named: a person's work, kept even
    /// when the detector no longer finds the box.
    has_work: bool,
}

fn iou(a: [f64; 4], b: [f64; 4]) -> f64 {
    let x1 = a[0].max(b[0]);
    let y1 = a[1].max(b[1]);
    let x2 = a[2].min(b[2]);
    let y2 = a[3].min(b[3]);
    let inter = (x2 - x1).max(0.0) * (y2 - y1).max(0.0);
    let area = |b: [f64; 4]| (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let union = area(a) + area(b) - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

/// Merge one frame's freshly detected subjects into its stored detections, in a
/// single transaction, and refresh the frame's own measured columns. Matched
/// subjects keep their identity and every marking; only the measurement is
/// rewritten. Split out so the preserve rule is testable without a detector.
fn merge_detected_frame(
    d: &Desktop,
    _job: i64,
    image: i64,
    f: &crate::FrameResult,
    cfg: &Settings,
) -> Result<MergeCounts> {
    let stored = rows(
        &lock(&d.reader),
        "SELECT id,x1,y1,x2,y2,COALESCE(region_type,'vehicle') AS region,\
         COALESCE(bystander,0) AS bystander,COALESCE(af_bystander,0) AS af_bystander,\
         COALESCE(reviewed,0) AS reviewed,read_at,stars,COALESCE(rejected,0) AS rejected,\
         COALESCE(number,'') AS number,COALESCE(plate,'') AS plate,COALESCE(attributes,'') AS attributes \
         FROM detections WHERE image_id=?",
        [image],
    )?;
    let stored: Vec<StoredDet> = stored
        .iter()
        .map(|r| {
            let c = |k: &str| r[k].as_f64().unwrap_or_default();
            let attributes = r["attributes"].as_str().unwrap_or("");
            StoredDet {
                id: r["id"].as_i64().unwrap_or_default(),
                bbox: [c("x1"), c("y1"), c("x2"), c("y2")],
                region: r["region"].as_str().unwrap_or("vehicle").to_string(),
                manual_bystander: r["bystander"].as_i64() == Some(1)
                    && r["af_bystander"].as_i64() != Some(1),
                has_work: r["reviewed"].as_i64() == Some(1)
                    || r["read_at"].as_f64().is_some()
                    || r["stars"].as_i64().is_some()
                    || r["rejected"].as_i64() == Some(1)
                    || !r["number"].as_str().unwrap_or("").is_empty()
                    || !r["plate"].as_str().unwrap_or("").is_empty()
                    || !matches!(attributes, "" | "{}" | "null"),
            }
        })
        .collect();

    let mut counts = MergeCounts::default();
    let mut used: HashSet<i64> = HashSet::new();
    // Each new subject takes the best overlapping stored row of the same region
    // that no other new subject has already claimed.
    let mut matches: Vec<(usize, Option<i64>)> = Vec::new();
    for s in &f.subjects {
        let best = stored
            .iter()
            .filter(|o| !used.contains(&o.id) && o.region == s.region_type)
            .map(|o| (o.id, iou(o.bbox, s.bbox)))
            .filter(|(_, score)| *score >= MATCH_IOU)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id);
        if let Some(id) = best {
            used.insert(id);
            counts.matched += 1;
        } else {
            counts.added += 1;
        }
        matches.push((matches.len(), best));
    }

    let thumb = d.root.join(format!("cache/native/thumb-{image}.jpg"));
    if !f.thumb_saved {
        crate::desktop::save_jpeg(&f.thumb, &thumb)?;
    }
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    for (i, s) in f.subjects.iter().enumerate() {
        let (_, best) = matches[i];
        match best {
            Some(id) => {
                let old = stored
                    .iter()
                    .find(|o| o.id == id)
                    .ok_or("Lost a matched subject")?;
                // A person's bystander mark stands; otherwise the camera's fresh
                // verdict applies. The reading and every other marking stay.
                let (bystander, af_bystander) = if old.manual_bystander {
                    (Some(1), Some(0))
                } else {
                    (s.bystander.then_some(1), s.af_bystander.then_some(1))
                };
                tx.execute(
                    "UPDATE detections SET x1=?,y1=?,x2=?,y2=?,cls=?,conf=?,sharpness=?,rating=?,\
                     rating_verdict=?,panning=?,sharp_end=?,cull_reason=?,features=?,heuristic=?,\
                     quality=?,cut_sides=?,subject_frac=?,lead_room=?,framing_score=?,bystander=?,af_bystander=? \
                     WHERE id=?",
                    params![
                        s.bbox[0], s.bbox[1], s.bbox[2], s.bbox[3], s.class, s.conf,
                        s.sharpness, s.rating,
                        conrod_vision::sharpness::rating_for(s.rating, cfg.sharp_at, cfg.blurred_below),
                        s.panning, s.sharp_end, s.cull_reason,
                        serde_json::to_string(&s.features).map_err(err)?, s.heuristic,
                        s.quality, s.cut_sides, s.subject_frac, s.lead_room, s.framing_score,
                        bystander, af_bystander, id
                    ],
                )
                .map_err(err)?;
            }
            None => {
                insert_subject(&tx, image, s)?;
            }
        }
    }
    // A stored subject the detector no longer finds: drop it only when nobody
    // has marked or read it. A reviewed, read, starred, rejected or named box is
    // the photographer's work and stays, even if the detector now misses it.
    for o in &stored {
        if used.contains(&o.id) {
            continue;
        }
        if o.has_work {
            counts.kept += 1;
        } else {
            tx.execute("DELETE FROM detections WHERE id=?", [o.id])
                .map_err(err)?;
            counts.removed += 1;
        }
    }
    write_frame_measurements(&tx, image, f, &thumb)?;
    tx.commit().map_err(err)?;
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;
    use serde_json::json;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn iou_matches_the_same_box_and_rejects_a_distant_one() {
        let a = [100.0, 100.0, 300.0, 200.0];
        assert!((iou(a, a) - 1.0).abs() < 1e-9);
        assert!(
            iou(a, [110.0, 105.0, 310.0, 205.0]) >= MATCH_IOU,
            "a shifted box still matches"
        );
        assert!(iou(a, [1000.0, 1000.0, 1200.0, 1100.0]) < MATCH_IOU);
        assert_eq!(
            iou([0.0, 0.0, 0.0, 0.0], a),
            0.0,
            "an empty box never matches"
        );
    }

    #[test]
    fn the_guard_refuses_a_second_rerun_and_a_busy_album() {
        let lib = Lib::new("rerun-guard");
        lib.frame("a.jpg", None);
        // A background operation on the album blocks a rerun.
        lib.d().operations.lock().unwrap().insert(
            format!("Identifying:{}", lib.job),
            Arc::new(AtomicBool::new(false)),
        );
        let err = lib
            .run("rerun_stage", json!({"jobId": lib.job, "stage": "pick"}))
            .unwrap_err();
        assert!(err.contains("background operation"), "{err}");
        lib.d().operations.lock().unwrap().clear();
        // A rerun on another album blocks this one: only one rerun at a time.
        lib.d().operations.lock().unwrap().insert(
            format!("{KIND} detect:{}", lib.job + 1),
            Arc::new(AtomicBool::new(false)),
        );
        let err = lib
            .run("rerun_stage", json!({"jobId": lib.job, "stage": "pick"}))
            .unwrap_err();
        assert!(err.contains("already running again"), "{err}");
    }

    #[test]
    fn a_pick_rerun_keeps_the_hand_pick_and_recounts_the_suggestions() {
        let lib = Lib::new("rerun-pick");
        // One burst: a sharp frame the engine would suggest, and a hand pick on
        // its softer burstmate. The hand pick is the effective keeper.
        let a = lib.frame("a.jpg", Some(1));
        let b = lib.frame("b.jpg", Some(1));
        lib.detection(a, "vehicle", 0.95);
        lib.detection(b, "vehicle", 0.70);
        lib.sql("UPDATE images SET rating=0.95 WHERE id=?", [a]);
        lib.sql("UPDATE images SET rating=0.70,picked=1 WHERE id=?", [b]);
        let out = lib
            .run("rerun_stage", json!({"jobId": lib.job, "stage": "pick"}))
            .unwrap();
        lib.wait_idle();
        assert_eq!(out["operation"], json!(format!("{KIND} pick:{}", lib.job)));
        // The engine still marks its own suggestion, but the hand pick stands:
        // the picked frame is never overwritten by the rerun.
        let picked: i64 = lib.one("SELECT picked FROM images WHERE id=?", [b]);
        assert_eq!(picked, 1, "the hand pick survives");
        assert_eq!(lib.task(&format!("{KIND} pick"))["state"], "done");
    }

    #[test]
    fn a_detect_merge_keeps_read_and_reviewed_markings_and_drops_only_unmarked_boxes() {
        let lib = Lib::new("rerun-detect-merge");
        let image = lib.frame("a.jpg", None);
        // Three stored boxes: one read, one reviewed, one plain.
        let read = lib.detection(image, "vehicle", 0.9);
        let reviewed = lib.detection(image, "vehicle", 0.9);
        let plain = lib.detection(image, "vehicle", 0.9);
        lib.sql("UPDATE detections SET x1=1000,y1=1000,x2=2000,y2=2000,read_at=1700000000,number='7',attributes='{\"make\":\"BMW\"}' WHERE id=?", [read]);
        lib.sql(
            "UPDATE detections SET x1=2500,y1=1000,x2=3500,y2=2000,reviewed=1,stars=5 WHERE id=?",
            [reviewed],
        );
        lib.sql(
            "UPDATE detections SET x1=4000,y1=1000,x2=5000,y2=2000 WHERE id=?",
            [plain],
        );

        // The detector finds the read box again (shifted a little), a brand new
        // box where the plain one was, and nothing where the reviewed box was.
        let mut f = blank_frame(&lib, image);
        f.subjects = vec![
            subject([1050.0, 1020.0, 2050.0, 2020.0], "vehicle"), // matches `read`
            subject([4100.0, 1000.0, 5100.0, 2000.0], "vehicle"), // matches `plain`
            subject([100.0, 100.0, 400.0, 400.0], "vehicle"),     // genuinely new
        ];
        let cfg = settings(lib.d(), lib.job).unwrap();
        let counts = merge_detected_frame(lib.d(), lib.job, image, &f, &cfg).unwrap();
        assert_eq!(
            (counts.matched, counts.added, counts.removed, counts.kept),
            (2, 1, 0, 1)
        );

        // The read box kept its number, reading and read_at; only its box moved.
        let (number, read_at, x1): (String, f64, f64) = lib
            .db()
            .query_row(
                "SELECT number,read_at,x1 FROM detections WHERE id=?",
                [read],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(number, "7");
        assert_eq!(read_at, 1700000000.0);
        assert_eq!(x1, 1050.0, "the box was re-found");
        // The reviewed box the detector missed is kept: a person marked it.
        let still: i64 = lib.one("SELECT count(*) FROM detections WHERE id=?", [reviewed]);
        assert_eq!(still, 1);
        // The plain box was matched and updated, not deleted.
        let plain_x1: f64 = lib.one("SELECT x1 FROM detections WHERE id=?", [plain]);
        assert_eq!(plain_x1, 4100.0);
        // One new subject was inserted.
        let total: i64 = lib.one("SELECT count(*) FROM detections WHERE image_id=?", [image]);
        assert_eq!(total, 4, "read + reviewed + plain + new");
    }

    #[test]
    fn a_detect_merge_drops_an_unmarked_box_the_detector_no_longer_finds() {
        let lib = Lib::new("rerun-detect-drop");
        let image = lib.frame("a.jpg", None);
        let plain = lib.detection(image, "vehicle", 0.9);
        lib.sql(
            "UPDATE detections SET x1=1000,y1=1000,x2=2000,y2=2000 WHERE id=?",
            [plain],
        );
        // The detector now finds nothing in this frame.
        let f = blank_frame(&lib, image);
        let cfg = settings(lib.d(), lib.job).unwrap();
        let counts = merge_detected_frame(lib.d(), lib.job, image, &f, &cfg).unwrap();
        assert_eq!(
            (counts.matched, counts.added, counts.removed, counts.kept),
            (0, 0, 1, 0)
        );
        let gone: i64 = lib.one("SELECT count(*) FROM detections WHERE id=?", [plain]);
        assert_eq!(gone, 0, "an unmarked box the detector misses is dropped");
    }

    #[test]
    fn a_rate_rerun_remeasures_but_never_lowers_a_hand_star() {
        let lib = Lib::new("rerun-rate");
        let image = lib.frame("a.jpg", Some(1));
        let d = lib.detection(image, "vehicle", 0.9);
        // A hand five-star frame whose stored focus score is soft: the rerun
        // re-derives the measured rating but must never touch the hand star.
        lib.sql(
            "UPDATE detections SET heuristic=0.3,features='[0.3]',stars=5 WHERE id=?",
            [d],
        );
        lib.sql(
            "UPDATE images SET sharpness=0.9,rating=0.9 WHERE id=?",
            [image],
        );
        let out = lib
            .run("rerun_stage", json!({"jobId": lib.job, "stage": "rate"}))
            .unwrap();
        lib.wait_idle();
        assert_eq!(out["operation"], json!(format!("{KIND} rate:{}", lib.job)));
        let stars: i64 = lib.one("SELECT stars FROM detections WHERE id=?", [d]);
        assert_eq!(stars, 5, "a photographer's star is never lowered");
        let rating: f64 = lib.one("SELECT rating FROM detections WHERE id=?", [d]);
        assert!(
            rating < 0.5,
            "the measured rating follows the stored score: {rating}"
        );
        assert_eq!(lib.task(&format!("{KIND} rate"))["state"], "done");
    }

    #[test]
    fn an_identify_rerun_runs_under_the_rerun_name_and_is_incremental_by_default() {
        let lib = Lib::new("rerun-identify");
        // Readers off so the dispatch is testable without models or files; the
        // incremental semantics themselves are pinned by the identify tests.
        lib.run(
            "update_job_settings",
            json!({"jobId": lib.job, "patch": {
                "read_plates": false, "read_numbers": false, "read_text": false,
                "use_vlm": false, "group_vehicles": false,
            }}),
        )
        .unwrap();
        lib.detection(lib.frame("a.jpg", None), "vehicle", 0.9);
        let out = lib
            .run(
                "rerun_stage",
                json!({"jobId": lib.job, "stage": "identify"}),
            )
            .unwrap();
        assert_eq!(
            out["operation"],
            json!(format!("{KIND} identify:{}", lib.job))
        );
        // Stop it straight away: this pins the dispatch, not a model run.
        lib.run("cancel_operation", json!({"key": out["operation"]}))
            .unwrap();
        lib.wait_idle();
        // A full run is the same command with `full`: it parses and dispatches too.
        let full = lib
            .run(
                "rerun_stage",
                json!({"jobId": lib.job, "stage": "identify", "full": true}),
            )
            .unwrap();
        assert_eq!(
            full["operation"],
            json!(format!("{KIND} identify:{}", lib.job))
        );
        lib.run("cancel_operation", json!({"key": full["operation"]}))
            .unwrap();
        lib.wait_idle();
    }

    #[test]
    fn an_incremental_decision_rerun_on_a_judged_album_says_nothing_new_and_full_judges_again() {
        let lib = Lib::new("rerun-decision-nothing-new");
        let judged = lib.frame("a.jpg", Some(1));
        lib.sql(
            "UPDATE images SET burst_pick=1,moments='{\"backend\":\"x\",\"readings\":[]}' WHERE id=?",
            [judged],
        );
        let args = json!({"jobId": lib.job, "stage": "identify:decision"});
        let out = lib.run("stage_rerun", args).unwrap();
        assert_eq!(out["nothing_new"], true, "{out}");
        assert_eq!(out["judged"], 0);
        assert!(
            out["message"].as_str().unwrap().contains("Nothing new"),
            "{out}"
        );
        assert!(
            out.get("operation").is_none(),
            "no operation starts when there is nothing to ask: {out}"
        );
        assert!(lock(&lib.d().operations).is_empty());

        // `full` is the same command asking for every frame again: a real run.
        let full = lib
            .run(
                "stage_rerun",
                json!({"jobId": lib.job, "stage": "identify:decision", "full": true}),
            )
            .unwrap();
        assert_eq!(
            full["operation"],
            json!(format!("{KIND} moments:{}", lib.job))
        );
        assert!(full.get("nothing_new").is_none());
        lib.run("cancel_operation", json!({"key": full["operation"]}))
            .unwrap();
        lib.wait_idle();
    }

    #[test]
    fn an_incremental_decision_rerun_with_an_unjudged_frame_still_runs() {
        let lib = Lib::new("rerun-decision-new-frame");
        let fresh = lib.frame("b.jpg", Some(1));
        lib.sql("UPDATE images SET burst_pick=1 WHERE id=?", [fresh]);
        let out = lib
            .run(
                "stage_rerun",
                json!({"jobId": lib.job, "stage": "identify:decision"}),
            )
            .unwrap();
        assert!(out.get("nothing_new").is_none(), "{out}");
        assert_eq!(
            out["operation"],
            json!(format!("{KIND} moments:{}", lib.job))
        );
        lib.run("cancel_operation", json!({"key": out["operation"]}))
            .unwrap();
        lib.wait_idle();
    }
    /// A `FrameResult` with no subjects and the frame's stored size, for the merge
    /// tests: only `subjects`, `size`, `thumb` and `rating` are read by the merge.
    fn blank_frame(lib: &Lib, image: i64) -> crate::FrameResult {
        let (w, h): (i64, i64) = lib
            .db()
            .query_row("SELECT width,height FROM images WHERE id=?", [image], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        crate::FrameResult {
            path: lib.root.join("a.jpg"),
            camera: String::new(),
            taken: None,
            size: (w as usize, h as usize),
            subjects: Vec::new(),
            whole: None,
            stars: 0,
            thumb: conrod_vision::imageops::Rgb::new(1, 1, vec![0, 0, 0]),
            rating: 0.0,
            exposure: None,
            exif: Default::default(),
            lens: None,
            file_size: 0,
            level: None,
            eye_open_min: None,
            smile_max: None,
            buck: None,
            whole_raw: None,
            pan_share: None,
            af: None,
            af_sharpness: None,
            kept: Vec::new(),
            thumb_saved: true,
        }
    }

    fn subject(bbox: [f64; 4], region: &'static str) -> crate::Subject {
        crate::Subject {
            class: "car",
            conf: 0.9,
            bbox,
            sharpness: 0.9,
            panning: false,
            sharp_end: "subject",
            rating: 0.9,
            stars: 4,
            cull_reason: String::new(),
            features: vec![0.9],
            heuristic: 0.9,
            region_type: region,
            quality: None,
            eyes_open: [None, None],
            cut_sides: 0,
            subject_frac: 0.2,
            lead_room: None,
            framing_score: None,
            bystander: false,
            af_bystander: false,
        }
    }
}
