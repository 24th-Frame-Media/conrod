//! A live view of one identify run: which car each worker is reading, what the
//! model was asked and answered, the reading steps, and the fields that came
//! out of it.
//!
//! The identify workers call into this from their hot loop, so every method is
//! a short lock and a few moves: the snapshot is built by the app's status
//! thread four times a second and sent as one `identify-progress` event. The
//! screen keeps the last snapshot after the run ends, so the finished summary
//! stays readable; the next run replaces it.

use crate::lock;
use conrod_core::analysis::VehicleAnalysis;
use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// How many finished items one snapshot carries, newest first.
pub const RECENT_LIMIT: usize = 16;
/// How many in-flight cars one snapshot carries, one per identify worker.
pub const ACTIVE_LIMIT: usize = 8;
/// How many distinct run-wide problems one snapshot carries.
pub const ERRORS_LIMIT: usize = 32;

/// The per-item stage the screen shows: load RAW, crop/plate/OCR, ask the
/// vision model, merge with the entry list and save.
pub const READ: &str = "read";
pub const CROP: &str = "crop";
pub const VISION: &str = "vision";
pub const MERGE: &str = "merge";
pub const DONE: &str = "done";

fn epoch() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

/// One car, either still being read (in `live`) or finished (in `recent`).
#[derive(Debug, Clone)]
struct Item {
    detection_id: i64,
    image_id: i64,
    path: String,
    thumb_path: Option<String>,
    preview_path: Option<String>,
    crop_path: Option<String>,
    stage: &'static str,
    prompt: Option<String>,
    reply: Value,
    trace: Vec<String>,
    number: Option<String>,
    number_source: Option<String>,
    number_conf: f64,
    plate: Option<String>,
    make: Option<String>,
    model: Option<String>,
    team: Option<String>,
    colour: Option<String>,
    /// Visible action effects, as the model reported them.
    action: Vec<String>,
    failures: Vec<String>,
    ms: f64,
    at: f64,
    /// The identify worker reading this car, 0-based: the screen draws one
    /// panel per worker and needs to know which lane a car belongs to.
    worker: usize,
    /// When the worker started this car, for the failure path's `ms`.
    since: Instant,
}

impl Item {
    fn new(
        detection_id: i64,
        image_id: i64,
        path: &str,
        thumb_path: Option<&str>,
        preview_path: Option<&str>,
    ) -> Item {
        Item {
            detection_id,
            image_id,
            path: path.to_string(),
            thumb_path: thumb_path.map(str::to_owned),
            preview_path: preview_path.map(str::to_owned),
            crop_path: None,
            stage: READ,
            prompt: None,
            reply: Value::Null,
            trace: Vec::new(),
            number: None,
            number_source: None,
            number_conf: 0.0,
            plate: None,
            make: None,
            model: None,
            team: None,
            colour: None,
            action: Vec::new(),
            failures: Vec::new(),
            ms: 0.0,
            at: epoch(),
            worker: 0,
            since: Instant::now(),
        }
    }

    /// The item as the event's JSON. `with_prompt` is true only for the car
    /// being read: the prompt is most of an item's bytes and the finished list
    /// does not need sixteen copies of it.
    fn value(&self, with_prompt: bool) -> Value {
        let mut map = Map::new();
        map.insert("detection_id".into(), json!(self.detection_id));
        map.insert("image_id".into(), json!(self.image_id));
        map.insert("path".into(), json!(self.path));
        map.insert("thumb_path".into(), json!(self.thumb_path));
        map.insert("preview_path".into(), json!(self.preview_path));
        map.insert("crop_path".into(), json!(self.crop_path));
        map.insert("stage".into(), json!(self.stage));
        if with_prompt {
            map.insert("prompt".into(), json!(self.prompt));
        }
        map.insert("reply".into(), self.reply.clone());
        map.insert("trace".into(), json!(self.trace));
        map.insert("number".into(), json!(self.number));
        map.insert("number_source".into(), json!(self.number_source));
        // A car still being read has no measured confidence yet. Null shows as
        // "—" on the screen; 0.0 would look like a failed read.
        map.insert(
            "number_conf".into(),
            if self.stage == DONE {
                json!(self.number_conf)
            } else {
                Value::Null
            },
        );
        map.insert("plate".into(), json!(self.plate));
        map.insert("make".into(), json!(self.make));
        map.insert("model".into(), json!(self.model));
        map.insert("team".into(), json!(self.team));
        map.insert("colour".into(), json!(self.colour));
        map.insert("action".into(), json!(self.action));
        map.insert("failures".into(), json!(self.failures));
        map.insert("ms".into(), json!(self.ms));
        map.insert("at".into(), json!(self.at));
        map.insert("worker".into(), json!(self.worker));
        Value::Object(map)
    }
}

