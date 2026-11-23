//! Per-stage run controls: pause, skip, and run one stage again.
//!
//! One [`StageControl`] per album holds the photographer's choices for every
//! wire stage id. Workers consult it at their next unit boundary (between
//! frames, between model asks, at a whole-album pass's edge) and park on its
//! condvar; no call is ever interrupted mid-request. The choices are written to
//! the album's `settings_json` (`native_stage_skip`, `native_stage_pause`), so a
//! crash or a reopen restores exactly the same control state.
//!
//! Skip and rerun never touch reviewed or hand-set work: a skip only prevents
//! work, and every rerun goes through the established guards (`read_at`,
//! `reviewed`, `only-new`, judged-burst skip, live re-read before writes).

use crate::commands::{RerunStage, StagePauseArgs, StageRerunArgs, StageSkipArgs};
use crate::desktop::{Desktop, Result};
use crate::lock;
use conrod_core::progress::{
    known_stage, skip_consequence, skippable, IDENTIFY_DECISION, IDENTIFY_OCR, IDENTIFY_VISION,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// The album setting that holds the skipped stage ids (a JSON array).
pub const STAGE_SKIP: &str = "native_stage_skip";
/// The album setting that holds the paused stage ids (a JSON array).
pub const STAGE_PAUSE: &str = "native_stage_pause";

/// Whether `stage` is one of the cull lanes: pausing it holds the whole scan.
pub fn cull_stage(stage: &str) -> bool {
    CULL_STAGES.contains(&stage)
}

/// The stages the cull lanes run; pausing one of these holds the whole scan,
/// exactly as the run-level Pause does.
const CULL_STAGES: [&str; 4] = ["read", "detect", "sharpness", "faces"];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Control {
    paused: BTreeSet<&'static str>,
    skipped: BTreeSet<&'static str>,
}

/// The stage controls of one album, shared by the run and the commands.
#[derive(Debug)]
pub struct StageControl {
    job: i64,
    inner: Mutex<Control>,
    wake: Condvar,
}

impl StageControl {
    pub fn new(job: i64) -> Arc<StageControl> {
        Arc::new(StageControl {
            job,
            inner: Mutex::new(Control::default()),
            wake: Condvar::new(),
        })
    }

    /// The controls an album's stored settings record. Unknown ids are dropped:
    /// a snapshot from a future build never makes this one skip a stage it does
    /// not know.
    pub fn from_map(job: i64, settings: &Map<String, Value>) -> Arc<StageControl> {
        let control = StageControl::new(job);
        {
            let mut inner = lock(&control.inner);
            for stage in stored_stages(settings, STAGE_PAUSE) {
                inner.paused.insert(stage);
            }
            for stage in stored_stages(settings, STAGE_SKIP) {
                inner.skipped.insert(stage);
            }
        }
        control
    }

    pub fn job(&self) -> i64 {
        self.job
    }

    pub fn is_paused(&self, stage: &str) -> bool {
        lock(&self.inner).paused.contains(stage)
    }

    pub fn is_skipped(&self, stage: &str) -> bool {
        lock(&self.inner).skipped.contains(stage)
    }

    /// Pause or resume `stage`, waking every parked worker so a resume takes
    /// effect at once.
    pub fn set_paused(&self, stage: &'static str, paused: bool) {
        {
            let mut inner = lock(&self.inner);
            if paused {
                inner.paused.insert(stage);
            } else {
                inner.paused.remove(stage);
            }
        }
        self.wake.notify_all();
    }

    pub fn set_skipped(&self, stage: &'static str, skipped: bool) {
        {
            let mut inner = lock(&self.inner);
            if skipped {
                inner.skipped.insert(stage);
            } else {
                inner.skipped.remove(stage);
            }
        }
        self.wake.notify_all();
    }

    pub fn paused_stages(&self) -> Vec<&'static str> {
        lock(&self.inner).paused.iter().copied().collect()
    }

    pub fn skipped_stages(&self) -> Vec<&'static str> {
        lock(&self.inner).skipped.iter().copied().collect()
    }

    /// Whether a cull lane is paused: the scan itself has to hold.
    pub fn cull_paused(&self) -> bool {
        let inner = lock(&self.inner);
        CULL_STAGES.iter().any(|stage| inner.paused.contains(stage))
    }

    /// Park until `stage` is resumed, or `stop` is set. The check happens only
    /// at a unit boundary, so an ask already in flight always finishes. Returns
    /// false when the run was stopped while parked.
    pub fn park(&self, stage: &str, stop: &AtomicBool) -> bool {
        let mut inner = lock(&self.inner);
        while inner.paused.contains(stage) && !stop.load(Ordering::Relaxed) {
            inner = self
                .wake
                .wait_timeout(inner, std::time::Duration::from_millis(100))
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        !stop.load(Ordering::Relaxed)
    }

    /// [`park`] with the progress report the screen needs: the stage says
    /// `paused` while workers are parked, and goes back to its real status on
    /// resume. `None` progress is a manual run without a scan screen.
    pub fn park_with(
        &self,
        stage: &'static str,
        stop: &AtomicBool,
        progress: Option<&crate::progress::Progress>,
    ) -> bool {
        if !self.is_paused(stage) {
            return !stop.load(Ordering::Relaxed);
        }
        if let Some(p) = progress {
            p.pause_stage(stage);
        }
        let go = self.park(stage, stop);
        if let Some(p) = progress {
            p.resume_stage(stage);
        }
        go
    }
}

