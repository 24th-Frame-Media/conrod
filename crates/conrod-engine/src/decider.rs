//! The decision model as an engine service: install state, VRAM gating, a bounded slot
//! (one request on the GPU at a time), and a live feed for the flowchart's DECIDER node.
//!
//! Everything degrades silently: when the model is off, not installed, or the GPU is too small,
//! [`Service::decide`] returns an `Err` the caller treats as "no opinion", and nothing in a
//! scan waits for it. The moment-judge that asks the questions lives elsewhere; it calls
//! [`service`]`().decide(..)` as one more transport.

use crate::lock;
use conrod_core::models;
use conrod_core::settings::{data_root, Settings};
use conrod_core::tasks::TaskHub;
use conrod_decider::gating::{self, Quant, GIB};
use conrod_decider::vram::{self, GpuMemory};
use conrod_decider::{
    protocol, Answer, Decider, DeciderPaths, Device, LoadOptions, Question, Request, Response,
    DEFAULT_PIXELS,
};
use conrod_io::assets;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Requests on the GPU at once. One: the model fills the card, and a second request only
/// queues behind the first inside the runtime.
pub const CAPACITY: usize = 1;
/// Give the VRAM back to the detector and faces after this long without a request.
const IDLE_UNLOAD: Duration = Duration::from_secs(45);
/// Finished requests the node remembers.
const RECENT: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The user has it turned off.
    Off,
    NoGpu,
    NoVram,
    NotInstalled,
    Installing,
    Ready,
    Loading,
    Running,
    Error,
}

impl State {
    pub fn word(self) -> &'static str {
        match self {
            State::Off => "off",
            State::NoGpu => "no_gpu",
            State::NoVram => "no_vram",
            State::NotInstalled => "not_installed",
            State::Installing => "installing",
            State::Ready => "ready",
            State::Loading => "loading",
            State::Running => "running",
            State::Error => "error",
        }
    }
}

/// What is on disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Installed {
    pub runtime: bool,
    pub mmproj: bool,
    pub int8: bool,
    pub int4: bool,
}

impl Installed {
    pub fn probe() -> Installed {
        Installed {
            runtime: models::decider_runtime().is_some(),
            mmproj: models::find(models::DECIDER_MMPROJ).is_some(),
            int8: models::find(models::DECIDER_INT8).is_some(),
            int4: models::find(models::DECIDER_INT4).is_some(),
        }
    }
    fn has(&self, q: Quant) -> bool {
        self.runtime
            && self.mmproj
            && match q {
                Quant::Int8 => self.int8,
                Quant::Int4 => self.int4,
            }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub state: State,
    pub quant: Option<Quant>,
    pub message: String,
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / GIB as f64)
}

/// The state a machine is in, from facts only (testable without a GPU or files).
pub fn plan(
    enabled: bool,
    forced: Option<Quant>,
    installed: Installed,
    gpu: Option<&GpuMemory>,
    installing: bool,
) -> Plan {
    let make = |state, quant, message: &str| Plan {
        state,
        quant,
        message: message.to_string(),
    };
    let Some(gpu) = gpu else {
        return make(
            State::NoGpu,
            None,
            "No graphics card was found for the decision model.",
        );
    };
    let quant = gating::choose(gpu.free, forced);
    let Some(quant) = quant else {
        let needs = forced.unwrap_or(Quant::Int4).vram_needed();
        return Plan {
            state: State::NoVram,
            quant: None,
            message: format!(
                "Not enough video memory: {} free, needs {}.",
                gb(gpu.free),
                gb(needs)
            ),
        };
    };
    if !enabled {
        return make(State::Off, Some(quant), "Turned off in Settings.");
    }
    if installing {
        return make(
            State::Installing,
            Some(quant),
            "Installing the decision model.",
        );
    }
    if !installed.has(quant) {
        return make(State::NotInstalled, Some(quant), "Not installed yet.");
    }
    make(State::Ready, Some(quant), "Ready.")
}

#[derive(Debug, Clone)]
struct Ask {
    id: String,
    kind: &'static str,
    text: String,
    options: usize,
}

#[derive(Debug, Clone)]
struct Said {
    id: String,
    kind: &'static str,
    text: String,
    label: String,
    p: f64,
    bars: Vec<(String, f64)>,
}

#[derive(Debug, Clone)]
struct Current {
    label: String,
    thumb: Option<String>,
    asks: Vec<Ask>,
    since: Instant,
    queued: bool,
}

