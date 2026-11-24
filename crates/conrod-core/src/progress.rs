//! What the scan screen shows while a scan runs: the stage list, the honest
//! rate and ETA, the frames that just went through, and the pipeline snapshot
//! (which pool is doing what, on which model and device).
//!
//! Pure data and arithmetic: the engine fills these in from cheap shared state
//! and the app serialises one snapshot per event. Field names are the wire
//! format, so the frontend reads exactly these.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// The identification sub-stage that reads plates, numbers and visible text.
pub const IDENTIFY_OCR: &str = "identify:ocr";
/// The identification sub-stage that asks the decision model the shoot's
/// questions and compares each burst for its peak.
pub const IDENTIFY_DECISION: &str = "identify:decision";
/// The identification sub-stage that asks the vision model about each subject.
pub const IDENTIFY_VISION: &str = "identify:vision";

/// The stages a one-shot scan runs, in order: `(id, label)`. The ids are the
/// wire format; the frontend draws the flowchart from the snapshot's own
/// `pipeline.stages`, which carries these in this order.
///
/// Identify is three first-class stages, in this order: OCR reads the frame,
/// Decision asks the shoot's questions and picks each burst's peak, and Vision
/// reads each subject. An older snapshot that still says `identify` renders:
/// [`ScanProgress::stage_index`] maps it onto the first sub-stage.
pub const STAGES: [(&str, &str); 9] = [
    ("read", "Read"),
    ("detect", "Detect"),
    ("sharpness", "Sharpness"),
    ("faces", "Faces"),
    (IDENTIFY_OCR, "Identify · OCR"),
    (IDENTIFY_DECISION, "Identify · Decision"),
    (IDENTIFY_VISION, "Identify · Vision"),
    ("group", "Group"),
    ("pick", "Pick"),
];

/// The old single identify stage id, kept so a snapshot written before the
/// split still resolves to a place in the flowchart.
pub const LEGACY_IDENTIFY: &str = "identify";

/// The wire id `id` names, if it is one of [`STAGES`]. The control commands
/// accept only these: an unknown id would persist a stage nothing reads.
pub fn known_stage(id: &str) -> Option<&'static str> {
    STAGES
        .iter()
        .find(|(stage, _)| *stage == id)
        .map(|(stage, _)| *stage)
}

/// What skipping `stage` does, in one plain line the screen shows before it
/// confirms. `None` means the stage cannot be skipped: it feeds every stage
/// after it, so the run could not continue.
pub fn skip_consequence(stage: &str) -> Option<&'static str> {
    match stage {
        "faces" => Some(
            "Skipping Faces: no eye-open or smile reads, so people frames rank on sharpness alone.",
        ),
        IDENTIFY_OCR => {
            Some("Skipping OCR: no plate or number reads; Vision reads any numbers on its own.")
        }
        IDENTIFY_DECISION => Some(
            "Skipping Decision: no type suggestion and no peak picks; ranking is unchanged, and a mixed album keeps its stored type.",
        ),
        IDENTIFY_VISION => {
            Some("Skipping Vision: OCR and Decision results stay, but there are no make/model reads for this album.")
        }
        "group" => Some("Skipping Group: every frame keeps its seed burst; nothing is regrouped."),
        "pick" => Some("Skipping Pick: no keeper suggestions; hand picks stand."),
        _ => None,
    }
}

/// Whether the control surface may skip `stage` at all.
pub fn skippable(stage: &str) -> bool {
    skip_consequence(stage).is_some()
}

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

/// One kept detection on a frame: the class as the detector names it, its
/// confidence, and the box as fractions (0..1) of the upright frame,
/// `[x1, y1, x2, y2]`, so it maps straight onto the thumbnail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveBox {
    pub cls: String,
    pub conf: f32,
    #[serde(rename = "box")]
    pub bbox: [f32; 4],
}

impl LiveBox {
    /// A detection's pixel box as fractions of a `width` x `height` frame,
    /// clamped into it. Display only: the pixel box is what the rating used.
    pub fn from_pixels(cls: &str, conf: f32, bbox: [f64; 4], width: usize, height: usize) -> Self {
        let (w, h) = (width.max(1) as f64, height.max(1) as f64);
        let unit = |v: f64, edge: f64| (v / edge).clamp(0.0, 1.0) as f32;
        LiveBox {
            cls: cls.to_string(),
            conf,
            bbox: [
                unit(bbox[0], w),
                unit(bbox[1], h),
                unit(bbox[2], w),
                unit(bbox[3], h),
            ],
        }
    }
}

/// Whether a camera's autofocus area is one point or a grid's union.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AfKind {
    Point,
    Grid,
}

/// The frame a worker lane holds right now, with whatever has been found on it
/// so far. Every field after `step` fills in as the steps finish and is left out
/// of the payload until then.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveFrame {
    pub image_id: i64,
    /// The file name only.
    pub file: String,
    /// `read`, `decode`, `detect`, `sharpness`, `faces` or `persist`.
    pub step: String,
    /// The thumbnail path the review grid uses (`cache/native/thumb-N.jpg`), as
    /// soon as it has been written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb_url: Option<String>,
    /// The kept detections, once the detector has run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boxes: Option<Vec<LiveBox>>,
    /// The camera's autofocus area as fractions of the upright frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub af: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub af_kind: Option<AfKind>,
    /// 1..5, once the frame is rated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stars: Option<u8>,
    /// `sharp`, `soft` or `blurred`, once the frame is rated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panning: Option<bool>,
}

