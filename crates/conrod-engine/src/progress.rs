//! The scan's own report of itself: which stage is running, how fast, what is
//! coming out, and what every worker of every pool is doing.
//!
//! The pools call into this from their hot loop, so every method is a short
//! lock and a few assignments -- no formatting, no allocation except the file
//! name a worker is on. The app asks for a snapshot four times a second and
//! sends it as one event; the frontend draws the scan screen from that.

use conrod_core::progress::{
    Found, LiveFrame, Overall, Pipeline, RateMeter, RecentFrame, ScanProgress, StageNode,
    WorkerLane, IDENTIFY_DECISION, IDENTIFY_OCR, IDENTIFY_VISION, RECENT_LIMIT, STAGES,
};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Unix epoch milliseconds, for a worker's `last_report_at`.
fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// One stage node, as the scan lays it out before it starts.
#[derive(Debug, Clone)]
pub struct StageSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub model: String,
    pub device: String,
    pub workers: u32,
    /// Switched off for this album: it starts `skipped`, with no counters.
    pub skipped: bool,
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
            skipped: false,
        }
    }

    /// A sub-stage the album switched off: no workers, no counters.
    pub fn off(mut self) -> StageSpec {
        self.skipped = true;
        self.workers = 0;
        self
    }
}

#[derive(Debug)]
struct StageState {
    id: &'static str,
    label: &'static str,
    model: String,
    device: String,
    workers_total: u32,
    /// The worker count the scan laid this stage out with; an un-skip restores
    /// it after a skip zeroed the live count.
    workers_planned: u32,
    done: u64,
    total: u64,
    errors: u64,
    status: &'static str,
    error: Option<String>,
    /// The plain line skipping this stage would show; `None` when it cannot be
    /// skipped.
    skip_consequence: Option<&'static str>,
    /// Workers of this stage held at its pause point right now.
    parked: u32,
    meter: RateMeter,
}

/// `state` of a lane, as a number so a lane update never allocates.
const IDLE: u8 = 0;
const BUSY: u8 = 1;
const WAITING: u8 = 2;
const FAILED: u8 = 3;

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
    /// Epoch milliseconds of the last report, for the screen's stalled check.
    last_report_at: i64,
    /// The frame the lane holds, filled in by the worker as it learns things.
    /// Dropped whenever the lane moves to another file or goes idle.
    current: Option<LiveFrame>,
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
    /// Configured seconds of identify work per crop: the VLM estimate over the
    /// identify worker count.
    identify_cost_per_item: f64,
}

/// The configured cost of one VLM crop: 8 s of qwen2.5vl:7b on the reference
/// machine. Identify's weight is this over its worker count.
pub const VLM_SECONDS_PER_CROP: f64 = 8.0;
/// The cull lane's cost per frame before it has measured one (~15 fps).
const FALLBACK_FRAME_SECONDS: f64 = 1.0 / 15.0;
/// Items a stage needs before its ETA is worth believing.
pub const ETA_CONFIDENT_AFTER: u64 = 20;

