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
    // ponytail: polls desktop state instead of blocking on a signal so the
    // `stop` flag stays checkable every 200ms; a Condvar woken on both scan
    // completion and cancellation would remove the poll if needed later.
    while !d.status()["activeJob"].is_null() {
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

/// What the identify stage runs on, for the pipeline view.
fn identify_models(settings: &Settings) -> (String, &'static str) {
    let mut parts: Vec<String> = Vec::new();
    if settings.read_plates {
        parts.push(format!("plate {}", models::PLATE_DETECTOR));
    }
    if settings.read_numbers || settings.read_text {
        parts.push("PP-OCRv4".into());
    }
    if settings.use_vlm {
        match settings.enabled_vlm_service() {
            Some(service) => parts.push(format!("{} via {}", service.model, service.provider)),
            None => parts.push("no vision service".into()),
        }
    }
    if parts.is_empty() {
        parts.push("embeddings only".into());
    }
    let device = if settings.use_vlm && settings.enabled_vlm_service().is_some() {
        "ollama"
    } else {
        "CPU"
    };
    (parts.join(" + "), device)
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
}

impl Run<'_> {
    fn advance(&self, by: usize) {
        let n = self.done.fetch_add(by, Ordering::Relaxed) + by;
        self.task.progress(n as u64, self.total as u64);
        if let Some(p) = self.progress {
            p.advance("identify", by as u64);
        }
        if let Some(live) = self.live {
            live.advance_by(by);
        }
    }

    /// Say which file this worker is on.
    fn busy(&self, lane: usize, file: &str, step: &'static str) {
        if let Some(p) = self.progress {
            p.busy("identify", lane, "identify", step, file);
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

    /// One frame's detections: the frame is decoded once, however many there are.
    fn frame(
        &self,
        rows: &[Value],
        readers: &mut Readers,
        embedder: &mut Option<similarity::Embedder>,
        lane: usize,
    ) -> Result<()> {
        let s = self.settings;
        let mut wanted = Vec::new();
        for row in rows {
            if ml_eligible(self.d, row["id"].as_i64().ok_or("Invalid detection")?)? {
                wanted.push(row);
            }
        }
        self.advance(rows.len() - wanted.len());
        if wanted.is_empty() {
            return Ok(());
        }
        let path = wanted[0]["path"].as_str().ok_or("Missing image path")?;
        let filename = Path::new(path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.busy(lane, &filename, "read");
        // The frame is decoded once for all its cars. Show the first one while
        // the RAW is being read, then each car as its turn comes.
        let mut started: Option<i64> = None;
        if let Some(live) = self.live {
            if let Some(row) = wanted.first() {
                let id = row["id"].as_i64().ok_or("Invalid detection")?;
                live.start_item(
                    lane,
                    id,
                    row["image_id"].as_i64().unwrap_or_default(),
                    path,
                    row["thumb_path"].as_str(),
                    row["preview_path"].as_str(),
                );
                live.phase(id, identify_live::READ);
                started = Some(id);
            }
        }
        let raw = conrod_io::raw::read(Path::new(path))?;
        let image = Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation);
        for row in wanted {
            let id = row["id"].as_i64().ok_or("Invalid detection")?;
            let image_id = row["image_id"].as_i64().unwrap_or_default();
            if !ml_eligible(self.d, id)? {
                if started == Some(id) {
                    if let Some(live) = self.live {
                        live.finish_error(
                            id,
                            format!("detection {id} is no longer eligible and was skipped"),
                        );
                    }
                    started = None;
                }
                self.advance(1);
                continue;
            }
            if let Some(live) = self.live {
                if started != Some(id) {
                    live.start_item(
                        lane,
                        id,
                        image_id,
                        path,
                        row["thumb_path"].as_str(),
                        row["preview_path"].as_str(),
                    );
                    started = Some(id);
                }
                live.phase(id, identify_live::CROP);
            }
            let item_start = Instant::now();
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
                self.advance(1);
                continue;
            }
            // Everything reads the padded, size-capped `cut` crop; the tight
            // native-resolution box is only for the plate search. Reading
            // the tight box clipped plates at its edge.
            let crop = crate::cut(
                &image,
                detect::expand_box(subject, image.width, image.height, self.crop),
                s,
            );
            let native = (s.plate_native_search && s.read_plates).then_some(&tight);
            let kind = row["cls"].as_str().unwrap_or("car");
            let burst = row["burst_key"].as_i64().unwrap_or_default();
            let seen_numbers = lock(&self.burst_numbers)
                .get(&burst)
                .cloned()
                .unwrap_or_default();
            self.task.detail(format!("{filename} · {kind}"));
            self.busy(lane, &filename, "identify");
            if let Some(live) = self.live {
                live.phase(id, identify_live::VISION);
            }
            // The prompt goes out while the model is thinking, not after it
            // answers, so the screen can show what was asked.
            let prompt_sink = self.live.as_ref().map(|live| {
                let live = Arc::clone(live);
                move |prompt: &str| live.set_prompt(id, prompt)
            });
            let outcome = analyze::analyze_with(
                &crop,
                native,
                kind,
                kind == "motorcycle",
                &Context {
                    settings: s,
                    plates: self.plates,
                    known: self.known,
                    seen_numbers: &seen_numbers,
                },
                readers,
                prompt_sink.as_ref().map(|sink| sink as &dyn Fn(&str)),
            )?;
            if let Some(live) = self.live {
                live.phase(id, identify_live::MERGE);
                // Fallback for a prompt that the callback could not attach.
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
            // What the frame reads as, for the live strip on the scan screen.
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
            self.advance(1);
        }
        Ok(())
    }
}

/// One worker's own readers: its plate models and vision client, and the shared OCR.
fn load_readers(settings: &Settings, ocr: &Option<Arc<ocr::Ocr>>) -> Result<Readers> {
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
        vlm: settings.use_vlm.then(|| {
            (
                vlm::VlmClient::new(Arc::new(vlm::RealClock::default())),
                IoSettings::from_core(settings),
            )
        }),
    })
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
    let settings = settings(d, job)?;
    let vision_off = vision_off(d, job)?;
    let lanes = settings.lanes();
    let finds_faces = lanes.names;
    if !lanes.identifies() && !finds_faces && !lanes.moments_detect {
        return Err("There are no identifiable subjects in this shoot preset".into());
    }
    // The command's scope overrides the album's setting for this run only;
    // the scan's own auto-identify passes none and follows the album.
    let scope = run_scope(&settings.identify_scope, scope);
    let eligibility = identify_eligibility(scope);
    // An incremental run leaves every subject that has already been read
    // alone; a full run reads them again (the photographer asked for it).
    let fresh = if only_new {
        " AND d.read_at IS NULL"
    } else {
        ""
    };
    launch(d, job, kind, move |d, stop, task| {
        if !wait_for_cull(d, stop, task) {
            return Ok(());
        }
        if !lanes.identifies() {
            if lanes.names {
                let faces = crate::passes::embed_faces_missing(d, job, stop, task)?;
                task.detail(format!("Prepared {faces} faces for name suggestions"));
            }
            // No vehicles to read, but the shoot may still have moments to judge.
            if crate::moments::run(d, job, stop, task, &settings, &lanes, scope, false)? > 0 {
                crate::passes::pick_keepers(d, job)?;
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
        let pending = rows(&lock(&d.reader), &format!("SELECT d.*,i.path,i.thumb_path,i.preview_path,i.burst_key FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle')='{region}' AND d.reviewed=0 AND COALESCE(i.copy_of,0)=0 AND {eligibility}{fresh} ORDER BY i.id,d.id"), [job])?;
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
            if lanes.names {
                crate::passes::embed_faces_missing(d, job, stop, task)?;
            }
            task.detail(if scope == conrod_core::settings::IDENTIFY_EVERYTHING {
                "No subjects need identification"
            } else {
                "No keepers need identification"
            });
            if crate::moments::run(d, job, stop, task, &settings, &lanes, scope, false)? > 0 {
                crate::passes::pick_keepers(d, job)?;
            }
            note_identify_stale(d, job, false);
            end.finish("done");
            return Ok(());
        }
        let reads_text = settings.read_plates || settings.read_numbers || settings.read_text;
        let mut needs: Vec<&str> = Vec::new();
        if settings.read_plates {
            needs.extend(setup::IDENTIFY_PLATES);
        }
        if reads_text {
            needs.extend(setup::TEXT);
        }
        if settings.group_vehicles {
            needs.extend(setup::SIMILARITY);
        }
        setup::ensure(&d.hub, stop, &needs)?;
        let ocr_engine = if reads_text {
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
        let (next, done) = (AtomicUsize::new(0), AtomicUsize::new(0));
        let failure: Mutex<Option<String>> = Mutex::new(None);
        let reported = Mutex::new(HashSet::new());
        // The scan screen shows this lane when the one-shot scan gets here.
        let progress = d.progress();
        if let Some(p) = &progress {
            let (model, device) = identify_models(&settings);
            p.describe("identify", model, device);
            p.total("identify", pending.len() as u64);
            p.phase("identify");
        }
        let workers = identify_workers(&settings).min(frames.len().max(1));
        live.set_workers(workers);
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
        };
        let fail = |e: String| {
            lock(&failure).get_or_insert(e);
        };
        // Moments run from the start, beside the vision workers, so the decision model answers
        // each frame as it comes rather than sitting silent until the end of the run. They are
        // joined before the pick, which ranks on them.
        let moments: Mutex<Option<Result<usize>>> = Mutex::new(None);
        std::thread::scope(|s| {
            s.spawn(|| {
                let result =
                    crate::moments::run(d, job, stop, task, &settings, &lanes, scope, false);
                *lock(&moments) = Some(result);
            });
            for lane in 0..workers {
                let (settings, ocr_engine, stop, next, failure, frames, pending, run) = (
                    &settings,
                    &ocr_engine,
                    &stop,
                    &next,
                    &failure,
                    &frames,
                    &pending,
                    &run,
                );
                let (progress, fail) = (&progress, &fail);
                s.spawn(move || {
                    let mut readers = match load_readers(settings, ocr_engine) {
                        Ok(r) => r,
                        Err(e) => return fail(e),
                    };
                    let mut embedder = match settings.group_vehicles.then(|| {
                        similarity::Embedder::load(
                            &conrod_core::models::expected(conrod_core::models::SIMILARITY),
                            Device::Cpu,
                        )
                    }) {
                        Some(Err(e)) => return fail(e),
                        Some(Ok(e)) => Some(e),
                        None => None,
                    };
                    while !stop.load(Ordering::Relaxed) && lock(failure).is_none() {
                        let Some(frame) = frames.get(next.fetch_add(1, Ordering::Relaxed)) else {
                            break;
                        };
                        if let Err(e) =
                            run.frame(&pending[frame.clone()], &mut readers, &mut embedder, lane)
                        {
                            return fail(e);
                        }
                    }
                    if let Some(p) = progress.as_ref() {
                        p.idle("identify", lane);
                    }
                });
            }
        });
        if let Some(e) = failure.into_inner().unwrap_or_else(|e| e.into_inner()) {
            end.finish("error");
            return Err(e);
        }
        if let Some(Err(e)) = moments.into_inner().unwrap_or_else(|e| e.into_inner()) {
            // The frame reads are done; a moments failure still ends the run the way it did
            // when moments were the last step.
            end.finish("error");
            return Err(e);
        }
        if !stop.load(Ordering::Relaxed) {
            if lanes.names {
                crate::passes::embed_faces_missing(d, job, stop, task)?;
            }
            task.detail("Grouping similar vehicles");
            crate::passes::consolidate(d, job, task)?;
            task.detail("Finding re-shot frames");
            crate::passes::near_duplicates(d, job, stop)?;
            crate::catalog::seed(d, Some(job))?;
            // Groups are now known, so the keeper of each pass can be chosen
            // properly: the one-shot scan ends on this stage.
            if let Some(p) = &progress {
                p.phase("pick");
            }
            // Moments were joined above, while the cars were read, so the pick ranks on them.
            task.detail("Picking keepers");
            crate::passes::pick_keepers(d, job)?;
            note_identify_stale(d, job, false);
            end.finish("done");
        } else {
            end.finish("stopped");
        }
        Ok(())
    })
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
            let suggested = !rejected
                && !hand_pick
                && (frame["burst_pick"].as_i64() == Some(1)
                    || detections
                        .iter()
                        .any(|d| d["burst_pick"].as_i64() == Some(1)))
                && !burst.is_some_and(|key| picked_bursts.contains(&key));
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
        let (model, _) = identify_models(&settings);
        assert!(model.contains("glm-5.3-flash via opencode-go"), "{model}");
        assert!(!model.contains("qwen2.5vl"), "{model}");

        let all_off = Settings {
            vlm_services: vec![service("off", "ollama", "qwen2.5vl:7b", false)],
            ..settings
        };
        let (model, device) = identify_models(&all_off);
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
}
