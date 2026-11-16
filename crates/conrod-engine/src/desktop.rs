//! Application operations shared by Tauri and the CLI. Database writes and
//! scans stay here; the frontend receives data, never arbitrary SQL or paths.
use crate::commands::{Command, KnownArgs, MarkArgs, ScanArgs};
use crate::lock;
use crate::{FrameResult, Scan};
use conrod_core::{
    profile::{ScanProfile, ShootPreset},
    settings::Settings,
    tasks::TaskHub,
};
use conrod_vision::{imageops::Rgb, similarity};
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

pub type Result<T> = std::result::Result<T, String>;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

/// The burst gap to group an album with: its own setting, or the one default.
fn burst_gap(setting: f64) -> f64 {
    conrod_core::settings::burst_gap_or_default(setting)
}

/// Burst keys stay small and positive; a keep-set's key is offset past them so
/// a bracket never collides with a burst number.
const SET_KEY_BASE: i64 = 1_000_000;

fn set_kind_name(kind: conrod_core::sets::SetKind) -> &'static str {
    use conrod_core::sets::SetKind;
    match kind {
        SetKind::Bracket => "bracket",
        SetKind::FocusStack => "focus",
        SetKind::Interval => "interval",
    }
}

/// Which keep-together set each image belongs to, from the EXIF the scan
/// stored. An album scanned before those columns existed reads the headers
/// once; that is the only slow path in a regroup, and only for old albums.
fn keep_sets(
    images: &[Value],
) -> Vec<Option<(i64, conrod_core::sets::SetKind)>> {
    use conrod_core::sets::{find_sets, SetFrame};
    let mut out = vec![None; images.len()];
    if images.is_empty() {
        return out;
    }
    let stored = images
        .iter()
        .any(|v| v["exposure_time"].as_f64().is_some() || v["exposure_bias"].as_f64().is_some());
    let mut order: Vec<usize> = (0..images.len()).collect();
    order.sort_by(|&a, &c| {
        images[a]["taken_at"]
            .as_f64()
            .unwrap_or_default()
            .total_cmp(&images[c]["taken_at"].as_f64().unwrap_or_default())
            .then_with(|| {
                images[a]["path"]
                    .as_str()
                    .unwrap_or_default()
                    .cmp(images[c]["path"].as_str().unwrap_or_default())
            })
    });
    let mut frames = Vec::with_capacity(images.len());
    for &i in &order {
        let v = &images[i];
        let path = v["path"].as_str().unwrap_or_default();
        let read = if stored {
            None
        } else {
            conrod_io::raw::exposure_meta(std::path::Path::new(path)).ok()
        };
        let number = |key: &str, from: Option<f64>| {
            if stored {
                v[key].as_f64()
            } else {
                from
            }
        };
        frames.push(SetFrame {
            taken: v["taken_at"].as_f64(),
            camera: v["camera"].as_str().unwrap_or_default().to_string(),
            exposure_bias: number("exposure_bias", read.as_ref().and_then(|e| e.exposure_bias)),
            exposure_time: number("exposure_time", read.as_ref().and_then(|e| e.exposure_time)),
            f_number: number("f_number", read.as_ref().and_then(|e| e.f_number)),
            focal_length: number("focal_length", read.as_ref().and_then(|e| e.focal_length)),
            focus_distance: number(
                "focus_distance",
                read.as_ref().and_then(|e| e.focus_distance),
            ),
            width: v["width"].as_u64().unwrap_or_default() as u32,
            height: v["height"].as_u64().unwrap_or_default() as u32,
        });
    }
    let found = find_sets(&frames);
    for (n, set) in found.into_iter().enumerate() {
        out[order[n]] = set.map(|(first, kind)| {
            let key = images[order[first as usize]]["id"].as_i64().unwrap_or_default();
            (key, kind)
        });
    }
    out
}

pub struct Desktop {
    pub hub: TaskHub,
    pub root: PathBuf,
    pub(crate) db: Mutex<Connection>,
    /// A second connection for reads: WAL lets it run while a scan writes, so the
    /// review grid and the album list never wait on (or hold up) the writer.
    pub(crate) reader: Mutex<Connection>,
    pub(crate) settings: Mutex<Settings>,
    quit: AtomicBool,
    finishing: AtomicBool,
    pub(crate) watching: Mutex<crate::watching::Watching>,
    active: Mutex<Option<(i64, Arc<Scan>)>>,
    /// The scan in progress (or just finished), for `scan-progress` events.
    progress: Mutex<Option<Arc<crate::progress::Progress>>>,
    pub(crate) operations:
        Mutex<std::collections::BTreeMap<String, Arc<std::sync::atomic::AtomicBool>>>,
}