impl Progress {
    /// Lay out the pipeline. `stages` is the flowchart, `pools` is
    /// `(home stage, workers)` per worker pool: each pool gets one lane per
    /// worker, which rests on its home stage when idle. Identify's estimate is
    /// one worker's VLM crop, since its worker count is not known here.
    pub fn new(job: i64, stages: Vec<StageSpec>, pools: &[(&'static str, usize)]) -> Arc<Progress> {
        Progress::with_identify_cost(job, stages, pools, VLM_SECONDS_PER_CROP)
    }

    /// [`Progress::new`] with identify's configured seconds per crop.
    pub fn with_identify_cost(
        job: i64,
        stages: Vec<StageSpec>,
        pools: &[(&'static str, usize)],
        identify_cost_per_item: f64,
    ) -> Arc<Progress> {
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
                    last_report_at: now_ms(),
                    current: None,
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
                        workers_planned: s.workers,
                        done: 0,
                        total: 0,
                        errors: 0,
                        status: if s.skipped { "skipped" } else { "pending" },
                        error: None,
                        skip_consequence: conrod_core::progress::skip_consequence(s.id),
                        parked: 0,
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
            identify_cost_per_item,
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

    /// `items` more frames are in front of `stage`.
    pub fn plan(&self, stage: &'static str, items: u64) {
        self.with_node(stage, |node| node.total = node.total.saturating_add(items));
    }

    /// `items` frames went through `stage` in an earlier run of this album: they
    /// are in its total and its done count, so a resume reads `500 / 1000`, not
    /// `0 / 500`.
    pub fn carry(&self, stage: &'static str, items: u64) {
        self.with_node(stage, |node| {
            node.total = node.total.saturating_add(items);
            node.done = node.done.saturating_add(items);
        });
    }

    /// A pass over the whole album (grouping, picking) finished `items` frames.
    pub fn settle(&self, stage: &'static str, items: u64) {
        self.with_node(stage, |node| {
            node.total = items;
            node.done = items;
        });
    }

    /// A frame that will not pass `stages[from]` or anything after it. With
    /// `failed` it came to nothing inside `stages[from]` (counted in that stage's
    /// `errors`); without, it was never going to be read (a hand reject). Either
    /// way it leaves those stages' totals, so a stage's `done` still reaches its
    /// `total`.
    pub fn abandon(&self, stages: &[&'static str], from: usize, failed: bool) {
        let mut nodes = crate::lock(&self.stages);
        for (n, id) in stages.iter().enumerate().skip(from) {
            if let Some(node) = nodes.iter_mut().find(|node| node.id == *id) {
                node.total = node.total.saturating_sub(1);
                if failed && n == from {
                    node.errors += 1;
                }
            }
        }
    }

    /// Fill in what a stage runs on, once its session is loaded.
    pub fn describe(
        &self,
        stage: &'static str,
        model: impl Into<String>,
        device: impl Into<String>,
    ) {
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

    /// This stage finished: its node reads `done`, and its counters agree.
    pub fn finish_stage(&self, stage: &'static str) {
        self.with_node(stage, |node| {
            node.status = "done";
            node.done = node.total;
            node.error = None;
        });
    }

    /// This stage was skipped (switched off, or skipped by the photographer):
    /// `skipped`, with no counters pretending there was work.
    pub fn skip_stage(&self, stage: &'static str) {
        self.with_node(stage, |node| {
            node.status = "skipped";
            node.done = 0;
            node.total = 0;
            node.errors = 0;
            node.workers_total = 0;
            node.error = None;
        });
    }

    /// A skipped stage was un-skipped: it is pending again, with the workers it
    /// was laid out with.
    pub fn reset_stage(&self, stage: &'static str) {
        self.with_node(stage, |node| {
            node.status = "pending";
            node.done = 0;
            node.total = 0;
            node.errors = 0;
            node.workers_total = node.workers_planned;
            node.error = None;
        });
    }

    /// This stage failed, with the plain reason the screen shows.
    pub fn fail_stage(&self, stage: &'static str, reason: impl Into<String>) {
        self.with_node(stage, |node| {
            node.status = "failed";
            node.error = Some(reason.into());
        });
    }

    /// This stage is paused between items: it says so instead of looking busy.
    /// A finished stage keeps `done`; pausing it would be a lie about work that
    /// is over.
    pub fn pause_stage(&self, stage: &'static str) {
        self.with_node(stage, |node| {
            if matches!(node.status, "pending" | "running") {
                node.status = "paused";
            }
        });
    }

    /// A worker of `stage` parked at its pause point. The stage says `paused`
    /// and `parked` from the first parked worker until the last one leaves.
    pub fn park_stage(&self, stage: &'static str) {
        self.with_node(stage, |node| {
            node.parked += 1;
            if matches!(node.status, "pending" | "running") {
                node.status = "paused";
            }
        });
    }

    /// A parked worker of `stage` is going on. The stage leaves `paused` only
    /// when no worker is left parked; one worker leaving never un-pauses its
    /// siblings.
    pub fn unpark_stage(&self, stage: &'static str) {
        self.with_node(stage, |node| {
            node.parked = node.parked.saturating_sub(1);
            if node.parked == 0 && node.status == "paused" {
                node.status = if node.done > 0 { "running" } else { "pending" };
            }
        });
    }

    /// A stage left `paused` and is working again. A stage that had not started
    /// goes back to `pending`; the snapshot derives `running` from real work.
    pub fn resume_stage(&self, stage: &'static str) {
        self.with_node(stage, |node| {
            if node.status == "paused" {
                node.status = if node.done > 0 { "running" } else { "pending" };
            }
        });
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
        self.lane(
            pool,
            index,
            stage,
            WAITING,
            Some(reason),
            Some(step),
            Some(file),
        );
    }

    /// `pool`'s worker `index` has nothing in hand.
    pub fn idle(&self, pool: &'static str, index: usize) {
        self.lane(pool, index, pool, IDLE, None, None, None);
    }

    /// `pool`'s worker `index` is parked at a stage pause point: it keeps the
    /// stage, step and file it was on, but says `paused` instead of looking
    /// busy. Only a worker that is not already failed changes.
    pub fn worker_paused(&self, pool: &'static str, index: usize) {
        let mut lanes = crate::lock(&self.lanes);
        let Some(lane) = lanes
            .iter_mut()
            .find(|lane| lane.pool == pool && lane.index as usize == index)
        else {
            return;
        };
        if lane.state != FAILED {
            lane.state = WAITING;
            lane.waiting_for = Some("paused");
            lane.last_report_at = now_ms();
        }
    }

    /// `pool`'s worker `index` failed: it stays on the stage and step it was on,
    /// with the honest `failed` lifecycle, instead of looking busy forever.
    pub fn worker_failed(&self, pool: &'static str, index: usize) {
        let mut lanes = crate::lock(&self.lanes);
        let Some(lane) = lanes
            .iter_mut()
            .find(|lane| lane.pool == pool && lane.index as usize == index)
        else {
            return;
        };
        lane.state = FAILED;
        lane.waiting_for = None;
        lane.last_report_at = now_ms();
    }

    /// `pool`'s worker `index` picked up `file` (database id `image_id`): it is
    /// at the first step, with nothing found on it yet.
    pub fn pickup(&self, pool: &'static str, index: usize, image_id: i64, file: &str) {
        self.lane(pool, index, pool, BUSY, None, Some("read"), Some(file));
        self.publish(pool, index, |_| {}, Some((image_id, file)));
    }

    /// Record what the worker has learned about the frame it holds. Does
    /// nothing when the lane holds no frame.
    pub fn live(&self, pool: &'static str, index: usize, f: impl FnOnce(&mut LiveFrame)) {
        self.publish(pool, index, f, None);
    }

    fn publish(
        &self,
        pool: &'static str,
        index: usize,
        f: impl FnOnce(&mut LiveFrame),
        start: Option<(i64, &str)>,
    ) {
        let mut lanes = crate::lock(&self.lanes);
        let Some(lane) = lanes
            .iter_mut()
            .find(|lane| lane.pool == pool && lane.index as usize == index)
        else {
            return;
        };
        lane.last_report_at = now_ms();
        if let Some((image_id, file)) = start {
            lane.current = Some(LiveFrame::new(image_id, file));
        }
        if let Some(current) = lane.current.as_mut() {
            f(current);
        }
    }

    #[allow(clippy::too_many_arguments)]
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
        // The slot belongs to one file: another file, or none, clears it.
        if file.is_none() || lane.file.as_deref() != file {
            lane.current = None;
        }
        lane.stage = stage;
        lane.state = state;
        lane.waiting_for = waiting_for;
        lane.step = step;
        lane.last_report_at = now_ms();
        if let (Some(current), Some(step)) = (lane.current.as_mut(), step) {
            // `detect:wait` is still the detect step to the step ladder.
            current.step = step.split(':').next().unwrap_or(step).to_string();
        }
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

    /// The album this progress object reports on.
    pub fn job(&self) -> i64 {
        self.job
    }

    pub fn snapshot(&self) -> ScanProgress {
        let now = self.started.elapsed().as_secs_f64();
        let phase = self.phase.load(Ordering::Relaxed);
        let mut stages = crate::lock(&self.stages);
        let mut measured = Vec::with_capacity(stages.len());
        for node in stages.iter_mut() {
            node.meter.observe(now, node.done);
            measured.push((
                round2(node.meter.rate().unwrap_or(0.0)),
                node.meter.eta(node.done, node.total).map(round1),
            ));
        }
        let lanes = crate::lock(&self.lanes);
        let mut nodes = Vec::with_capacity(stages.len());
        for (index, (node, (rate, eta))) in stages.iter().zip(measured).enumerate() {
            let busy = lanes
                .iter()
                .filter(|lane| lane.state == BUSY && lane.stage == node.id)
                .count() as u32;
            // Frames the nearest upstream stage has finished that this one has
            // not taken yet; the first stage is a source and never has a queue.
            let upstream = stages[..index]
                .iter()
                .rev()
                .find(|stage| stage.total > 0)
                .map_or(0, |stage| stage.done);
            let status = match node.status {
                "pending" if node.total > 0 && node.done >= node.total => "done",
                "pending" if node.done > 0 || busy > 0 => "running",
                other => other,
            };
            nodes.push(StageNode {
                id: node.id.to_string(),
                label: node.label.to_string(),
                model: node.model.clone(),
                device: node.device.clone(),
                workers_total: node.workers_total,
                workers_busy: busy,
                queue_len: upstream.saturating_sub(node.done),
                done: node.done,
                total: node.total,
                errors: node.errors,
                status: status.to_string(),
                error: node.error.clone(),
                skip_consequence: node.skip_consequence.map(str::to_owned),
                parked: node.parked > 0,
                rate_per_s: rate,
                eta_s: eta,
            });
        }
        let overall = self.overall(&nodes);
        let (stage, done, total, rate, eta) = stages.get(phase).or_else(|| stages.first()).map_or(
            ("read".to_string(), 0, 0, 0.0, None),
            |node| {
                (
                    node.id.to_string(),
                    node.done,
                    node.total,
                    round2(node.meter.rate().unwrap_or(0.0)),
                    node.meter.eta(node.done, node.total).map(round1),
                )
            },
        );
        let lane_out: Vec<WorkerLane> = lanes
            .iter()
            .enumerate()
            .map(|(position, lane)| WorkerLane {
                lane: position as u32,
                current: lane.current.clone(),
                stage: if lane.state == IDLE {
                    lane.pool.to_string()
                } else {
                    lane.stage.to_string()
                },
                worker_index: lane.index,
                state: match lane.state {
                    BUSY => "working",
                    WAITING if lane.waiting_for == Some("paused") => "paused",
                    WAITING => "working",
                    FAILED => "failed",
                    _ => "done",
                }
                .into(),
                waiting_for: lane.waiting_for.map(str::to_owned),
                last_report_at: lane.last_report_at,
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
            overall,
        }
    }

    /// The whole scan in weighted work units: each stage's items cost its
    /// measured seconds per item, so the header's bar follows the work, not the
    /// stage with the most items. A stage that has not started yet contributes
    /// its configured cost (`identify` = the VLM estimate), which is why the
    /// ETA is flagged unsure until every real stage is 20 items in.
    fn overall(&self, nodes: &[StageNode]) -> Overall {
        let frame_cost = nodes
            .iter()
            .find(|node| {
                matches!(node.id.as_str(), "read" | "detect" | "sharpness" | "faces")
                    && node.rate_per_s > 0.0
            })
            .map_or(FALLBACK_FRAME_SECONDS, |node| 1.0 / node.rate_per_s);
        let cost = |id: &str| match id {
            IDENTIFY_VISION => self.identify_cost_per_item.max(0.001),
            // The decision stage's asks are cheaper than a vision crop, and
            // OCR is cheaper again.
            IDENTIFY_DECISION => (self.identify_cost_per_item / 2.0).max(0.05),
            IDENTIFY_OCR => frame_cost / 2.0,
            // Grouping and picking are one cheap pass over the album.
            "group" | "pick" => frame_cost / 16.0,
            _ => frame_cost / 4.0,
        };
        let mut done = 0.0;
        let mut total = 0.0;
        let mut rate = 0.0;
        for node in nodes {
            let weight = cost(&node.id);
            done += node.done as f64 * weight;
            total += node.total as f64 * weight;
            if node.total > 0 && node.done < node.total {
                // A measured lane at its own rate; an unstarted stage streams
                // its own work at the configured cost, which is 1 unit/s.
                rate += if node.rate_per_s > 0.0 {
                    node.rate_per_s * weight
                } else if matches!(node.id.as_str(), "group" | "pick") {
                    // A short pass at the end: its work is counted in `left`
                    // but it does not add a stream of its own while it waits.
                    0.0
                } else {
                    1.0
                };
            }
        }
        let left = (total - done).max(0.0);
        let fraction = if total > 0.0 {
            (done / total).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let eta = (rate > 0.0 && left > 0.0).then(|| left / rate);
        // Grouping and picking are one pass at the end with no rate of their
        // own, so they cannot be "warm" while the frames are still being read.
        let eta_confident = eta.is_some()
            && nodes
                .iter()
                .filter(|node| !matches!(node.id.as_str(), "group" | "pick"))
                .filter(|node| node.total >= ETA_CONFIDENT_AFTER)
                .all(|node| node.done >= ETA_CONFIDENT_AFTER);
        Overall {
            done: round2(done),
            total: round2(total),
            fraction: round2(fraction * 100.0) / 100.0,
            rate_per_s: round2(rate),
            eta_s: eta.map(round1),
            eta_confident,
        }
    }
}

/// The wire name of a camera's autofocus area kind.
pub(crate) fn af_kind(kind: conrod_io::af::AreaKind) -> conrod_core::progress::AfKind {
    match kind {
        conrod_io::af::AreaKind::Point => conrod_core::progress::AfKind::Point,
        conrod_io::af::AreaKind::Grid => conrod_core::progress::AfKind::Grid,
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
    let seats = crate::vram_fit::seats(settings);
    let decision_w = (seats.decision as u32).clamp(1, 16);
    let vision_w = (seats.vision as u32).clamp(1, 16);
    let pool_w = identify
        .max(decision_w)
        .max(vision_w)
        .max(settings.identify_decision_worker_count().clamp(1, 16) as u32)
        .max(settings.identify_vision_worker_count().clamp(1, 16) as u32);
    let lanes = settings.lanes();
    let ocr_on = settings.identify_ocr
        && (settings.read_plates || settings.read_numbers || settings.read_text);
    let decision_on = settings.identify_decision && lanes.moments_detect;
    let vision_on = settings.use_vlm && lanes.asks_model() && settings.has_vision_service();
    let stage = |id: &'static str, workers: u32, on: bool| {
        if on {
            StageSpec::new(id, workers)
        } else {
            StageSpec::new(id, workers).off()
        }
    };
    let stages = vec![
        StageSpec::new("read", cull),
        StageSpec::new("detect", cull),
        StageSpec::new("sharpness", cull),
        StageSpec::new("faces", if faces { cull } else { 0 }),
        stage(IDENTIFY_OCR, identify, ocr_on),
        stage(IDENTIFY_DECISION, decision_w, decision_on),
        stage(IDENTIFY_VISION, vision_w, vision_on),
        StageSpec::new("group", 1),
        StageSpec::new("pick", 1),
    ];
    let pools: Vec<(&'static str, usize)> =
        vec![("read", cull as usize), (IDENTIFY_OCR, pool_w as usize)];
    // One crop's VLM time spread over the vision workers: a crop waits
    // VLM_SECONDS_PER_CROP / workers before it comes back.
    Progress::with_identify_cost(
        job,
        stages,
        &pools,
        VLM_SECONDS_PER_CROP / vision_w.max(1) as f64,
    )
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
                StageSpec::new(IDENTIFY_OCR, 1),
                StageSpec::new(IDENTIFY_DECISION, 1),
                StageSpec::new(IDENTIFY_VISION, 1),
                StageSpec::new("group", 1),
                StageSpec::new("pick", 1),
            ],
            &[("read", 2), (IDENTIFY_OCR, 1)],
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
        assert_eq!(waiting_snap.pipeline.workers[0].state, "working");
        assert_eq!(waiting_snap.pipeline.workers[0].stage, "detect");
        assert_eq!(
            waiting_snap.pipeline.workers[0].waiting_for.as_deref(),
            Some("lock")
        );
        assert_eq!(
            waiting_snap.pipeline.workers[1].state, "done",
            "a lane with nothing in hand is not working"
        );
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
        assert_eq!(
            busy.pipeline.stages[1].workers_busy, 1,
            "one worker is inside detect"
        );
        assert_eq!(busy.pipeline.workers[0].state, "working");
        assert_eq!(
            busy.pipeline.stages[1].queue_len, 0,
            "nothing upstream yet: read has not finished a frame"
        );
        // read hands 10 frames on; detect has taken 4 of them.
        progress.total("read", 10);
        progress.advance("read", 10);
        progress.advance("detect", 4);
        assert_eq!(
            progress.snapshot().pipeline.stages[1].queue_len,
            6,
            "frames finished upstream that detect has not taken"
        );
        progress.advance("detect", 6);
        progress.add_found(|found| found.subjects = 12);
        progress.recent(RecentFrame {
            image_id: 4,
            thumb_url: "thumb-4.jpg".into(),
            subject: "vehicle".into(),
            verdict: "sharp".into(),
            panning: true,
            label: None,
            file: None,
            stars: None,
            boxes: Vec::new(),
            af: None,
            af_kind: None,
        });
        let snap = progress.snapshot();
        assert_eq!(snap.stage, "detect");
        assert_eq!(snap.stage_index, 1);
        assert_eq!(snap.stage_count, 9);
        assert_eq!((snap.done, snap.total), (10, 10));
        assert_eq!(snap.found.subjects, 12);
        assert_eq!(snap.recent[0].image_id, 4);
        assert_eq!(snap.recent[0].verdict, "sharp");
        assert!(
            snap.recent[0].panning,
            "a pan is a flag beside the sharpness verdict"
        );
        assert_eq!(snap.pipeline.stages[1].model, "yolo11s-960.onnx");
        progress.idle("read", 0);
        assert_eq!(progress.snapshot().pipeline.stages[1].workers_busy, 0);
    }

    #[test]
    fn the_overall_is_weighted_by_cost_and_the_eta_is_unsure_until_stages_are_warm() {
        let progress = Progress::with_identify_cost(
            2,
            vec![
                StageSpec::new("read", 1),
                StageSpec::new("detect", 1),
                StageSpec::new("sharpness", 1),
                StageSpec::new("faces", 1),
                StageSpec::new(IDENTIFY_VISION, 2),
                StageSpec::new("group", 1),
                StageSpec::new("pick", 1),
            ],
            &[("read", 1), (IDENTIFY_OCR, 2)],
            4.0,
        );
        for stage in ["read", "detect", "sharpness", "faces"] {
            progress.total(stage, 100);
            progress.advance(stage, 10);
        }
        progress.total(IDENTIFY_VISION, 40);
        let first = progress.snapshot();
        assert!(first.overall.total > 0.0);
        assert!(
            first.overall.eta_s.is_some(),
            "an unstarted identify still contributes its configured estimate"
        );
        assert!(
            !first.overall.eta_confident,
            "10 of 100 frames is not enough"
        );
        assert!(first.overall.fraction > 0.0 && first.overall.fraction < 1.0);
        // Identify weighs 4 s a crop against a cull frame's ~0.025 s, so 40
        // crops dominate the bar even though there are only 40 of them.
        let identify_share = 40.0 * 4.0 / first.overall.total;
        assert!(identify_share > 0.5, "identify dominates: {identify_share}");

        for stage in ["read", "detect", "sharpness", "faces"] {
            progress.advance(stage, 20);
        }
        progress.advance(IDENTIFY_VISION, 20);
        let warm = progress.snapshot();
        assert!(
            warm.overall.eta_confident,
            "every stage is 20+ items in: {warm:?}"
        );
        assert!(warm.overall.eta_s.is_some());
        assert_eq!(
            warm.pipeline.stages[4].queue_len, 10,
            "faces done 30, vision took 20"
        );
        assert!(
            warm.overall.fraction > first.overall.fraction,
            "the bar follows the work"
        );
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
        assert_eq!(json["pipeline"]["stages"].as_array().unwrap().len(), 9);
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
                panning: false,
                label: None,
                file: None,
                stars: None,
                boxes: Vec::new(),
                af: None,
                af_kind: None,
            });
        }
        let snap = progress.snapshot();
        assert_eq!(snap.recent.len(), RECENT_LIMIT);
        assert_eq!(snap.recent[0].image_id, RECENT_LIMIT as i64 + 4);
    }

    #[test]
    fn a_lane_publishes_the_frame_it_holds_and_clears_it_when_it_finishes() {
        let progress = fake(5);
        let idle = progress.snapshot();
        assert!(idle.pipeline.workers.iter().all(|w| w.current.is_none()));
        let lanes: Vec<u32> = idle.pipeline.workers.iter().map(|w| w.lane).collect();
        assert_eq!(lanes, [0, 1, 2], "dense ids over every lane");

        progress.pickup("read", 1, 42, "IMG_1.CR3");
        let at_pickup = progress.snapshot().pipeline.workers[1].clone();
        let current = at_pickup.current.expect("the lane holds a frame");
        assert_eq!((current.image_id, current.file.as_str()), (42, "IMG_1.CR3"));
        assert_eq!(current.step, "read");
        assert!(current.boxes.is_none() && current.stars.is_none());
        assert!(progress.snapshot().pipeline.workers[0].current.is_none());

        progress.busy("read", 1, "read", "decode", "IMG_1.CR3");
        progress.live("read", 1, |f| f.thumb_url = Some("thumb-42.jpg".into()));
        progress.waiting("read", 1, "detect", "detector", "detect:wait", "IMG_1.CR3");
        let waiting = progress.snapshot().pipeline.workers[1].clone();
        assert_eq!(
            waiting.state, "working",
            "a blocked worker is still working"
        );
        assert_eq!(waiting.waiting_for.as_deref(), Some("detector"));
        let current = waiting.current.unwrap();
        assert_eq!(current.step, "detect", "the wait is still the detect step");
        assert_eq!(current.thumb_url.as_deref(), Some("thumb-42.jpg"));

        progress.busy("read", 1, "detect", "detect", "IMG_1.CR3");
        progress.live("read", 1, |f| {
            f.boxes = Some(vec![conrod_core::progress::LiveBox::from_pixels(
                "car",
                0.9,
                [10.0, 10.0, 50.0, 30.0],
                100,
                100,
            )]);
            f.af = Some([0.2, 0.2, 0.3, 0.3]);
            f.af_kind = Some(conrod_core::progress::AfKind::Grid);
        });
        progress.busy("read", 1, "sharpness", "sharpness", "IMG_1.CR3");
        progress.live("read", 1, |f| {
            f.stars = Some(4);
            f.verdict = Some("sharp".into());
            f.panning = Some(true);
        });
        let json = serde_json::to_value(progress.snapshot()).unwrap();
        let live = &json["pipeline"]["workers"][1]["current"];
        assert_eq!(live["step"], "sharpness");
        assert_eq!(live["boxes"][0]["cls"], "car");
        assert_eq!(live["boxes"][0]["box"][2], 0.5);
        assert_eq!(live["af_kind"], "grid");
        assert_eq!(live["stars"], 4);
        assert_eq!(live["verdict"], "sharp");
        assert_eq!(live["panning"], true);

        // The next frame starts empty: nothing of the last one is left.
        progress.pickup("read", 1, 43, "IMG_2.CR3");
        let next = progress.snapshot().pipeline.workers[1].current.clone();
        let next = next.unwrap();
        assert_eq!(next.image_id, 43);
        assert!(next.boxes.is_none() && next.af.is_none() && next.stars.is_none());

        // Finishing clears the slot, and a stray update after that is ignored.
        progress.idle("read", 1);
        progress.live("read", 1, |f| f.stars = Some(5));
        let done = progress.snapshot();
        assert!(done.pipeline.workers[1].current.is_none());
        let json = serde_json::to_value(done).unwrap();
        assert!(json["pipeline"]["workers"][1]["current"].is_null());
    }

    #[test]
    fn counters_follow_the_frames_and_add_up_to_the_album() {
        let progress = fake(6);
        // An album of 10 frames, 4 of them read in an earlier run.
        for stage in ["read", "detect", "sharpness"] {
            progress.carry(stage, 4);
            progress.plan(stage, 6);
        }
        for stage in ["group", "pick"] {
            progress.plan(stage, 10);
        }
        let stage = |p: &Arc<Progress>, id: &str| {
            let snap = p.snapshot();
            let node = snap.pipeline.stages.iter().find(|s| s.id == id).unwrap();
            (node.done, node.total, node.errors)
        };
        assert_eq!(stage(&progress, "read"), (4, 10, 0), "a resume starts at 4");
        assert_eq!(
            stage(&progress, "faces"),
            (0, 0, 0),
            "an off stage stays empty"
        );

        // Frame 1 gets all the way through; frame 2 is read but the detector fails it.
        let stages = ["read", "detect", "sharpness"];
        for id in stages {
            progress.advance(id, 1);
        }
        progress.advance("read", 1);
        progress.abandon(&stages, 1, true);
        // Frame 3 is a hand reject: never read, it leaves every total.
        progress.abandon(&stages, 0, false);

        assert_eq!(
            stage(&progress, "read"),
            (6, 9, 0),
            "read took 2 of 6 new frames"
        );
        assert_eq!(
            stage(&progress, "detect"),
            (5, 8, 1),
            "the failed frame is the detect stage's error"
        );
        assert_eq!(stage(&progress, "sharpness"), (5, 8, 0));

        // The rest of the album finishes.
        progress.advance("read", 3);
        for id in ["detect", "sharpness"] {
            progress.advance(id, 3);
        }
        for id in stages {
            let (done, total, _) = stage(&progress, id);
            assert_eq!(done, total, "{id} ends with every frame through it");
        }
        // album = frames in the stage + frames that failed in it or before it + hand rejects
        let (_, read_total, _) = stage(&progress, "read");
        assert_eq!(read_total + 1, 10, "10 frames less the one hand reject");

        progress.settle("group", 8);
        progress.settle("pick", 8);
        assert_eq!(stage(&progress, "group"), (8, 8, 0));
        assert_eq!(stage(&progress, "pick"), (8, 8, 0));
    }

    #[test]
    fn the_overall_eta_covers_the_stages_that_have_not_started() {
        let progress = fake(7);
        for stage in ["read", "detect", "sharpness"] {
            progress.plan(stage, 100);
            progress.advance(stage, 50);
        }
        // Vision has not started, but its work is still ahead.
        progress.plan(IDENTIFY_VISION, 100);
        progress.plan("group", 100);
        progress.plan("pick", 100);
        // A rate needs two samples with work between them: the first snapshot
        // starts the meter's clock.
        let _ = progress.snapshot();
        std::thread::sleep(std::time::Duration::from_millis(50));
        for stage in ["read", "detect", "sharpness"] {
            progress.advance(stage, 1);
        }
        let snap = progress.snapshot();
        let cull_eta = snap
            .pipeline
            .stages
            .iter()
            .find(|stage| stage.id == "read")
            .and_then(|stage| stage.eta_s)
            .expect("the running stage has an ETA");
        let overall = snap.overall.eta_s.expect("the whole scan has an ETA");
        assert!(
            overall > cull_eta,
            "the header's ETA includes identify/group/pick: {overall} vs {cull_eta}"
        );
    }

    #[test]
    fn a_pass_with_no_rate_of_its_own_does_not_hold_the_eta_back() {
        let progress = fake(7);
        for stage in ["read", "detect", "sharpness"] {
            progress.plan(stage, 100);
            progress.advance(stage, 30);
        }
        // Grouping waits at the end with the whole album in front of it.
        progress.plan("group", 100);
        progress.plan("pick", 100);
        let snap = progress.snapshot();
        assert!(snap.overall.eta_s.is_some());
        assert!(
            snap.overall.eta_confident,
            "an unstarted group pass is not a cold stage"
        );
    }

    #[test]
    fn a_skipped_sub_stage_says_skipped_and_has_no_counters() {
        let progress = fake(11);
        progress.plan(IDENTIFY_OCR, 10);
        progress.advance(IDENTIFY_OCR, 3);
        progress.skip_stage(IDENTIFY_OCR);
        let node = progress
            .snapshot()
            .pipeline
            .stages
            .into_iter()
            .find(|s| s.id == IDENTIFY_OCR)
            .unwrap();
        assert_eq!(node.status, "skipped");
        assert_eq!((node.done, node.total), (0, 0), "no phantom work");
        assert_eq!(node.workers_total, 0);
    }

    #[test]
    fn a_worker_reports_its_lifecycle_and_when_it_last_reported() {
        let progress = fake(12);
        progress.busy(IDENTIFY_OCR, 0, IDENTIFY_OCR, "ocr", "IMG_1.CR3");
        let working = progress.snapshot();
        let lane = &working.pipeline.workers[2];
        assert_eq!(lane.state, "working");
        assert_eq!(lane.step.as_deref(), Some("ocr"));
        assert!(lane.last_report_at > 0, "the screen's heartbeat");
        assert_eq!(lane.stage, IDENTIFY_OCR);

        progress.waiting(
            IDENTIFY_OCR,
            0,
            IDENTIFY_DECISION,
            "paused",
            "wait",
            "IMG_1.CR3",
        );
        assert_eq!(
            progress.snapshot().pipeline.workers[2].state,
            "paused",
            "a paused worker never looks busy"
        );

        progress.worker_failed(IDENTIFY_OCR, 0);
        assert_eq!(
            progress.snapshot().pipeline.workers[2].state,
            "failed",
            "a dead worker says failed, never busy"
        );

        progress.idle(IDENTIFY_OCR, 0);
        assert_eq!(
            progress.snapshot().pipeline.workers[2].state,
            "done",
            "a worker with nothing in hand is not working"
        );

        progress.fail_stage(IDENTIFY_VISION, "The vision service did not answer");
        let node = progress
            .snapshot()
            .pipeline
            .stages
            .into_iter()
            .find(|s| s.id == IDENTIFY_VISION)
            .unwrap();
        assert_eq!(node.status, "failed");
        assert_eq!(
            node.error.as_deref(),
            Some("The vision service did not answer"),
            "a failure carries a plain reason"
        );
    }

    #[test]
    fn a_progress_node_carries_its_skip_consequence_and_can_be_unskipped() {
        let progress = fake(14);
        let node = |id: &str| {
            progress
                .snapshot()
                .pipeline
                .stages
                .into_iter()
                .find(|s| s.id == id)
                .unwrap()
        };
        assert_eq!(
            node(IDENTIFY_VISION).skip_consequence.as_deref(),
            conrod_core::progress::skip_consequence(IDENTIFY_VISION)
        );
        assert_eq!(
            node("read").skip_consequence,
            None,
            "a feed stage cannot be skipped, so it has no consequence"
        );
        progress.skip_stage(IDENTIFY_VISION);
        assert_eq!(node(IDENTIFY_VISION).status, "skipped");
        progress.reset_stage(IDENTIFY_VISION);
        let back = node(IDENTIFY_VISION);
        assert_eq!(back.status, "pending");
        assert_eq!(
            back.workers_total, 1,
            "the laid-out worker count comes back with the un-skip"
        );
    }

    #[test]
    fn a_stage_is_parked_until_its_last_worker_leaves_the_pause_point() {
        let progress = fake(15);
        let node = |p: &Progress| {
            p.snapshot()
                .pipeline
                .stages
                .into_iter()
                .find(|n| n.id == "detect")
                .unwrap()
        };
        assert!(!node(&progress).parked);
        progress.park_stage("detect");
        progress.park_stage("detect");
        let parked = node(&progress);
        assert!(parked.parked, "a worker is holding at the stage pause");
        assert_eq!(parked.status, "paused");
        progress.unpark_stage("detect");
        let parked = node(&progress);
        assert!(parked.parked, "one worker left, one is still held");
        assert_eq!(parked.status, "paused", "a sibling leaving never un-pauses");
        progress.unpark_stage("detect");
        let running = node(&progress);
        assert!(!running.parked);
        assert_ne!(running.status, "paused");
    }

    #[test]
    fn a_paused_worker_reports_paused_even_from_the_cull_pool() {
        let progress = fake(15);
        progress.busy("read", 0, "detect", "detect", "IMG_1.CR3");
        progress.worker_paused("read", 0);
        let lane = &progress.snapshot().pipeline.workers[0];
        assert_eq!(lane.state, "paused");
        assert_eq!(lane.waiting_for.as_deref(), Some("paused"));
        assert_eq!(
            lane.stage, "detect",
            "the lane keeps the stage it was on, but is not busy in it"
        );
    }

    #[test]
    fn a_disabled_sub_stage_is_laid_out_skipped_by_for_scan() {
        let settings = conrod_core::settings::Settings {
            identify_ocr: false,
            ..conrod_core::settings::Settings::default()
        };
        let progress = for_scan(13, &settings, false);
        let snap = progress.snapshot();
        let node = |id: &str| {
            snap.pipeline
                .stages
                .iter()
                .find(|s| s.id == id)
                .unwrap()
                .clone()
        };
        let ocr = node(IDENTIFY_OCR);
        assert_eq!(ocr.status, "skipped", "OCR is off for this album");
        assert_eq!((ocr.workers_total, ocr.total), (0, 0), "no phantom lane");
        assert_ne!(
            node(IDENTIFY_VISION).status,
            "skipped",
            "vision follows use_vlm"
        );
    }

    #[test]
    fn for_scan_sizes_identify_pool_for_vision_seats_and_gates_decision() {
        let settings = conrod_core::settings::Settings {
            analysis_workers: 1,
            identify_vision_workers: 4,
            identify_decision_workers: 2,
            identify_decision: false,
            ..conrod_core::settings::Settings::default()
        };
        let highest = settings.identify_vision_worker_count().max(1) as usize - 1;
        assert!(
            highest >= settings.identify_worker_count() as usize,
            "vision seat count should exceed analysis_workers"
        );
        let progress = for_scan(16, &settings, false);
        progress.busy(
            IDENTIFY_OCR,
            highest,
            IDENTIFY_VISION,
            "vision",
            "IMG_1.CR3",
        );
        let snap = progress.snapshot();
        let lane = snap
            .pipeline
            .workers
            .iter()
            .find(|w| w.stage == IDENTIFY_VISION && w.worker_index as usize == highest)
            .expect("highest vision worker index must have a lane in the identify pool");
        assert_eq!(lane.state, "working");
        assert_eq!(lane.file.as_deref(), Some("IMG_1.CR3"));
        let decision = snap
            .pipeline
            .stages
            .iter()
            .find(|s| s.id == IDENTIFY_DECISION)
            .unwrap();
        assert_eq!(
            decision.status, "skipped",
            "identify_decision: false starts identify:decision as skipped"
        );
        assert_eq!(decision.workers_total, 0);
    }
}