/// The known stage ids in a stored JSON array.
fn stored_stages(settings: &Map<String, Value>, key: &str) -> Vec<&'static str> {
    settings
        .get(key)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter_map(known_stage)
                .collect()
        })
        .unwrap_or_default()
}

/// What the screen hydrates from: the album's stored/current control sets.
pub fn controls_json(job: i64, settings: &Map<String, Value>) -> Value {
    let control = StageControl::from_map(job, settings);
    json!({
        "paused": control.paused_stages(),
        "skipped": control.skipped_stages(),
    })
}

/// Resolve a wire stage id, refusing one no snapshot carries.
fn stage_id(stage: &str) -> Result<&'static str> {
    known_stage(stage).ok_or_else(|| format!("Unknown stage `{stage}`"))
}

/// The stage's plain label, for messages.
fn stage_label(stage: &str) -> &str {
    conrod_core::progress::STAGES
        .iter()
        .find(|(id, _)| *id == stage)
        .map_or(stage, |(_, label)| *label)
}

/// `stage_pause {jobId, stage}`: hold `stage` at its next unit boundary, or
/// resume it when `paused` is false. Persisted on the album so a reopen keeps
/// the same state.
pub fn pause(d: &Arc<Desktop>, a: &StagePauseArgs) -> Result<Value> {
    let stage = stage_id(&a.stage)?;
    let paused = a.paused.unwrap_or(true);
    let control = d.stage_control(a.job_id);
    control.set_paused(stage, paused);
    d.persist_stage_control(a.job_id, &control);
    // A cull stage has no per-stage worker loop of its own: its lanes are the
    // scan, so pausing one holds the scan until every paused cull stage is
    // resumed.
    if cull_stage(stage) {
        d.set_scan_paused(control.cull_paused());
    }
    if let Some(progress) = d.progress().filter(|p| p.job() == a.job_id) {
        if paused {
            progress.pause_stage(stage);
        } else {
            progress.resume_stage(stage);
        }
    }
    Ok(json!({"stage": stage, "paused": paused}))
}

/// `stage_skip {jobId, stage}`: mark `stage` skipped, with its one-line
/// consequence, and let the run continue to the next stage. The run reads this
/// at the stage's own boundary, so a stage already finished is left as it was.
pub fn skip(d: &Arc<Desktop>, a: &StageSkipArgs) -> Result<Value> {
    let stage = stage_id(&a.stage)?;
    let skip = a.skip.unwrap_or(true);
    if skip && !skippable(stage) {
        return Err(format!(
            "{} cannot be skipped: every stage after it needs its work.",
            stage_label(stage)
        ));
    }
    let control = d.stage_control(a.job_id);
    control.set_skipped(stage, skip);
    d.persist_stage_control(a.job_id, &control);
    if let Some(progress) = d.progress().filter(|p| p.job() == a.job_id) {
        if !skip {
            progress.reset_stage(stage);
        } else {
            // A stage already working finishes the unit in hand and stops; its
            // own finish path then says `skipped`. A stage that has not started
            // says it at once.
            let running = progress
                .snapshot()
                .pipeline
                .stages
                .iter()
                .any(|node| node.id == stage && node.status == "running");
            if !running {
                progress.skip_stage(stage);
            }
        }
    }
    Ok(json!({
        "stage": stage,
        "skipped": skip,
        "consequence": skip_consequence(stage),
    }))
}