impl LiveFrame {
    /// A frame a lane has just picked up: only its identity and first step.
    pub fn new(image_id: i64, file: &str) -> Self {
        LiveFrame {
            image_id,
            file: file.to_string(),
            step: "read".to_string(),
            thumb_url: None,
            boxes: None,
            af: None,
            af_kind: None,
            stars: None,
            verdict: None,
            panning: None,
        }
    }
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
    /// How sharp the subject is: `sharp`, `soft` or `blurred`. A pan is a separate
    /// flag, so a panned subject still says whether it came out sharp.
    pub verdict: String,
    /// True when the frame was taken as a pan: the subject is sharp and the
    /// background is streaked.
    #[serde(default)]
    pub panning: bool,
    /// What the frame reads as once known: a race number or plate. `null` until
    /// identification has looked at it.
    pub label: Option<String>,
    /// The file name only: lets a lane find its last finished thumbnail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// 1..5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stars: Option<u8>,
    /// The kept detections; left out when there were none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boxes: Vec<LiveBox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub af: Option<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub af_kind: Option<AfKind>,
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
    /// Frames that finished this stage. A frame that came to nothing inside it
    /// is in `errors` instead, and no longer in `total` here or downstream, so
    /// `done == total` at the end of a run whatever failed. The carried frames
    /// of a resumed album are in both.
    pub done: u64,
    pub total: u64,
    /// Frames that failed inside this stage in this run.
    #[serde(default)]
    pub errors: u64,
    /// `pending`, `running`, `paused`, `skipped`, `done` or `failed`. A stage
    /// that is switched off for the album says `skipped` and keeps its
    /// counters at zero: it never pretends to have work.
    #[serde(default = "pending_status")]
    pub status: String,
    /// The plain reason a stage failed, if it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// What skipping this stage would do, in one plain line. Present for every
    /// stage the controls can skip (the screen shows it before confirming),
    /// `null` for a stage that feeds every stage after it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skip_consequence: Option<String>,
    /// True while at least one worker of this stage is held at the stage's own
    /// pause point (the per-stage Pause control). It is the stage's state, not
    /// the scan's: the scan keeps running and other stages keep working, so the
    /// screen must not read it as a paused run. `status` is `paused` at the same
    /// time; `parked` says a worker is actually holding, not just that a pause
    /// was asked for.
    #[serde(default)]
    pub parked: bool,
    pub rate_per_s: f64,
    pub eta_s: Option<f64>,
}

/// The default [`StageNode::status`] for a snapshot written before the field
/// existed: the caller knows from its counters and worker count.
fn pending_status() -> String {
    "pending".into()
}