struct State {
    done: usize,
    /// The stage the run is in, for the header: the last phase a worker set.
    stage: &'static str,
    /// The car the screen follows: the car that has been in flight longest, so
    /// it stays on screen from LOAD to RESULT while the other workers run on
    /// behind it.
    current: Option<i64>,
    /// Every car a worker is inside right now (one per identify worker).
    live: Vec<Item>,
    /// Finished cars, newest first.
    recent: VecDeque<Item>,
    /// Run-wide problems, deduplicated: a model that is down must not put one
    /// line per crop in the feed.
    errors: Vec<String>,
    state: &'static str,
    /// The vision model this run will actually call, for the flow's VISION
    /// node. Empty when the shoot has no vision model.
    model: String,
    /// How many identify workers this run uses: the screen draws one panel each.
    workers: usize,
}

/// One identify run's live feed. `Desktop` keeps it in an `Arc` so the status
/// thread can snapshot it without touching the run itself.
pub struct IdentifyLive {
    job: i64,
    total: usize,
    started: Instant,
    started_at: f64,
    inner: Mutex<State>,
    finished: AtomicBool,
    final_sent: AtomicBool,
}

impl IdentifyLive {
    pub fn new(job: i64, total: usize) -> IdentifyLive {
        IdentifyLive {
            job,
            total,
            started: Instant::now(),
            started_at: epoch(),
            inner: Mutex::new(State {
                done: 0,
                stage: READ,
                current: None,
                live: Vec::new(),
                recent: VecDeque::new(),
                errors: Vec::new(),
                state: "running",
                model: String::new(),
                workers: 0,
            }),
            finished: AtomicBool::new(false),
            final_sent: AtomicBool::new(false),
        }
    }

    /// A worker has picked up one car. The screen shows it before the RAW is
    /// even open, so its image and path come from the pending query's join.
    /// `worker` is the 0-based lane: one panel per worker on the screen.
    #[allow(clippy::too_many_arguments)]
    pub fn start_item(
        &self,
        worker: usize,
        detection_id: i64,
        image_id: i64,
        path: &str,
        thumb_path: Option<&str>,
        preview_path: Option<&str>,
    ) {
        let mut item = Item::new(detection_id, image_id, path, thumb_path, preview_path);
        item.worker = worker;
        let mut state = lock(&self.inner);
        state.live.retain(|live| live.detection_id != detection_id);
        state.live.push(item);
        // Follow the car that started first, not the newest start: with three
        // workers a newest-start rule switched the stage every second, usually
        // to a car still at LOAD. This keeps one car on screen for its whole
        // read and moves on when it finishes.
        if state.current.is_none() {
            state.current = state.live.first().map(|item| item.detection_id);
        }
        state.stage = READ;
    }

    /// Move one car into a stage. The top-level `stage` follows the last
    /// update, which is the coarse stage the screen labels the run with.
    pub fn phase(&self, detection_id: i64, stage: &'static str) {
        let mut state = lock(&self.inner);
        state.stage = stage;
        if let Some(item) = state
            .live
            .iter_mut()
            .find(|item| item.detection_id == detection_id)
        {
            item.stage = stage;
        }
    }

    /// The question the vision model is about to be asked, while it thinks.
    pub fn set_prompt(&self, detection_id: i64, prompt: &str) {
        let mut state = lock(&self.inner);
        if let Some(item) = state
            .live
            .iter_mut()
            .find(|item| item.detection_id == detection_id)
        {
            item.prompt = Some(prompt.to_owned());
        }
    }