/// `stage_rerun {jobId, stage}`: run one stage again through the established
/// rerun entry points. The same guards as every rerun: no scan running, no
/// queued scan, one rerun at a time, and the stage's own incremental rules
/// (read subjects, judged bursts, hand picks and reviewed work are left alone).
pub fn rerun(d: &Arc<Desktop>, a: &StageRerunArgs) -> Result<Value> {
    let stage = stage_id(&a.stage)?;
    // Rerunning a skipped stage is the photographer asking for its work after
    // all: the skip is cleared first.
    let control = d.stage_control(a.job_id);
    if control.is_skipped(stage) {
        control.set_skipped(stage, false);
        d.persist_stage_control(a.job_id, &control);
    }
    let status = d.status();
    if !status["activeJob"].is_null() {
        return Err("A scan is running. Stop it first.".into());
    }
    if status["queue"]
        .as_array()
        .is_some_and(|q| q.iter().any(|j| j.as_i64() == Some(a.job_id)))
    {
        return Err(
            "This album is waiting to scan. Let it finish or take it out of the line first.".into(),
        );
    }
    match stage {
        "detect" => crate::rerun::rerun_stage(d, a.job_id, RerunStage::Detect, false, None),
        "sharpness" => crate::rerun::rerun_stage(d, a.job_id, RerunStage::Rate, false, None),
        // Each identify sub-stage runs only itself, incremental by default
        // (only subjects nobody has read are asked; `full` reads them again).
        IDENTIFY_OCR => crate::rerun::rerun_stage_only(
            d,
            a.job_id,
            RerunStage::Identify,
            a.full,
            None,
            Some(IDENTIFY_OCR),
        ),
        IDENTIFY_VISION => crate::rerun::rerun_stage_only(
            d,
            a.job_id,
            RerunStage::Identify,
            a.full,
            None,
            Some(IDENTIFY_VISION),
        ),
        // Decision is the type round plus the peak comparison: incremental by
        // default, full rejudge when `full` is set.
        IDENTIFY_DECISION => crate::rerun::rerun_stage_only(
            d,
            a.job_id,
            RerunStage::Moments,
            a.full,
            None,
            Some(IDENTIFY_DECISION),
        ),
        "group" => crate::passes::group(d, a.job_id),
        "pick" => crate::rerun::rerun_stage(d, a.job_id, RerunStage::Pick, false, None),
        "read" => {
            Err("Read cannot run on its own: scan the album again to re-read its frames.".into())
        }
        other => Err(format!("`{other}` cannot be run again")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;
    use std::sync::atomic::AtomicBool;
    use std::time::{Duration, Instant};

    #[test]
    fn a_skip_and_a_pause_survive_a_reopen() {
        let lib = Lib::new("stage-controls");
        let skip = lib
            .run(
                "stage_skip",
                json!({"jobId": lib.job, "stage": IDENTIFY_VISION}),
            )
            .unwrap();
        assert_eq!(skip["skipped"], true);
        assert!(
            skip["consequence"]
                .as_str()
                .unwrap()
                .contains("no make/model"),
            "the plain consequence comes back with the skip: {skip}"
        );
        lib.run("stage_pause", json!({"jobId": lib.job, "stage": "group"}))
            .unwrap();

        // A second open of the same library is the reopen the plan asks about.
        let reopened = Desktop::open(lib.root.clone()).unwrap();
        let control = reopened.stage_control(lib.job);
        assert!(control.is_skipped(IDENTIFY_VISION));
        assert!(control.is_paused("group"));
        assert!(!control.is_skipped("pick"));

        // And resuming through the command clears it for good.
        reopened
            .dispatch(
                "stage_pause",
                json!({"jobId": lib.job, "stage": "group", "paused": false}),
            )
            .unwrap();
        assert!(!control.is_paused("group"));
        let fresh = Desktop::open(lib.root.clone()).unwrap();
        assert!(!fresh.stage_control(lib.job).is_paused("group"));
    }

    #[test]
    fn every_skippable_stage_has_a_consequence_and_the_feed_stages_refuse() {
        let lib = Lib::new("stage-skip-text");
        for (stage, needle) in [
            ("identify:ocr", "no plate or number reads"),
            ("identify:decision", "no type suggestion and no peak picks"),
            ("identify:vision", "no make/model"),
            ("group", "keeps its seed burst"),
            ("pick", "no keeper suggestions"),
            ("faces", "no eye-open or smile reads"),
        ] {
            let out = lib
                .run("stage_skip", json!({"jobId": lib.job, "stage": stage}))
                .unwrap();
            let text = out["consequence"].as_str().unwrap();
            assert!(text.contains(needle), "{stage}: {text}");
        }
        for stage in ["read", "detect", "sharpness"] {
            let err = lib
                .run("stage_skip", json!({"jobId": lib.job, "stage": stage}))
                .unwrap_err();
            assert!(err.contains("cannot be skipped"), "{stage}: {err}");
        }
        let err = lib
            .run("stage_skip", json!({"jobId": lib.job, "stage": "nope"}))
            .unwrap_err();
        assert!(err.contains("Unknown stage"), "{err}");
    }

    #[test]
    fn a_paused_worker_finishes_its_in_flight_unit_then_parks() {
        let control = StageControl::new(1);
        let stop = AtomicBool::new(false);
        // The worker is inside one ask: the pause lands while it is in flight.
        control.set_paused(IDENTIFY_VISION, true);
        let unit_done = AtomicBool::new(false);
        let parked_at = Mutex::new(None::<Instant>);
        std::thread::scope(|s| {
            let worker = s.spawn(|| {
                // One whole ask, never interrupted.
                std::thread::sleep(Duration::from_millis(60));
                unit_done.store(true, Ordering::Release);
                // The boundary after it is where the pause takes effect.
                *parked_at.lock().unwrap() = None;
                let go = control.park(IDENTIFY_VISION, &stop);
                assert!(go, "the run was not stopped");
                *parked_at.lock().unwrap() = Some(Instant::now());
            });
            std::thread::sleep(Duration::from_millis(120));
            assert!(unit_done.load(Ordering::Acquire), "the ask finished");
            assert!(
                parked_at.lock().unwrap().is_none(),
                "the worker parks after the ask, not mid-request"
            );
            control.set_paused(IDENTIFY_VISION, false);
            worker.join().unwrap();
        });
        assert!(parked_at.lock().unwrap().is_some(), "resume released it");
    }

    #[test]
    fn a_stop_wakes_a_parked_worker_instead_of_hanging_the_run() {
        let control = StageControl::new(2);
        let stop = Arc::new(AtomicBool::new(false));
        control.set_paused("group", true);
        std::thread::scope(|s| {
            let worker = {
                let stop = stop.clone();
                s.spawn(move || control.park("group", &stop))
            };
            std::thread::sleep(Duration::from_millis(50));
            stop.store(true, Ordering::Relaxed);
            // The parking loop checks the flag on its own cadence.
            let released = worker.join().unwrap();
            assert!(!released, "a stopped run does not continue");
        });
    }

    #[test]
    fn an_identify_stage_rerun_reads_only_the_subjects_nobody_read() {
        let lib = Lib::new("stage-rerun-read");
        let image = lib.frame("read.cr3", Some(1));
        let det = lib.detection(image, "vehicle", 4.0);
        lib.sql(
            "UPDATE detections SET read_at=unixepoch('now'), plate='ABC123', attributes='{\"plate\":\"ABC123\"}' WHERE id=?",
            [det],
        );
        lib.run(
            "stage_rerun",
            json!({"jobId": lib.job, "stage": "identify:ocr"}),
        )
        .unwrap();
        lib.wait_idle();
        let kept: String = lib.one("SELECT plate FROM detections WHERE id=?", [det]);
        assert_eq!(kept, "ABC123", "a read subject is not asked again");
        assert!(
            !lib.d()
                .hub
                .log()
                .iter()
                .any(|line| line.starts_with("failed: Rerunning identify")),
            "the incremental rerun finished without asking anything"
        );
    }

    #[test]
    fn rerun_is_guarded_and_clears_a_skip() {
        let lib = Lib::new("stage-rerun");
        lib.run("stage_skip", json!({"jobId": lib.job, "stage": "pick"}))
            .unwrap();
        let out = lib
            .run("stage_rerun", json!({"jobId": lib.job, "stage": "pick"}))
            .unwrap();
        assert_eq!(
            out["operation"], "Rerunning pick:1",
            "the stage rerun is a background operation: {out}"
        );
        lib.wait_idle();
        assert!(
            !lib.d().stage_control(lib.job).is_skipped("pick"),
            "rerunning a stage clears its skip"
        );
        let err = lib
            .run("stage_rerun", json!({"jobId": lib.job, "stage": "read"}))
            .unwrap_err();
        assert!(err.contains("cannot run on its own"), "{err}");
        let err = lib
            .run("stage_rerun", json!({"jobId": lib.job, "stage": "nope"}))
            .unwrap_err();
        assert!(err.contains("Unknown stage"), "{err}");
    }

    #[test]
    fn resuming_one_cull_stage_keeps_cull_paused_until_every_cull_stage_resumes() {
        let lib = Lib::new("stage-cull-multi-pause");
        let control = lib.d().stage_control(lib.job);
        assert!(!control.cull_paused());

        lib.run(
            "stage_pause",
            json!({"jobId": lib.job, "stage": "read", "paused": true}),
        )
        .unwrap();
        lib.run(
            "stage_pause",
            json!({"jobId": lib.job, "stage": "detect", "paused": true}),
        )
        .unwrap();
        assert!(control.cull_paused());

        // Resuming `read` while `detect` is still paused must keep cull paused.
        lib.run(
            "stage_pause",
            json!({"jobId": lib.job, "stage": "read", "paused": false}),
        )
        .unwrap();
        assert!(!control.is_paused("read"));
        assert!(control.is_paused("detect"));
        assert!(
            control.cull_paused(),
            "cull stays paused while another cull stage is still paused"
        );

        // Once `detect` resumes too, cull is no longer paused.
        lib.run(
            "stage_pause",
            json!({"jobId": lib.job, "stage": "detect", "paused": false}),
        )
        .unwrap();
        assert!(!control.cull_paused());
    }
}
