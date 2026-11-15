//! The scan's own report of itself: which stage is running, how fast, what is
//! coming out, and what every worker of every pool is doing.
//!
//! The pools call into this from their hot loop, so every method is a short
//! lock and a few assignments -- no formatting, no allocation except the file
//! name a worker is on. The app asks for a snapshot four times a second and
//! sends it as one event; the frontend draws the scan screen from that.

use conrod_core::progress::{
    Found, Pipeline, RateMeter, RecentFrame, ScanProgress, StageNode, WorkerLane, RECENT_LIMIT,
    STAGES,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// One stage node, as the scan lays it out before it starts.
#[derive(Debug, Clone)]
pub struct StageSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub model: String,
    pub device: String,
    pub workers: u32,
}

impl StageSpec {
    /// A stage of the standard flowchart, with its model and device filled in
    /// once the session is loaded.
    pub fn new(id: &'static str, workers: u32) -> StageSpec {
        let label = STAGES
            .iter()
            .find(|(stage, _)| *stage == id)
            .map_or(id, |(_, label)| *label);
        StageSpec {
            id,
            label,
            model: String::new(),
            device: String::new(),
            workers,
        }
    }
}

#[derive(Debug)]
struct StageState {
    id: &'static str,
    label: &'static str,
    model: String,
    device: String,
    workers_total: u32,
    done: u64,
    total: u64,
    meter: RateMeter,
}

/// `state` of a lane, as a number so a lane update never allocates.
const IDLE: u8 = 0;
const BUSY: u8 = 1;
const WAITING: u8 = 2;

#[derive(Debug)]
struct LaneState {
    /// The pool this lane belongs to; also where it rests when idle.
    pool: &'static str,
    index: u32,
    /// The stage it is inside right now (or its pool's home stage when idle).
    stage: &'static str,
    state: u8,
    waiting_for: Option<&'static str>,
    file: Option<String>,
    step: Option<&'static str>,
    since: Instant,
}

pub struct Progress {
    job: i64,
    started: Instant,
    stages: Mutex<Vec<StageState>>,
    lanes: Mutex<Vec<LaneState>>,
    found: Mutex<Found>,
    recent: Mutex<VecDeque<RecentFrame>>,
    /// Index into `stages` the whole scan is on: the phase, for the header.
    phase: AtomicUsize,
    state: Mutex<&'static str>,
    finished: AtomicBool,
    final_sent: AtomicBool,
}