#[derive(Debug, Clone)]
struct Finished {
    label: String,
    thumb: Option<String>,
    said: Vec<Said>,
    ms: u64,
    tokens: u32,
    ok: bool,
}

#[derive(Default)]
struct Live {
    done: u64,
    errors: u64,
    total_ms: u64,
    current: Option<Current>,
    recent: VecDeque<Finished>,
    last_error: Option<String>,
}

struct Loaded {
    decider: Arc<Decider>,
    quant: Quant,
    last_used: Instant,
}

struct Config {
    enabled: bool,
    forced: Option<Quant>,
}

/// A counting semaphore for the slot.
struct Slot {
    free: Mutex<usize>,
    cv: Condvar,
}

impl Slot {
    fn acquire(&self) {
        let mut free = lock(&self.free);
        while *free == 0 {
            free = self.cv.wait(free).unwrap_or_else(|e| e.into_inner());
        }
        *free -= 1;
    }
    fn release(&self) {
        *lock(&self.free) += 1;
        self.cv.notify_one();
    }
}

pub struct Service {
    cfg: Mutex<Config>,
    model: Mutex<Option<Loaded>>,
    loading: AtomicBool,
    live: Mutex<Live>,
    version: AtomicU64,
    sent: AtomicU64,
    installing: Mutex<Option<(u64, u64)>>,
    active: AtomicUsize,
    queued: AtomicUsize,
    slot: Slot,
}

impl Default for Service {
    fn default() -> Self {
        Service {
            cfg: Mutex::new(Config {
                enabled: false,
                forced: None,
            }),
            model: Mutex::new(None),
            loading: AtomicBool::new(false),
            live: Mutex::new(Live::default()),
            version: AtomicU64::new(1),
            sent: AtomicU64::new(0),
            installing: Mutex::new(None),
            active: AtomicUsize::new(0),
            queued: AtomicUsize::new(0),
            slot: Slot {
                free: Mutex::new(CAPACITY),
                cv: Condvar::new(),
            },
        }
    }
}

/// The process-wide decision-model service.
pub fn service() -> &'static Service {
    static SERVICE: OnceLock<Service> = OnceLock::new();
    SERVICE.get_or_init(Service::default)
}

fn forced_quant(word: &str) -> Option<Quant> {
    match word {
        "int8" => Some(Quant::Int8),
        "int4" => Some(Quant::Int4),
        _ => None,
    }
}

/// Whether the in-process model is wanted. A settings file with moment
/// services says so through its local card; one from before the list (none at
/// all) still follows the opt-in switch.
fn wants_decider(settings: &Settings) -> bool {
    let local: Vec<bool> = settings
        .moments_services
        .iter()
        .filter(|service| service.provider == "in_process")
        .map(|service| service.enabled)
        .collect();
    if local.is_empty() {
        settings.use_decider
    } else {
        local.into_iter().any(|enabled| enabled)
    }
}

fn summarize(question: &Question, answer: &Answer) -> (String, f64, Vec<(String, f64)>) {
    let top = || {
        let mut all = answer.bars();
        all.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        all.truncate(4);
        all
    };
    if let Some(noul) = answer.noul {
        return (
            if noul >= 0.5 { "yes" } else { "no" }.to_string(),
            noul.max(1.0 - noul),
            vec![("yes".into(), noul), ("no".into(), 1.0 - noul)],
        );
    }
    if let Some(choice) = &answer.choice {
        let bars = top();
        let p = bars.first().map_or(0.0, |b| b.1);
        return (choice.clone(), p, bars);
    }
    if let Some(score) = answer.score {
        let levels = match question {
            Question::Score { criteria, .. } => criteria.len().saturating_sub(1),
            _ => 0,
        };
        let bars = top();
        let p = bars.first().map_or(0.0, |b| b.1);
        return (format!("{score:.1} / {levels}"), p, bars);
    }
    ("no answer".into(), 0.0, Vec::new())
}

fn question_text(question: &Question) -> String {
    match question.instructions() {
        Some(v) if !matches!(v, Value::Null) && !protocol::describe(v).is_empty() => {
            protocol::describe(v)
        }
        _ => protocol::DEFAULT_INSTRUCTIONS.to_string(),
    }
}

impl Service {
    fn bump(&self) {
        self.version.fetch_add(1, Ordering::AcqRel);
    }

