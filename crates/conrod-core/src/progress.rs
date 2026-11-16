//! What the scan screen shows while a scan runs: the stage list, the honest
//! rate and ETA, the frames that just went through, and the pipeline snapshot
//! (which pool is doing what, on which model and device).
//!
//! Pure data and arithmetic: the engine fills these in from cheap shared state
//! and the app serialises one snapshot per event. Field names are the wire
//! format, so the frontend reads exactly these.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// The stages a one-shot scan runs, in order: `(id, label)`. The ids are the
/// wire format; the frontend draws the flowchart from the snapshot's own
/// `pipeline.stages`, which carries these in this order.
pub const STAGES: [(&str, &str); 7] = [
    ("read", "Read"),
    ("detect", "Detect"),
    ("sharpness", "Sharpness"),
    ("faces", "Faces"),
    ("identify", "Identify"),
    ("group", "Group"),
    ("pick", "Pick"),
];

/// How many recent frames a snapshot carries.
pub const RECENT_LIMIT: usize = 24;

/// A sliding-window rate: how many items per second finished over the last
/// `span` seconds. A moving average rather than a lifetime mean, so a scan that
/// slows down at the far end of a burst says so instead of hiding it.
#[derive(Debug, Clone)]
pub struct RateMeter {
    span: f64,
    /// `(seconds, done)` in the order observed.
    samples: VecDeque<(f64, u64)>,
}

impl RateMeter {
    pub fn new(span_seconds: f64) -> RateMeter {
        RateMeter {
            span: span_seconds.max(0.5),
            samples: VecDeque::new(),
        }
    }

    /// Record that `done` items had finished at `at` seconds. Time must not go
    /// backwards; a sample at the same instant replaces nothing and is kept, so
    /// a burst of progress inside one emission window still counts.
    pub fn observe(&mut self, at: f64, done: u64) {
        self.samples.push_back((at, done));
        while self.samples.len() > 2 {
            let Some(&(first, _)) = self.samples.front() else {
                break;
            };
            if at - first > self.span {
                self.samples.pop_front();
            } else {
                break;
            }
        }
    }

    /// Items per second over the window, or `None` before there is a span.
    pub fn rate(&self) -> Option<f64> {
        let (first, last) = (self.samples.front()?, self.samples.back()?);
        let seconds = last.0 - first.0;
        if seconds <= 0.0 {
            return None;
        }
        Some(last.1.saturating_sub(first.1) as f64 / seconds)
    }

    /// Seconds left at the window's rate, or `None` when there is no rate or
    /// nothing left to do.
    pub fn eta(&self, done: u64, total: u64) -> Option<f64> {
        let rate = self.rate()?;
        (rate > 0.0 && total > done).then(|| (total - done) as f64 / rate)
    }
}

/// What the scan has found so far.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub subjects: u64,
    pub soft: u64,
    pub closed_eyes: u64,
    pub numbers: u64,
    pub plates: u64,
    pub bursts: u64,
    pub keepers: u64,
}

/// One frame that just finished, for the live strip on the scan screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentFrame {
    pub image_id: i64,
    /// The thumbnail path the review grid already uses (`cache/native/thumb-N.jpg`);
    /// the UI turns it into an asset URL exactly as it does `frames[].thumb_path`.
    pub thumb_url: String,
    /// The frame's main subject class, `"vehicle"`, `"person"` or `"frame"`.
    pub subject: String,
    /// How sharp the subject is: `sharp` or `soft`. A pan is a separate flag, so a
    /// panned subject still says whether it came out sharp.
    pub verdict: String,
    /// True when the frame was taken as a pan: the subject is sharp and the
    /// background is streaked.
    #[serde(default)]
    pub panning: bool,
    /// What the frame reads as once known: a race number or plate. `null` until
    /// identification has looked at it.
    pub label: Option<String>,
}