    /// One car is done: its final fields, the model's own reply, the reading
    /// steps and the per-crop failures. `crop_path` is the saved crop, once it
    /// has been written.
    #[allow(clippy::too_many_arguments)]
    pub fn finish_item(
        &self,
        detection_id: i64,
        analysis: &VehicleAnalysis,
        reply: Value,
        trace: Vec<String>,
        failures: Vec<String>,
        ms: f64,
        crop_path: Option<&Path>,
    ) {
        let mut state = lock(&self.inner);
        let mut item = take_live(&mut state, detection_id);
        item.stage = DONE;
        item.number = analysis.race_number.clone();
        item.number_source = analysis.number_source.clone();
        item.number_conf = analysis.number_conf;
        item.plate = analysis.plate.clone();
        item.make = analysis.make.clone();
        item.model = analysis.model.clone();
        item.team = analysis.team.clone();
        item.colour = analysis.colour.clone();
        item.action = analysis.action.clone();
        item.reply = reply;
        item.trace = trace;
        item.failures = failures;
        item.ms = round1(ms);
        item.crop_path = crop_path.map(|path| path.to_string_lossy().into_owned());
        item.at = epoch();
        push_recent(&mut state, item);
    }

    /// A car that never reached the readers (an empty box, a lost eligibility
    /// race): it still lands in `recent`, so the screen never stalls on it.
    pub fn finish_error(&self, detection_id: i64, message: impl Into<String>) {
        let mut state = lock(&self.inner);
        let mut item = take_live(&mut state, detection_id);
        item.stage = DONE;
        item.failures = vec![message.into()];
        item.ms = round1(item.since.elapsed().as_secs_f64() * 1000.0);
        item.at = epoch();
        push_recent(&mut state, item);
    }

    /// A run-wide problem, deduplicated. The list is capped so one wedged
    /// reader cannot grow the event without bound.
    pub fn error(&self, message: impl Into<String>) {
        let message = message.into();
        let mut state = lock(&self.inner);
        if state.errors.len() < ERRORS_LIMIT && !state.errors.contains(&message) {
            state.errors.push(message);
        }
    }

    /// One more car is accounted for (`total` includes skipped ones).
    pub fn advance(&self) {
        self.advance_by(1);
    }

    pub fn advance_by(&self, by: usize) {
        let mut state = lock(&self.inner);
        state.done = state.done.saturating_add(by).min(self.total);
    }

    pub fn set_state(&self, state: &'static str) {
        lock(&self.inner).state = state;
    }

    /// Name the vision model this run calls, so the flow shows the one that
    /// actually answers rather than the album's fallback name.
    pub fn set_model(&self, model: &str) {
        lock(&self.inner).model = model.to_string();
    }

    /// How many workers run at once, for the one-panel-per-worker view.
    pub fn set_workers(&self, workers: usize) {
        lock(&self.inner).workers = workers;
    }

    /// The run ended: `done`, `stopped` or `error`. The car in flight (if any)
    /// is dropped; the run has no more snapshots to follow.
    pub fn finish(&self, state: &'static str) {
        {
            let mut inner = lock(&self.inner);
            inner.state = state;
            inner.stage = DONE;
            inner.current = None;
            inner.live.clear();
        }
        self.finished.store(true, Ordering::Release);
    }

    pub fn finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    /// The snapshot for one event, or `None` once the final one has gone out.
    pub fn emit_snapshot(&self) -> Option<Value> {
        if self.finished() && self.final_sent.swap(true, Ordering::AcqRel) {
            return None;
        }
        Some(self.snapshot())
    }

    pub fn snapshot(&self) -> Value {
        let state = lock(&self.inner);
        let current = state
            .current
            .and_then(|id| state.live.iter().find(|item| item.detection_id == id))
            .map(|item| item.value(true));
        json!({
            "job_id": self.job,
            "state": state.state,
            "done": state.done,
            "total": self.total,
            "started_at": self.started_at,
            "elapsed_ms": round1(self.started.elapsed().as_secs_f64() * 1000.0),
            "stage": state.stage,
            "model": state.model,
            "workers": state.workers,
            "current": current,
            // Every car a worker is inside right now, including `current`:
            // identify runs several workers, and the screen shows them all.
            // The prompt rides along so each worker's panel can show its own.
            "active": state
                .live
                .iter()
                .rev()
                .take(ACTIVE_LIMIT)
                .map(|item| item.value(true))
                .collect::<Vec<_>>(),
            "recent": state.recent.iter().map(|item| item.value(false)).collect::<Vec<_>>(),
            "errors": state.errors,
        })
    }
}