    /// Apply the settings (at start-up and after every save). Turning it off frees the GPU now.
    pub fn configure(&self, settings: &Settings) {
        let wanted = wants_decider(settings);
        {
            let mut cfg = lock(&self.cfg);
            cfg.enabled = wanted;
            cfg.forced = forced_quant(&settings.decider_quant);
        }
        if !wanted {
            self.unload();
        }
        self.bump();
    }

    pub fn enabled(&self) -> bool {
        lock(&self.cfg).enabled
    }

    /// Whether a request would reach the model: turned on, installed, and the card has the
    /// room (or the model is already loaded). The moment judge uses it to choose the
    /// in-process transport.
    pub fn available(&self) -> bool {
        self.enabled() && self.plan_now().state == State::Ready
    }

    /// Whether the model and the detector would crowd the card: the 8-bit model on a card of
    /// 8 GB or less. Then a model call waits for any running scan's GPU stages
    /// ([`crate::gpu`]) instead of loading beside them.
    fn crowds_the_card(&self) -> bool {
        let loaded = lock(&self.model).as_ref().map(|l| l.quant);
        let quant = loaded.or_else(|| self.plan_now().quant);
        quant == Some(Quant::Int8)
            && vram::query().is_some_and(|g| g.total <= crate::gpu::SMALL_CARD)
    }

    fn plan_now(&self) -> Plan {
        let (enabled, forced) = {
            let cfg = lock(&self.cfg);
            (cfg.enabled, cfg.forced)
        };
        let installing = lock(&self.installing).is_some();
        if let Some(loaded) = lock(&self.model).as_ref() {
            return Plan {
                state: State::Ready,
                quant: Some(loaded.quant),
                message: "Ready.".into(),
            };
        }
        let gpu = vram::query();
        plan(
            enabled,
            forced,
            Installed::probe(),
            gpu.as_ref(),
            installing,
        )
    }

    /// Settings card + Health row: everything the UI needs, in one object.
    pub fn status(&self) -> Value {
        let plan = self.plan_now();
        let installed = Installed::probe();
        let gpu = vram::query();
        let manifest = assets::manifest();
        let missing_bytes: u64 = manifest
            .iter()
            .filter(|a| {
                [
                    models::DECIDER_RUNTIME,
                    models::DECIDER_INT8,
                    models::DECIDER_MMPROJ,
                ]
                .contains(&a.id())
            })
            .filter(|a| !crate::setup::present(a))
            .map(|a| a.size)
            .sum();
        let state = self.display_state(&plan);
        let download = *lock(&self.installing);
        json!({
            "state": state.word(),
            "enabled": self.enabled(),
            "message": self.message(state, &plan),
            "quant": plan.quant.map(Quant::label),
            "forced": lock(&self.cfg).forced.map(Quant::label),
            "installed": installed.has(Quant::Int8) || installed.has(Quant::Int4),
            "needs_install": plan.quant.is_some_and(|q| !installed.has(q)),
            "download_bytes": missing_bytes,
            "gpu": gpu.as_ref().map(|g| json!({"name": g.name, "total": g.total, "free": g.free})),
            "needs": {"int8": Quant::Int8.vram_needed(), "int4": Quant::Int4.vram_needed()},
            "install": download.map(|(done, total)| json!({"done": done, "total": total})),
        })
    }

    /// The Health/Setup row (`desktop.rs models()`), shaped like the others plus `optional`.
    /// `downloaded` is disk truth (weights + projector + runtime on disk), independent
    /// of whether the model is switched on or the GPU can hold it; `ready` is serving.
    pub fn model_row(&self) -> Value {
        let plan = self.plan_now();
        let state = self.display_state(&plan);
        let installed = Installed::probe();
        let downloaded = installed.has(Quant::Int8) || installed.has(Quant::Int4);
        let ready = matches!(state, State::Ready | State::Loading | State::Running);
        let detail = if ready || downloaded {
            format!(
                "{} ({})",
                self.message(state, &plan),
                plan.quant.map_or("-", Quant::label)
            )
        } else {
            self.message(state, &plan)
        };
        json!({
            "id": "decider",
            "name": "Decision model",
            "file": models::DECIDER_INT8,
            "ready": ready,
            "downloaded": downloaded,
            "optional": true,
            "available": true,
            "detail": detail,
        })
    }