pub fn rows(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(sql).map_err(err)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let result = stmt
        .query_map(args, |row| {
            let mut item = Map::new();
            for (i, name) in names.iter().enumerate() {
                use rusqlite::types::ValueRef;
                let value = match row.get_ref(i)? {
                    ValueRef::Null => Value::Null,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => json!(n),
                    ValueRef::Text(s) => Value::String(String::from_utf8_lossy(s).into_owned()),
                    ValueRef::Blob(_) => Value::Null,
                };
                item.insert(name.clone(), value);
            }
            Ok(Value::Object(item))
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(result)
}

/// `rows`, minus the fields that are null or empty.
///
/// A review payload repeats every key on every row, and more than a third of
/// this table's fields are empty; the UI reads an absent key as null, so the
/// empty ones cost bytes on the wire and `JSON.parse` time for nothing.
pub fn rows_compact(
    conn: &Connection,
    sql: &str,
    args: impl rusqlite::Params,
) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(sql).map_err(err)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let result = stmt
        .query_map(args, |row| {
            let mut item = Map::new();
            for (i, name) in names.iter().enumerate() {
                use rusqlite::types::ValueRef;
                let value = match row.get_ref(i)? {
                    ValueRef::Null => continue,
                    ValueRef::Integer(n) => json!(n),
                    ValueRef::Real(n) => json!(n),
                    ValueRef::Text(s) => {
                        if s.is_empty() {
                            continue;
                        }
                        Value::String(String::from_utf8_lossy(s).into_owned())
                    }
                    ValueRef::Blob(_) => continue,
                };
                item.insert(name.clone(), value);
            }
            Ok(Value::Object(item))
        })
        .map_err(err)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(err)?;
    Ok(result)
}

/// Everything the Review screen opens with: the album's frames and detections.
///
/// Only what the UI reads: `SELECT *` was 17 MB and 0.8 s for 4,808 frames,
/// 3.6 MB of it the raw sharpness `features`. Detections carrying an identity
/// embedding are matched against the known vehicles and people here, and the
/// embedding itself never leaves this function.
pub fn review_payload(db: &Connection, job: i64) -> Result<Value> {
    let frames = rows_compact(db,"SELECT i.id,i.path,i.status,i.thumb_path,i.preview_path,i.width,i.height,i.rating,i.rejected,i.picked,i.burst_key,i.dupe_key,i.error, COALESCE(i.stars,(SELECT max(d.stars) FROM detections d WHERE d.image_id=i.id)) AS manual_stars, COALESCE((SELECT max(d.burst_pick) FROM detections d WHERE d.image_id=i.id AND d.burst_pick=1), i.burst_pick) AS burst_pick, i.set_id,i.set_kind,i.exposure_verdict,i.highlight_clip,i.shadow_clip,i.dark_frame,i.tilt_deg,i.converge_deg,i.level_verdict,i.eye_open_min,i.smile_max,i.buck_score,i.taste FROM images i WHERE job_id=? ORDER BY path",[job])?;
    let mut detections = rows_compact(db,"SELECT d.id,d.image_id,d.cls,d.crop_path,d.x1,d.y1,d.x2,d.y2,d.sharpness,d.panning,d.number,d.plate,d.cull_reason,d.attributes,d.burst_pick,d.region_type,d.reviewed,d.rejected,d.bystander,d.stars,d.group_key,d.group_size,d.group_agreement,d.embedding,d.rating,d.rating_verdict,d.sharp_end,d.quality,d.cut_sides,d.subject_frac,d.lead_room,d.framing_score FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? ORDER BY d.image_id,d.id",[job])?;
    let known: Vec<(Value, Vec<f32>)> = rows(db, "SELECT plate,make,model,colour,team,race_number,driver,country,embedding FROM known_vehicles WHERE embedding IS NOT NULL AND embedding!=''", [])?
        .into_iter()
        .filter_map(|row| similarity::unpack(row["embedding"].as_str()?).map(|vector| (row, vector)))
        .collect();
    let people: Vec<(Value, Vec<f32>)> =
        rows(db, "SELECT name,country,embedding FROM known_people", [])?
            .into_iter()
            .filter_map(|row| {
                similarity::unpack(row["embedding"].as_str()?).map(|vector| (row, vector))
            })
            .collect();
    for detection in &mut detections {
        if let Some(vector) = detection["embedding"].as_str().and_then(similarity::unpack) {
            let is_face = detection["region_type"].as_str() == Some("face");
            let candidates = if is_face { &people } else { &known };
            // SFace faces match at its own threshold; an older DINOv2 one
            // must be near-identical.
            let threshold = if is_face && vector.len() == conrod_vision::faces::IDENTITY_DIM {
                f64::from(conrod_vision::faces::SAME_PERSON)
            } else if is_face {
                0.97
            } else {
                conrod_core::grouping::SAME_CAR
            };
            let best = candidates
                .iter()
                .filter(|(_, candidate)| candidate.len() == vector.len())
                .filter_map(|(item, candidate)| {
                    let score = similarity::nearness(&vector, candidate);
                    (f64::from(score) >= threshold).then_some((score, item))
                })
                .max_by(|a, b| a.0.total_cmp(&b.0));
            if let Some((score, item)) = best {
                let mut item = item.clone();
                item.as_object_mut().map(|v| v.remove("embedding"));
                item["similarity"] = json!((score * 1000.0).round() / 1000.0);
                detection[if is_face {
                    "known_person_match"
                } else {
                    "known_match"
                }] = item;
            }
        }
        detection.as_object_mut().map(|d| d.remove("embedding"));
    }
    Ok(json!({"frames":frames,"detections":detections,"tasteHead":crate::taste::available()}))
}

impl Desktop {
    pub fn open(root: PathBuf) -> Result<Arc<Self>> {
        let hub = TaskHub::new();
        let task = hub.start("Opening library", 0);
        std::fs::create_dir_all(root.join("cache/native")).map_err(err)?;
        let db = conrod_store::connect(Some(&root.join("conrod.db"))).map_err(err)?;
        let reader = conrod_store::connect(Some(&root.join("conrod.db"))).map_err(err)?;
        crate::region_training::prepare(&db)?;
        let settings = Settings::load(&root.join("settings.json"));
        task.finish();
        let desktop = Arc::new(Self {
            hub,
            root,
            db: Mutex::new(db),
            reader: Mutex::new(reader),
            settings: Mutex::new(settings),
            quit: AtomicBool::new(false),
            finishing: AtomicBool::new(false),
            watching: Mutex::new(Default::default()),
            active: Mutex::new(None),
            progress: Mutex::new(None),
            operations: Mutex::new(std::collections::BTreeMap::new()),
        });
        crate::watching::restore(&desktop);
        Ok(desktop)
    }

    /// The scan in progress (or the one that just ended), if any.
    pub fn progress(&self) -> Option<Arc<crate::progress::Progress>> {
        lock(&self.progress).clone()
    }

    /// The `scan-progress` payload for this tick, throttled by the caller: one
    /// per tick while the scan runs, then one final snapshot and nothing.
    pub fn progress_snapshot(&self) -> Option<Value> {
        let progress = self.progress()?;
        progress.emit_snapshot().map(|snapshot| json!(snapshot))
    }

    pub fn request_quit(&self) {
        self.quit.store(true, Ordering::Release);
    }
    pub fn quit_requested(&self) -> bool {
        self.quit.load(Ordering::Acquire)
    }
    /// Whether closing the window should leave Conrod running in the tray.
    pub fn close_to_tray(&self) -> bool {
        lock(&self.settings).close_to_tray
    }
    pub fn scanning(&self) -> bool {
        self.active
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|(_, scan)| !scan.is_finished() || self.finishing.load(Ordering::Acquire))
    }
    pub(crate) fn resume_album(self: &Arc<Self>, job: i64) -> Result<()> {
        let stored = crate::operations::settings(self, job)?;
        let stage = stored
            .extra
            .get("native_scan_stage")
            .cloned()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();
        self.start_scan(&ScanArgs {
            job_id: Some(job),
            root: None,
            profile: None,
            label: None,
            stage,
            recursive: true,
            read_plates: None,
            read_numbers: None,
            include_people: None,
            scan_profile: None,
            blur_strictness: None,
            burst_gap: None,
        })
        .map(|_| ())
    }
    pub fn dispatch(self: &Arc<Self>, action: &str, args: Value) -> Result<Value> {
        let result = self.run(Command::parse(action, args)?);
        if result.is_ok()
            && [
                "mark",
                "edit_detection",
                "bulk_edit",
                "rename_job",
                "update_job_settings",
                "regroup",
                "reset_all",
                "reset_detections",
                "reset_identifications",
                "reset_ratings",
                "save_settings",
                "delete_job",
            ]
            .contains(&action)
        {
            self.hub.note(format!("Library changed: {action}"));
        }
        result
    }

    pub fn run(self: &Arc<Self>, command: Command) -> Result<Value> {
        match command {
            Command::Bootstrap {} => Ok(
                json!({"settings": *lock(&self.settings), "jobs": self.jobs()?, "models": self.models(), "status": self.status()}),
            ),
            Command::Health {} => {
                let settings = lock(&self.settings).clone();
                let mut models = self.models();
                models
                    .as_array_mut()
                    .unwrap()
                    .extend(conrod_io::health::services(
                        &conrod_io::settings::Settings::from_core(&settings),
                        settings.use_vlm,
                    ));
                Ok(models)
            }
            Command::Status {} => Ok(self.status()),
            Command::Jobs {} => Ok(json!(self.jobs()?)),
            Command::Review(a) => self.review(a.job_id),
            Command::Scan(a) => self.start_scan(&a),
            Command::Pause {} => self.control(|scan| scan.pause(true)),
            Command::ResumeScan {} => self.control(|scan| scan.pause(false)),
            Command::Stop {} => {
                self.hub.note("Stopping scan...");
                self.control(Scan::cancel)
            }
            Command::DeleteJob(a) => self.delete_job(a.job_id),
            Command::Identify(a) => crate::operations::identify(self, a.job_id),
            Command::Identified(a) => self.identified(a.job_id),
            Command::Write(a) => {
                crate::operations::write(self, a.job_id, a.dry_run, a.embed_in_raw)
            }
            Command::CancelOperation(k) => {
                if let Some(flag) = lock(&self.operations).get(&k.key) {
                    flag.store(true, Ordering::Relaxed);
                }
                Ok(Value::Null)
            }
            Command::InstallModels {} => crate::setup::install_missing(self),
            Command::SaveSettings(updates) => self.save_settings(&updates),
            Command::Mark(a) => self.mark(&a),
            Command::EditDetection(a) => crate::edits::edit_detection(self, &a),
            Command::Preview(a) => self.preview(a.image_id),
            Command::Known {} => Ok(json!(rows(
                &lock(&self.reader),
                "SELECT * FROM known_vehicles ORDER BY plate",
                []
            )?)),
            Command::ExportKnown {} => crate::catalog::export(self),
            Command::ImportKnown(a) => crate::catalog::import(self, &a.csv),
            Command::SeedKnown(a) => crate::catalog::seed(self, a.job_id),
            Command::ImportEntries(a) => crate::catalog::entries(self, &a.csv),
            Command::SaveKnown(a) => self.save_known(&a),
            Command::DeleteKnown(a) => self.delete_known(&a.plate),
            Command::DeleteAllKnown {} => self.delete_all_known(),
            Command::TrainingStatus {} => crate::region_training::status(self),
            Command::TrainLabel(a) => crate::region_training::label(self, &a),
            Command::UndoLabel {} => crate::region_training::undo(self),
            Command::TrainModel(a) => crate::region_training::train(self, a.region),
            Command::ForgetModel(a) => crate::region_training::forget(self, a.region),
            Command::TrainTaste {} => crate::region_training::taste(self),
            Command::Rescore(a) => crate::rescore::rescore(self, a.job_id),
            Command::Rescan(a) => crate::rescore::rescan(self, a.job_id),
            Command::PickKeepers(a) => crate::passes::pick_keepers(self, a.job_id),
            Command::TasteRank(a) => crate::taste::taste_rank(self, a.job_id, a.force),
            Command::Group(a) => crate::passes::group(self, a.job_id),
            Command::Regroup(a) => self.regroup(a.job_id),
            Command::BulkEdit(a) => crate::edits::bulk_edit(self, &a),
            Command::RenameJob(a) => crate::library::rename_job(self, &a),
            Command::UpdateJobSettings(a) => crate::library::update_job_settings(self, &a),
            Command::SetIncludePeople(a) => {
                crate::passes::set_include_people(self, a.job_id, a.value)
            }
            Command::Summary(a) => crate::library::summary(self, a.job_id),
            Command::Filling(a) => crate::library::filling(self, a.job_id),
            Command::Cover(a) => crate::library::cover(self, a.job_id),
            Command::CacheInfo {} => crate::housekeeping::cache_info(self),
            Command::CacheClear(a) => crate::housekeeping::cache_clear(self, &a),
            Command::ResetIdentifications(a) => {
                crate::housekeeping::reset_identifications(self, a.job_id)
            }
            Command::ResetRatings(a) => crate::housekeeping::reset_ratings(self, a.job_id),
            Command::ResetDetections(a) => crate::housekeeping::reset_detections(self, a.job_id),
            Command::ResetAll {} => crate::housekeeping::reset_all(self),
            Command::OllamaModels(a) => {
                let host = a
                    .host
                    .unwrap_or_else(|| lock(&self.settings).vlm_host.clone());
                Ok(conrod_io::health::ollama_models(&host))
            }
            Command::WatchStatus {} => Ok(crate::watching::status(self)),
            Command::SetWatch(a) => crate::watching::set(self, &a),
            Command::BenchPick {} => crate::bench::pick(self),
            Command::BenchRun(a) => crate::bench::run(self, &a),
            Command::WorkerLimits {} => Ok(self.worker_limits()),
            Command::DescribeImage { path } => crate::bench::describe_one(self, &path),
        }
    }

    /// Pause, resume or stop whatever scan is running.
    fn control(&self, act: impl FnOnce(&Scan)) -> Result<Value> {
        if let Some((_, scan)) = lock(&self.active).as_ref() {
            act(scan);
        }
        Ok(self.status())
    }

    fn save_settings(&self, updates: &Map<String, Value>) -> Result<Value> {
        let mut stored = lock(&self.settings);
        let mut next = stored.clone().apply(updates);
        // The Settings screen saves the whole object, so only a changed value
        // is a choice.
        if next.blur_strictness != stored.blur_strictness {
            next.blur_explicit = true;
        }
        if !(0.0..=1.0).contains(&next.detect_conf)
            || next.crop_min_edge < 1
            || next.crop_max_edge < next.crop_min_edge
        {
            return Err("Invalid detection or crop settings".into());
        }
        let task = self.hub.start("Saving settings", 0);
        next.save(&self.root.join("settings.json")).map_err(err)?;
        *stored = next;
        task.finish();
        Ok(json!(*stored))
    }

    fn delete_job(&self, job: i64) -> Result<Value> {
        if lock(&self.active).as_ref().is_some_and(|(j, s)| {
            *j == job && !s.is_finished() || self.finishing.load(Ordering::Acquire)
        }) {
            return Err("Stop the scan before removing its album".into());
        }
        let task = self.hub.start("Removing album from library", 0);
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute("DELETE FROM jobs WHERE id=?", [job])
            .map_err(err)?;
        task.finish();
        Ok(Value::Null)
    }

    fn save_known(&self, a: &KnownArgs) -> Result<Value> {
        let plate = a.plate.trim();
        if plate.is_empty() {
            return Err("Plate is required".into());
        }
        let db = lock(&self.db);
        if let Some(ref old) = a.old_plate {
            let old = old.trim();
            if !old.is_empty() && !old.eq_ignore_ascii_case(plate) {
                db.execute(
                    "DELETE FROM known_vehicles WHERE plate=?",
                    [old.to_uppercase()],
                )
                .map_err(err)?;
            }
        }
        db.execute("INSERT INTO known_vehicles(plate,make,model,colour,team,race_number,driver,country,updated_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(plate) DO UPDATE SET make=excluded.make,model=excluded.model,colour=excluded.colour,team=excluded.team,race_number=excluded.race_number,driver=excluded.driver,country=excluded.country,updated_at=excluded.updated_at",params![plate.to_uppercase(),a.make,a.model,a.colour,a.team,a.race_number,a.driver,a.country,now()]).map_err(err)?;
        Ok(Value::Null)
    }

    fn delete_known(&self, plate: &str) -> Result<Value> {
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute("DELETE FROM known_vehicles WHERE plate=?", [plate])
            .map_err(err)?;
        Ok(Value::Null)
    }

    fn delete_all_known(&self) -> Result<Value> {
        let removed = self
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute("DELETE FROM known_vehicles", [])
            .map_err(err)?;
        Ok(json!({"removed": removed}))
    }

    fn models(&self) -> Value {
        crate::setup::rows()
    }

    /// The worker count of every pool, its cap and whether a change applies
    /// live. No pool can be resized mid-scan (each is a scoped thread set), so
    /// every `live` is false and the UI says "applies next scan".
    pub fn worker_limits(&self) -> Value {
        let settings = lock(&self.settings);
        let caps = conrod_core::settings::worker_caps();
        let pool = |current: i64, max: i64, default: i64| {
            json!({"current": current, "min": 1, "max": max, "default": default, "live": false})
        };
        json!({
            "cores": caps.cores,
            "cull": pool(settings.cull_workers(), caps.cull, (caps.cores - 2).max(2).min(caps.cull)),
            "detect": pool(
                conrod_core::settings::clamp_workers(settings.detect_workers, caps.detect, 1),
                caps.detect,
                1,
            ),
            "preview": pool(
                conrod_core::settings::clamp_workers(settings.preview_workers, caps.preview, 4),
                caps.preview,
                4,
            ),
            "identify": pool(settings.identify_worker_count(), caps.identify, 3),
            "note": "Worker counts apply to the next scan; a running pool cannot be resized.",
        })
    }

    pub fn status(&self) -> Value {
        let tasks: Vec<_> = self.hub.snapshot().iter().map(|t| json!({"id":t.id,"label":t.label,"detail":t.detail,"state":format!("{:?}",t.state).to_lowercase(),"done":t.done,"total":t.total,"elapsed":t.elapsed.as_secs_f64(),"eta":t.eta.map(|d|d.as_secs_f64()),"error":t.error})).collect();
        let active = lock(&self.active);
        let operations: Vec<_> = lock(&self.operations).keys().cloned().collect();
        json!({"revision":self.hub.version(),"tasks":tasks,"log":self.hub.log(),"operations":operations,"activeJob":active.as_ref().filter(|(_,s)|!s.is_finished()).map(|(j,_)|*j)})
    }

    fn jobs(&self) -> Result<Vec<Value>> {
        let fallback_gap = lock(&self.settings).burst_gap;
        let mut jobs = rows(&lock(&self.reader), "SELECT j.*, (SELECT count(*) FROM images i WHERE i.job_id=j.id) AS total, (SELECT count(*) FROM images i WHERE i.job_id=j.id AND i.status='done') AS done FROM jobs j ORDER BY j.id DESC", [])?;
        for job in &mut jobs {
            if job["label"].as_str().is_none_or(|s| s.trim().is_empty()) {
                job["label"] = json!(Path::new(job["root"].as_str().unwrap_or_default())
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy());
            }
            // The album's own burst gap, or the global default when it has none.
            job["burst_gap"] = json!(conrod_core::settings::burst_gap_or_default(fallback_gap));
            if let Some(sj) = job.get("settings_json").and_then(|v| v.as_str()) {
                if let Ok(mut val) = serde_json::from_str::<Map<String, Value>>(sj) {
                    // An album stored before per-album gaps means the old
                    // effective default, not its ignored 4.0 sentinel.
                    conrod_core::settings::migrate_burst_gap(&mut val);
                    if let Some(profile) = val.get("scan_profile").and_then(|p| p.as_str()) {
                        job["scan_profile"] = json!(profile);
                    }
                    job["include_people"] = json!(val
                        .get("include_people")
                        .and_then(Value::as_bool)
                        .unwrap_or(false));
                    if let Some(gap) = val.get("burst_gap").and_then(Value::as_f64) {
                        job["burst_gap"] = json!(conrod_core::settings::burst_gap_or_default(gap));
                    }
                }
            }
        }
        Ok(jobs)
    }

    fn review(&self, job: i64) -> Result<Value> {
        review_payload(&lock(&self.reader), job)
    }

    /// What the identify pass has read in this album: numbers, plates and the
    /// vision model's attributes, newest work first. A light query for the
    /// identifications list, not the whole review payload.
    fn identified(&self, job: i64) -> Result<Value> {
        const CAP: i64 = 400;
        let db = lock(&self.reader);
        let read = "COALESCE(d.number,'')<>'' OR COALESCE(d.plate,'')<>'' OR COALESCE(d.attributes,'') NOT IN ('','{}','null')";
        let count = |extra: &str| -> Result<i64> {
            let sql = format!(
                "SELECT count(*) FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?1 AND ({read}) {extra}"
            );
            let found = rows(&db, &sql, [job])?;
            Ok(found
                .first()
                .and_then(Value::as_object)
                .and_then(|obj| obj.values().next())
                .and_then(Value::as_i64)
                .unwrap_or(0))
        };
        let total = count("")?;
        let unreviewed = count("AND COALESCE(d.reviewed,0)=0")?;
        let sql = format!(
            "SELECT d.id,d.image_id,d.cls,d.region_type,d.crop_path,d.number,d.plate,d.number_conf,d.plate_conf,
                    d.attributes,d.quality,d.reviewed,d.rejected,d.group_key,
                    i.path AS frame_path,i.thumb_path AS frame_thumb
             FROM detections d JOIN images i ON i.id=d.image_id
             WHERE i.job_id=?1 AND ({read})
             ORDER BY COALESCE(d.reviewed,0) ASC,
                      (COALESCE(d.number,'')='' AND COALESCE(d.plate,'')='') ASC,
                      d.image_id, d.id LIMIT {CAP}"
        );
        let rows = rows_compact(&db, &sql, [job])?;
        Ok(json!({
            "rows": rows,
            "total": total,
            "unreviewed": unreviewed,
            "shown": CAP.min(total),
        }))
    }

    fn mark(&self, a: &MarkArgs) -> Result<Value> {
        if let Some(Some(stars)) = a.stars {
            if !(0..=5).contains(&stars) {
                return Err("Stars must be 0–5 or null".into());
            }
        }
        let db = lock(&self.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        if let Some(stars) = a.stars {
            tx.execute(
                "UPDATE images SET stars=? WHERE id=?",
                params![stars, a.image_id],
            )
            .map_err(err)?;
            tx.execute(
                "UPDATE detections SET stars=? WHERE image_id=?",
                params![stars, a.image_id],
            )
            .map_err(err)?;
        }
        if let Some(rejected) = a.rejected {
            tx.execute(
                "UPDATE images SET rejected=?,picked=CASE WHEN ?1=0 THEN picked ELSE 0 END WHERE id=?2",
                params![rejected, a.image_id],
            )
            .map_err(err)?;
            tx.execute(
                "UPDATE detections SET rejected=?,cull_reason=CASE WHEN ?1=0 THEN NULL ELSE cull_reason END WHERE image_id=?2",
                params![rejected, a.image_id],
            )
            .map_err(err)?;
        }
        if let Some(picked) = a.picked {
            tx.execute(
                "UPDATE images SET picked=?,rejected=CASE WHEN ?1=0 THEN rejected ELSE 0 END WHERE id=?2",
                params![picked, a.image_id],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(Value::Null)
    }

    fn start_scan(self: &Arc<Self>, a: &ScanArgs) -> Result<Value> {
        use crate::commands::ScanStage;
        if a.stage == ScanStage::Identify {
            return crate::operations::identify(
                self,
                a.job_id.ok_or("Identify requires an existing album")?,
            );
        }
        let mut active = lock(&self.active);
        if active
            .as_ref()
            .is_some_and(|(_, s)| !s.is_finished() || self.finishing.load(Ordering::Acquire))
        {
            return Err("A scan is already running".into());
        }
        let mut settings = lock(&self.settings).clone();
        let task = self.hub.start("Indexing album", 0);
        let (job, paths) = if let Some(job) = a.job_id {
            let db = lock(&self.db);
            let stored: String = db
                .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
                    r.get(0)
                })
                .map_err(err)?;
            if let Ok(map) = serde_json::from_str::<Map<String, Value>>(&stored) {
                settings = settings.apply(&map);
            }
            let paths = rows(
                &db,
                "SELECT path FROM images WHERE job_id=? AND status!='done' AND COALESCE(rejected,0)=0",
                [job],
            )?
            .iter()
            .filter_map(|v| v["path"].as_str().map(PathBuf::from))
            .collect();
            (job, paths)
        } else {
            // dunce: plain `canonicalize` returns a `\\?\` verbatim path on
            // Windows, which would be stored and then no longer match the
            // paths Python and exiftool use.
            let root = dunce::canonicalize(a.root.as_deref().ok_or("Choose a photo folder")?)
                .map_err(err)?;
            if !root.is_dir() {
                return Err("Choose a photo folder".into());
            }
            // The sheet's choices belong to the album being created, not to the
            // global defaults: they are written once, into its settings_json.
            let profile = a
                .scan_profile
                .as_deref()
                .or(a.profile.as_deref())
                .unwrap_or(&settings.scan_profile);
            settings.scan_profile = ShootPreset::parse(profile).name().into();
            if let Some(read) = a.read_plates {
                settings.read_plates = read;
            }
            if let Some(read) = a.read_numbers {
                settings.read_numbers = read;
            }
            if let Some(people) = a.include_people {
                settings.include_people = people;
            }
            if let Some(strictness) = a.blur_strictness.as_deref() {
                if !["strict", "moderate", "lenient"].contains(&strictness) {
                    return Err("Strictness must be strict, moderate or lenient".into());
                }
                settings.blur_strictness = strictness.to_string();
                settings.blur_explicit = true;
            }
            if let Some(gap) = a.burst_gap {
                if !(gap.is_finite() && gap > 0.0) {
                    return Err("Burst gap must be a positive number of seconds".into());
                }
                settings.burst_gap = gap;
            }
            settings.extra.insert(
                "native_scan_stage".into(),
                json!(match a.stage {
                    ScanStage::Index => "index",
                    ScanStage::All => "all",
                    _ => "cull",
                }),
            );
            let paths = crate::files_with_recursion(&root, a.recursive);
            if paths.is_empty() {
                return Err("No CR2, CR3 or JPEG photos found in that folder".into());
            }
            let db = lock(&self.db);
            let tx = db.unchecked_transaction().map_err(err)?;
            let job = conrod_store::create_job(&tx, &root, a.label.as_deref(), &json!(settings))
                .map_err(err)?;
            conrod_store::add_images(&tx, job, &paths).map_err(err)?;
            tx.commit().map_err(err)?;
            (job, paths)
        };
        if a.stage == ScanStage::Index {
            self.db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .execute("UPDATE jobs SET status='indexed' WHERE id=?", [job])
                .map_err(err)?;
            task.finish();
            return Ok(json!({"jobId": job}));
        }
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute("UPDATE jobs SET status='scanning' WHERE id=?", [job])
            .map_err(err)?;
        task.finish();
        let desktop = self.clone();
        let eligibility = self.clone();
        // One progress object per scan: the pools write to it and the app
        // emits its snapshot as `scan-progress`.
        let progress = crate::progress::for_scan(
            job,
            &settings,
            ShootPreset::parse(&settings.scan_profile).wants_faces(settings.include_people),
        );
        *lock(&self.progress) = Some(progress.clone());
        let scan = Arc::new(crate::scan_files_filtered(
            paths,
            settings.clone(),
            ScanProfile::parse(&settings.scan_profile),
            self.hub.clone(),
            Some(progress.clone()),
            move |path| {
                eligibility
                    .reader
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .query_row(
                        "SELECT COALESCE(rejected,0)=0 FROM images WHERE job_id=? AND path=?",
                        params![job, path.to_string_lossy()],
                        |r| r.get::<_, bool>(0),
                    )
                    .unwrap_or(false)
            },
            move |result| match result {
                Ok(frame) => {
                    if let Err(e) = desktop.persist_frame(job, &frame, &settings) {
                        desktop.hub.start("Saving scan result", 0).fail(e);
                    }
                }
                Err((path, e)) => {
                    let _ = lock(&desktop.db).execute(
                        "UPDATE images SET status='error',error=? WHERE job_id=? AND path=?",
                        params![e, job, path.to_string_lossy()],
                    );
                }
            },
        ));
        self.finishing.store(true, Ordering::Release);
        *active = Some((job, scan.clone()));
        let desktop = self.clone();
        // A one-shot scan runs every stage to the end: bursts and keepers, then
        // identification when the album's preset reads numbers or plates.
        let one_shot = a.stage == ScanStage::All;
        std::thread::spawn(move || {
            // ponytail: polls an AtomicBool rather than joining a handle
            // because `scan` has none to join -- its workers report through
            // `finished`/`error`, not a thread we own here. A Condvar paired
            // with that flag would remove the poll if the wakeup latency
            // ever matters.
            while !scan.is_finished() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let stopped = scan.stop.load(Ordering::Relaxed);
            let status = if stopped {
                "stopped"
            } else if lock(&scan.error).is_some() {
                "error"
            } else {
                "done"
            };
            let mut final_status = status;
            if !stopped {
                if let Some(p) = &desktop.progress() {
                    p.phase("group");
                }
                if let Err(e) = desktop.regroup(job) {
                    desktop.hub.start("Grouping album", 0).fail(e);
                    final_status = "error";
                }
                if let Some(p) = &desktop.progress() {
                    p.phase("pick");
                }
                // Identify when this shoot's presets read numbers or plates:
                // identification is the slow lane, so it runs last and the
                // scan is not "done" until it is.
                let wants_identify = one_shot
                    && final_status == "done"
                    && crate::operations::settings(&desktop, job).is_ok_and(|cfg| {
                        ScanProfile::parse(&cfg.scan_profile).identifies_vehicles()
                            && (cfg.read_plates || cfg.read_numbers)
                    });
                if wants_identify {
                    if let Err(e) = crate::operations::identify(&desktop, job) {
                        desktop.hub.start("Identification", 0).fail(e);
                    } else {
                        // `identify` runs as its own operation; hold the scan
                        // open until it has finished, so Review waits for the
                        // whole pipeline rather than opening half-identified.
                        let key = format!("Identifying:{job}");
                        while lock(&desktop.operations).contains_key(&key) {
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                }
                // The learned taste order comes last, after the pick it sits
                // beside: without a head file it is skipped without a word.
                if final_status == "done" {
                    if let Some(p) = &desktop.progress() {
                        p.phase("pick");
                    }
                    crate::taste::after_scan(&desktop, job);
                }
            }
            let db = lock(&desktop.db);
            let incomplete: i64 = db
                .query_row(
                    "SELECT count(*) FROM images WHERE job_id=? AND status!='done' AND COALESCE(rejected,0)=0",
                    [job],
                    |r| r.get(0),
                )
                .unwrap_or(1);
            if !stopped && final_status == "done" && incomplete > 0 {
                final_status = "error";
            }
            let _ = db.execute(
                "UPDATE jobs SET status=? WHERE id=?",
                params![final_status, job],
            );
            drop(db);
            if let Some(progress) = desktop.progress() {
                progress.finish(final_status);
            }
            desktop.finishing.store(false, Ordering::Release);
            desktop.hub.note(if stopped {
                "Scan stopped"
            } else {
                "Album processing updated"
            });
        });
        Ok(json!({"jobId":job}))
    }

    fn persist_frame(&self, job: i64, f: &FrameResult, settings: &Settings) -> Result<()> {
        let image: i64 = self
            .db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .query_row(
                "SELECT id FROM images WHERE job_id=? AND path=?",
                params![job, f.path.to_string_lossy()],
                |r| r.get(0),
            )
            .map_err(err)?;
        // Encoding and writing the thumbnail is the slow part; do it before taking
        // the write lock, so sixteen workers do not queue behind each other for it.
        let thumb = self.root.join(format!("cache/native/thumb-{image}.jpg"));
        save_jpeg(&f.thumb, &thumb)?;
        let db = lock(&self.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        tx.execute("DELETE FROM detections WHERE image_id=?", [image])
            .map_err(err)?;
        for s in &f.subjects {
            insert_subject(&tx, image, s)?;
        }
        let rating = f.rating;
        tx.execute("UPDATE images SET status='done',error=NULL,width=?,height=?,camera=?,taken_at=?,sharpness=?,rating=?,thumb_path=?,exposure_verdict=?,highlight_clip=?,shadow_clip=?,dark_frame=?,tilt_deg=?,converge_deg=?,level_verdict=?,eye_open_min=?,smile_max=?,buck_score=?,exposure_time=?,exposure_bias=?,f_number=?,focal_length=?,focus_distance=? WHERE id=?",params![f.size.0 as i64,f.size.1 as i64,f.camera,f.taken,f.whole,rating,thumb.to_string_lossy(),f.exposure.as_ref().map(|e| e.verdict),f.exposure.as_ref().map(|e| e.highlight_clip),f.exposure.as_ref().map(|e| e.shadow_clip),f.exposure.as_ref().map(|e| e.dark_frame),f.level.as_ref().and_then(|l| l.tilt_deg),f.level.as_ref().and_then(|l| l.verticals_converge_deg),f.level.as_ref().map(|l| l.verdict),f.eye_open_min,f.smile_max,f.buck,f.exif.exposure_time,f.exif.exposure_bias,f.exif.f_number,f.exif.focal_length,f.exif.focus_distance,image]).map_err(err)?;
        tx.commit().map_err(err)?;
        self.report_frame(image, &thumb, f, settings);
        Ok(())
    }

    /// Tell the scan screen what this frame was: its verdict, its main subject
    /// and the running found counts.
    fn report_frame(&self, image: i64, thumb: &Path, f: &FrameResult, settings: &Settings) {
        let Some(progress) = self.progress() else {
            return;
        };
        // A pan is an extra flag, never a replacement for the sharpness verdict:
        // a panned car can still come out soft, and the strip must say which.
        let panning = f.subjects.iter().any(|subject| subject.panning);
        let verdict = match conrod_vision::sharpness::verdict_for(
            f.rating,
            settings.sharp_at,
            settings.blurred_below,
        ) {
            "sharp" => "sharp",
            _ => "soft",
        };
        let subject = f
            .subjects
            .iter()
            .max_by(|a, b| {
                let area = |s: &crate::Subject| (s.bbox[2] - s.bbox[0]) * (s.bbox[3] - s.bbox[1]);
                area(a).total_cmp(&area(b))
            })
            .map_or("frame", |s| s.region_type);
        let soft = verdict == "soft";
        let closed = f
            .subjects
            .iter()
            .filter(|s| s.cull_reason == crate::EYES_CLOSED)
            .count();
        progress.add_found(|found| {
            found.subjects += f.subjects.len() as u64;
            found.soft += u64::from(soft);
            found.closed_eyes += closed as u64;
        });
        progress.recent(conrod_core::progress::RecentFrame {
            image_id: image,
            thumb_url: thumb.to_string_lossy().into_owned(),
            subject: subject.to_string(),
            verdict: verdict.to_string(),
            panning,
            label: None,
        });
    }

    /// Group the album's bursts and moments again, in place: no pixels are
    /// read, and nothing a person decided (picked, rejected, stars) is
    /// touched. The keeper of each pass is then chosen again by the same rule
    /// the identify stage uses, so a regroup leaves exactly the keeper a later
    /// pick would choose.
    ///
    /// Returns `{bursts, moments, keepers}`: the counts the UI shows after a
    /// burst-gap or preset change.
    pub(crate) fn regroup(&self, job: i64) -> Result<Value> {
        let task = self.hub.start("Grouping bursts", 0);
        // Settings before the write lock: reading them takes it too.
        let cfg = crate::operations::settings(self, job)?;
        let profile = ScanProfile::parse(&cfg.scan_profile);
        let gap = burst_gap(cfg.burst_gap);
        let db = lock(&self.db);
        let images = rows(
            &db,
            "SELECT id,path,camera,taken_at,rating,width,height,exposure_time,exposure_bias,f_number,focal_length,focus_distance FROM images WHERE job_id=? AND status='done' ORDER BY path",
            [job],
        )?;
        // Keep-together sets from the EXIF: a bracket is meant to hold a dark
        // and a bright frame, so its frames are one unit and are never culled
        // for exposure or blur individually.
        let sets = keep_sets(&images);
        let mut frames: Vec<_> = images
            .iter()
            .map(|v| conrod_core::bursts::Frame {
                path: v["path"].as_str().unwrap_or_default().into(),
                camera: v["camera"].as_str().unwrap_or_default().into(),
                taken: v["taken_at"].as_f64(),
                burst: 0,
            })
            .collect();
        conrod_core::bursts::assign_bursts(&mut frames, gap);
        // Each image's largest subject, as a box: pixels until normalised below.
        let mut main: std::collections::HashMap<i64, (f64, [f64; 4])> =
            std::collections::HashMap::new();
        for d in rows(
            &db,
            "SELECT d.image_id AS image,d.x1,d.y1,d.x2,d.y2 FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?",
            [job],
        )? {
            let bbox = ["x1", "y1", "x2", "y2"].map(|k| d[k].as_f64().unwrap_or_default());
            let area = (bbox[2] - bbox[0]) * (bbox[3] - bbox[1]);
            let image = d["image"].as_i64().unwrap_or_default();
            if main.get(&image).is_none_or(|&(seen, _)| area > seen) {
                main.insert(image, (area, bbox));
            }
        }
        let mut order: Vec<usize> = (0..frames.len()).collect();
        order.sort_by(|&a, &b| {
            frames[a]
                .taken
                .unwrap_or_default()
                .total_cmp(&frames[b].taken.unwrap_or_default())
                .then_with(|| frames[a].path.cmp(&frames[b].path))
        });
        let moment_frames: Vec<_> = order
            .iter()
            .map(|&i| {
                let (w, h) = (images[i]["width"].as_f64(), images[i]["height"].as_f64());
                let id = images[i]["id"].as_i64().unwrap_or_default();
                let subject = match (main.get(&id), w, h) {
                    (Some(&(_, [x1, y1, x2, y2])), Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                        Some([x1 / w, y1 / h, x2 / w, y2 / h])
                    }
                    _ => None,
                };
                conrod_core::bursts::MomentFrame {
                    burst: i64::from(frames[i].burst),
                    taken: frames[i].taken,
                    subject,
                }
            })
            .collect();
        let mut key = vec![0; frames.len()];
        for (&i, moment) in order
            .iter()
            .zip(conrod_core::bursts::split_moments(&moment_frames))
        {
            key[i] = moment;
        }
        // A bracket, focus stack or interval is one stack in every preset: the
        // frames were shot to be judged together, whatever the shoot type.
        for (i, set) in sets.iter().enumerate() {
            if let Some((first, _)) = set {
                key[i] = SET_KEY_BASE + first;
            }
        }
        let bursts = frames
            .iter()
            .map(|frame| frame.burst)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let moments = key.iter().collect::<std::collections::BTreeSet<_>>().len();
        let tx = db.unchecked_transaction().map_err(err)?;
        for (i, frame) in frames.iter().enumerate() {
            let set = sets[i];
            tx.execute(
                "UPDATE images SET burst_key=?,set_id=?,set_kind=? WHERE job_id=? AND path=?",
                params![
                    key[i],
                    set.map(|(id, _)| id),
                    set.map(|(_, kind)| set_kind_name(kind)),
                    job,
                    frame.path
                ],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        // The same rule the identify stage uses, so the keeper regroup leaves
        // is the one a later pick would choose anyway.
        let picks = crate::passes::pick_of_pass(
            &db,
            job,
            profile,
            cfg.include_people,
            cfg.use_quality_score,
        )?;
        let keepers = picks["picks"].as_u64().unwrap_or_default();
        task.detail(format!("{bursts} bursts · {moments} moments · {keepers} keepers"));
        if let Some(progress) = self.progress() {
            progress.add_found(|found| {
                found.bursts = bursts as u64;
                found.keepers = keepers;
            });
        }
        task.finish();
        Ok(json!({"bursts": bursts, "moments": moments, "keepers": keepers}))
    }

    fn preview(&self, image: i64) -> Result<Value> {
        let output = self.root.join(format!("cache/native/view-{image}.jpg"));
        if !output.is_file() {
            let task = self.hub.start("Loading preview", 0);
            let path: String = self
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .query_row("SELECT path FROM images WHERE id=?", [image], |r| r.get(0))
                .map_err(err)?;
            let raw = conrod_io::raw::read(Path::new(&path))?;
            let rgb = Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation);
            save_jpeg(&rgb, &output)?;
            task.finish();
        }
        Ok(json!(output))
    }
}

/// Store one measured subject of a frame.
/// A face's `attributes`: OCEC's verdict, which re-measuring keeps (see
/// `rescore::write`). `None` for anything not judged.
fn eyes_json(open: [Option<f32>; 2]) -> Option<String> {
    open.iter()
        .any(Option::is_some)
        .then(|| json!({"eyes_open": open, "eyes_closed": crate::eyes_closed(open)}).to_string())
}

pub(crate) fn insert_subject(
    db: &rusqlite::Connection,
    image: i64,
    s: &crate::Subject,
) -> Result<()> {
    db.execute("INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,sharpness,rating,rating_verdict,panning,sharp_end,cull_reason,features,heuristic,region_type,quality,attributes,cut_sides,subject_frac,lead_room,framing_score) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",params![image,s.bbox[0],s.bbox[1],s.bbox[2],s.bbox[3],s.class,s.conf,s.sharpness,s.rating,conrod_vision::sharpness::rating_for(s.rating,0.825,0.606),s.panning,s.sharp_end,s.cull_reason,serde_json::to_string(&s.features).map_err(err)?,s.heuristic,s.region_type,s.quality,eyes_json(s.eyes_open),s.cut_sides,s.subject_frac,s.lead_room,s.framing_score]).map_err(err)?;
    Ok(())
}

pub fn save_jpeg(rgb: &Rgb, path: &Path) -> Result<()> {
    let file = std::fs::File::create(path).map_err(err)?;
    image::codecs::jpeg::JpegEncoder::new_with_quality(std::io::BufWriter::new(file), 90)
        .encode(
            &rgb.data,
            rgb.width as u32,
            rgb.height as u32,
            image::ExtendedColorType::Rgb8,
        )
        .map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gap_setting_honours_an_album_value_and_falls_back_to_the_one_default() {
        assert_eq!(
            burst_gap(Settings::default().burst_gap),
            conrod_core::bursts::DEFAULT_BURST_GAP_SECONDS,
            "the stored default is the one documented default"
        );
        assert_eq!(
            burst_gap(conrod_core::bursts::DEFAULT_BURST_GAP_SECONDS),
            1.5
        );
        for nonsense in [0.0, -2.5, f64::NAN] {
            assert_eq!(burst_gap(nonsense), 1.5, "setting {nonsense} falls back");
        }
        assert_eq!(burst_gap(2.5), 2.5, "a user's gap is honoured");
        assert_eq!(burst_gap(4.0), 4.0, "4.0 is no longer a sentinel");
    }

    #[test]
    fn an_exposure_bracket_becomes_one_keep_set_and_is_never_culled_alone() {
        use crate::testkit::Lib;
        let lib = Lib::new("bracket-set");
        // A landscape album: keep-sets, deliver-most, level.
        lib.run(
            "update_job_settings",
            json!({"jobId": lib.job, "patch": {"scan_profile": "landscape"}}),
        )
        .unwrap();
        // Four frames on one body, half a second apart, two stops apart: a
        // bracket, not a burst.
        let mut images = Vec::new();
        for (i, shutter) in [1.0 / 100.0, 1.0 / 50.0, 1.0 / 25.0, 1.0 / 12.5]
            .into_iter()
            .enumerate()
        {
            let image = lib.frame(&format!("{i}.jpg"), None);
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=?,exposure_time=?,f_number=8.0,focal_length=24.0 WHERE id=?",
                rusqlite::params![f64::from(i as u32) * 0.5, shutter, image],
            );
            lib.detection(image, "vehicle", 0.5);
            images.push(image);
        }
        let counts = lib.d().regroup(lib.job).unwrap();
        assert_eq!(counts["bursts"], 1);
        assert_eq!(counts["moments"], 1, "the bracket is one stack");
        let kind: String = lib.one(
            "SELECT set_kind FROM images WHERE id=?",
            [images[0]],
        );
        assert_eq!(kind, "bracket");
        let set_ids: i64 = lib.one(
            "SELECT count(DISTINCT set_id) FROM images WHERE job_id=? AND set_id IS NOT NULL",
            [lib.job],
        );
        assert_eq!(set_ids, 1);
        let keys: i64 = lib.one(
            "SELECT count(DISTINCT burst_key) FROM images WHERE job_id=?",
            [lib.job],
        );
        assert_eq!(keys, 1, "one key for the whole bracket");

        // A culled (say, darker) bracket frame still competes: the bracket is
        // meant to hold one frame that looks wrong on its own.
        lib.sql(
            "UPDATE detections SET rating=0.9, sharpness=0.9, cull_reason='too blurred' WHERE image_id=?",
            [images[0]],
        );
        lib.sql(
            "UPDATE detections SET rating=0.2, sharpness=0.2, cull_reason='' WHERE image_id=?",
            [images[1]],
        );
        crate::passes::pick_keepers(lib.d(), lib.job).unwrap();
        assert_eq!(
            lib.one::<i64>(
                "SELECT image_id FROM detections WHERE burst_pick=1 AND image_id=?",
                [images[0]]
            ),
            1,
            "the cull exemption lets the bracket's best frame win"
        );
    }

    #[test]
    fn regroup_in_place_keeps_manual_decisions_and_reports_counts() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-preserve");
        let (picked, rejected, starred) = (
            lib.frame("1.jpg", None),
            lib.frame("2.jpg", None),
            lib.frame("3.jpg", None),
        );
        for (taken, image) in [(0.0, picked), (0.4, rejected), (0.8, starred)] {
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![taken, image],
            );
            lib.detection(image, "vehicle", 0.6);
        }
        lib.run("mark", json!({"imageId": picked, "picked": true}))
            .unwrap();
        lib.run("mark", json!({"imageId": rejected, "rejected": true}))
            .unwrap();
        lib.run("mark", json!({"imageId": starred, "stars": 5}))
            .unwrap();

        let counts = lib.d().regroup(lib.job).unwrap();

        assert_eq!(counts["bursts"], 1, "three frames a half second apart");
        assert_eq!(counts["moments"], 1);
        assert!(counts["keepers"].as_u64().unwrap() >= 1, "{counts}");
        assert_eq!(
            lib.one::<i64>("SELECT picked FROM images WHERE id=?", [picked]),
            1,
            "a manual pick survives a regroup"
        );
        assert_eq!(
            lib.one::<i64>("SELECT rejected FROM images WHERE id=?", [rejected]),
            1,
            "a manual reject survives a regroup"
        );
        assert_eq!(
            lib.one::<Option<i64>>("SELECT stars FROM images WHERE id=?", [starred]),
            Some(5),
            "manual stars survive a regroup"
        );
        assert_eq!(
            lib.one::<Option<i64>>("SELECT burst_key FROM images WHERE id=?", [rejected]),
            Some(1),
            "every frame gets its moment key"
        );
        // The command path returns the same counts.
        let out = lib.run("regroup", json!({"jobId": lib.job})).unwrap();
        assert_eq!(out["bursts"], 1);
        assert_eq!(out["moments"], 1);
        assert!(out["keepers"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn a_new_album_takes_the_sheets_choices_without_touching_the_global_defaults() {
        let root =
            std::env::temp_dir().join(format!("conrod-scanargs-{}-{}", std::process::id(), now()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.jpg"), b"x").unwrap();
        let desktop = Desktop::open(root.clone()).unwrap();
        let before = lock(&desktop.settings).clone();

        let out = desktop
            .dispatch(
                "scan",
                json!({
                    "root": root.to_string_lossy(),
                    "label": "Sheet album",
                    "stage": "index",
                    "scanProfile": "portrait",
                    "blurStrictness": "lenient",
                    "burstGap": 2.5,
                }),
            )
            .unwrap();
        let job = out["jobId"].as_i64().unwrap();

        let stored: String = lock(&desktop.db)
            .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| r.get(0))
            .unwrap();
        let map: Map<String, Value> = serde_json::from_str(&stored).unwrap();
        assert!(
            map["scan_profile"].as_str().unwrap().starts_with("portrait"),
            "{map:?}"
        );
        assert_eq!(map["blur_strictness"], "lenient");
        assert_eq!(map["blur_explicit"], true, "a sheet choice must beat the preset's default");
        assert_eq!(map["burst_gap"], 2.5);

        let after = lock(&desktop.settings).clone();
        assert_eq!(after, before, "the sheet never writes the global defaults");

        // The album's own settings report what the sheet chose.
        let effective = crate::operations::settings(&desktop, job).unwrap();
        assert_eq!(effective.blur_strictness, "lenient");
        assert_eq!(effective.burst_gap, 2.5);

        // Nonsense is refused before an album exists.
        for args in [
            json!({"root": root.to_string_lossy(), "stage": "index", "burstGap": 0}),
            json!({"root": root.to_string_lossy(), "stage": "index", "blurStrictness": "loud"}),
        ] {
            assert!(desktop.dispatch("scan", args).is_err());
        }
        drop(desktop);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_all_known_removes_every_vehicle() {
        let root =
            std::env::temp_dir().join(format!("conrod-known-{}-{}", std::process::id(), now()));
        let desktop = Desktop::open(root.clone()).unwrap();
        {
            let db = lock(&desktop.db);
            db.execute(
                "INSERT INTO known_vehicles(plate, updated_at) VALUES('ABC123', 1), ('XYZ789', 1)",
                [],
            )
            .unwrap();
        }

        let result = desktop.dispatch("delete_all_known", json!({})).unwrap();

        assert_eq!(result["removed"], 2);
        assert_eq!(
            desktop
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .query_row("SELECT count(*) FROM known_vehicles", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(desktop);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn regroup_alone_picks_the_keeper_the_same_way_pick_of_pass_does() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-pick");
        // Two frames a half second apart: one burst, no grouping stage.
        let (f1, f2) = (lib.frame("1.jpg", None), lib.frame("2.jpg", None));
        for (taken, image) in [(0.0, f1), (0.5, f2)] {
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![taken, image],
            );
        }
        let (weak, strong) = (
            lib.detection(f1, "vehicle", 0.3),
            lib.detection(f2, "vehicle", 0.7),
        );
        // The frame rating would favour the first; the subjects favour the second.
        lib.sql("UPDATE images SET rating=0.9 WHERE id=?", [f1]);
        lib.sql("UPDATE images SET rating=0.1 WHERE id=?", [f2]);
        lib.d().regroup(lib.job).unwrap();
        assert_eq!(
            lib.one::<i64>("SELECT id FROM detections WHERE burst_pick=1", []),
            strong,
            "regroup must not leave a keeper of its own choosing"
        );
        assert_eq!(
            lib.one::<Option<i64>>("SELECT burst_pick FROM images WHERE id=?", [f1]),
            None
        );
        // A later pick by the identify rule keeps exactly that frame.
        let profile = ScanProfile::parse(&Settings::default().scan_profile);
        crate::passes::pick_of_pass(&lock(&lib.d().db), lib.job, profile, false, false).unwrap();
        assert_eq!(
            lib.one::<i64>("SELECT id FROM detections WHERE burst_pick=1", []),
            strong
        );
        let _ = weak;
    }

    #[test]
    fn regroup_splits_a_burst_whose_subject_grows_and_picks_once_per_moment() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-moments");
        // Eight frames, a second apart, in one burst; the car grows from 2% to 30% of the frame.
        for i in 0..8 {
            let image = lib.frame(&format!("{i}.jpg"), None);
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![f64::from(i) * 0.2, image],
            );
            let detection = lib.detection(image, "vehicle", 0.5);
            let scale = (0.02 + 0.04 * f64::from(i)).sqrt();
            let (w, h) = (6000.0 * scale, 4000.0 * scale);
            lib.sql(
                "UPDATE detections SET x1=?,y1=?,x2=?,y2=? WHERE id=?",
                params![
                    3000.0 - w / 2.0,
                    2000.0 - h / 2.0,
                    3000.0 + w / 2.0,
                    2000.0 + h / 2.0,
                    detection
                ],
            );
        }
        lib.d().regroup(lib.job).unwrap();
        let keys: i64 = lib.one(
            "SELECT count(DISTINCT burst_key) FROM images WHERE job_id=?",
            [lib.job],
        );
        assert!(
            keys > 1,
            "a growing subject must split the burst, got {keys} key(s)"
        );
        let picks: i64 = lib.one("SELECT count(*) FROM detections WHERE burst_pick=1", []);
        assert_eq!(picks, keys, "one pick per burst_key");
        let one_each: i64 = lib.one(
            "SELECT count(*) FROM (SELECT i.burst_key FROM detections d JOIN images i ON i.id=d.image_id WHERE d.burst_pick=1 GROUP BY i.burst_key HAVING count(*)=1)",
            [],
        );
        assert_eq!(one_each, keys, "every key gets exactly one pick");
    }

    #[test]
    fn manual_zero_and_region_label_undo_survive_reopen() {
        let root =
            std::env::temp_dir().join(format!("conrod-desktop-{}-{}", std::process::id(), now()));
        let desktop = Desktop::open(root.clone()).unwrap();
        let (job, image, detection) = {
            let db = lock(&desktop.db);
            let job =
                conrod_store::create_job(&db, &root, Some("Test"), &json!(Settings::default()))
                    .unwrap();
            conrod_store::add_images(&db, job, &[root.join("test.jpg")]).unwrap();
            let image: i64 = db
                .query_row("SELECT id FROM images WHERE job_id=?", [job], |r| r.get(0))
                .unwrap();
            db.execute("INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,features,region_type) VALUES(?,1,2,30,40,'person',0.9,?,'face')",params![image,serde_json::to_string(&vec![0.5;conrod_vision::sharpness::FEATURE_NAMES.len()]).unwrap()]).unwrap();
            (job, image, db.last_insert_rowid())
        };
        desktop
            .dispatch("mark", json!({"imageId":image,"stars":0}))
            .unwrap();
        desktop
            .dispatch("train_label", json!({"detectionId":detection,"stars":2}))
            .unwrap();
        desktop
            .dispatch("train_label", json!({"detectionId":detection,"stars":5}))
            .unwrap();
        desktop.dispatch("undo_label", json!({})).unwrap();
        assert_eq!(
            desktop
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .query_row("SELECT stars FROM native_labels", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            desktop
                .db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .query_row("SELECT count(*) FROM sharpness_labels", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(desktop);
        let reopened = Desktop::open(root.clone()).unwrap();
        let review = reopened.dispatch("review", json!({"jobId":job})).unwrap();
        assert_eq!(review["frames"][0]["manual_stars"], 0);
        reopened.dispatch("undo_label", json!({})).unwrap();
        assert_eq!(
            reopened.dispatch("training_status", json!({})).unwrap()["labels"],
            0
        );
        drop(reopened);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn picking_and_rejecting_a_frame_are_mutually_exclusive() {
        let root =
            std::env::temp_dir().join(format!("conrod-pick-{}-{}", std::process::id(), now()));
        let desktop = Desktop::open(root.clone()).unwrap();
        let (job, image) = {
            let db = lock(&desktop.db);
            let job =
                conrod_store::create_job(&db, &root, Some("Test"), &json!(Settings::default()))
                    .unwrap();
            conrod_store::add_images(&db, job, &[root.join("test.jpg")]).unwrap();
            let image: i64 = db
                .query_row("SELECT id FROM images WHERE job_id=?", [job], |r| r.get(0))
                .unwrap();
            (job, image)
        };

        desktop
            .dispatch("mark", json!({"imageId":image,"picked":true}))
            .unwrap();
        let review = desktop.dispatch("review", json!({"jobId":job})).unwrap();
        assert_eq!(review["frames"][0]["picked"], 1);
        assert_eq!(review["frames"][0]["rejected"], 0);

        desktop
            .dispatch("mark", json!({"imageId":image,"rejected":true}))
            .unwrap();
        let review = desktop.dispatch("review", json!({"jobId":job})).unwrap();
        assert_eq!(review["frames"][0]["rejected"], 1);
        assert_eq!(review["frames"][0]["picked"], 0, "rejecting clears a pick");

        drop(desktop);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The real album the user opens (default `~/.conrod/conrod.db`, job 106:
    /// 3,061 frames and 41,779 detections), read-only. Measures the SQL read,
    /// the payload build and the JSON encode -- the three parts between the
    /// click and the grid.
    ///
    /// `cargo test -p conrod-engine --release review_payload_speed -- --ignored --nocapture`
    /// (override with `CONROD_REVIEW_DB` / `CONROD_REVIEW_JOB`).
    #[test]
    #[ignore]
    fn review_payload_speed() {
        use rusqlite::OpenFlags;
        use std::time::Instant;
        let path = std::env::var("CONROD_REVIEW_DB")
            .map(PathBuf::from)
            .unwrap_or_else(|_| conrod_store::db_path());
        if !path.exists() {
            eprintln!("no album database at {}; skipping", path.display());
            return;
        }
        let job: i64 = std::env::var("CONROD_REVIEW_JOB")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(106);
        let db = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open the album read-only");
        const FRAME_SQL: &str = "SELECT i.id,i.path,i.status,i.thumb_path,i.preview_path,i.width,i.height,i.rating,i.rejected,i.picked,i.burst_key,i.dupe_key,i.error, COALESCE(i.stars,(SELECT max(d.stars) FROM detections d WHERE d.image_id=i.id)) AS manual_stars, COALESCE((SELECT max(d.burst_pick) FROM detections d WHERE d.image_id=i.id AND d.burst_pick=1), i.burst_pick) AS burst_pick, i.set_id,i.set_kind,i.exposure_verdict,i.highlight_clip,i.shadow_clip,i.dark_frame,i.tilt_deg,i.converge_deg,i.level_verdict,i.eye_open_min,i.smile_max,i.buck_score FROM images i WHERE job_id=? ORDER BY path";
        const DET_SQL: &str = "SELECT d.id,d.image_id,d.cls,d.crop_path,d.x1,d.y1,d.x2,d.y2,d.sharpness,d.panning,d.number,d.plate,d.cull_reason,d.attributes,d.burst_pick,d.region_type,d.reviewed,d.rejected,d.bystander,d.stars,d.group_key,d.group_size,d.group_agreement,d.embedding,d.rating,d.rating_verdict,d.sharp_end,d.quality,d.cut_sides,d.subject_frac,d.lead_room,d.framing_score FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? ORDER BY d.image_id,d.id";

        let mut sql = f64::MAX;
        let mut payload = f64::MAX;
        let mut encode = f64::MAX;
        let mut bytes = 0;
        let mut counted = (0, 0);
        for _ in 0..3 {
            let t = Instant::now();
            let frames = rows(&db, FRAME_SQL, [job]).unwrap();
            let detections = rows(&db, DET_SQL, [job]).unwrap();
            sql = sql.min(t.elapsed().as_secs_f64());
            counted = (frames.len(), detections.len());
            drop(frames);

            let t = Instant::now();
            let value = review_payload(&db, job).unwrap();
            payload = payload.min(t.elapsed().as_secs_f64());

            let t = Instant::now();
            bytes = serde_json::to_string(&value).unwrap().len();
            encode = encode.min(t.elapsed().as_secs_f64());
        }
        println!(
            "review job {job}: {} frames, {} detections; sql {sql:.3} s, payload {payload:.3} s, json {encode:.3} s, {} MB",
            counted.0,
            counted.1,
            bytes / (1024 * 1024)
        );
        assert!(
            payload < 1.0,
            "the review payload takes {payload:.2} s (target under 1 s)"
        );
    }
}