impl Progress {
    /// Lay out the pipeline. `stages` is the flowchart, `pools` is
    /// `(home stage, workers)` per worker pool: each pool gets one lane per
    /// worker, which rests on its home stage when idle.
    pub fn new(job: i64, stages: Vec<StageSpec>, pools: &[(&'static str, usize)]) -> Arc<Progress> {
        let mut lanes = Vec::new();
        for &(pool, workers) in pools {
            for index in 0..workers.max(1) {
                lanes.push(LaneState {
                    pool,
                    index: index as u32,
                    stage: pool,
                    state: IDLE,
                    waiting_for: None,
                    file: None,
                    step: None,
                    since: Instant::now(),
                });
            }
        }
        Arc::new(Progress {
            job,
            started: Instant::now(),
            stages: Mutex::new(
                stages
                    .into_iter()
                    .map(|s| StageState {
                        id: s.id,
                        label: s.label,
                        model: s.model,
                        device: s.device,
                        workers_total: s.workers,
                        done: 0,
                        total: 0,
                        meter: RateMeter::new(8.0),
                    })
                    .collect(),
            ),
            lanes: Mutex::new(lanes),
            found: Mutex::new(Found::default()),
            recent: Mutex::new(VecDeque::new()),
            phase: AtomicUsize::new(0),
            state: Mutex::new("running"),
            finished: AtomicBool::new(false),
            final_sent: AtomicBool::new(false),
        })
    }

    /// The stage the header shows, e.g. `detect` while frames are going
    /// through the cull lane.
    pub fn phase(&self, stage: &str) {
        if let Some(index) = ScanProgress::stage_index(stage) {
            self.phase.store(index, Ordering::Relaxed);
        }
    }

    /// Set a stage's total: how many items it has in front of it.
    pub fn total(&self, stage: &'static str, total: u64) {
        self.with_node(stage, |node| node.total = total);
    }

    /// One more item through `stage`.
    pub fn advance(&self, stage: &'static str, by: u64) {
        self.with_node(stage, |node| node.done = node.done.saturating_add(by));
    }

    /// Fill in what a stage runs on, once its session is loaded.
    pub fn describe(&self, stage: &'static str, model: impl Into<String>, device: impl Into<String>) {
        let (model, device) = (model.into(), device.into());
        self.with_node(stage, |node| {
            node.model = model;
            node.device = device;
        });
    }

    fn with_node(&self, stage: &str, f: impl FnOnce(&mut StageState)) {
        let mut stages = crate::lock(&self.stages);
        if let Some(node) = stages.iter_mut().find(|node| node.id == stage) {
            f(node);
        }
    }

    /// `pool`'s worker `index` is inside `step` of `stage`, on `file`.
    pub fn busy(
        &self,
        pool: &'static str,
        index: usize,
        stage: &'static str,
        step: &'static str,
        file: &str,
    ) {
        self.lane(pool, index, stage, BUSY, None, Some(step), Some(file));
    }

    /// `pool`'s worker `index` is blocked on `stage`, waiting for `reason`
    /// (`gpu`, `lock`, `ollama` or `paused`).
    pub fn waiting(
        &self,
        pool: &'static str,
        index: usize,
        stage: &'static str,
        reason: &'static str,
        step: &'static str,
        file: &str,
    ) {
        self.lane(pool, index, stage, WAITING, Some(reason), Some(step), Some(file));
    }

    /// `pool`'s worker `index` has nothing in hand.
    pub fn idle(&self, pool: &'static str, index: usize) {
        self.lane(pool, index, pool, IDLE, None, None, None);
    }

    fn lane(
        &self,
        pool: &'static str,
        index: usize,
        stage: &'static str,
        state: u8,
        waiting_for: Option<&'static str>,
        step: Option<&'static str>,
        file: Option<&str>,
    ) {
        let mut lanes = crate::lock(&self.lanes);
        let Some(lane) = lanes
            .iter_mut()
            .find(|lane| lane.pool == pool && lane.index as usize == index)
        else {
            return;
        };
        // Only the first step of a frame resets the clock: the rest of the
        // steps are the same file moving on.
        if lane.file.as_deref() != file || lane.state == IDLE {
            lane.since = Instant::now();
        }
        lane.stage = stage;
        lane.state = state;
        lane.waiting_for = waiting_for;
        lane.step = step;
        if let Some(file) = file {
            if lane.file.as_deref() != Some(file) {
                lane.file = Some(file.to_owned());
            }
        } else {
            lane.file = None;
        }
    }

    pub fn add_found(&self, f: impl FnOnce(&mut Found)) {
        f(&mut crate::lock(&self.found));
    }

    /// A frame that just finished, newest first.
    pub fn recent(&self, frame: RecentFrame) {
        let mut recent = crate::lock(&self.recent);
        recent.push_front(frame);
        recent.truncate(RECENT_LIMIT);
    }

    /// Fill in what a frame reads as, while it is still on the live strip.
    pub fn label(&self, image_id: i64, label: String) {
        let mut recent = crate::lock(&self.recent);
        if let Some(frame) = recent
            .iter_mut()
            .find(|frame| frame.image_id == image_id && frame.label.is_none())
        {
            frame.label = Some(label);
        }
    }

    /// How the scan ended: `done`, `stopped` or `error`. One final snapshot is
    /// still emitted after this.
    pub fn finish(&self, state: &'static str) {
        *crate::lock(&self.state) = state;
        self.finished.store(true, Ordering::Release);
    }

    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    /// The snapshot for one event, or `None` once the final one has gone out.
    pub fn emit_snapshot(&self) -> Option<ScanProgress> {
        if self.finished() && self.final_sent.swap(true, Ordering::AcqRel) {
            return None;
        }
        Some(self.snapshot())
    }

    pub fn snapshot(&self) -> ScanProgress {
        let now = self.started.elapsed().as_secs_f64();
        let phase = self.phase.load(Ordering::Relaxed);
        let mut stages = crate::lock(&self.stages);
        let mut nodes = Vec::with_capacity(stages.len());
        let lanes = crate::lock(&self.lanes);
        for node in stages.iter_mut() {
            node.meter.observe(now, node.done);
            let busy = lanes
                .iter()
                .filter(|lane| lane.state == BUSY && lane.stage == node.id)
                .count() as u32;
            nodes.push(StageNode {
                id: node.id.to_string(),
                label: node.label.to_string(),
                model: node.model.clone(),
                device: node.device.clone(),
                workers_total: node.workers_total,
                workers_busy: busy,
                queue_len: node.total.saturating_sub(node.done),
                done: node.done,
                total: node.total,
                rate_per_s: round2(node.meter.rate().unwrap_or(0.0)),
                eta_s: node.meter.eta(node.done, node.total).map(round1),
            });
        }
        let (stage, done, total, rate, eta) = stages
            .get(phase)
            .or_else(|| stages.first())
            .map_or(("read".to_string(), 0, 0, 0.0, None), |node| {
                (
                    node.id.to_string(),
                    node.done,
                    node.total,
                    round2(node.meter.rate().unwrap_or(0.0)),
                    node.meter.eta(node.done, node.total).map(round1),
                )
            });
        let lane_out: Vec<WorkerLane> = lanes
            .iter()
            .map(|lane| WorkerLane {
                stage: if lane.state == IDLE {
                    lane.pool.to_string()
                } else {
                    lane.stage.to_string()
                },
                worker_index: lane.index,
                state: match lane.state {
                    BUSY => "busy",
                    WAITING => "waiting",
                    _ => "idle",
                }
                .into(),
                waiting_for: lane.waiting_for.map(str::to_owned),
                file: lane.file.clone(),
                step: lane.step.map(str::to_owned),
                ms_in_step: round1(lane.since.elapsed().as_secs_f64() * 1000.0),
            })
            .collect();
        ScanProgress {
            job_id: self.job,
            state: (*crate::lock(&self.state)).to_string(),
            stage,
            stage_index: phase.min(STAGES.len().saturating_sub(1)) as u32,
            stage_count: stages.len() as u32,
            done,
            total,
            rate_per_s: rate,
            eta_s: eta,
            recent: crate::lock(&self.recent).iter().cloned().collect(),
            found: crate::lock(&self.found).clone(),
            pipeline: Pipeline {
                stages: nodes,
                workers: lane_out,
            },
        }
    }
}

/// The pipeline a one-shot scan shows: every stage of the flowchart, with the
/// two real worker pools as lanes.
pub fn for_scan(
    job: i64,
    settings: &conrod_core::settings::Settings,
    faces: bool,
) -> Arc<Progress> {
    let cull = settings.cull_workers().clamp(1, 64) as u32;
    let identify = settings.identify_worker_count().clamp(1, 16) as u32;
    let stages = vec![
        StageSpec::new("read", cull),
        StageSpec::new("detect", cull),
        StageSpec::new("sharpness", cull),
        StageSpec::new("faces", if faces { cull } else { 0 }),
        StageSpec::new("identify", identify),
        StageSpec::new("group", 1),
        StageSpec::new("pick", 1),
    ];
    let lanes: Vec<(&'static str, usize)> = vec![
        ("read", cull as usize),
        ("identify", identify as usize),
    ];
    Progress::new(job, stages, &lanes)
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(job: i64) -> Arc<Progress> {
        Progress::new(
            job,
            vec![
                StageSpec::new("read", 2),
                StageSpec::new("detect", 2),
                StageSpec::new("sharpness", 2),
                StageSpec::new("faces", 2),
                StageSpec::new("identify", 1),
                StageSpec::new("group", 1),
                StageSpec::new("pick", 1),
            ],
            &[("read", 2), ("identify", 1)],
        )
    }

    #[test]
    fn a_fake_scan_reports_its_stage_busy_workers_found_and_recent_frames() {
        let progress = fake(9);
        progress.describe("detect", "yolo11s-960.onnx", "DirectML");
        progress.total("detect", 10);
        progress.phase("detect");
        assert_eq!(progress.emit_snapshot().unwrap().state, "running");
        // Blocked on another worker's detector session.
        progress.waiting("read", 0, "detect", "lock", "detect:wait", "1.CR3");
        let waiting_snap = progress.snapshot();
        let detect = waiting_snap
            .pipeline
            .stages
            .iter()
            .find(|s| s.id == "detect")
            .unwrap();
        assert_eq!(detect.workers_total, 2);
        assert_eq!(detect.workers_busy, 0, "waiting is not inside the stage");
        assert_eq!(waiting_snap.pipeline.workers[0].state, "waiting");
        assert_eq!(waiting_snap.pipeline.workers[0].stage, "detect");
        assert_eq!(waiting_snap.pipeline.workers[0].waiting_for.as_deref(), Some("lock"));
        assert_eq!(waiting_snap.pipeline.workers[1].state, "idle");
        assert_eq!(
            waiting_snap.pipeline.workers[1].stage, "read",
            "an idle worker rests at its pool's home stage"
        );
        assert_eq!(
            waiting_snap.pipeline.stages[2].workers_busy, 0,
            "nobody is in sharpness"
        );
        // Now inside the detector.
        progress.busy("read", 0, "detect", "detect", "1.CR3");
        let busy = progress.snapshot();
        assert_eq!(busy.pipeline.stages[1].workers_busy, 1, "one worker is inside detect");
        assert_eq!(busy.pipeline.workers[0].state, "busy");
        assert_eq!(
            busy.pipeline.stages[1].queue_len, 10,
            "queue is what is left"
        );
        progress.advance("detect", 10);
        progress.add_found(|found| found.subjects = 12);
        progress.recent(RecentFrame {
            image_id: 4,
            thumb_url: "thumb-4.jpg".into(),
            subject: "vehicle".into(),
            verdict: "panning".into(),
            label: None,
        });
        let snap = progress.snapshot();
        assert_eq!(snap.stage, "detect");
        assert_eq!(snap.stage_index, 1);
        assert_eq!(snap.stage_count, 7);
        assert_eq!((snap.done, snap.total), (10, 10));
        assert_eq!(snap.found.subjects, 12);
        assert_eq!(snap.recent[0].image_id, 4);
        assert_eq!(snap.recent[0].verdict, "panning");
        assert_eq!(snap.pipeline.stages[1].model, "yolo11s-960.onnx");
        progress.idle("read", 0);
        assert_eq!(progress.snapshot().pipeline.stages[1].workers_busy, 0);
    }

    #[test]
    fn one_final_snapshot_is_emitted_after_the_scan_ends() {
        let progress = fake(3);
        assert!(progress.emit_snapshot().is_some());
        progress.finish("done");
        let last = progress.emit_snapshot().unwrap();
        assert_eq!(last.state, "done");
        assert!(progress.emit_snapshot().is_none(), "then it goes quiet");
        let json = serde_json::to_value(last).unwrap();
        assert_eq!(json["state"], "done");
        assert_eq!(json["job_id"], 3);
        assert!(json["eta_s"].is_null());
        assert_eq!(json["pipeline"]["stages"].as_array().unwrap().len(), 7);
        assert_eq!(json["pipeline"]["workers"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn recent_frames_are_newest_first_and_capped() {
        let progress = fake(1);
        for image in 0..(RECENT_LIMIT as i64 + 5) {
            progress.recent(RecentFrame {
                image_id: image,
                thumb_url: String::new(),
                subject: "frame".into(),
                verdict: "sharp".into(),
                label: None,
            });
        }
        let snap = progress.snapshot();
        assert_eq!(snap.recent.len(), RECENT_LIMIT);
        assert_eq!(snap.recent[0].image_id, RECENT_LIMIT as i64 + 4);
    }
}