    fn display_state(&self, plan: &Plan) -> State {
        if self.active.load(Ordering::Acquire) > 0 {
            State::Running
        } else if self.loading.load(Ordering::Acquire) {
            State::Loading
        } else if lock(&self.live).last_error.is_some() && plan.state == State::Ready {
            State::Error
        } else {
            plan.state
        }
    }

    fn message(&self, state: State, plan: &Plan) -> String {
        match state {
            State::Error => lock(&self.live)
                .last_error
                .clone()
                .unwrap_or_else(|| "The decision model failed.".into()),
            State::Running => "Answering questions.".into(),
            State::Loading => "Loading the decision model onto the GPU.".into(),
            _ => plan.message.clone(),
        }
    }

    /// Drop the model: the VRAM goes back to the detector and faces.
    pub fn unload(&self) {
        if lock(&self.model).take().is_some() {
            self.bump();
        }
    }

    fn files(quant: Quant) -> Option<DeciderPaths> {
        let runtime = models::decider_runtime()?;
        let mmproj = models::find(models::DECIDER_MMPROJ)?;
        let model = models::find(match quant {
            Quant::Int8 => models::DECIDER_INT8,
            Quant::Int4 => models::DECIDER_INT4,
        })?;
        Some(DeciderPaths::new(runtime, model, mmproj))
    }

    fn load(&self) -> Result<Arc<Decider>, String> {
        let mut model = lock(&self.model);
        if let Some(l) = model.as_mut() {
            l.last_used = Instant::now();
            return Ok(l.decider.clone());
        }
        let plan = {
            let (enabled, forced) = {
                let cfg = lock(&self.cfg);
                (cfg.enabled, cfg.forced)
            };
            let gpu = vram::query();
            plan(enabled, forced, Installed::probe(), gpu.as_ref(), false)
        };
        let quant = match (plan.state, plan.quant) {
            (State::Ready, Some(q)) => q,
            _ => return Err(plan.message),
        };
        let paths = Self::files(quant).ok_or("The decision model files are missing.")?;
        self.loading.store(true, Ordering::Release);
        self.bump();
        let loaded = Decider::load(
            &paths,
            &LoadOptions {
                device: Device::Auto,
                max_pixels: DEFAULT_PIXELS,
                ..LoadOptions::default()
            },
        );
        self.loading.store(false, Ordering::Release);
        match loaded {
            Ok(d) => {
                let decider = Arc::new(d);
                lock(&self.live).last_error = None;
                *model = Some(Loaded {
                    decider: decider.clone(),
                    quant,
                    last_used: Instant::now(),
                });
                self.bump();
                Ok(decider)
            }
            Err(e) => {
                lock(&self.live).last_error = Some(e.to_string());
                self.bump();
                Err(e.to_string())
            }
        }
    }

    /// Ask the model. `label` and `thumb` are what the DECIDER node shows for this frame.
    /// `Err` means "no opinion" (off, not installed, no VRAM, failed): the caller carries on.
    pub fn decide(
        &self,
        label: &str,
        thumb: Option<&Path>,
        request: &Request,
    ) -> Result<Response, String> {
        if !self.enabled() {
            return Err("The decision model is turned off.".into());
        }
        let asks: Vec<Ask> = request
            .questions
            .iter()
            .map(|(id, q)| Ask {
                id: id.clone(),
                kind: q.kind(),
                text: question_text(q),
                options: protocol::options(q).0.len(),
            })
            .collect();
        let thumb = thumb.map(|p| p.to_string_lossy().into_owned());
        self.queued.fetch_add(1, Ordering::AcqRel);
        lock(&self.live).current = Some(Current {
            label: label.to_string(),
            thumb: thumb.clone(),
            asks,
            since: Instant::now(),
            queued: true,
        });
        self.bump();
        self.slot.acquire();
        // On a small card, wait here (still shown as queued) for the scan's GPU stages.
        let gate = self.crowds_the_card().then(crate::gpu::exclusive);
        self.queued.fetch_sub(1, Ordering::AcqRel);
        self.active.fetch_add(1, Ordering::AcqRel);
        if let Some(c) = lock(&self.live).current.as_mut() {
            c.queued = false;
            c.since = Instant::now();
        }
        self.bump();
        let started = Instant::now();
        let result = self
            .load()
            .and_then(|d| d.decide_timed(request).map_err(|e| e.to_string()));
        self.active.fetch_sub(1, Ordering::AcqRel);
        drop(gate);
        self.slot.release();
        let ms = started.elapsed().as_millis() as u64;
        {
            let mut live = lock(&self.live);
            let asks = live.current.take().map(|c| c.asks).unwrap_or_default();
            let finished = match &result {
                Ok((response, _)) => {
                    live.done += 1;
                    live.total_ms += ms;
                    let said = request
                        .questions
                        .iter()
                        .zip(asks.iter())
                        .filter_map(|((id, q), ask)| {
                            let answer = response.answers.get(id)?;
                            let (label, p, bars) = summarize(q, answer);
                            Some(Said {
                                id: id.clone(),
                                kind: ask.kind,
                                text: ask.text.clone(),
                                label,
                                p,
                                bars,
                            })
                        })
                        .collect();
                    Finished {
                        label: label.to_string(),
                        thumb,
                        said,
                        ms,
                        tokens: response.usage.map_or(0, |u| u.input_tokens),
                        ok: true,
                    }
                }
                Err(e) => {
                    live.errors += 1;
                    live.last_error = Some(e.clone());
                    Finished {
                        label: label.to_string(),
                        thumb,
                        said: Vec::new(),
                        ms,
                        tokens: 0,
                        ok: false,
                    }
                }
            };
            live.recent.push_front(finished);
            live.recent.truncate(RECENT);
        }
        self.bump();
        result.map(|(r, _)| r)
    }

