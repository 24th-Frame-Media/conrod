//! The slower identification and metadata lanes. Work is visible and cancellable.
use crate::analyze::{self, Context, Readers};
use crate::desktop::{rows, Desktop, Result};
use crate::err;
use crate::identify_live::{self, IdentifyLive};
use crate::lock;
use crate::setup;
use conrod_core::{
    analysis::VehicleAnalysis,
    keywords, models,
    profile::ShootPreset,
    progress::{IDENTIFY_DECISION, IDENTIFY_OCR, IDENTIFY_VISION},
    registry::{self, KnownVehicle},
    settings::{Settings, VisionOff},
};
use conrod_io::{settings::Settings as IoSettings, vlm, writer::MetadataWriter};
use conrod_vision::{
    colour,
    detect::{self, Device},
    imageops::Rgb,
    ocr, plates, sharpness, similarity,
};
use rusqlite::params;
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

// All identification and embedding entry points share this rule. A manual
// rating can rescue an automatic cull, but never a deliberate reject.
pub(crate) const ML_ELIGIBLE: &str = "COALESCE(i.rejected,0)=0 AND COALESCE(d.rejected,0)=0 AND COALESCE(d.bystander,0)=0 AND (COALESCE(d.cull_reason,'')='' OR d.stars IS NOT NULL OR i.stars IS NOT NULL)";

pub(crate) fn ml_eligible(d: &Desktop, id: i64) -> Result<bool> {
    lock(&d.reader).query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM detections d JOIN images i ON i.id=d.image_id WHERE d.id=? AND {ML_ELIGIBLE})"),
        [id], |r| r.get(0),
    ).map_err(err)
}

/// The eligibility rule the identify lane's pending query appends, by scope.
/// "keepers" is the *effective* keeper the UI shows (`keepersOf`): a frame the
/// photographer picked by hand (`images.picked`) identifies its own subjects
/// and suppresses the engine's suggestion for its burst; with no hand pick in
/// the burst, the suggestion (`burst_pick` on the detection or its frame) is
/// the keeper. A hand pick on a frame with no burst is still a keeper, and a
/// hand star alone is not. "everything" is every unreviewed, non-bystander
/// subject, culled or not, so a cull measured by an earlier scan cannot hide
/// one. The base filters (`job_id`, `region_type`, `reviewed`, `copy_of`) are
/// in the query itself.
pub(crate) const KEEPERS_ELIGIBLE: &str = "COALESCE(d.bystander,0)=0 AND (COALESCE(i.picked,0)=1 \
     OR (NOT EXISTS (SELECT 1 FROM images p WHERE p.job_id=i.job_id \
     AND p.burst_key=i.burst_key AND COALESCE(p.copy_of,0)=0 AND COALESCE(p.picked,0)=1) \
     AND (COALESCE(d.burst_pick,0)=1 OR COALESCE(i.burst_pick,0)=1)))";
pub(crate) const EVERYTHING_ELIGIBLE: &str = "COALESCE(d.bystander,0)=0";

pub(crate) fn identify_eligibility(scope: &str) -> &'static str {
    if conrod_core::settings::identify_scope_word(scope)
        == conrod_core::settings::IDENTIFY_EVERYTHING
    {
        EVERYTHING_ELIGIBLE
    } else {
        KEEPERS_ELIGIBLE
    }
}

/// The rows an identify run reads, in frame order: the scope's subjects, never
/// one the photographer settled by hand (`reviewed`), and -- for an incremental
/// run -- never one already read (`read_at`).
///
/// Both halves are needed. A hand edit writes `read_at` and `reviewed=1`, so it
/// is neither offered again nor counted as unread; a row a hand edit of an
/// older build touched may carry `reviewed=1` without `read_at`, and the
/// `reviewed` filter keeps it out all the same. `reviewed` is nullable on
/// libraries the legacy app wrote, hence the `COALESCE`.
pub(crate) fn pending_detections(region: &str, eligibility: &str, only_new: bool) -> String {
    let fresh = if only_new {
        " AND d.read_at IS NULL"
    } else {
        ""
    };
    format!(
        "SELECT d.*,i.path,i.thumb_path,i.preview_path,i.burst_key FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle')='{region}' AND COALESCE(d.reviewed,0)=0 AND COALESCE(i.copy_of,0)=0 AND {eligibility}{fresh} ORDER BY i.id,d.id"
    )
}

/// The scope one identify run uses: the command's own argument when it has
/// one, otherwise the album's stored setting (keepers by default).
pub(crate) fn run_scope(album_scope: &str, arg: Option<&str>) -> &'static str {
    conrod_core::settings::identify_scope_word(arg.unwrap_or(album_scope))
}
pub(crate) fn settings(d: &Desktop, job: i64) -> Result<Settings> {
    // The lanes the album (or its preset) leaves off are off for every reader.
    Ok(merged(d, job)?.0.resolved())
}

/// The album's settings laid over the current global ones, before the lane
/// switches are folded in, and the album's own vision choice (`None`: it
/// follows the global switch).
fn merged(d: &Desktop, job: i64) -> Result<(Settings, Option<bool>)> {
    crate::library::require_job(d, job)?; // "No such album", not a bare "Query returned no rows"
    let raw: Option<String> = lock(&d.db)
        .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
            r.get(0)
        })
        .map_err(err)?;
    let current = lock(&d.settings).clone();
    let mut choice = None;
    let mut s = raw
        .and_then(|s| serde_json::from_str::<Map<String, Value>>(&s).ok())
        .map(|mut map| {
            // An album stored before per-album gaps keeps grouping at the old
            // effective default, not at its stored 4.0 sentinel.
            conrod_core::settings::migrate_burst_gap(&mut map);
            // An album stored before vision had a per-album switch holds only a
            // stale copy of the global one: it follows the global.
            conrod_core::settings::follow_global_vlm(&mut map);
            // An album from before moment services holds the retired transport
            // fields; synthesise the list once so the rest of the read is the
            // same shape as a new album's.
            conrod_core::settings::migrate_moments_services(&mut map);
            choice = conrod_core::settings::album_vlm_choice(&map);
            current.clone().apply(&map)
        })
        .unwrap_or(current.clone());
    // Credentials and host availability are current; analysis choices belong to the job.
    s.vlm_api_key = current.vlm_api_key;
    s.vlm_host = current.vlm_host;
    s.vlm_extra_hosts = current.vlm_extra_hosts;
    s.vlm_services = current.vlm_services;
    s.vlm_strategy = current.vlm_strategy;
    // Moment services, their keys and their strategy are current too: an
    // album's copy is only ever stale. Its threshold stays its own.
    s.moments_services = current.moments_services;
    s.moments_strategy = current.moments_strategy;
    s.moments_vlm_fallback = current.moments_vlm_fallback;
    s.moments_services_scale = current.moments_services_scale;
    Ok((s, choice))
}

/// Why this album's identify run does not ask the vision model, or `None`
/// when it does.
pub(crate) fn vision_off(d: &Desktop, job: i64) -> Result<Option<VisionOff>> {
    let (settings, choice) = merged(d, job)?;
    Ok(settings.vision_off(choice))
}
pub(crate) fn launch(
    d: &Arc<Desktop>,
    job: i64,
    kind: &str,
    work: impl FnOnce(&Desktop, &AtomicBool, &conrod_core::tasks::Task) -> Result<()> + Send + 'static,
) -> Result<Value> {
    let key = format!("{kind}:{job}");
    let flag = Arc::new(AtomicBool::new(false));
    {
        let mut ops = lock(&d.operations);
        if ops.keys().any(|k| k.ends_with(&format!(":{job}"))) {
            return Err("This album already has a background operation".into());
        }
        ops.insert(key.clone(), flag.clone());
    }
    let task = d.hub.start(format!("{kind} · album {job}"), 0);
    let desktop = d.clone();
    let operation = key.clone();
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            work(&desktop, &flag, &task)
        }));
        if flag.load(Ordering::Relaxed) {
            // The user stopped it. Whatever the worker was in the middle of is
            // not an error, and the status area should not light up red.
            task.cancel();
        } else {
            match result {
                Ok(Ok(())) => task.finish(),
                Ok(Err(e)) => task.fail(e),
                Err(_) => task.fail("Worker stopped unexpectedly; this operation can be retried"),
            }
        }
        lock(&desktop.operations).remove(&operation);
    });
    Ok(json!({"operation":key}))
}
pub(crate) fn wait_for_cull(
    d: &Desktop,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
) -> bool {
    // Wait only for the cull lane (Stage A) — not for the post-scan wrapper
    // (`self.finishing`) that invokes `identify` itself.
    while d.cull_running() {
        if stop.load(Ordering::Relaxed) {
            return false;
        }
        task.detail("Waiting for the cull lane");
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    task.detail("");
    !stop.load(Ordering::Relaxed)
}
/// A model the identify run cannot go without, named where to put it if absent.
fn required(name: &str) -> Result<PathBuf> {
    models::find(name).ok_or_else(|| {
        format!(
            "Missing model {name}. Install it into {}",
            models::expected(name)
                .parent()
                .unwrap_or(Path::new(""))
                .display()
        )
    })
}

/// How many frames are identified at once. Each worker owns its plate models and
/// embedder; the OCR engine is shared. The sessions run on the CPU, so this is
/// about a fifth of the cores; the album's `analysis_workers` setting moves it
/// within Ollama's own parallelism.
fn identify_workers(settings: &Settings) -> usize {
    settings.identify_worker_count() as usize
}

/// The Vision stage's model line: the service that will actually answer.
fn vision_models(settings: &Settings) -> (String, &'static str) {
    match settings.enabled_vlm_service() {
        Some(service) if service.provider.eq_ignore_ascii_case("builtin") => {
            ("Qwen3-VL 8B (built-in)".into(), "GPU")
        }
        Some(service) if service.provider.eq_ignore_ascii_case("ollama") => (
            format!("{} via {}", service.model, service.provider),
            "ollama",
        ),
        Some(service) => (
            format!("{} via {}", service.model, service.provider),
            "cloud",
        ),
        None => ("no vision service".into(), "CPU"),
    }
}

/// The OCR stage's model line: no vision service is loaded here.
fn ocr_models(settings: &Settings) -> (String, &'static str) {
    let mut parts: Vec<String> = Vec::new();
    if settings.read_plates {
        parts.push(format!("plate {}", models::PLATE_DETECTOR));
    }
    if settings.read_numbers || settings.read_text {
        parts.push("PP-OCRv4".into());
    }
    (parts.join(" + "), "CPU")
}

/// The Decision stage's model line: whichever service will answer.
fn decision_models(settings: &Settings) -> (String, &'static str) {
    if settings.use_decider {
        (
            format!("pplx-decider-flash ({})", settings.decider_quant),
            "GPU",
        )
    } else if let Some(service) = settings
        .moments_services
        .iter()
        .find(|service| service.enabled)
    {
        (
            format!("{} via {}", service.model, service.provider),
            "cloud",
        )
    } else {
        ("no decision service".into(), "CPU")
    }
}

/// The settings and prompt guidance the vision stage uses. An accepted type
/// governs the vision reads still to come, whether the album started as `auto`
/// or with a hand-set type: the suggestion itself is never applied, only the
/// photographer's explicit accept. Without an accept, a typed album keeps its
/// own settings and an `auto` album keeps the neutral mix policy, because
/// nothing was applied silently.
pub(crate) fn resolved_vision_settings(
    d: &Desktop,
    job: i64,
    settings: &Settings,
) -> (Settings, Option<&'static str>) {
    let accepted: Option<String> = lock(&d.db)
        .query_row(
            "SELECT shoot_type_accepted FROM jobs WHERE id=?",
            [job],
            |row| row.get(0),
        )
        .ok()
        .flatten();
    let Some(preset) = accepted
        .as_deref()
        .and_then(|name| ShootPreset::ALL.iter().copied().find(|p| p.name() == name))
    else {
        return (settings.clone(), None);
    };
    let mut resolved = settings.clone();
    resolved.scan_profile = preset.name().into();
    (resolved.resolved(), Some(preset.prompt_context()))
}

/// OCR workers need the plate models and the shared OCR engine, never the
/// vision client: this stage asks no model.
fn load_ocr_readers(settings: &Settings, ocr: &Option<Arc<ocr::Ocr>>) -> Result<Readers> {
    let plates = if settings.read_plates {
        Some((
            plates::PlateDetector::load(&required(models::PLATE_DETECTOR)?, Device::Cpu)?,
            plates::PlateReader::load(&required(models::PLATE_READER)?, Device::Cpu)?,
        ))
    } else {
        None
    };
    Ok(Readers {
        plates,
        ocr: ocr.clone(),
        vlm: None,
    })
}

/// What every worker of one identify run shares.
struct Run<'a> {
    d: &'a Desktop,
    settings: &'a Settings,
    plates: &'a plates::PlateOptions,
    crop: &'a detect::DetectOptions,
    known: &'a HashMap<String, KnownVehicle>,
    task: &'a conrod_core::tasks::Task,
    done: &'a AtomicUsize,
    total: usize,
    reported: &'a Mutex<HashSet<String>>,
    progress: Option<&'a crate::progress::Progress>,
    /// The live feed the identify screen draws; shared by all workers.
    live: Option<&'a Arc<IdentifyLive>>,
    /// How often each number was read in each burst, for the tiebreak that
    /// settles two equally prominent panels. Keyed by `images.burst_key`.
    burst_numbers: Mutex<HashMap<i64, HashMap<String, u32>>>,
    /// The album's stage controls: pause parks between asks, never inside one.
    control: &'a crate::stage_control::StageControl,
    /// The run's stop flag, so a park is released by a Stop.
    stop: &'a AtomicBool,
}