/// One stage of the flowchart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageNode {
    pub id: String,
    pub label: String,
    /// The model this stage runs, as loaded: e.g. `yolo11s-960.onnx`, or
    /// `qwen2.5vl:7b via ollama`.
    pub model: String,
    /// Where it runs: `DirectML`, `CPU`, `ollama`.
    pub device: String,
    /// How many workers this stage can use.
    pub workers_total: u32,
    /// How many are inside it right now.
    pub workers_busy: u32,
    /// Frames the upstream stage has finished that this stage has not taken
    /// yet (upstream.done - done); 0 for the first stage and 0 when the lanes
    /// run in lockstep, as read/detect/sharpness/faces do in the cull pool.
    pub queue_len: u64,
    pub done: u64,
    pub total: u64,
    pub rate_per_s: f64,
    pub eta_s: Option<f64>,
}

/// One worker lane: what that worker is doing this instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerLane {
    pub stage: String,
    pub worker_index: u32,
    /// `idle`, `busy` or `waiting`.
    pub state: String,
    /// Why it is waiting: `gpu`, `lock`, `ollama` or `paused`. `null` unless
    /// `state` is `waiting`.
    pub waiting_for: Option<String>,
    /// The file, as a name the UI can show next to a thumbnail.
    pub file: Option<String>,
    pub step: Option<String>,
    /// How long the worker has been in this step.
    pub ms_in_step: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Pipeline {
    pub stages: Vec<StageNode>,
    pub workers: Vec<WorkerLane>,
}

/// Progress across every stage at once, in weighted frame/work units: each
/// stage's items are weighted by its measured cost per item, so a bar does not
/// jump because one stage has more items than another. `eta_confident` is false
/// until every stage with real throughput is at least 20 items in, because an
/// ETA from one or two items is a guess.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Overall {
    pub done: f64,
    pub total: f64,
    /// 0..1, the weighted work finished.
    pub fraction: f64,
    /// Weighted work units per second, from the lanes running now; a stage that
    /// has not started contributes its configured estimate.
    pub rate_per_s: f64,
    pub eta_s: Option<f64>,
    pub eta_confident: bool,
}

/// One progress event. `state` is `running`, `done`, `stopped` or `error`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScanProgress {
    pub job_id: i64,
    pub state: String,
    /// The stage the whole scan is on, and its place in `STAGES`.
    pub stage: String,
    pub stage_index: u32,
    pub stage_count: u32,
    pub done: u64,
    pub total: u64,
    pub rate_per_s: f64,
    pub eta_s: Option<f64>,
    pub recent: Vec<RecentFrame>,
    pub found: Found,
    pub pipeline: Pipeline,
    /// The whole scan: the header's bar and time left.
    pub overall: Overall,
}