/// Remove the in-flight item (or an empty stand-in if the worker ended after
/// the run cleared it) and let the screen follow another live car.
fn take_live(state: &mut State, detection_id: i64) -> Item {
    let index = state
        .live
        .iter()
        .position(|item| item.detection_id == detection_id);
    let item = match index {
        Some(index) => state.live.remove(index),
        None => Item::new(detection_id, 0, "", None, None),
    };
    if state.current == Some(detection_id) {
        state.current = state.live.first().map(|item| item.detection_id);
    }
    item
}

fn push_recent(state: &mut State, item: Item) {
    state.recent.push_front(item);
    state.recent.truncate(RECENT_LIMIT);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analysis(number: &str, make: &str) -> VehicleAnalysis {
        VehicleAnalysis {
            race_number: Some(number.into()),
            number_source: Some("vlm".into()),
            number_conf: 0.95,
            make: Some(make.into()),
            model: Some("GR Supra".into()),
            team: Some("Brad Jones Racing".into()),
            colour: Some("black and yellow".into()),
            plate: None,
            ..VehicleAnalysis::default()
        }
    }

    fn finish(live: &IdentifyLive, id: i64, number: &str) {
        live.phase(id, VISION);
        live.finish_item(
            id,
            &analysis(number, "Toyota"),
            json!({"number": number, "make": "Toyota"}),
            vec![format!("vlm 1 ask(s): number Some({number:?})")],
            Vec::new(),
            1234.0,
            None,
        );
        live.advance();
    }

    #[test]
    fn a_car_moves_from_read_through_vision_into_recent() {
        let live = IdentifyLive::new(108, 3);
        let empty = live.snapshot();
        assert_eq!(empty["state"], "running");
        assert_eq!(empty["done"], 0);
        assert_eq!(empty["total"], 3);
        assert_eq!(empty["stage"], "read");
        assert!(empty["current"].is_null());
        assert!(empty["recent"].as_array().unwrap().is_empty());

        live.start_item(0, 55, 42, "D:/shoot/A.CR3", Some("thumb.jpg"), Some("preview.jpg"));
        live.phase(55, VISION);
        live.set_prompt(55, "You are reading one competition car ...");
        let reading = live.snapshot();
        let current = &reading["current"];
        assert_eq!(reading["stage"], "vision");
        assert_eq!(current["detection_id"], 55);
        assert_eq!(current["image_id"], 42);
        assert_eq!(current["path"], "D:/shoot/A.CR3");
        assert_eq!(current["thumb_path"], "thumb.jpg");
        assert_eq!(current["preview_path"], "preview.jpg");
        assert_eq!(current["stage"], "vision");
        assert_eq!(current["prompt"], "You are reading one competition car ...");
        assert!(current["reply"].is_null());
        assert!(current["crop_path"].is_null());

        live.finish_item(
            55,
            &analysis("96", "Toyota"),
            json!({"number": "96", "make": "Toyota"}),
            vec!["entry list: team Some(\"Brad Jones Racing\")".into()],
            vec!["text read: timed out".into()],
            1234.0,
            Some(Path::new("cache/native/crop-55.jpg")),
        );
        live.advance();
        let finished = live.snapshot();
        assert_eq!(finished["done"], 1);
        assert!(finished["current"].is_null(), "the finished car is not current");
        let recent = finished["recent"].as_array().unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0]["detection_id"], 55);
        assert_eq!(recent[0]["stage"], "done");
        assert_eq!(recent[0]["number"], "96");
        assert_eq!(recent[0]["number_source"], "vlm");
        assert_eq!(recent[0]["number_conf"], 0.95);
        assert_eq!(recent[0]["make"], "Toyota");
        assert_eq!(recent[0]["model"], "GR Supra");
        assert_eq!(recent[0]["team"], "Brad Jones Racing");
        assert_eq!(recent[0]["colour"], "black and yellow");
        assert_eq!(recent[0]["reply"]["number"], "96");
        assert_eq!(recent[0]["failures"][0], "text read: timed out");
        assert_eq!(recent[0]["crop_path"], "cache/native/crop-55.jpg");
        assert_eq!(recent[0]["ms"], 1234.0);
        assert!(
            recent[0].get("prompt").is_none(),
            "the finished list carries no prompt, only the car being read does"
        );
    }

    #[test]
    fn with_parallel_workers_each_finish_lands_on_its_own_car() {
        let live = IdentifyLive::new(108, 2);
        live.start_item(0, 1, 10, "a.CR3", None, None);
        live.phase(1, CROP);
        live.start_item(1, 2, 11, "b.CR3", None, None);
        live.phase(2, VISION);
        assert_eq!(
            live.snapshot()["current"]["detection_id"],
            1,
            "the first-started car stays on screen while the others run"
        );
        // The first worker finishes while the second is still in the model.
        live.finish_item(1, &analysis("1", "Ford"), Value::Null, Vec::new(), Vec::new(), 10.0, None);
        live.advance();
        let snapshot = live.snapshot();
        assert_eq!(
            snapshot["current"]["detection_id"], 2,
            "when it finishes the screen follows the car still in flight"
        );
        assert_eq!(snapshot["recent"][0]["detection_id"], 1);
        assert_eq!(snapshot["recent"][0]["number"], "1");
        assert_eq!(snapshot["recent"][0]["make"], "Ford");
        live.finish_item(2, &analysis("2", "Toyota"), Value::Null, Vec::new(), Vec::new(), 20.0, None);
        live.advance();
        let snapshot = live.snapshot();
        assert!(snapshot["current"].is_null());
        assert_eq!(snapshot["done"], 2);
        assert_eq!(snapshot["recent"][0]["detection_id"], 2);
        assert_eq!(snapshot["recent"][1]["detection_id"], 1);
    }

    #[test]
    fn a_car_still_being_read_reports_no_confidence() {
        let live = IdentifyLive::new(1, 1);
        live.start_item(0, 5, 9, "a.CR3", None, None);
        live.phase(5, VISION);
        assert!(
            live.snapshot()["current"]["number_conf"].is_null(),
            "an unread car must not look like a 0% reading"
        );
        live.finish_item(5, &analysis("7", "Ford"), Value::Null, Vec::new(), Vec::new(), 10.0, None);
        live.advance();
        assert_eq!(live.snapshot()["recent"][0]["number_conf"], 0.95);
    }

    #[test]
    fn every_worker_car_is_in_the_active_list() {
        let live = IdentifyLive::new(1, 3);
        live.start_item(0, 1, 10, "a.CR3", None, None);
        live.start_item(1, 2, 11, "b.CR3", None, None);
        live.phase(2, VISION);
        let snapshot = live.snapshot();
        let active = snapshot["active"].as_array().unwrap();
        assert_eq!(active.len(), 2, "one entry per car a worker is inside");
        assert_eq!(active[0]["detection_id"], 2, "newest first");
        assert_eq!(active[0]["stage"], VISION);
        assert_eq!(active[0]["worker"], 1, "the second car belongs to lane 1");
        assert_eq!(active[1]["worker"], 0);
        assert!(
            active.iter().any(|item| item["detection_id"] == snapshot["current"]["detection_id"]),
            "the narrated car is one of the active ones"
        );
        live.finish_item(1, &analysis("1", "Ford"), Value::Null, Vec::new(), Vec::new(), 5.0, None);
        live.advance();
        let active = live.snapshot()["active"].as_array().unwrap().clone();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0]["detection_id"], 2);
    }

    #[test]
    fn recent_is_newest_first_and_capped_at_sixteen() {
        let live = IdentifyLive::new(7, 40);
        for id in 0..20 {
            live.start_item(0, id, id + 100, "frame.CR3", None, None);
            finish(&live, id, &id.to_string());
        }
        let snapshot = live.snapshot();
        let recent = snapshot["recent"].as_array().unwrap();
        assert_eq!(recent.len(), RECENT_LIMIT);
        assert_eq!(recent[0]["detection_id"], 19);
        assert_eq!(
            recent[RECENT_LIMIT - 1]["detection_id"],
            20 - RECENT_LIMIT as i64
        );
        assert_eq!(snapshot["done"], 20);
        assert_eq!(snapshot["total"], 40);
    }

    #[test]
    fn failures_and_errors_are_recorded_and_done_never_passes_total() {
        let live = IdentifyLive::new(9, 2);
        live.error("vehicle description: model is down");
        live.error("vehicle description: model is down");
        live.error("text read: timed out");
        live.start_item(0, 3, 4, "x.CR3", None, None);
        live.finish_error(3, "detection 3 has an empty box and was skipped");
        live.advance_by(2);
        live.advance();
        let snapshot = live.snapshot();
        assert_eq!(snapshot["done"], 2, "clamped to the total");
        assert_eq!(snapshot["errors"].as_array().unwrap().len(), 2, "deduplicated");
        assert_eq!(
            snapshot["recent"][0]["failures"][0],
            "detection 3 has an empty box and was skipped"
        );
    }

    #[test]
    fn the_last_snapshot_reports_how_the_run_ended_and_then_goes_quiet() {
        let live = IdentifyLive::new(108, 1);
        let going = live.emit_snapshot().unwrap();
        assert_eq!(going["state"], "running");
        assert_eq!(going["job_id"], 108);
        live.finish("done");
        let last = live.emit_snapshot().unwrap();
        assert_eq!(last["state"], "done");
        assert_eq!(last["stage"], "done");
        assert!(last["current"].is_null());
        assert!(last["elapsed_ms"].as_f64().unwrap() >= 0.0);
        assert!(live.emit_snapshot().is_none(), "then nothing more is sent");
        assert!(live.finished());
    }

    /// The event's key set, pinned: the frontend agent builds against it.
    #[test]
    fn the_snapshot_has_the_documented_shape() {
        let live = IdentifyLive::new(108, 2);
        let keys = |value: &Value| {
            value
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert_eq!(
            keys(&live.snapshot()),
            [
                "active",
                "current",
                "done",
                "elapsed_ms",
                "errors",
                "job_id",
                "model",
                "recent",
                "started_at",
                "stage",
                "state",
                "total",
                "workers",
            ]
            .map(String::from)
            .into_iter()
            .collect(),
            "top-level payload"
        );
        // One car finished, one still being read.
        live.start_item(0, 1, 10, "D:/a.CR3", Some("thumb-a.jpg"), Some("preview-a.jpg"));
        live.phase(1, MERGE);
        live.finish_item(
            1,
            &analysis("96", "Toyota"),
            json!({"number": "96"}),
            Vec::new(),
            Vec::new(),
            1234.0,
            Some(Path::new("cache/native/crop-1.jpg")),
        );
        live.advance();
        live.start_item(1, 2, 11, "D:/b.CR3", Some("thumb-b.jpg"), Some("preview-b.jpg"));
        live.phase(2, READ);
        live.set_prompt(2, "You are reading one competition car ...");
        live.set_workers(3);
        let snapshot = live.snapshot();
        assert_eq!(snapshot["workers"], 3);
        assert_eq!(
            snapshot["active"][0]["prompt"],
            "You are reading one competition car ...",
            "each worker's panel gets its own prompt"
        );
        let current = snapshot["current"].as_object().unwrap();
        assert_eq!(
            current
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "at", "action", "colour", "crop_path", "detection_id", "failures", "image_id", "make", "model",
                "ms", "number", "number_conf", "number_source", "path", "plate", "preview_path",
                "prompt", "reply", "stage", "team", "thumb_path", "trace", "worker",
            ]
            .map(String::from)
            .into_iter()
            .collect(),
            "the car being read"
        );
        let recent = snapshot["recent"][0].as_object().unwrap();
        assert_eq!(
            recent
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "at", "action", "colour", "crop_path", "detection_id", "failures", "image_id", "make", "model",
                "ms", "number", "number_conf", "number_source", "path", "plate", "preview_path",
                "reply", "stage", "team", "thumb_path", "trace", "worker",
            ]
            .map(String::from)
            .into_iter()
            .collect(),
            "a finished item, with no prompt"
        );
        println!("{}", serde_json::to_string_pretty(&snapshot).unwrap());
    }
}