impl Run<'_> {
    /// Park at an ask boundary: the lane says `paused`, never busy, until the
    /// photographer resumes or the run stops. Called between asks only, so a
    /// model call already in flight always finishes. False when stopped.
    fn park(&self, stage: &'static str, lane: usize, file: &str, step: &'static str) -> bool {
        // A skip pressed while the stage runs takes effect at this boundary:
        // the worker stops taking units, and the stage's finish path says
        // `skipped` instead of `done`.
        if self.control.is_skipped(stage) {
            return false;
        }
        if !self.control.is_paused(stage) {
            return !self.stop.load(Ordering::Relaxed);
        }
        self.task.paused(true);
        if let Some(p) = self.progress {
            p.pause_stage(stage);
            p.waiting(IDENTIFY_OCR, lane, stage, "paused", "paused", file);
        }
        let go = self.control.park(stage, self.stop);
        if let Some(p) = self.progress {
            p.resume_stage(stage);
            p.busy(IDENTIFY_OCR, lane, stage, step, file);
        }
        self.task.paused(false);
        go && !self.control.is_skipped(stage)
    }

    /// Say which file this worker is on.
    fn busy(&self, stage: &'static str, lane: usize, file: &str, step: &'static str) {
        if let Some(p) = self.progress {
            p.busy(IDENTIFY_OCR, lane, stage, step, file);
        }
    }

    /// Say a failure once: a vision model that is down would otherwise put one
    /// line per detection in the status log.
    fn report(&self, failure: String) {
        if let Some(live) = self.live {
            live.error(failure.clone());
        }
        if lock(self.reported).insert(failure.clone()) {
            self.d
                .hub
                .start(format!("Identify: {failure}"), 0)
                .fail(failure);
        }
    }

    /// One frame's detections for the OCR stage: read the plate, roundels and
    /// text once, store the partial reading, and hand the pass to the vision
    /// stage. The wavelet is not asked and read_at waits for the vision stage
    /// (or for this stage when vision is off).
    #[allow(clippy::too_many_arguments)]
    fn ocr_frame(
        &self,
        rows: &[Value],
        readers: &mut Readers,
        lane: usize,
        mark_read: bool,
        advance_task: bool,
        passes: &Mutex<HashMap<i64, analyze::OcrPass>>,
    ) -> Result<()> {
        let s = self.settings;
        let mut wanted = Vec::new();
        for row in rows {
            if ml_eligible(self.d, row["id"].as_i64().ok_or("Invalid detection")?)? {
                wanted.push(row);
            }
        }
        for _ in 0..(rows.len() - wanted.len()) {
            self.stage_advance(IDENTIFY_OCR, 1);
            if advance_task {
                self.task_advance(1);
            }
        }
        if wanted.is_empty() {
            return Ok(());
        }
        let path = wanted[0]["path"].as_str().ok_or("Missing image path")?;
        let filename = Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.busy(IDENTIFY_OCR, lane, &filename, "ocr");
        let raw = conrod_io::raw::read(Path::new(path))?;
        let image = Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation);
        for row in wanted {
            // Between asks: a paused OCR stage parks here, never inside a read.
            if !self.park(IDENTIFY_OCR, lane, &filename, "ocr") {
                return Ok(());
            }
            let id = row["id"].as_i64().ok_or("Invalid detection")?;
            if !ml_eligible(self.d, id)? {
                self.stage_advance(IDENTIFY_OCR, 1);
                if advance_task {
                    self.task_advance(1);
                }
                continue;
            }
            if let Some(live) = self.live {
                live.phase(id, identify_live::CROP);
            }
            let subject = ["x1", "y1", "x2", "y2"].map(|k| row[k].as_f64().unwrap_or(0.0).max(0.0));
            let bbox = subject.map(|v| v as usize);
            let tight = image.crop(
                bbox[0].min(image.width),
                bbox[1].min(image.height),
                bbox[2].min(image.width),
                bbox[3].min(image.height),
            );
            if tight.width == 0 || tight.height == 0 {
                let message = format!("detection {id} has an empty box and was skipped");
                self.report(message.clone());
                if let Some(live) = self.live {
                    live.finish_error(id, message);
                }
                self.stage_advance(IDENTIFY_OCR, 1);
                if advance_task {
                    self.task_advance(1);
                }
                continue;
            }
            let crop = crate::cut(
                &image,
                detect::expand_box(subject, image.width, image.height, self.crop),
                s,
            );
            let native = (s.plate_native_search && s.read_plates).then_some(&tight);
            let kind = row["cls"].as_str().unwrap_or("car");
            let seen_numbers = lock(&self.burst_numbers)
                .get(&row["burst_key"].as_i64().unwrap_or_default())
                .cloned()
                .unwrap_or_default();
            let ctx = Context {
                settings: s,
                plates: self.plates,
                known: self.known,
                seen_numbers: &seen_numbers,
                prompt_context: None,
            };
            let pass = analyze::read_ocr(&crop, native, kind, kind == "motorcycle", &ctx, readers)?;
            for failure in &pass.failures {
                self.report(failure.clone());
            }
            // Keep every model field an earlier run stored: this stage only
            // owns the OCR half. `reviewed=0` is the hard guard.
            let stored: Option<String> = lock(&self.d.reader)
                .query_row(
                    "SELECT attributes FROM detections WHERE id=? AND COALESCE(reviewed,0)=0",
                    [id],
                    |r| r.get(0),
                )
                .ok();
            let mut merged = VehicleAnalysis::from_json(stored.as_deref());
            merged.kind = pass.analysis.kind.clone();
            merged.is_bike = pass.analysis.is_bike;
            merged.plate = pass.analysis.plate.clone();
            merged.plate_state = pass.analysis.plate_state.clone();
            merged.plate_conf = pass.analysis.plate_conf;
            merged.plate_bbox = pass.analysis.plate_bbox;
            merged.race_number = pass.analysis.race_number.clone();
            merged.number_source = pass.analysis.number_source.clone();
            merged.number_conf = pass.analysis.number_conf;
            let read_now = if mark_read {
                "unixepoch('now')"
            } else {
                "NULL"
            };
            lock(&self.d.db)
                .execute(
                    &format!(
                        "UPDATE detections SET attributes=?,plate=?,plate_state=?,plate_conf=?,number=?,number_source=?,number_conf=?,read_at=COALESCE(read_at,{read_now}) WHERE id=? AND COALESCE(reviewed,0)=0"
                    ),
                    params![
                        serde_json::to_string(&merged).map_err(err)?,
                        merged.plate,
                        merged.plate_state,
                        merged.plate_conf,
                        merged.race_number,
                        merged.number_source,
                        merged.number_conf,
                        id
                    ],
                )
                .map_err(err)?;
            lock(passes).insert(id, pass);
            self.stage_advance(IDENTIFY_OCR, 1);
            if advance_task {
                self.task_advance(1);
            }
        }
        Ok(())
    }

    /// One frame's detections for the vision stage: merge the stored OCR pass
    /// with the model's read, resolve the number at the end of the stage, and
    /// write the final reading. The live re-read guard is the `reviewed=0`
    /// condition on the one write; a hand-set reading is never overwritten.
    #[allow(clippy::too_many_arguments)]
    fn vision_frame(
        &self,
        rows: &[Value],
        vision_settings: &Settings,
        prompt_context: Option<&str>,
        readers: &mut Readers,
        embedder: &mut Option<similarity::Embedder>,
        lane: usize,
        passes: &Mutex<HashMap<i64, analyze::OcrPass>>,
    ) -> Result<()> {
        let s = vision_settings;
        let mut wanted = Vec::new();
        for row in rows {
            if ml_eligible(self.d, row["id"].as_i64().ok_or("Invalid detection")?)? {
                wanted.push(row);
            }
        }
        for _ in 0..(rows.len() - wanted.len()) {
            self.stage_advance(IDENTIFY_VISION, 1);
            self.task_advance(1);
        }
        if wanted.is_empty() {
            return Ok(());
        }
        let path = wanted[0]["path"].as_str().ok_or("Missing image path")?;
        let filename = Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.busy(IDENTIFY_VISION, lane, &filename, "vision");
        let raw = conrod_io::raw::read(Path::new(path))?;
        let image = Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation);
        for row in wanted {
            // Between asks: a paused vision stage parks here, so the model call
            // already in flight finishes and the worker then holds.
            if !self.park(IDENTIFY_VISION, lane, &filename, "vision") {
                return Ok(());
            }
            let id = row["id"].as_i64().ok_or("Invalid detection")?;
            let image_id = row["image_id"].as_i64().unwrap_or_default();
            if !ml_eligible(self.d, id)? {
                self.stage_advance(IDENTIFY_VISION, 1);
                self.task_advance(1);
                continue;
            }
            let pass = lock(passes).remove(&id).unwrap_or_else(|| {
                let stored: Option<String> = lock(&self.d.reader)
                    .query_row(
                        "SELECT attributes FROM detections WHERE id=? AND COALESCE(reviewed,0)=0",
                        [id],
                        |r| r.get(0),
                    )
                    .ok();
                analyze::OcrPass::from_stored(VehicleAnalysis::from_json(stored.as_deref()))
            });
            if let Some(live) = self.live {
                live.start_item(
                    lane,
                    id,
                    image_id,
                    path,
                    row["thumb_path"].as_str(),
                    row["preview_path"].as_str(),
                );
                live.phase(id, identify_live::VISION);
            }
            let item_start = Instant::now();
            let subject = ["x1", "y1", "x2", "y2"].map(|k| row[k].as_f64().unwrap_or(0.0).max(0.0));
            let bbox = subject.map(|v| v as usize);
            let crop = crate::cut(
                &image,
                detect::expand_box(subject, image.width, image.height, self.crop),
                s,
            );
            let kind = row["cls"].as_str().unwrap_or("car");
            let burst = row["burst_key"].as_i64().unwrap_or_default();
            let seen_numbers = lock(&self.burst_numbers)
                .get(&burst)
                .cloned()
                .unwrap_or_default();
            self.task.detail(format!("{filename} · {kind}"));
            self.busy(IDENTIFY_VISION, lane, &filename, "vision");
            let prompt_sink = self.live.as_ref().map(|live| {
                let live = Arc::clone(live);
                move |prompt: &str| live.set_prompt(id, prompt)
            });
            let ctx = Context {
                settings: s,
                plates: self.plates,
                known: self.known,
                seen_numbers: &seen_numbers,
                prompt_context,
            };
            let outcome = analyze::read_vision(
                &crop,
                kind,
                kind == "motorcycle",
                &ctx,
                readers,
                pass,
                prompt_sink.as_ref().map(|sink| sink as &dyn Fn(&str)),
            )?;
            if let Some(live) = self.live {
                live.phase(id, identify_live::MERGE);
                if let Some(prompt) = outcome.vlm_prompt.as_deref() {
                    live.set_prompt(id, prompt);
                }
            }
            for failure in &outcome.failures {
                self.report(failure.clone());
            }
            let mut analysis = outcome.analysis;
            if let Some(number) = analysis.race_number.clone() {
                lock(&self.burst_numbers)
                    .entry(burst)
                    .or_default()
                    .entry(number)
                    .and_modify(|count| *count += 1)
                    .or_insert(1);
            }
            if let Some(label) = analysis
                .race_number
                .clone()
                .or_else(|| analysis.plate.clone())
                .filter(|label| !label.is_empty())
            {
                if let Some(p) = self.progress {
                    p.label(image_id, label);
                }
            }
            // `plate_bbox` (when set) is in the native tight-crop's own pixels; `bbox` is that
            // crop's origin in the full frame, so offsetting gives full-frame pixels directly -
            // the same coordinate space the frontend already reads vehicle boxes in.
            if let Some([x1, y1, x2, y2]) = analysis.plate_bbox {
                let (ox, oy) = (bbox[0] as i64, bbox[1] as i64);
                analysis.plate_bbox = Some([ox + x1, oy + y1, ox + x2, oy + y2]);
            }
            let swatch = colour::dominant(&crop);
            let embedding = embedder
                .as_mut()
                .map(|e| e.embed(&crop).map(|v| similarity::pack(&v)))
                .transpose()?;
            let crop_path = self.d.root.join(format!("cache/native/crop-{id}.jpg"));
            crate::desktop::save_jpeg(&crop, &crop_path)?;
            lock(&self.d.db).execute("UPDATE detections SET attributes=?,plate=?,plate_state=?,plate_conf=?,number=?,number_source=?,number_conf=?,embedding=?,colour_hex=?,crop_path=?,read_at=unixepoch('now') WHERE id=? AND reviewed=0",params![serde_json::to_string(&analysis).map_err(err)?,analysis.plate,analysis.plate_state,analysis.plate_conf,analysis.race_number,analysis.number_source,analysis.number_conf,embedding,swatch,crop_path.to_string_lossy(),id]).map_err(err)?;
            if let Some(live) = self.live {
                live.finish_item(
                    id,
                    &analysis,
                    outcome.vlm_reply,
                    outcome.trace,
                    outcome.failures,
                    item_start.elapsed().as_secs_f64() * 1000.0,
                    Some(&crop_path),
                );
            }
            self.stage_advance(IDENTIFY_VISION, 1);
            self.task_advance(1);
        }
        Ok(())
    }

    /// One subject through the OCR stage's counter only.
    fn stage_advance(&self, stage: &'static str, by: usize) {
        if let Some(p) = self.progress {
            p.advance(stage, by as u64);
        }
    }

    /// One subject finished for the run's own bar and the live feed.
    fn task_advance(&self, by: usize) {
        let n = self.done.fetch_add(by, Ordering::Relaxed) + by;
        self.task.progress(n as u64, self.total as u64);
        if let Some(live) = self.live {
            live.advance_by(by);
        }
    }
}