impl ScanProgress {
    /// The stage index (0-based) of `id` in [`STAGES`]; `None` for a stage the
    /// snapshot does not carry.
    pub fn stage_index(id: &str) -> Option<usize> {
        STAGES.iter().position(|(stage, _)| *stage == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_is_a_moving_average_over_the_window_not_a_lifetime_mean() {
        let mut meter = RateMeter::new(4.0);
        // 10 items a second for four seconds, then 1 a second for four more.
        for (at, done) in [(0.0, 0), (1.0, 10), (2.0, 20), (3.0, 30), (4.0, 40)] {
            meter.observe(at, done);
        }
        assert_eq!(meter.rate(), Some(10.0));
        assert_eq!(meter.eta(40, 80), Some(4.0));
        for (at, done) in [(5.0, 41), (6.0, 42), (7.0, 43), (8.0, 44)] {
            meter.observe(at, done);
        }
        let rate = meter.rate().unwrap();
        assert!(
            (rate - 1.0).abs() < 0.51,
            "recent seconds decide the rate, got {rate}"
        );
        assert!(meter.eta(44, 144).unwrap() > 90.0, "the ETA follows it down");
    }

    #[test]
    fn no_span_no_rate_and_nothing_left_no_eta() {
        let mut meter = RateMeter::new(4.0);
        assert_eq!(meter.rate(), None);
        assert_eq!(meter.eta(0, 10), None);
        meter.observe(5.0, 10);
        assert_eq!(meter.rate(), None, "one sample is a point, not a rate");
        meter.observe(5.0, 20);
        assert_eq!(meter.rate(), None, "no time passed");
        assert_eq!(meter.eta(20, 20), None, "nothing left to do");
        meter.observe(6.0, 20);
        assert_eq!(meter.eta(20, 20), None);
        assert_eq!(meter.rate(), Some(10.0), "the window still holds the burst");
        meter.observe(12.0, 20);
        assert_eq!(meter.rate(), Some(0.0), "a stalled window reads zero");
        assert_eq!(meter.eta(20, 30), None, "a stall has no honest ETA");
    }

    #[test]
    fn a_snapshot_carries_the_wire_field_names_and_the_fake_pipeline() {
        let mut meter = RateMeter::new(4.0);
        meter.observe(0.0, 0);
        meter.observe(2.0, 40);
        let progress = ScanProgress {
            job_id: 7,
            state: "running".into(),
            stage: "detect".into(),
            stage_index: 1,
            stage_count: STAGES.len() as u32,
            done: 40,
            total: 140,
            rate_per_s: meter.rate().unwrap(),
            eta_s: meter.eta(40, 140),
            recent: vec![RecentFrame {
                image_id: 12,
                thumb_url: "cache/native/thumb-12.jpg".into(),
                subject: "vehicle".into(),
                verdict: "sharp".into(),
                label: None,
            }],
            found: Found {
                subjects: 61,
                soft: 4,
                closed_eyes: 1,
                numbers: 2,
                plates: 1,
                bursts: 9,
                keepers: 9,
            },
            pipeline: Pipeline {
                stages: vec![StageNode {
                    id: "detect".into(),
                    label: "Detect".into(),
                    model: "yolo11s-960.onnx".into(),
                    device: "DirectML".into(),
                    workers_total: 3,
                    workers_busy: 2,
                    queue_len: 100,
                    done: 40,
                    total: 140,
                    rate_per_s: 20.0,
                    eta_s: Some(5.0),
                }],
                workers: vec![WorkerLane {
                    stage: "detect".into(),
                    worker_index: 1,
                    state: "waiting".into(),
                    waiting_for: Some("lock".into()),
                    file: Some("IMG_4231.CR3".into()),
                    step: Some("detect:wait".into()),
                    ms_in_step: 12.5,
                }],
            },
            overall: Overall {
                done: 40.0,
                total: 140.0,
                fraction: 0.2857,
                rate_per_s: 20.0,
                eta_s: Some(5.0),
                eta_confident: false,
            },
        };
        let value = serde_json::to_value(&progress).unwrap();
        for key in [
            "job_id",
            "state",
            "stage",
            "stage_index",
            "stage_count",
            "done",
            "total",
            "rate_per_s",
            "eta_s",
            "recent",
            "found",
            "pipeline",
            "overall",
        ] {
            assert!(value.get(key).is_some(), "missing {key}: {value}");
        }
        let overall = &value["overall"];
        for key in [
            "done",
            "total",
            "fraction",
            "rate_per_s",
            "eta_s",
            "eta_confident",
        ] {
            assert!(overall.get(key).is_some(), "missing overall.{key}: {overall}");
        }
        assert_eq!(overall["eta_confident"], false);
        let stage = &value["pipeline"]["stages"][0];
        for key in [
            "id",
            "label",
            "model",
            "device",
            "workers_total",
            "workers_busy",
            "queue_len",
            "done",
            "total",
            "rate_per_s",
        ] {
            assert!(stage.get(key).is_some(), "missing stage.{key}: {stage}");
        }
        let worker = &value["pipeline"]["workers"][0];
        for key in [
            "stage",
            "worker_index",
            "state",
            "waiting_for",
            "file",
            "step",
            "ms_in_step",
        ] {
            assert!(worker.get(key).is_some(), "missing worker.{key}: {worker}");
        }
        assert_eq!(value["pipeline"]["workers"][0]["waiting_for"], "lock");
        assert_eq!(ScanProgress::stage_index("faces"), Some(3));
        assert_eq!(ScanProgress::stage_index("nope"), None);
    }
}