/// One worker lane: what that worker is doing this instant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerLane {
    pub stage: String,
    pub worker_index: u32,
    /// A stable 0-based id that survives the lane changing `stage`, dense over
    /// every lane the snapshot lists.
    #[serde(default)]
    pub lane: u32,
    /// `working`, `paused`, `done` or `failed`: what the worker is doing right
    /// now, in the UI's own lifecycle words. A blocked worker (waiting on a
    /// detector, the GPU or a lock) is still `working`; `waiting_for` says what
    /// it is blocked on.
    pub state: String,
    /// Why it is waiting: `detector` (a detector session), `faces` (the face
    /// pool), `lock`, `ollama` or `paused`. `null` unless `state` is `working`
    /// and it is blocked.
    pub waiting_for: Option<String>,
    /// When this worker last reported, in Unix epoch milliseconds. The screen
    /// uses it to tell a slow ask from a dead one honestly.
    #[serde(default)]
    pub last_report_at: i64,
    /// The file, as a name the UI can show next to a thumbnail.
    pub file: Option<String>,
    pub step: Option<String>,
    /// How long the worker has been in this step.
    pub ms_in_step: f64,
    /// The frame the lane holds, `null` while it is idle.
    #[serde(default)]
    pub current: Option<LiveFrame>,
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
    /// snapshot does not carry. The old single `identify` id resolves to the
    /// OCR sub-stage, so an old snapshot still has a place in the flowchart.
    pub fn stage_index(id: &str) -> Option<usize> {
        STAGES
            .iter()
            .position(|(stage, _)| *stage == id)
            .or_else(|| {
                (id == LEGACY_IDENTIFY)
                    .then(|| STAGES.iter().position(|(stage, _)| *stage == IDENTIFY_OCR))
                    .flatten()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert!(
            meter.eta(44, 144).unwrap() > 90.0,
            "the ETA follows it down"
        );
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
                panning: false,
                label: None,
                file: Some("IMG_4212.CR3".into()),
                stars: Some(4),
                boxes: vec![LiveBox::from_pixels(
                    "car",
                    0.92,
                    [100.0, 50.0, 300.0, 150.0],
                    400,
                    200,
                )],
                af: Some([0.4, 0.4, 0.5, 0.5]),
                af_kind: Some(AfKind::Point),
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
                    errors: 0,
                    status: "running".into(),
                    error: None,
                    skip_consequence: skip_consequence("detect").map(str::to_owned),
                    parked: false,
                    rate_per_s: 20.0,
                    eta_s: Some(5.0),
                }],
                workers: vec![WorkerLane {
                    stage: "detect".into(),
                    worker_index: 1,
                    lane: 1,
                    state: "working".into(),
                    waiting_for: Some("lock".into()),
                    last_report_at: 1_700_000_000_000,
                    file: Some("IMG_4231.CR3".into()),
                    step: Some("detect:wait".into()),
                    ms_in_step: 12.5,
                    current: Some(LiveFrame::new(31, "IMG_4231.CR3")),
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
            assert!(
                overall.get(key).is_some(),
                "missing overall.{key}: {overall}"
            );
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
            "errors",
            "status",
            "rate_per_s",
        ] {
            assert!(stage.get(key).is_some(), "missing stage.{key}: {stage}");
        }
        let worker = &value["pipeline"]["workers"][0];
        for key in [
            "stage",
            "worker_index",
            "lane",
            "state",
            "waiting_for",
            "last_report_at",
            "file",
            "step",
            "ms_in_step",
            "current",
        ] {
            assert!(worker.get(key).is_some(), "missing worker.{key}: {worker}");
        }
        assert_eq!(value["pipeline"]["workers"][0]["waiting_for"], "lock");
        assert_eq!(value["pipeline"]["workers"][0]["state"], "working");
        assert_eq!(
            value["pipeline"]["workers"][0]["last_report_at"],
            1_700_000_000_000i64
        );
        assert_eq!(ScanProgress::stage_index("faces"), Some(3));
        assert_eq!(
            ScanProgress::stage_index("identify"),
            ScanProgress::stage_index("identify:ocr"),
            "an old snapshot's single identify stage still finds its place"
        );
        assert_eq!(ScanProgress::stage_index("nope"), None);
        assert_eq!(
            STAGES.map(|(id, _)| id),
            [
                "read",
                "detect",
                "sharpness",
                "faces",
                "identify:ocr",
                "identify:decision",
                "identify:vision",
                "group",
                "pick"
            ],
            "OCR, then Decision, then Vision"
        );
        let recent = &value["recent"][0];
        assert_eq!(recent["file"], "IMG_4212.CR3");
        assert_eq!(recent["stars"], 4);
        assert_eq!(recent["boxes"][0]["cls"], "car");
        assert_eq!(recent["boxes"][0]["box"], json!([0.25, 0.25, 0.75, 0.75]));
        assert_eq!(recent["af_kind"], "point");
    }

    #[test]
    fn every_skippable_stage_names_its_plain_consequence() {
        for stage in [
            "faces",
            "identify:ocr",
            "identify:decision",
            "identify:vision",
            "group",
            "pick",
        ] {
            let text = skip_consequence(stage).unwrap_or_else(|| panic!("{stage} has no text"));
            assert!(skippable(stage));
            assert!(
                text.len() > 20 && text.contains("Skipping"),
                "{stage}: {text}"
            );
        }
        for stage in ["read", "detect", "sharpness"] {
            assert_eq!(
                skip_consequence(stage),
                None,
                "{stage} feeds every stage after it and cannot be skipped"
            );
            assert!(!skippable(stage));
        }
        assert!(known_stage("identify:vision").is_some());
        assert!(
            known_stage("identify").is_none(),
            "the old single id is a snapshot fallback, not a control target"
        );
        assert!(known_stage("nope").is_none());
    }

    #[test]
    fn a_live_frame_leaves_out_what_is_not_known_yet() {
        let fresh = serde_json::to_value(LiveFrame::new(7, "IMG_1.CR3")).unwrap();
        assert_eq!(
            fresh,
            json!({"image_id": 7, "file": "IMG_1.CR3", "step": "read"})
        );
        let mut rated = LiveFrame::new(7, "IMG_1.CR3");
        rated.af = Some([0.1, 0.2, 0.3, 0.4]);
        rated.af_kind = Some(AfKind::Grid);
        rated.stars = Some(3);
        rated.verdict = Some("soft".into());
        rated.panning = Some(false);
        let value = serde_json::to_value(&rated).unwrap();
        assert_eq!(value["af_kind"], "grid");
        assert_eq!(value["verdict"], "soft");
        assert_eq!(value["panning"], false);
        assert!(value.get("boxes").is_none(), "no detect step yet");
    }

    #[test]
    fn a_pixel_box_becomes_clamped_fractions_of_the_frame() {
        let b = LiveBox::from_pixels("person", 0.5, [-10.0, 0.0, 6100.0, 2000.0], 6000, 4000);
        assert_eq!(b.bbox, [0.0, 0.0, 1.0, 0.5]);
        let json = serde_json::to_value(&b).unwrap();
        assert!(json.get("box").is_some() && json.get("bbox").is_none());
    }
}