/// Vision workers need only the vision client: the OCR stage already read the
/// plate, number and text, and the pass travels with the subject.
fn load_vision_readers(settings: &Settings) -> Readers {
    Readers {
        plates: None,
        ocr: None,
        vlm: settings.use_vlm.then(|| {
            (
                vlm::VlmClient::new(Arc::new(vlm::RealClock::default())),
                IoSettings::from_core(settings),
            )
        }),
    }
}

/// The album setting that says its last identification is out of date because
/// a reader setting changed. Identification itself stays done; the review
/// screen only offers to read the album again.
pub(crate) const IDENTIFY_STALE: &str = "native_identify_stale";

/// The settings whose change makes an old reading incomplete. A grouping or
/// culling setting is not here: only what Identify itself reads.
pub(crate) fn identify_setting_key(key: &str) -> bool {
    matches!(
        key,
        "read_numbers"
            | "read_plates"
            | "read_text"
            | "read_action"
            | "identify_make_model"
            | "identify_colour"
            | "identify_team"
            | "group_vehicles"
            | "include_cars"
            | "include_trucks"
            | "include_bikes"
            | "include_people"
            | "use_vlm"
            | "identify_ocr"
            | "identify_decision"
            | "scan_profile"
            | "identify_scope"
    )
}

/// Set or clear the album's "identification is stale" note, best effort.
fn note_identify_stale(d: &Desktop, job: i64, stale: bool) {
    let db = lock(&d.db);
    let Ok(stored) = db.query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
        r.get::<_, String>(0)
    }) else {
        return;
    };
    let mut map: Map<String, Value> = serde_json::from_str(&stored).unwrap_or_default();
    if stale {
        map.insert(IDENTIFY_STALE.into(), Value::Bool(true));
    } else if map.remove(IDENTIFY_STALE).is_none() {
        return;
    }
    if let Ok(saved) = serde_json::to_string(&map) {
        let _ = db.execute(
            "UPDATE jobs SET settings_json=? WHERE id=?",
            params![saved, job],
        );
    }
}

/// Accept every unread subject as it stands: the album is identified without
/// a model call, and Review opens. New subjects found later still count as
/// unread, so they can be offered as an incremental run.
pub fn skip_identify(d: &Desktop, job: i64) -> Result<Value> {
    settings(d, job)?;
    if lock(&d.operations)
        .keys()
        .any(|k| k.ends_with(&format!(":{job}")))
    {
        return Err("This album has a background operation running".into());
    }
    let skipped = lock(&d.db)
        .execute(
            "UPDATE detections SET read_at=unixepoch('now')
             WHERE read_at IS NULL
               AND COALESCE(region_type,'vehicle') IN ('vehicle','person')
               AND image_id IN (SELECT id FROM images WHERE job_id=? AND COALESCE(copy_of,0)=0)",
            [job],
        )
        .map_err(err)?;
    note_identify_stale(d, job, false);
    Ok(json!({"ok": true, "skipped": skipped}))
}

pub fn identify(d: &Arc<Desktop>, job: i64, scope: Option<&str>, only_new: bool) -> Result<Value> {
    identify_with_kind(d, job, scope, only_new, "Identifying")
}

/// [`identify`] under a chosen operation name. The scan and the Identify command
/// use "Identifying"; a manual stage rerun uses "Rerunning identify" so the busy
/// guard and the screen can tell a rerun from the first read.
pub(crate) fn identify_with_kind(
    d: &Arc<Desktop>,
    job: i64,
    scope: Option<&str>,
    only_new: bool,
    kind: &str,
) -> Result<Value> {
    identify_stage(d, job, scope, only_new, kind, None)
}