    /// A new identification run starts. The live page (frames decided, their
    /// answers and timing) belongs to one run: start it clean so a re-run never
    /// shows the previous run's frames as work this run has already done.
    pub fn begin_run(&self) {
        *lock(&self.live) = Live::default();
        self.bump();
    }

    /// Called every tick: free the GPU after a quiet spell.
    pub fn tick(&self) {
        if self.active.load(Ordering::Acquire) > 0 || self.queued.load(Ordering::Acquire) > 0 {
            return;
        }
        let idle = lock(&self.model)
            .as_ref()
            .is_some_and(|l| l.last_used.elapsed() > IDLE_UNLOAD);
        if idle {
            self.unload();
        }
    }

    /// The `decider-progress` payload: `Some` when something changed (and every tick while busy).
    pub fn snapshot(&self) -> Option<Value> {
        self.tick();
        let version = self.version.load(Ordering::Acquire);
        let busy = self.active.load(Ordering::Acquire) + self.queued.load(Ordering::Acquire) > 0;
        if !busy && self.sent.swap(version, Ordering::AcqRel) == version {
            return None;
        }
        Some(self.live_json())
    }

    pub fn live_json(&self) -> Value {
        let plan = self.plan_now();
        let state = self.display_state(&plan);
        let install = (*lock(&self.installing)).map(|(d, t)| json!({"done": d, "total": t}));
        let live = lock(&self.live);
        let (device, quant, model_name) = match lock(&self.model).as_ref() {
            Some(l) => (
                l.decider.device.as_ref().map(|d| d.name.clone()),
                Some(l.quant.label()),
                Some("pplx-decider-flash-v1.2"),
            ),
            None => (
                None,
                plan.quant.map(Quant::label),
                Some("pplx-decider-flash-v1.2"),
            ),
        };
        let said = |s: &Said| {
            json!({
                "id": s.id, "kind": s.kind, "text": s.text, "label": s.label, "p": s.p,
                "bars": s.bars.iter().map(|(k, v)| json!({"label": k, "p": v})).collect::<Vec<_>>(),
            })
        };
        json!({
            "state": state.word(),
            "enabled": self.enabled(),
            "message": self.message(state, &plan),
            "model": model_name,
            "device": device,
            "quant": quant,
            "capacity": CAPACITY,
            "busy": self.active.load(Ordering::Acquire),
            "queued": self.queued.load(Ordering::Acquire),
            "done": live.done,
            "errors": live.errors,
            "avg_ms": (live.done > 0).then(|| live.total_ms / live.done),
            "current": live.current.as_ref().map(|c| json!({
                "label": c.label,
                "thumb": c.thumb,
                "queued": c.queued,
                "elapsed_ms": c.since.elapsed().as_millis() as u64,
                "asking": c.asks.iter().map(|a| json!({"id": a.id, "kind": a.kind, "text": a.text, "options": a.options})).collect::<Vec<_>>(),
            })),
            "recent": live.recent.iter().map(|f| json!({
                "label": f.label, "thumb": f.thumb, "ms": f.ms, "tokens": f.tokens, "ok": f.ok,
                "says": f.said.iter().map(&said).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
            "install": install,
        })
    }

    /// Download what is missing (resumable, hash-checked) and prepare the 4-bit file when the
    /// GPU calls for it. Progress is in `status().install`; the task hub shows each step.
    pub fn install(&self, hub: &TaskHub, stop: &AtomicBool) -> Result<(), String> {
        let (forced, enabled) = {
            let cfg = lock(&self.cfg);
            (cfg.forced, cfg.enabled)
        };
        let _ = enabled;
        let gpu = vram::query();
        let Some(gpu) = gpu else {
            let e = "No graphics card was found for the decision model.".to_string();
            crate::setup::progress_error("decider", &e);
            return Err(e);
        };
        let Some(quant) = gating::choose(gpu.free, forced) else {
            let e = plan(true, forced, Installed::default(), Some(&gpu), false).message;
            crate::setup::progress_error("decider", &e);
            return Err(e);
        };
        if lock(&self.installing).is_some() {
            let e = "The decision model is already being installed.".to_string();
            crate::setup::progress_error("decider", &e);
            return Err(e);
        }
        *lock(&self.installing) = Some((0, 1));
        self.bump();
        let result = self.install_steps(hub, stop, quant);
        *lock(&self.installing) = None;
        self.bump();
        result
    }

    fn install_steps(&self, hub: &TaskHub, stop: &AtomicBool, quant: Quant) -> Result<(), String> {
        let manifest = assets::manifest();
        let wanted = [
            models::DECIDER_MMPROJ,
            models::DECIDER_RUNTIME,
            models::DECIDER_INT8,
        ];
        let todo: Vec<&assets::Asset> = wanted
            .iter()
            .filter_map(|id| manifest.iter().find(|a| a.id() == *id))
            .filter(|a| !crate::setup::present(a))
            .collect();
        let total: u64 = todo.iter().map(|a| a.size).sum::<u64>().max(1);
        crate::setup::progress_begin("decider", total);
        let mut before = 0u64;
        for asset in todo {
            let task = hub.start(format!("Installing decision model: {}", asset.name), 100);
            let size = asset.size;
            let mut report = |done: u64, all: u64| {
                task.progress(done * 100 / all.max(1), 100);
                *lock(&self.installing) = Some((before + done.min(size), total));
                crate::setup::progress_note("decider", before + done.min(size), total);
            };
            match assets::install(asset, &asset.folder(), stop, &mut report) {
                Ok(_) => task.finish(),
                Err(e) => {
                    task.fail(e.clone());
                    crate::setup::progress_error("decider", &e);
                    return Err(e);
                }
            }
            before += size;
        }
        if quant == Quant::Int4 && models::find(models::DECIDER_INT4).is_none() {
            let (Some(runtime), Some(int8)) = (
                models::decider_runtime(),
                models::find(models::DECIDER_INT8),
            ) else {
                return Err("The decision model files are missing.".into());
            };
            let dir = data_root().join("models");
            // The 4-bit file is derived locally and needs about half the int8's
            // room; refuse plainly rather than filling the disk mid-quantize.
            let need = std::fs::metadata(&int8).map(|m| m.len() / 2).unwrap_or(0);
            if let Some(free) = assets::free_bytes(&dir) {
                if free < need.saturating_add(64 << 20) {
                    let e = format!(
                        "Not enough disk space to prepare the 4-bit decision model: it needs about {:.1} GB free, but only {:.1} GB is left. Free some space and try again.",
                        need as f64 / (1u64 << 30) as f64,
                        free as f64 / (1u64 << 30) as f64,
                    );
                    crate::setup::progress_error("decider", &e);
                    return Err(e);
                }
            }
            let part = dir.join(format!("{}.part", models::DECIDER_INT4));
            let task = hub.start("Preparing the 4-bit decision model", 0);
            let result = Decider::quantize_int4(&runtime, &int8, &part)
                .map_err(|e| e.to_string())
                .and_then(|()| {
                    std::fs::rename(&part, dir.join(models::DECIDER_INT4))
                        .map_err(|e| e.to_string())
                });
            match result {
                Ok(()) => task.finish(),
                Err(e) => {
                    let _ = std::fs::remove_file(&part);
                    task.fail(e.clone());
                    crate::setup::progress_error("decider", &e);
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}

/// The Settings button: install in the background; one at a time.
pub fn install_in_background(d: &Arc<crate::desktop::Desktop>) -> Result<Value, String> {
    let key = "Installing decision model".to_string();
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut ops = lock(&d.operations);
        if ops.contains_key(&key) {
            return Err("The decision model is already being installed".into());
        }
        ops.insert(key.clone(), flag.clone());
    }
    let desktop = d.clone();
    let operation = key.clone();
    std::thread::spawn(move || {
        match service().install(&desktop.hub, &flag) {
            Ok(()) => crate::setup::progress_done(),
            Err(e) => {
                if e != "stopped" {
                    desktop.hub.note(format!("Decision model: {e}"));
                } else {
                    crate::setup::progress_done();
                }
            }
        }
        lock(&desktop.operations).remove(&operation);
    });
    Ok(json!({"operation": key}))
}

/// Where the int8 model is expected (for messages).
pub fn model_path() -> PathBuf {
    models::expected(models::DECIDER_INT8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(free_gib: f64) -> GpuMemory {
        GpuMemory {
            name: "Test GPU".into(),
            total: (free_gib * GIB as f64) as u64 + GIB,
            free: (free_gib * GIB as f64) as u64,
        }
    }

    fn all() -> Installed {
        Installed {
            runtime: true,
            mmproj: true,
            int8: true,
            int4: true,
        }
    }

    #[test]
    fn it_is_unavailable_without_enough_video_memory_and_says_how_much() {
        let p = plan(true, None, all(), Some(&gpu(3.0)), false);
        assert_eq!(p.state, State::NoVram);
        assert!(p.message.contains("3.0 GB free"), "{}", p.message);
        assert!(p.message.contains("needs 4.5 GB"), "{}", p.message);
        // Even when turned off, a machine that cannot run it says so.
        assert_eq!(
            plan(false, None, all(), Some(&gpu(3.0)), false).state,
            State::NoVram
        );
        assert_eq!(plan(true, None, all(), None, false).state, State::NoGpu);
    }

    #[test]
    fn a_big_enough_card_picks_the_largest_quantization_it_can_hold() {
        assert_eq!(
            plan(true, None, all(), Some(&gpu(12.0)), false).quant,
            Some(Quant::Int8)
        );
        assert_eq!(
            plan(true, None, all(), Some(&gpu(5.0)), false).quant,
            Some(Quant::Int4)
        );
        // A forced quantization wins when it fits, and is refused when it does not.
        assert_eq!(
            plan(true, Some(Quant::Int4), all(), Some(&gpu(12.0)), false).quant,
            Some(Quant::Int4)
        );
        assert_eq!(
            plan(true, Some(Quant::Int8), all(), Some(&gpu(5.0)), false).state,
            State::NoVram
        );
    }

    #[test]
    fn the_user_can_turn_it_off_even_with_room() {
        let p = plan(false, None, all(), Some(&gpu(12.0)), false);
        assert_eq!(p.state, State::Off);
        assert_eq!(
            plan(true, None, all(), Some(&gpu(12.0)), false).state,
            State::Ready
        );
    }

    #[test]
    fn installing_and_missing_files_are_told_apart() {
        let none = Installed::default();
        assert_eq!(
            plan(true, None, none, Some(&gpu(8.0)), false).state,
            State::NotInstalled
        );
        assert_eq!(
            plan(true, None, none, Some(&gpu(8.0)), true).state,
            State::Installing
        );
        // int8 chosen but only the 4-bit file is present: not ready, install fixes it.
        let only4 = Installed {
            int8: false,
            ..all()
        };
        assert_eq!(
            plan(true, None, only4, Some(&gpu(8.0)), false).state,
            State::NotInstalled
        );
        assert_eq!(
            plan(true, Some(Quant::Int4), only4, Some(&gpu(8.0)), false).state,
            State::Ready
        );
    }

    #[test]
    fn the_row_says_downloaded_from_disk_even_when_switched_off() {
        // Moves CONROD_HOME: run in the serial pass, not the parallel suite
        // (the main gate pass skips this test by name).
        let _guard = crate::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("conrod-decider-row-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("models")).unwrap();
        let runtime = dir.join("tools").join(models::DECIDER_RUNTIME);
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::write(dir.join("models").join(models::DECIDER_INT8), b"x").unwrap();
        std::fs::write(dir.join("models").join(models::DECIDER_MMPROJ), b"x").unwrap();
        std::fs::write(runtime.join("llama.dll"), b"x").unwrap();
        let old = std::env::var_os("CONROD_HOME");
        std::env::set_var("CONROD_HOME", &dir);
        service().configure(&Settings::default()); // use_decider off
        let row = service().model_row();
        match old {
            Some(v) => std::env::set_var("CONROD_HOME", v),
            None => std::env::remove_var("CONROD_HOME"),
        }
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(row["id"], "decider");
        assert_eq!(row["downloaded"], true, "the files are on disk");
        assert_eq!(row["ready"], false, "switched off is not serving");
        // The detail names the state in plain words; the quant it names depends
        // on the machine's free VRAM, so only its presence is pinned here.
        assert!(!row["detail"].as_str().unwrap().is_empty());
    }

    #[test]
    fn a_disabled_service_never_asks_the_model_and_shows_no_activity() {
        let service = Service::default();
        let request = Request::new("x").ask("a", Question::noul("Is it?"));
        let err = service.decide("frame", None, &request).unwrap_err();
        assert!(err.contains("turned off"), "{err}");
        let live = service.live_json();
        assert_eq!(live["busy"], 0);
        assert_eq!(live["done"], 0);
        assert!(live["current"].is_null());
    }

    #[test]
    fn a_call_shows_on_the_node_even_when_the_model_cannot_answer() {
        // Turned on but not installed here (or no card): the call is refused, and the node
        // still records what was asked, under the frame's name, and pushes an update.
        let service = Service::default();
        service.configure(&Settings {
            use_decider: true,
            ..Settings::default()
        });
        let request = Request::new("x")
            .ask("m_kiss", Question::noul("Is the couple kissing?"))
            .ask("strength", Question::score("How strong?", &["low", "high"]));
        let before = service.snapshot();
        assert!(before.is_some(), "a settings change is worth one update");
        let _ = service.decide("IMG_0001.CR3", None, &request);
        let live = service.live_json();
        assert_eq!(live["recent"][0]["label"], "IMG_0001.CR3");
        assert!(live["current"].is_null(), "nothing in flight afterwards");
        assert!(service.snapshot().is_some(), "the finished call is pushed");
        assert_eq!(live["busy"], 0);
    }

    #[test]
    fn answers_are_summarised_for_the_node() {
        let q = Question::choice("Which?", [("a", None), ("b", None), ("c", None)]);
        let answer = Answer {
            choice: Some("b".into()),
            probabilities: Some(json!({"a": 0.1, "b": 0.7, "c": 0.2})),
            confidence: Some(0.5),
            ..Answer::default()
        };
        let (label, p, bars) = summarize(&q, &answer);
        assert_eq!(label, "b");
        assert!((p - 0.7).abs() < 1e-9);
        assert_eq!(bars[0].0, "b");
        let (label, p, _) = summarize(
            &Question::noul("?"),
            &Answer {
                noul: Some(0.2),
                ..Answer::default()
            },
        );
        assert_eq!(label, "no");
        assert!((p - 0.8).abs() < 1e-9);
    }

    #[test]
    fn the_slot_is_bounded() {
        let slot = Slot {
            free: Mutex::new(CAPACITY),
            cv: Condvar::new(),
        };
        slot.acquire();
        assert_eq!(*lock(&slot.free), 0);
        slot.release();
        assert_eq!(*lock(&slot.free), CAPACITY);
    }

    /// The whole path on a real GPU: `CONROD_DECIDER_RUNTIME/_MODEL/_MMPROJ` as in conrod-decider's
    /// parity test are not used here; this needs the models installed in the data folder and
    /// `use_decider` on.
    #[test]
    #[ignore = "needs the decision model installed and a GPU"]
    fn the_service_answers_and_reports_what_it_did() {
        let service = Service::default();
        let settings = Settings {
            use_decider: true,
            ..Settings::default()
        };
        service.configure(&settings);
        let image = image::RgbImage::from_pixel(256, 256, image::Rgb([200, 30, 30]));
        let file = std::env::temp_dir().join("conrod-decider-red.png");
        image.save(&file).unwrap();
        let mut request = Request::new("").ask("red", Question::noul("Is the picture mostly red?"));
        request.images.push(file.to_string_lossy().into_owned());
        let response = service.decide("test frame", None, &request).unwrap();
        assert!(response.noul("red").unwrap() > 0.5);
        let live = service.live_json();
        assert_eq!(live["done"], 1);
        assert_eq!(live["recent"][0]["says"][0]["label"], "yes");
        service.unload();
    }
}