/// [`identify_with_kind`], optionally restricted to a single identify sub-stage
/// (`IDENTIFY_OCR` or `IDENTIFY_VISION`) for per-stage reruns. `None` runs all
/// enabled stages in order: OCR -> Decision -> Vision.
pub(crate) fn identify_stage(
    d: &Arc<Desktop>,
    job: i64,
    scope: Option<&str>,
    only_new: bool,
    kind: &str,
    only_stage: Option<&'static str>,
) -> Result<Value> {
    let settings = settings(d, job)?;
    let vision_off = vision_off(d, job)?;
    // The photographer's per-stage controls: a skipped stage does not run even
    // when the album has it on, and a paused one parks between asks.
    let control = d.stage_control(job);
    let lanes = settings.lanes();
    let finds_faces = lanes.names;
    // The card: whether the decision model and a local vision model can be
    // resident at once (plan §5). The fit check never blocks: a card that holds
    // one runs the stages serially by stage, each model released before the
    // next loads; the album's seats shape the workers within each stage.
    let card = crate::vram_fit::plan(&settings);
    let seats = crate::vram_fit::seats_for(&settings, card.both_local());
    let iosettings = IoSettings::from_core(&settings);
    // Serial card: the Decision stage needs the whole card, so release any
    // vision model an earlier run left resident before the decider loads.
    if card.mode == crate::vram_fit::Mode::Serial
        && card.both_local()
        && settings.identify_decision
        && lanes.moments_detect
        && only_stage.is_none()
        && !control.is_skipped(IDENTIFY_DECISION)
    {
        vlm::release_local_models(&iosettings);
    }
    if !lanes.identifies() && !finds_faces && !lanes.moments_detect {
        return Err("There are no identifiable subjects in this shoot preset".into());
    }
    // The command's scope overrides the album's setting for this run only;
    // the scan's own auto-identify passes none and follows the album.
    let scope = run_scope(&settings.identify_scope, scope);
    let eligibility = identify_eligibility(scope);
    // An incremental run leaves every subject that has already been read
    // alone; a full run reads them again (the photographer asked for it).
    launch(d, job, kind, move |d, stop, task| {
        // This run's decision record starts empty: the screen shows the frames
        // this run decides, never the previous run's answers as if it were
        // working through them again.
        crate::decider::service().begin_run();
        if !wait_for_cull(d, stop, task) {
            return Ok(());
        }
        if !lanes.identifies() {
            if only_stage.is_some() {
                return Ok(());
            }
            if lanes.names {
                let faces = crate::passes::embed_faces_missing(d, job, stop, task)?;
                task.detail(format!("Prepared {faces} faces for name suggestions"));
            }
            if let Some(p) = d.progress() {
                p.skip_stage(IDENTIFY_OCR);
                p.skip_stage(IDENTIFY_VISION);
                p.skip_stage(IDENTIFY_DECISION);
            }
            // No vehicles to read, but the shoot may still have moments to judge.
            if settings.identify_decision
                && lanes.moments_detect
                && !control.is_skipped(IDENTIFY_DECISION)
            {
                let progress = d.progress();
                if let Some(p) = &progress {
                    p.phase(IDENTIFY_DECISION);
                }
                let judged = crate::moments::run_decision(
                    d,
                    job,
                    stop,
                    task,
                    &settings,
                    &lanes,
                    scope,
                    seats.decision,
                    progress.as_deref(),
                )?;
                if let Some(p) = &progress {
                    match judged {
                        Some(_) if control.is_skipped(IDENTIFY_DECISION) => {
                            p.skip_stage(IDENTIFY_DECISION);
                        }
                        Some(_) if !stop.load(Ordering::Relaxed) => {
                            p.finish_stage(IDENTIFY_DECISION)
                        }
                        Some(_) => {}
                        None => p.skip_stage(IDENTIFY_DECISION),
                    }
                }
                if judged.unwrap_or(0) > 0
                    && !stop.load(Ordering::Relaxed)
                    && !control.is_skipped(IDENTIFY_DECISION)
                {
                    pick_keepers_controlled(d, job, stop, &control)?;
                }
            }
            return Ok(());
        }
        // Field and court sport read jerseys off the people; everything else
        // reads vehicles.
        let region = if lanes.reads_players() {
            "person"
        } else {
            "vehicle"
        };
        let pending = rows(
            &lock(&d.reader),
            &pending_detections(region, eligibility, only_new),
            [job],
        )?;
        let ocr_on = settings.identify_ocr
            && (settings.read_plates || settings.read_numbers || settings.read_text)
            && matches!(only_stage, None | Some(IDENTIFY_OCR))
            && !control.is_skipped(IDENTIFY_OCR);
        let decision_on = settings.identify_decision
            && lanes.moments_detect
            && only_stage.is_none()
            && !control.is_skipped(IDENTIFY_DECISION);
        let vision_on = vision_off.is_none()
            && matches!(only_stage, None | Some(IDENTIFY_VISION))
            && !control.is_skipped(IDENTIFY_VISION);
        // The live feed the identify screen draws, replaced by the next run and
        // kept after this one so its summary stays on screen. The guard ends an
        // early return (`?`) as an error rather than leaving the feed running.
        let live = Arc::new(IdentifyLive::new(job, pending.len()));
        // The model the VISION node should name: the enabled service that will
        // actually answer, not the album's unused fallback.
        // Off is never a model name: the screen is told why instead. The name
        // comes from the enabled service, never the old fallback fields.
        match vision_off {
            Some(reason) => live.set_vision_off(reason.word()),
            None => live.set_model(&settings.vision_model()),
        }
        *lock(&d.identify) = Some(live.clone());
        let mut end = LiveRun(live.clone());
        if pending.is_empty() {
            if lanes.names && only_stage.is_none() {
                crate::passes::embed_faces_missing(d, job, stop, task)?;
            }
            task.detail(if scope == conrod_core::settings::IDENTIFY_EVERYTHING {
                "No subjects need identification"
            } else {
                "No keepers need identification"
            });
            let progress = d.progress();
            if let Some(p) = &progress {
                p.skip_stage(IDENTIFY_OCR);
                p.skip_stage(IDENTIFY_VISION);
            }
            if decision_on && !stop.load(Ordering::Relaxed) {
                if let Some(p) = &progress {
                    p.phase(IDENTIFY_DECISION);
                }
                let judged = crate::moments::run_decision(
                    d,
                    job,
                    stop,
                    task,
                    &settings,
                    &lanes,
                    scope,
                    seats.decision,
                    progress.as_deref(),
                )?;
                if let Some(p) = &progress {
                    match judged {
                        Some(_) if control.is_skipped(IDENTIFY_DECISION) => {
                            p.skip_stage(IDENTIFY_DECISION);
                        }
                        Some(_) if !stop.load(Ordering::Relaxed) => {
                            p.finish_stage(IDENTIFY_DECISION)
                        }
                        Some(_) => {}
                        None => p.skip_stage(IDENTIFY_DECISION),
                    }
                }
                if judged.unwrap_or(0) > 0
                    && !stop.load(Ordering::Relaxed)
                    && !control.is_skipped(IDENTIFY_DECISION)
                {
                    pick_keepers_controlled(d, job, stop, &control)?;
                }
            } else if let Some(p) = &progress {
                p.skip_stage(IDENTIFY_DECISION);
            }
            if only_stage.is_none() && !stop.load(Ordering::Relaxed) {
                note_identify_stale(d, job, false);
            }
            end.finish(if stop.load(Ordering::Relaxed) {
                "stopped"
            } else {
                "done"
            });
            return Ok(());
        }
        let reads_text = settings.read_plates || settings.read_numbers || settings.read_text;
        let mut needs: Vec<&str> = Vec::new();
        if ocr_on && settings.read_plates {
            needs.extend(setup::IDENTIFY_PLATES);
        }
        if ocr_on && reads_text {
            needs.extend(setup::TEXT);
        }
        if vision_on && settings.group_vehicles {
            needs.extend(setup::SIMILARITY);
        }
        if vision_on && vlm::uses_builtin_vision(&iosettings) && models::vision_server().is_none() {
            needs.push(models::DECIDER_RUNTIME);
        }
        setup::ensure(&d.hub, stop, &needs)?;
        let ocr_engine = if ocr_on && reads_text {
            let dir = models::ocr_dir().ok_or_else(|| {
                format!(
                    "Missing OCR models ({} and its recogniser)",
                    models::OCR_DETECTOR
                )
            })?;
            Some(Arc::new(ocr::Ocr::load(&dir)?))
        } else {
            None
        };
        let known = if settings.use_known_vehicles {
            known_vehicles(d)?
        } else {
            HashMap::new()
        };
        let opts = plates::PlateOptions {
            plate_conf: settings.plate_conf as f32,
            plate_reader: settings.plate_reader,
            plate_reader_min_conf: settings.plate_reader_min_conf as f32,
            plate_native_search: settings.plate_native_search,
            plate_native_lower: settings.plate_native_lower,
            plate_tile_edge: settings.plate_tile_edge.max(32) as usize,
            plate_tile_overlap: settings.plate_tile_overlap,
            plate_pad_x: settings.plate_pad_x,
            plate_pad_y: settings.plate_pad_y,
            plate_ocr_edge: settings.plate_ocr_edge.max(32) as usize,
            plate_min_len: settings.plate_min_len.max(1) as usize,
            plate_max_len: settings.plate_max_len.max(1) as usize,
            max_plates_per_vehicle: settings.max_plates_per_vehicle.max(1) as usize,
            number_min_len: settings.number_min_len.max(1) as usize,
            number_max_len: settings.number_max_len.max(1) as usize,
        };
        let crop_opts = detect::DetectOptions {
            crop_padding: settings.crop_padding,
            dominant_subject_fraction: settings.dominant_subject_fraction,
            ..detect::DetectOptions::default()
        };
        // Rows come ordered by frame; a frame's detections are one unit of work.
        let mut frames: Vec<std::ops::Range<usize>> = Vec::new();
        for (i, row) in pending.iter().enumerate() {
            match frames.last_mut() {
                Some(f) if pending[f.start]["image_id"] == row["image_id"] => f.end = i + 1,
                _ => frames.push(i..i + 1),
            }
        }
        let (done, reported) = (AtomicUsize::new(0), Mutex::new(HashSet::new()));
        // The scan screen shows these lanes when the one-shot scan gets here.
        let progress = d.progress();
        if let Some(p) = &progress {
            let (ocr_model, ocr_device) = ocr_models(&settings);
            let (decision_model, decision_device) = decision_models(&settings);
            let (vision_model, vision_device) = vision_models(&settings);
            p.describe(IDENTIFY_OCR, ocr_model, ocr_device);
            p.describe(IDENTIFY_DECISION, decision_model, decision_device);
            p.describe(IDENTIFY_VISION, vision_model, vision_device);
        }
        let workers = identify_workers(&settings).min(frames.len().max(1)).max(1);
        // The Vision stage's own seats: the album's preference when both models
        // run locally, else today's identify count. OCR keeps `workers`.
        let vision_workers = seats.vision.min(frames.len().max(1)).max(1);
        live.set_workers(workers);
        let passes: Mutex<HashMap<i64, analyze::OcrPass>> = Mutex::new(HashMap::new());
        let run = Run {
            d,
            settings: &settings,
            plates: &opts,
            crop: &crop_opts,
            known: &known,
            task,
            done: &done,
            total: pending.len(),
            reported: &reported,
            progress: progress.as_deref(),
            live: Some(&live),
            burst_numbers: Mutex::new(HashMap::new()),
            control: &control,
            stop,
        };

        // --- Identify: OCR ---------------------------------------------------
        if ocr_on {
            if let Some(p) = &progress {
                p.phase(IDENTIFY_OCR);
                p.total(IDENTIFY_OCR, pending.len() as u64);
            }
            // Mark `read_at` in OCR only when Vision is off/skipped for the
            // album (so an OCR-only rerun on a Vision-enabled album leaves
            // unread subjects available to a subsequent incremental Vision
            // rerun). Advance the task bar whenever Vision won't run here.
            let mark_read = vision_off.is_some() || control.is_skipped(IDENTIFY_VISION);
            let advance_task = !vision_on;
            let (next, failure) = (AtomicUsize::new(0), Mutex::new(None));
            std::thread::scope(|s| {
                for lane in 0..workers {
                    let (settings, ocr_engine, stop, next, failure, frames, pending, run) = (
                        &settings,
                        &ocr_engine,
                        stop,
                        &next,
                        &failure,
                        &frames,
                        &pending,
                        &run,
                    );
                    let (progress, passes) = (&progress, &passes);
                    s.spawn(move || {
                        let mut readers = match load_ocr_readers(settings, ocr_engine) {
                            Ok(readers) => readers,
                            Err(e) => {
                                lock(failure).get_or_insert(e);
                                if let Some(p) = progress.as_ref() {
                                    p.worker_failed(IDENTIFY_OCR, lane);
                                }
                                return;
                            }
                        };
                        while !stop.load(Ordering::Relaxed)
                            && !run.control.is_skipped(IDENTIFY_OCR)
                            && lock(failure).is_none()
                        {
                            let Some(frame) = frames.get(next.fetch_add(1, Ordering::Relaxed))
                            else {
                                break;
                            };
                            if let Err(e) = run.ocr_frame(
                                &pending[frame.clone()],
                                &mut readers,
                                lane,
                                mark_read,
                                advance_task,
                                passes,
                            ) {
                                lock(failure).get_or_insert(e);
                                if let Some(p) = progress.as_ref() {
                                    p.worker_failed(IDENTIFY_OCR, lane);
                                }
                                return;
                            }
                        }
                        if let Some(p) = progress.as_ref() {
                            p.idle(IDENTIFY_OCR, lane);
                        }
                    });
                }
            });
            let failure = failure.into_inner().unwrap_or_else(|e| e.into_inner());
            if let Some(e) = failure {
                if let Some(p) = &progress {
                    p.fail_stage(IDENTIFY_OCR, &e);
                }
                end.finish("error");
                return Err(e);
            }
            if let Some(p) = &progress {
                if control.is_skipped(IDENTIFY_OCR) {
                    p.skip_stage(IDENTIFY_OCR);
                } else if !stop.load(Ordering::Relaxed) {
                    p.finish_stage(IDENTIFY_OCR);
                }
            }
        } else if let Some(p) = &progress {
            p.skip_stage(IDENTIFY_OCR);
        }

        if stop.load(Ordering::Relaxed) {
            end.finish("stopped");
            return Ok(());
        }

        // One order for every card (review decision D2): the Decision stage
        // finishes every eligible frame and burst, then the Vision stage reads
        // every subject. A serial card's handoff unloads the decision model and
        // releases the vision host between them, so the two never share the
        // card; a concurrent card goes straight on with both allowed resident.
        let stage_run = crate::vram_fit::try_run_in_order(
            card.mode,
            stop,
            || -> Result<()> {
                let decision_now = settings.identify_decision
                    && lanes.moments_detect
                    && only_stage.is_none()
                    && !control.is_skipped(IDENTIFY_DECISION);
                if decision_now {
                    if let Some(p) = &progress {
                        p.phase(IDENTIFY_DECISION);
                    }
                    let judged = crate::moments::run_decision(
                        d,
                        job,
                        stop,
                        task,
                        &settings,
                        &lanes,
                        scope,
                        seats.decision,
                        progress.as_deref(),
                    )
                    .inspect_err(|e| {
                        if let Some(p) = &progress {
                            p.fail_stage(IDENTIFY_DECISION, e);
                        }
                    })?;
                    if let Some(p) = &progress {
                        match judged {
                            Some(_) if control.is_skipped(IDENTIFY_DECISION) => {
                                p.skip_stage(IDENTIFY_DECISION);
                            }
                            Some(_) if !stop.load(Ordering::Relaxed) => {
                                p.finish_stage(IDENTIFY_DECISION)
                            }
                            Some(_) => {}
                            None => p.skip_stage(IDENTIFY_DECISION),
                        }
                    }
                } else if let Some(p) = &progress {
                    p.skip_stage(IDENTIFY_DECISION);
                }
                Ok(())
            },
            || {
                let vision_now = vision_off.is_none()
                    && matches!(only_stage, None | Some(IDENTIFY_VISION))
                    && !control.is_skipped(IDENTIFY_VISION);
                // Only a handoff into a stage that will run is worth the
                // unload: a skipped Vision stage leaves the decider for the
                // next album, and its idle timer still frees the card.
                if vision_now {
                    crate::decider::service().unload();
                    vlm::release_local_models(&iosettings);
                }
            },
            || -> Result<()> {
                // --- Identify: Vision -------------------------------------------------
                let vision_now = vision_off.is_none()
                    && matches!(only_stage, None | Some(IDENTIFY_VISION))
                    && !control.is_skipped(IDENTIFY_VISION);
                if vision_now {
                    // The vision stage reads with the freshly resolved type:
                    // an `auto` album whose suggestion was accepted during
                    // Decision uses that type's prompts right away; a typed
                    // album keeps its own. Nothing is applied silently.
                    let (vision_settings, prompt_context) =
                        resolved_vision_settings(d, job, &settings);
                    live.set_model(&vision_settings.vision_model());
                    // A serial card's handoff guard: while the vision stage holds the
                    // card, any in-process model call (the decider now, in-process
                    // vision when phase V lands) waits for this stage rather than
                    // loading beside it.
                    let _gpu = (card.mode == crate::vram_fit::Mode::Serial).then(crate::gpu::stage);
                    if let Some(p) = &progress {
                        let (vision_model, vision_device) = vision_models(&vision_settings);
                        p.describe(IDENTIFY_VISION, vision_model, vision_device);
                        p.phase(IDENTIFY_VISION);
                        p.total(IDENTIFY_VISION, pending.len() as u64);
                    }
                    live.set_workers(vision_workers);
                    let (next, failure) = (AtomicUsize::new(0), Mutex::new(None));
                    std::thread::scope(|s| {
                        for lane in 0..vision_workers {
                            let (vision_settings, stop, next, failure, frames, pending, run) = (
                                &vision_settings,
                                stop,
                                &next,
                                &failure,
                                &frames,
                                &pending,
                                &run,
                            );
                            let (progress, passes) = (&progress, &passes);
                            s.spawn(move || {
                                let mut readers = load_vision_readers(vision_settings);
                                let mut embedder = match vision_settings.group_vehicles.then(|| {
                                    similarity::Embedder::load(
                                        &conrod_core::models::expected(
                                            conrod_core::models::SIMILARITY,
                                        ),
                                        Device::Cpu,
                                    )
                                }) {
                                    Some(Err(e)) => {
                                        lock(failure).get_or_insert(e);
                                        if let Some(p) = progress.as_ref() {
                                            p.worker_failed(IDENTIFY_VISION, lane);
                                        }
                                        return;
                                    }
                                    Some(Ok(e)) => Some(e),
                                    None => None,
                                };
                                while !stop.load(Ordering::Relaxed)
                                    && !run.control.is_skipped(IDENTIFY_VISION)
                                    && lock(failure).is_none()
                                {
                                    let Some(frame) =
                                        frames.get(next.fetch_add(1, Ordering::Relaxed))
                                    else {
                                        break;
                                    };
                                    if let Err(e) = run.vision_frame(
                                        &pending[frame.clone()],
                                        vision_settings,
                                        prompt_context,
                                        &mut readers,
                                        &mut embedder,
                                        lane,
                                        passes,
                                    ) {
                                        lock(failure).get_or_insert(e);
                                        if let Some(p) = progress.as_ref() {
                                            p.worker_failed(IDENTIFY_VISION, lane);
                                        }
                                        return;
                                    }
                                }
                                if let Some(p) = progress.as_ref() {
                                    p.idle(IDENTIFY_VISION, lane);
                                }
                            });
                        }
                    });
                    let failure = failure.into_inner().unwrap_or_else(|e| e.into_inner());
                    if let Some(e) = failure {
                        if let Some(p) = &progress {
                            p.fail_stage(IDENTIFY_VISION, &e);
                        }
                        return Err(e);
                    }
                    if let Some(p) = &progress {
                        if control.is_skipped(IDENTIFY_VISION) {
                            p.skip_stage(IDENTIFY_VISION);
                        } else if !stop.load(Ordering::Relaxed) {
                            p.finish_stage(IDENTIFY_VISION);
                        }
                    }
                } else if let Some(p) = &progress {
                    p.skip_stage(IDENTIFY_VISION);
                }
                Ok(())
            },
        );
        if let Err(e) = stage_run {
            end.finish("error");
            return Err(e);
        }

        if !stop.load(Ordering::Relaxed) {
            if only_stage.is_none() {
                if lanes.names {
                    crate::passes::embed_faces_missing(d, job, stop, task)?;
                }
                // Grouping is one whole-album pass: its pause and skip act at its
                // edge, before any burst is re-keyed.
                if control.is_skipped("group") {
                    if let Some(p) = &progress {
                        p.skip_stage("group");
                    }
                } else {
                    if let Some(p) = &progress {
                        p.phase("group");
                    }
                    if control.park_with("group", stop, progress.as_deref()) {
                        task.detail("Grouping similar vehicles");
                        crate::passes::consolidate(d, job, task)?;
                        task.detail("Finding re-shot frames");
                        crate::passes::near_duplicates(d, job, stop)?;
                    }
                }
                // The cross-album registry is not grouping: it is filled even when
                // the photographer kept the seed units.
                crate::catalog::seed(d, Some(job))?;
                // Groups are now known, so the keeper of each pass can be chosen
                // properly: the one-shot scan ends on this stage.
                if let Some(p) = &progress {
                    p.phase("pick");
                }
                // Moments were joined above, while the cars were read, so the pick ranks on them.
                task.detail("Picking keepers");
                pick_keepers_controlled(d, job, stop, &control)?;
                note_identify_stale(d, job, false);
            }
            end.finish("done");
        } else {
            end.finish("stopped");
        }
        Ok(())
    })
}

/// Pick keepers unless the photographer skipped the Pick stage. A skip leaves
/// every hand pick and every reviewed frame exactly as it is: the stage only
/// ever wrote suggestions. A pause takes effect before the pass starts.
fn pick_keepers_controlled(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    control: &crate::stage_control::StageControl,
) -> Result<()> {
    let progress = d.progress().filter(|p| p.job() == job);
    if control.is_skipped("pick") {
        if let Some(p) = &progress {
            p.skip_stage("pick");
        }
        return Ok(());
    }
    if control.park_with("pick", stop, progress.as_deref()) {
        crate::passes::pick_keepers(d, job)?;
    }
    Ok(())
}

/// Ends a live identify feed exactly once. The explicit paths say `done`,
/// `stopped` or `error`; an early `?` or a panic drops this as an error, so a
/// feed can never be left "running" and emitted forever.
struct LiveRun(Arc<IdentifyLive>);

impl LiveRun {
    fn finish(&mut self, state: &'static str) {
        self.0.finish(state);
    }
}

impl Drop for LiveRun {
    fn drop(&mut self) {
        if !self.0.finished() {
            self.0.finish("error");
        }
    }
}

/// The plate registry as `registry::fill` wants it: keyed by normalised plate.
fn known_vehicles(d: &Desktop) -> Result<HashMap<String, KnownVehicle>> {
    let mut known = HashMap::new();
    let data = rows(
        &lock(&d.db),
        "SELECT * FROM known_vehicles ORDER BY plate",
        [],
    )?;
    for v in &data {
        let text = |k: &str| v[k].as_str().map(str::to_owned);
        let row = registry::Row {
            plate: text("plate").unwrap_or_default(),
            make: text("make"),
            model: text("model"),
            colour: text("colour"),
            body_type: text("body_type"),
            team: text("team"),
            sponsors: text("sponsors"),
            race_number: text("race_number"),
        };
        known.insert(
            registry::normalise(Some(&row.plate)),
            KnownVehicle::from_row(&row),
        );
    }
    for v in &data {
        if let Some(vehicle) = known
            .get(&registry::normalise(v["plate"].as_str()))
            .cloned()
        {
            for alias in v["aliases"].as_str().unwrap_or_default().split(',') {
                let key = registry::normalise(Some(alias));
                if !key.is_empty() {
                    known.entry(key).or_insert_with(|| vehicle.clone());
                }
            }
        }
    }
    Ok(known)
}

/// The shell turns this on: only the desktop app applies verdicts by itself,
/// so the CLI and the tests never write a file they were not asked to.
static AUTO_APPLY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn enable_auto_apply() {
    AUTO_APPLY.store(true, Ordering::Release);
}

pub(crate) fn auto_apply_enabled() -> bool {
    AUTO_APPLY.load(Ordering::Acquire)
}

fn exiftool() -> String {
    models::exiftool().map_or_else(|| "exiftool".into(), |p| p.to_string_lossy().into_owned())
}

fn metadata_verdict(
    rejected: bool,
    auto_rejected: bool,
    manual_stars: Option<i64>,
    rating: f64,
    settings: &Settings,
    suggested: bool,
) -> (i32, Option<&'static str>, bool) {
    let culled = rejected || (settings.respect_culling && manual_stars.is_none() && auto_rejected);
    if culled {
        return (-1, Some("Red"), true);
    }
    let stars = manual_stars.unwrap_or_else(|| i64::from(sharpness::stars_for(rating))) as i32;
    // A suggested keeper carries the photographer's chosen "this is my pick"
    // colour, which Lightroom reads on the next metadata read. A hand pick
    // keeps its own label, and "None" leaves the choice to the photographer.
    let label = if suggested {
        pick_label_colour(&settings.pick_label)
    } else {
        Some(sharpness::label_for(sharpness::rating_for(
            rating,
            settings.sharp_at,
            settings.blurred_below,
        )))
    };
    (stars, label, false)
}

/// The colour a suggested pick is labelled with; the option names are the
/// words Lightroom shows, and the XMP value is the same word.
pub(crate) fn pick_label_colour(name: &str) -> Option<&'static str> {
    match name.trim().to_ascii_lowercase().as_str() {
        "red" => Some("Red"),
        "yellow" => Some("Yellow"),
        "green" => Some("Green"),
        "blue" => Some("Blue"),
        "purple" => Some("Purple"),
        _ => None,
    }
}

/// The photographer's hand-set flags on a frame: the values a metadata write
/// must never replace. The write paths read these with the frame rows and read
/// them again just before they touch a file, because the first read can be
/// minutes old while exiftool works through a big album.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct HandState {
    stars: Option<i64>,
    picked: Option<i64>,
    rejected: Option<i64>,
}

impl HandState {
    fn of(row: &Value) -> Self {
        Self {
            stars: row["stars"].as_i64(),
            picked: row["picked"].as_i64(),
            rejected: row["rejected"].as_i64(),
        }
    }

    /// The frame's flags as they are now.
    fn live(d: &Desktop, image: i64) -> Result<Self> {
        rows(
            &lock(&d.reader),
            "SELECT stars,picked,rejected FROM images WHERE id=?",
            [image],
        )?
        .first()
        .map(Self::of)
        .ok_or_else(|| format!("Frame {image} is gone"))
    }
}

/// Whether a frame's snapshot row still matches the frame's flags now. `false`
/// means the photographer picked, starred or rejected it after the snapshot was
/// taken: writing the snapshot's verdict would overwrite that hand work, so the
/// frame is skipped and the next write picks up the values it now carries.
pub(crate) fn snapshot_stands(d: &Desktop, frame: &Value) -> Result<bool> {
    let image = frame["id"].as_i64().ok_or("Invalid image id")?;
    Ok(HandState::of(frame) == HandState::live(d, image)?)
}

/// Whether the engine's suggestion still stands for a frame: a hand pick made
/// on any frame of its burst after the snapshot outranks the suggestion, just
/// as it does when the pick was already there when the album finished.
fn suggestion_stands(d: &Desktop, job: i64, frame: &Value) -> Result<bool> {
    let Some(burst) = frame["burst_key"].as_i64() else {
        return Ok(true);
    };
    Ok(rows(
        &lock(&d.reader),
        "SELECT 1 FROM images WHERE job_id=? AND burst_key=? AND COALESCE(picked,0)=1
           AND COALESCE(rejected,0)=0 AND COALESCE(copy_of,0)=0 LIMIT 1",
        params![job, burst],
    )?
    .is_empty())
}

pub fn write(d: &Arc<Desktop>, job: i64, dry_run: bool, embed_in_raw: bool) -> Result<Value> {
    let mut settings = settings(d, job)?;
    if embed_in_raw {
        settings.write_sidecar_for_raw = false;
    }
    if d.status()["activeJob"].as_i64() == Some(job) {
        return Err("Finish or stop scanning this album before writing metadata".into());
    }
    launch(d, job, "Writing metadata", move |d, stop, task| {
        if !dry_run {
            setup::ensure(&d.hub, stop, setup::WRITE)?;
        }
        let images = rows(
            &lock(&d.db),
            "SELECT * FROM images WHERE job_id=? AND status='done' AND COALESCE(copy_of,0)=0 ORDER BY id",
            [job],
        )?;
        // A hand pick in a burst suppresses every suggestion in it, exactly as
        // the review grid and the Lightroom package do.
        let picked_bursts: HashSet<i64> = rows(
            &lock(&d.db),
            "SELECT DISTINCT burst_key FROM images
              WHERE job_id=? AND picked=1 AND COALESCE(copy_of,0)=0
                AND COALESCE(rejected,0)=0 AND burst_key IS NOT NULL",
            [job],
        )?
        .iter()
        .filter_map(|row| row["burst_key"].as_i64())
        .collect();
        let mut writer = MetadataWriter::new(exiftool());
        let io = IoSettings::from_core(&settings);
        let mapping = crate::catalog::mapping(&d.root, &settings)?;
        let options = keywords::KeywordOptions {
            prefix: settings.keyword_prefix.clone(),
            write_plate: settings.write_plate_keyword,
        };
        for (i, frame) in images.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let image = frame["id"].as_i64().ok_or("Invalid image id")?;
            let detections = rows(
                &lock(&d.db),
                "SELECT * FROM detections WHERE image_id=?",
                [image],
            )?;
            let rejected = frame["rejected"].as_i64() == Some(1);
            let hand_pick = frame["picked"].as_i64() == Some(1);
            let burst = frame["burst_key"].as_i64();
            let mut suggested = !rejected
                && !hand_pick
                && (frame["burst_pick"].as_i64() == Some(1)
                    || detections
                        .iter()
                        .any(|d| d["burst_pick"].as_i64() == Some(1)))
                && !burst.is_some_and(|key| picked_bursts.contains(&key));
            // A hand pick made in Review while this loop runs takes over its
            // burst, exactly as one that was there when the album finished.
            if suggested && !suggestion_stands(d, job, frame)? {
                suggested = false;
            }
            let auto_rejected = !detections.is_empty()
                && detections
                    .iter()
                    .all(|d| d["cull_reason"].as_str().is_some_and(|s| !s.is_empty()));
            let (rating, label, culled) = metadata_verdict(
                rejected,
                auto_rejected,
                frame["stars"].as_i64(),
                frame["rating"].as_f64().unwrap_or(0.0),
                &settings,
                suggested,
            );
            let analyses: Vec<_> = detections
                .iter()
                .filter(|v| {
                    !culled
                        && v["rejected"].as_i64() != Some(1)
                        && match v["region_type"].as_str().unwrap_or("vehicle") {
                            "vehicle" => true,
                            // A jersey read off a player.
                            "person" => v["number"].as_str().is_some(),
                            _ => false,
                        }
                })
                .map(|v| {
                    let mut a = VehicleAnalysis::from_json(v["attributes"].as_str());
                    a.plate = v["plate"].as_str().map(str::to_owned).or(a.plate);
                    a.race_number = v["number"].as_str().map(str::to_owned).or(a.race_number);
                    a
                })
                .collect();
            let mut keywords = keywords::for_frame(&analyses, &options, mapping.as_ref());
            if hand_pick || (settings.mark_burst_picks && suggested) {
                keywords.push(format!("{}Pick", settings.keyword_prefix));
            }
            let caption = keywords::caption_for(&analyses);
            let path = Path::new(frame["path"].as_str().ok_or("Missing image path")?);
            task.detail(path.display().to_string());
            // The frames were read once, before the loop; exiftool can spend
            // minutes on a big album. A hand pick, star or reject made in Review
            // in the meantime is the photographer's last word, so the frame is
            // left alone instead of written with the snapshot's verdict, which
            // the keep rules would then make permanent.
            if !snapshot_stands(d, frame)? {
                d.hub.note(format!(
                    "Left {} alone: it changed in Review during the write",
                    path.display()
                ));
                task.progress((i + 1) as u64, images.len() as u64);
                continue;
            }
            if dry_run {
                d.hub.note(format!(
                    "Dry run: {}: {}",
                    path.display(),
                    keywords.join(", ")
                ));
                task.progress((i + 1) as u64, images.len() as u64);
                continue;
            }
            let result = writer
                .write_keywords(
                    path,
                    &keywords,
                    &io,
                    settings.write_caption.then_some(caption.as_str()),
                    settings.write_rating.then_some(rating),
                    label.filter(|_| settings.write_label),
                )
                .map_err(err)?;
            if !result.ok {
                return Err(format!("{}: {}", path.display(), result.message));
            }
            lock(&d.db)
                .execute(
                    "UPDATE images SET written_at=unixepoch('now') WHERE id=?",
                    [image],
                )
                .map_err(err)?;
            task.progress((i + 1) as u64, images.len() as u64);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pipeline_names_the_enabled_service_not_the_legacy_model() {
        let service = |id: &str, provider: &str, model: &str, enabled: bool| {
            conrod_core::settings::VlmService {
                id: id.into(),
                provider: provider.into(),
                model: model.into(),
                enabled,
                ..conrod_core::settings::VlmService::default()
            }
        };
        let settings = Settings {
            read_plates: false,
            read_numbers: false,
            read_text: false,
            use_vlm: true,
            vlm_provider: "ollama".into(),
            vlm_model: "qwen2.5vl:7b".into(),
            vlm_services: vec![
                service("off", "ollama", "qwen2.5vl:7b", false),
                service("on", "opencode-go", "glm-5.3-flash", true),
            ],
            ..Settings::default()
        };
        let (model, _) = vision_models(&settings);
        assert!(model.contains("glm-5.3-flash via opencode-go"), "{model}");
        assert!(!model.contains("qwen2.5vl"), "{model}");

        let all_off = Settings {
            vlm_services: vec![service("off", "ollama", "qwen2.5vl:7b", false)],
            ..settings
        };
        let (model, device) = vision_models(&all_off);
        assert_eq!(model, "no vision service");
        assert_eq!(device, "CPU");
    }

    /// The identify lane's pending query, with only the scope rule swapped.
    fn pending(db: &rusqlite::Connection, job: i64, scope: &str) -> Vec<i64> {
        let mut stmt = db
            .prepare(&format!(
                "SELECT d.id FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle')='vehicle' AND d.reviewed=0 AND COALESCE(i.copy_of,0)=0 AND {} ORDER BY d.id",
                identify_eligibility(scope)
            ))
            .unwrap();
        let ids = stmt
            .query_map([job], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        ids
    }

    #[test]
    fn keepers_are_the_suggested_pick_and_everything_takes_the_rest() {
        use crate::testkit::Lib;
        let lib = Lib::new("identify-scope");
        // One frame each, so a frame's pick and a detection's pick never
        // collide and the query's order is just the order they were seeded.
        let suggested = lib.detection(lib.frame("1.jpg", None), "vehicle", 0.9);
        let frame_pick = lib.detection(lib.frame("2.jpg", None), "vehicle", 0.9);
        let picked = lib.detection(lib.frame("3.jpg", None), "vehicle", 0.9);
        let starred = lib.detection(lib.frame("4.jpg", None), "vehicle", 0.9);
        let plain = lib.detection(lib.frame("5.jpg", None), "vehicle", 0.9);
        let culled = lib.detection(lib.frame("6.jpg", None), "vehicle", 0.2);
        let reviewed = lib.detection(lib.frame("7.jpg", None), "vehicle", 0.9);
        let bystander = lib.detection(lib.frame("8.jpg", None), "vehicle", 0.9);
        // The engine's suggested pick: on the detection or on its frame.
        lib.sql("UPDATE detections SET burst_pick=1 WHERE id=?", [suggested]);
        lib.sql(
            "UPDATE images SET burst_pick=1 WHERE id=(SELECT image_id FROM detections WHERE id=?)",
            [frame_pick],
        );
        // Hand choices: a pick on a frame with no burst is a keeper of its own,
        // a hand star alone is not.
        lib.sql(
            "UPDATE images SET picked=1 WHERE id=(SELECT image_id FROM detections WHERE id=?)",
            [picked],
        );
        lib.sql("UPDATE detections SET stars=5 WHERE id=?", [starred]);
        lib.sql(
            "UPDATE detections SET cull_reason='too blurred' WHERE id=?",
            [culled],
        );
        lib.sql("UPDATE detections SET reviewed=1 WHERE id=?", [reviewed]);
        lib.sql("UPDATE detections SET bystander=1 WHERE id=?", [bystander]);

        let query = |scope: &str| pending(&lib.db(), lib.job, scope);
        assert_eq!(
            query("keepers"),
            vec![suggested, frame_pick, picked],
            "a keeper is the engine's pick, plus a hand pick that is in no burst"
        );
        assert_eq!(
            query(" EVERYTHING "),
            vec![suggested, frame_pick, picked, starred, plain, culled],
            "everything takes every unreviewed subject, culled too, but never a bystander"
        );
        assert_eq!(
            query("nonsense"),
            query("keepers"),
            "an unknown word is keepers"
        );
    }

    #[test]
    fn the_command_scope_overrides_the_albums_setting_for_one_run() {
        use crate::testkit::Lib;
        let lib = Lib::new("identify-override");
        let plain = lib.detection(lib.frame("a.jpg", None), "vehicle", 0.9);
        let culled = lib.detection(lib.frame("b.jpg", None), "vehicle", 0.2);
        let suggested = lib.detection(lib.frame("c.jpg", None), "vehicle", 0.9);
        lib.sql("UPDATE detections SET burst_pick=1 WHERE id=?", [suggested]);
        lib.sql(
            "UPDATE detections SET cull_reason='too blurred' WHERE id=?",
            [culled],
        );

        // A new album carries the default: keepers.
        let album = crate::operations::settings(lib.d(), lib.job).unwrap();
        assert_eq!(album.identify_scope(), "keepers");
        let query = |scope: &str| pending(&lib.db(), lib.job, scope);
        assert_eq!(
            query(run_scope(&album.identify_scope, None)),
            vec![suggested],
            "with no argument the album's setting applies"
        );
        assert_eq!(
            query(run_scope(&album.identify_scope, Some("everything"))),
            vec![plain, culled, suggested],
            "the command's scope overrides keepers for this run"
        );

        // The other way round: an "everything" album, and an argument of
        // keepers narrows one run.
        lib.run(
            "update_job_settings",
            serde_json::json!({"jobId": lib.job, "patch": {"identify_scope": "everything"}}),
        )
        .unwrap();
        let album = crate::operations::settings(lib.d(), lib.job).unwrap();
        assert_eq!(album.identify_scope(), "everything");
        assert_eq!(
            query(run_scope(&album.identify_scope, None)),
            vec![plain, culled, suggested]
        );
        assert_eq!(
            query(run_scope(&album.identify_scope, Some("keepers"))),
            vec![suggested]
        );
    }

    #[test]
    fn a_hand_pick_takes_over_the_suggestion_for_its_burst() {
        use crate::testkit::Lib;
        let lib = Lib::new("identify-keeper-override");
        // One burst: A carries the engine's suggestion, B is its burstmate.
        let a = lib.frame("a.jpg", Some(7));
        let b = lib.frame("b.jpg", Some(7));
        let suggested = lib.detection(a, "vehicle", 0.9);
        let manual = lib.detection(b, "vehicle", 0.9);
        lib.sql("UPDATE detections SET burst_pick=1 WHERE id=?", [suggested]);
        let query = |scope: &str| pending(&lib.db(), lib.job, scope);
        assert_eq!(
            query("keepers"),
            vec![suggested],
            "with no hand pick the engine's suggestion is the keeper"
        );
        lib.sql("UPDATE images SET picked=1 WHERE id=?", [b]);
        assert_eq!(
            query("keepers"),
            vec![manual],
            "a hand pick on any frame of the burst replaces the suggestion"
        );
        assert_eq!(
            query("everything"),
            vec![suggested, manual],
            "everything still takes both frames"
        );
    }

    #[test]
    fn an_incremental_run_reads_only_the_subjects_nobody_has_read() {
        use crate::testkit::Lib;
        let lib = Lib::new("identify-only-new");
        let read = lib.detection(lib.frame("a.jpg", None), "vehicle", 0.9);
        let fresh = lib.detection(lib.frame("b.jpg", None), "vehicle", 0.9);
        lib.sql(
            "UPDATE detections SET attributes='{\"make\":\"BMW\"}',read_at=1 WHERE id=?",
            [read],
        );
        let only_new = |lib: &Lib| -> Vec<i64> {
            let db = lib.db();
            let mut stmt = db
                .prepare(
                    "SELECT d.id FROM detections d JOIN images i ON i.id=d.image_id \
                     WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle')='vehicle' \
                     AND d.reviewed=0 AND d.read_at IS NULL ORDER BY d.id",
                )
                .unwrap();
            let ids = stmt
                .query_map([lib.job], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect();
            ids
        };
        assert_eq!(
            only_new(&lib),
            vec![fresh],
            "the old reading is not read again"
        );

        // A full run reads every unaccepted subject, read or not.
        let db = lib.db();
        let mut stmt = db
            .prepare(
                "SELECT d.id FROM detections d JOIN images i ON i.id=d.image_id \
                 WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle')='vehicle' \
                 AND d.reviewed=0 ORDER BY d.id",
            )
            .unwrap();
        let all: Vec<i64> = stmt
            .query_map([lib.job], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(all, vec![read, fresh]);
    }

    #[test]
    fn a_hand_edited_car_is_never_offered_to_identify_again() {
        use crate::testkit::Lib;
        let lib = Lib::new("identify-hand-edited");
        let car = lib.detection(lib.frame("a.jpg", Some(1)), "vehicle", 0.9);
        lib.sql("UPDATE detections SET burst_pick=1 WHERE id=?", [car]);
        lib.run(
            "edit_detection",
            serde_json::json!({"detectionId": car, "make": "Hand"}),
        )
        .unwrap();
        let (read_at, reviewed): (Option<f64>, i64) = lib
            .db()
            .query_row(
                "SELECT read_at,reviewed FROM detections WHERE id=?",
                [car],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(
            read_at.is_some(),
            "a hand edit settles the box: it is not left unread"
        );
        assert_eq!(reviewed, 1);
        let offered = |only_new: bool| -> Vec<i64> {
            let db = lib.db();
            let mut stmt = db
                .prepare(&pending_detections(
                    "vehicle",
                    identify_eligibility("everything"),
                    only_new,
                ))
                .unwrap();
            stmt.query_map([lib.job], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert!(
            offered(true).is_empty(),
            "an incremental run does not offer a car the person already settled"
        );
        assert!(
            offered(false).is_empty(),
            "nor does a full run: reviewed keeps the hand reading"
        );
        // A row an older build's hand edit left without `read_at` is still not
        // offered: `reviewed`, not `read_at`, is the settled-box test.
        lib.sql(
            "UPDATE detections SET read_at=NULL,reviewed=1 WHERE id=?",
            [car],
        );
        assert!(offered(false).is_empty(), "reviewed alone is enough");
        assert!(offered(true).is_empty());
        // A car found later is still offered.
        let fresh = lib.detection(lib.frame("b.jpg", Some(1)), "vehicle", 0.9);
        assert_eq!(offered(true), vec![fresh]);
    }

    #[test]
    fn an_identified_album_shows_its_readings_and_asks_only_for_new_cars() {
        use crate::testkit::Lib;
        let lib = Lib::new("identify-no-reask");
        // Two cars, both read long ago but not yet accepted: the album is identified.
        let a = lib.detection(lib.frame("a.jpg", None), "vehicle", 0.9);
        let b = lib.detection(lib.frame("b.jpg", None), "vehicle", 0.9);
        for id in [a, b] {
            lib.sql(
                "UPDATE detections SET attributes='{\"make\":\"BMW\"}',read_at=1700000000,burst_pick=1 WHERE id=?",
                [id],
            );
        }
        // The review payload must carry `read_at`, or the screen cannot tell that
        // these were read and keeps asking to identify them again.
        let review = lib
            .run("review", serde_json::json!({"jobId": lib.job}))
            .unwrap();
        for d in review["detections"].as_array().unwrap() {
            assert!(
                d["read_at"].as_f64().is_some(),
                "a read subject carries read_at: {d}"
            );
        }
        // Nothing is pending: an incremental run reads no subject again.
        let pending = |lib: &Lib| -> Vec<i64> {
            let db = lib.db();
            let mut stmt = db
                .prepare(
                    "SELECT d.id FROM detections d JOIN images i ON i.id=d.image_id \
                     WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle')='vehicle' \
                     AND d.reviewed=0 AND d.read_at IS NULL ORDER BY d.id",
                )
                .unwrap();
            stmt.query_map([lib.job], |r| r.get(0))
                .unwrap()
                .map(|r| r.unwrap())
                .collect()
        };
        assert!(
            pending(&lib).is_empty(),
            "an identified album asks for nothing"
        );

        // A new car turns up: only it is offered, and the payload leaves it unread.
        let fresh = lib.detection(lib.frame("c.jpg", None), "vehicle", 0.9);
        lib.sql("UPDATE detections SET burst_pick=1 WHERE id=?", [fresh]);
        assert_eq!(pending(&lib), vec![fresh], "only the new car is pending");
        let review = lib
            .run("review", serde_json::json!({"jobId": lib.job}))
            .unwrap();
        let new = review["detections"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"].as_i64() == Some(fresh))
            .unwrap();
        assert!(new["read_at"].is_null(), "the new car has not been read");
    }

    #[test]
    fn skipping_identify_marks_the_unread_subjects_read_and_keeps_the_readings() {
        use crate::testkit::Lib;
        let lib = Lib::new("skip-identify");
        let read = lib.detection(lib.frame("a.jpg", None), "vehicle", 0.9);
        let unread = lib.detection(lib.frame("b.jpg", None), "vehicle", 0.9);
        lib.sql(
            "UPDATE detections SET attributes='{\"make\":\"BMW\"}',read_at=1 WHERE id=?",
            [read],
        );
        lib.sql(
            "UPDATE jobs SET settings_json=json_set(settings_json,'$.native_identify_stale',1) WHERE id=?",
            [lib.job],
        );

        let out = lib
            .run("skip_identify", serde_json::json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(out, serde_json::json!({"ok": true, "skipped": 1}));
        let (attributes, read_at): (Option<String>, Option<f64>) = lib
            .db()
            .query_row(
                "SELECT attributes,read_at FROM detections WHERE id=?",
                [unread],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert!(attributes.is_none(), "skipping invents no reading");
        assert!(
            read_at.is_some(),
            "but the album no longer asks to identify it"
        );
        let kept: String = lib.one("SELECT attributes FROM detections WHERE id=?", [read]);
        assert_eq!(
            kept, "{\"make\":\"BMW\"}",
            "an existing reading is untouched"
        );
        let stale: Option<bool> = lib
            .db()
            .query_row(
                "SELECT json_extract(settings_json,'$.native_identify_stale') FROM jobs WHERE id=?",
                [lib.job],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stale, None, "skip clears the stale note");
    }
    #[test]
    fn metadata_marks_manual_and_automatic_culls_as_rejected() {
        let settings = Settings::default();
        assert_eq!(
            metadata_verdict(true, false, Some(5), 1.0, &settings, false),
            (-1, Some("Red"), true)
        );
        assert_eq!(
            metadata_verdict(false, true, None, 0.2, &settings, false),
            (-1, Some("Red"), true)
        );
        assert_eq!(
            metadata_verdict(false, true, Some(4), 0.2, &settings, false),
            (4, Some("Red"), false)
        );
    }

    #[test]
    fn a_suggested_keep_takes_the_pick_colour_and_none_leaves_the_label_alone() {
        let mut settings = Settings::default();
        assert_eq!(
            metadata_verdict(false, false, None, 0.9, &settings, true).1,
            Some("Blue")
        );
        settings.pick_label = "Purple".into();
        assert_eq!(
            metadata_verdict(false, false, None, 0.9, &settings, true).1,
            Some("Purple")
        );
        settings.pick_label = "None".into();
        assert_eq!(
            metadata_verdict(false, false, None, 0.9, &settings, true).1,
            None
        );
        // A hand pick keeps the sharpness label; only the suggestion changes.
        settings.pick_label = "Blue".into();
        assert_eq!(
            metadata_verdict(false, false, Some(4), 0.9, &settings, false),
            (4, Some("Green"), false)
        );
    }

    /// The auto-apply write reads the album's rows once and then spends minutes
    /// in exiftool. A hand pick, star or reject made in that window is the
    /// photographer's last word: that frame fails the snapshot check, so both
    /// the XMP write and the Lightroom package leave it alone, while the frames
    /// nobody touched still pass and are written as before.
    #[test]
    fn a_hand_pick_during_the_write_wins_and_its_frame_is_skipped() {
        use crate::testkit::Lib;
        let lib = Lib::new("write-race");
        let picked = lib.frame("a.jpg", None);
        let starred = lib.frame("b.jpg", None);
        let suggested = lib.frame("c.jpg", Some(7));
        let burstmate = lib.frame("d.jpg", Some(7));
        lib.sql("UPDATE images SET burst_pick=1 WHERE id=?", [suggested]);
        // The one read the write and the export take, in album order.
        let frames = rows(
            &lock(&lib.d().db),
            "SELECT * FROM images WHERE job_id=? AND status='done' AND COALESCE(copy_of,0)=0 ORDER BY id",
            [lib.job],
        )
        .unwrap();
        assert_eq!(frames.len(), 4);
        assert!(
            frames.iter().all(|f| snapshot_stands(lib.d(), f).unwrap()),
            "nothing has moved when the snapshot is taken"
        );
        assert!(suggestion_stands(lib.d(), lib.job, &frames[2]).unwrap());
        // Review's own commands land while the write is still in exiftool.
        lib.run("mark", json!({"imageId": picked, "picked": true}))
            .unwrap();
        lib.run("mark", json!({"imageId": starred, "stars": 4}))
            .unwrap();
        lib.run("mark", json!({"imageId": burstmate, "picked": true}))
            .unwrap();
        let stands: Vec<bool> = frames
            .iter()
            .map(|f| snapshot_stands(lib.d(), f).unwrap())
            .collect();
        assert_eq!(
            stands,
            vec![false, false, true, false],
            "the hand-edited frames are skipped; the quiet one still writes"
        );
        assert!(
            !suggestion_stands(lib.d(), lib.job, &frames[2]).unwrap(),
            "the hand pick takes over its burst's suggestion too"
        );
    }

    #[test]
    fn accepting_a_type_suggestion_mid_run_reaches_vision_resolution() {
        use crate::testkit::Lib;
        let lib = Lib::new("mid-run-accept-type");
        lib.sql(
            "UPDATE jobs SET settings_json='{\"scan_profile\":\"auto\"}', shoot_type_suggested='sport-rodeo', shoot_type_confidence=0.88 WHERE id=?",
            [lib.job],
        );
        // Snapshot the album settings as identify_stage does when it launches,
        // before OCR or Decision runs.
        let started_settings = settings(lib.d(), lib.job).unwrap();
        let (before, ctx_before) = resolved_vision_settings(lib.d(), lib.job, &started_settings);
        assert_eq!(before.scan_profile, "auto");
        assert_eq!(ctx_before, None);

        // Photographer accepts the suggestion mid-run (after Decision, before
        // Vision resolves its settings).
        let out = lib
            .run(
                "accept_type_suggestion",
                json!({"jobId": lib.job, "type": "sport-rodeo"}),
            )
            .unwrap();
        assert_eq!(out["accepted"], true);

        // Vision resolves freshly against the same run-start settings and sees
        // the accepted preset and its prompt guidance immediately.
        let (after, ctx_after) = resolved_vision_settings(lib.d(), lib.job, &started_settings);
        assert_eq!(after.scan_profile, "sport-rodeo");
        assert_eq!(ctx_after, Some(ShootPreset::SportRodeo.prompt_context()));
        assert!(identify_setting_key("identify_decision"));
    }
}
