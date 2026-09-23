//! Application operations shared by Tauri and the CLI. Database writes and
//! scans stay here; the frontend receives data, never arbitrary SQL or paths.
use crate::commands::{Command, KnownArgs, MarkArgs, ScanArgs};
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
            operations: Mutex::new(std::collections::BTreeMap::new()),
        });
        crate::watching::restore(&desktop);
        Ok(desktop)
    }

    pub fn request_quit(&self) {
        self.quit.store(true, Ordering::Release);
    }
    pub fn quit_requested(&self) -> bool {
        self.quit.load(Ordering::Acquire)
    }
    /// Whether closing the window should leave Conrod running in the tray.
    pub fn close_to_tray(&self) -> bool {
        self.settings.lock().unwrap().close_to_tray
    }
    pub fn scanning(&self) -> bool {
        self.active
            .lock()
            .unwrap()
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
                json!({"settings": *self.settings.lock().unwrap(), "jobs": self.jobs()?, "models": self.models(), "status": self.status()}),
            ),
            Command::Health {} => {
                let settings = self.settings.lock().unwrap().clone();
                let mut models = self.models();
                models
                    .as_array_mut()
                    .unwrap()
                    .push(conrod_io::health::vision(
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
            Command::Write(a) => {
                crate::operations::write(self, a.job_id, a.dry_run, a.embed_in_raw)
            }
            Command::CancelOperation(k) => {
                if let Some(flag) = self.operations.lock().unwrap().get(&k.key) {
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
                &self.reader.lock().unwrap(),
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
            Command::PickKeepers(a) => crate::passes::pick_keepers(self, a.job_id),
            Command::Group(a) => crate::passes::group(self, a.job_id),
            Command::Regroup(a) => {
                let gap = self.settings.lock().unwrap().burst_gap;
                let _ = self.regroup(a.job_id, gap);
                crate::passes::group(self, a.job_id)
            }
            Command::BulkEdit(a) => crate::edits::bulk_edit(self, &a),
            Command::RenameJob(a) => crate::library::rename_job(self, &a),
            Command::UpdateJobSettings(a) => crate::library::update_job_settings(self, &a),
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
            Command::CheckUpdate {} => Ok(crate::updating::check()),
            Command::InstallUpdate {} => crate::updating::install(self),
            Command::WatchStatus {} => Ok(crate::watching::status(self)),
            Command::SetWatch(a) => crate::watching::set(self, &a),
        }
    }

    /// Pause, resume or stop whatever scan is running.
    fn control(&self, act: impl FnOnce(&Scan)) -> Result<Value> {
        if let Some((_, scan)) = self.active.lock().unwrap().as_ref() {
            act(scan);
        }
        Ok(self.status())
    }

    fn save_settings(&self, updates: &Map<String, Value>) -> Result<Value> {
        let mut stored = self.settings.lock().unwrap();
        let next = stored.clone().apply(updates);
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
        if self.active.lock().unwrap().as_ref().is_some_and(|(j, s)| {
            *j == job && !s.is_finished() || self.finishing.load(Ordering::Acquire)
        }) {
            return Err("Stop the scan before removing its album".into());
        }
        let task = self.hub.start("Removing album from library", 0);
        self.db
            .lock()
            .unwrap()
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
        let db = self.db.lock().unwrap();
        if let Some(ref old) = a.old_plate {
            let old = old.trim();
            if !old.is_empty() && !old.eq_ignore_ascii_case(plate) {
                db.execute("DELETE FROM known_vehicles WHERE plate=?", [old.to_uppercase()]).map_err(err)?;
            }
        }
        db.execute("INSERT INTO known_vehicles(plate,make,model,colour,team,race_number,driver,country,updated_at) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(plate) DO UPDATE SET make=excluded.make,model=excluded.model,colour=excluded.colour,team=excluded.team,race_number=excluded.race_number,driver=excluded.driver,country=excluded.country,updated_at=excluded.updated_at",params![plate.to_uppercase(),a.make,a.model,a.colour,a.team,a.race_number,a.driver,a.country,now()]).map_err(err)?;
        Ok(Value::Null)
    }

    fn delete_known(&self, plate: &str) -> Result<Value> {
        self.db
            .lock()
            .unwrap()
            .execute("DELETE FROM known_vehicles WHERE plate=?", [plate])
            .map_err(err)?;
        Ok(Value::Null)
    }

    fn delete_all_known(&self) -> Result<Value> {
        let removed = self
            .db
            .lock()
            .unwrap()
            .execute("DELETE FROM known_vehicles", [])
            .map_err(err)?;
        Ok(json!({"removed": removed}))
    }

    fn models(&self) -> Value {
        crate::setup::rows()
    }

    pub fn status(&self) -> Value {
        let tasks: Vec<_> = self.hub.snapshot().iter().map(|t| json!({"id":t.id,"label":t.label,"detail":t.detail,"state":format!("{:?}",t.state).to_lowercase(),"done":t.done,"total":t.total,"elapsed":t.elapsed.as_secs_f64(),"eta":t.eta.map(|d|d.as_secs_f64()),"error":t.error})).collect();
        let active = self.active.lock().unwrap();
        let operations: Vec<_> = self.operations.lock().unwrap().keys().cloned().collect();
        json!({"revision":self.hub.version(),"tasks":tasks,"log":self.hub.log(),"operations":operations,"activeJob":active.as_ref().filter(|(_,s)|!s.is_finished() || self.finishing.load(Ordering::Acquire)).map(|(j,_)|*j)})
    }

    fn jobs(&self) -> Result<Vec<Value>> {
        let mut jobs = rows(&self.reader.lock().unwrap(), "SELECT j.*, (SELECT count(*) FROM images i WHERE i.job_id=j.id) AS total, (SELECT count(*) FROM images i WHERE i.job_id=j.id AND i.status='done') AS done FROM jobs j ORDER BY j.id DESC", [])?;
        for job in &mut jobs {
            if job["label"].as_str().is_none_or(|s| s.trim().is_empty()) {
                job["label"] = json!(Path::new(job["root"].as_str().unwrap_or_default())
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy());
            }
            if let Some(sj) = job.get("settings_json").and_then(|v| v.as_str()) {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(sj) {
                    if let Some(profile) = val.get("scan_profile").and_then(|p| p.as_str()) {
                        job["scan_profile"] = json!(profile);
                    }
                }
            }
        }
        Ok(jobs)
    }

    fn review(&self, job: i64) -> Result<Value> {
        let db = self.reader.lock().unwrap();
        // Only what the UI reads: `SELECT *` was 17 MB and 0.8 s for 4,808
        // frames, 3.6 MB of it the raw sharpness `features`.
        let frames = rows(&db,"SELECT i.id,i.path,i.status,i.thumb_path,i.preview_path,i.width,i.height,i.rating,i.rejected,i.burst_key,i.error, COALESCE(i.stars,(SELECT max(d.stars) FROM detections d WHERE d.image_id=i.id)) AS manual_stars, (SELECT max(d.burst_pick) FROM detections d WHERE d.image_id=i.id) AS burst_pick FROM images i WHERE job_id=? ORDER BY path",[job])?;
        let mut detections = rows(&db,"SELECT d.id,d.image_id,d.cls,d.crop_path,d.x1,d.y1,d.x2,d.y2,d.sharpness,d.panning,d.number,d.plate,d.cull_reason,d.attributes,d.burst_pick,d.region_type,d.reviewed,d.rejected,d.bystander,d.stars,d.group_key,d.group_size,d.group_agreement,d.embedding FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? ORDER BY d.image_id,d.id",[job])?;
        let known: Vec<(Value, Vec<f32>)> = rows(&db, "SELECT plate,make,model,colour,team,race_number,driver,country,embedding FROM known_vehicles WHERE embedding IS NOT NULL AND embedding!=''", [])?
            .into_iter()
            .filter_map(|row| similarity::unpack(row["embedding"].as_str()?).map(|vector| (row, vector)))
            .collect();
        let people: Vec<(Value, Vec<f32>)> =
            rows(&db, "SELECT name,country,embedding FROM known_people", [])?
                .into_iter()
                .filter_map(|row| {
                    similarity::unpack(row["embedding"].as_str()?).map(|vector| (row, vector))
                })
                .collect();
        for detection in &mut detections {
            if let Some(vector) = detection["embedding"].as_str().and_then(similarity::unpack) {
                let is_face = detection["region_type"].as_str() == Some("face");
                let candidates = if is_face { &people } else { &known };
                let threshold = if is_face {
                    0.97
                } else {
                    conrod_core::grouping::SAME_CAR
                };
                let best = candidates
                    .iter()
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
        Ok(json!({"frames":frames,"detections":detections}))
    }

    fn mark(&self, a: &MarkArgs) -> Result<Value> {
        if let Some(Some(stars)) = a.stars {
            if !(0..=5).contains(&stars) {
                return Err("Stars must be 0–5 or null".into());
            }
        }
        let db = self.db.lock().unwrap();
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
                "UPDATE images SET rejected=? WHERE id=?",
                params![rejected, a.image_id],
            )
            .map_err(err)?;
            tx.execute(
                "UPDATE detections SET rejected=?,cull_reason=CASE WHEN ?1=0 THEN NULL ELSE cull_reason END WHERE image_id=?2",
                params![rejected, a.image_id],
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
        let mut active = self.active.lock().unwrap();
        if active
            .as_ref()
            .is_some_and(|(_, s)| !s.is_finished() || self.finishing.load(Ordering::Acquire))
        {
            return Err("A scan is already running".into());
        }
        let mut settings = self.settings.lock().unwrap().clone();
        let task = self.hub.start("Indexing album", 0);
        let (job, paths) = if let Some(job) = a.job_id {
            let db = self.db.lock().unwrap();
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
            let profile = a.profile.as_deref().unwrap_or(&settings.scan_profile);
            settings.scan_profile = ShootPreset::parse(profile).name().into();
            if let Some(read) = a.read_plates {
                settings.read_plates = read;
            }
            if let Some(read) = a.read_numbers {
                settings.read_numbers = read;
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
            let db = self.db.lock().unwrap();
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
                .unwrap()
                .execute("UPDATE jobs SET status='indexed' WHERE id=?", [job])
                .map_err(err)?;
            task.finish();
            return Ok(json!({"jobId": job}));
        }
        self.db
            .lock()
            .unwrap()
            .execute("UPDATE jobs SET status='scanning' WHERE id=?", [job])
            .map_err(err)?;
        task.finish();
        let desktop = self.clone();
        let eligibility = self.clone();
        let scan = Arc::new(crate::scan_files_filtered(
            paths,
            settings.clone(),
            ScanProfile::parse(&settings.scan_profile),
            self.hub.clone(),
            move |path| {
                eligibility
                    .reader
                    .lock()
                    .unwrap()
                    .query_row(
                        "SELECT COALESCE(rejected,0)=0 FROM images WHERE job_id=? AND path=?",
                        params![job, path.to_string_lossy()],
                        |r| r.get::<_, bool>(0),
                    )
                    .unwrap_or(false)
            },
            move |result| match result {
                Ok(frame) => {
                    if let Err(e) = desktop.persist_frame(job, &frame) {
                        desktop.hub.start("Saving scan result", 0).fail(e);
                    }
                }
                Err((path, e)) => {
                    let _ = desktop.db.lock().unwrap().execute(
                        "UPDATE images SET status='error',error=? WHERE job_id=? AND path=?",
                        params![e, job, path.to_string_lossy()],
                    );
                }
            },
        ));
        self.finishing.store(true, Ordering::Release);
        *active = Some((job, scan.clone()));
        let desktop = self.clone();
        let identify_after = a.stage == ScanStage::All;
        std::thread::spawn(move || {
            while !scan.is_finished() {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            let stopped = scan.stop.load(Ordering::Relaxed);
            let status = if stopped {
                "stopped"
            } else if scan.error.lock().unwrap().is_some() {
                "error"
            } else {
                "done"
            };
            if !stopped {
                if let Err(e) = desktop.regroup(job, settings.burst_gap) {
                    desktop.hub.start("Grouping album", 0).fail(e);
                }
            }
            let db = desktop.db.lock().unwrap();
            let incomplete: i64 = db
                .query_row(
                    "SELECT count(*) FROM images WHERE job_id=? AND status!='done' AND COALESCE(rejected,0)=0",
                    [job],
                    |r| r.get(0),
                )
                .unwrap_or(1);
            let final_status = if stopped {
                "stopped"
            } else if status == "done" && incomplete > 0 {
                "error"
            } else {
                status
            };
            let _ = db.execute(
                "UPDATE jobs SET status=? WHERE id=?",
                params![final_status, job],
            );
            drop(db);
            if identify_after && final_status == "done" {
                if let Err(e) = crate::operations::identify(&desktop, job) {
                    desktop.hub.start("Identification", 0).fail(e);
                }
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

    fn persist_frame(&self, job: i64, f: &FrameResult) -> Result<()> {
        let image: i64 = self
            .db
            .lock()
            .unwrap()
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
        let db = self.db.lock().unwrap();
        let tx = db.unchecked_transaction().map_err(err)?;
        tx.execute("DELETE FROM detections WHERE image_id=?", [image])
            .map_err(err)?;
        for s in &f.subjects {
            tx.execute("INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,sharpness,rating,rating_verdict,panning,sharp_end,cull_reason,features,heuristic,region_type) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",params![image,s.bbox[0],s.bbox[1],s.bbox[2],s.bbox[3],s.class,s.conf,s.sharpness,s.rating,conrod_vision::sharpness::rating_for(s.rating,0.825,0.606),s.panning,s.sharp_end,s.cull_reason,serde_json::to_string(&s.features).map_err(err)?,s.heuristic,s.region_type]).map_err(err)?;
        }
        let rating = f.rating;
        tx.execute("UPDATE images SET status='done',error=NULL,width=?,height=?,camera=?,taken_at=?,sharpness=?,rating=?,thumb_path=? WHERE id=?",params![f.size.0 as i64,f.size.1 as i64,f.camera,f.taken,f.whole,rating,thumb.to_string_lossy(),image]).map_err(err)?;
        tx.commit().map_err(err)?;
        Ok(())
    }

    fn regroup(&self, job: i64, gap: f64) -> Result<()> {
        let task = self.hub.start("Grouping bursts", 0);
        let db = self.db.lock().unwrap();
        let images = rows(
            &db,
            "SELECT id,path,camera,taken_at,rating FROM images WHERE job_id=? AND status='done'",
            [job],
        )?;
        let mut frames: Vec<_> = images
            .iter()
            .map(|v| conrod_core::bursts::Frame {
                path: v["path"].as_str().unwrap_or_default().into(),
                camera: v["camera"].as_str().unwrap_or_default().into(),
                taken: v["taken_at"].as_f64(),
                burst: 0,
            })
            .collect();
        let effective_gap = if (gap - 4.0).abs() < f64::EPSILON || gap <= 0.0 {
            1.5
        } else {
            gap
        };
        conrod_core::bursts::assign_bursts(&mut frames, effective_gap);
        let tx = db.unchecked_transaction().map_err(err)?;
        for frame in frames {
            tx.execute(
                "UPDATE images SET burst_key=? WHERE job_id=? AND path=?",
                params![frame.burst, job, frame.path],
            )
            .map_err(err)?;
        }
        tx.execute("UPDATE detections SET burst_pick=0 WHERE image_id IN(SELECT id FROM images WHERE job_id=?)",[job]).map_err(err)?;
        tx.execute("UPDATE detections SET burst_pick=1 WHERE image_id IN(SELECT id FROM (SELECT id, ROW_NUMBER() OVER(PARTITION BY burst_key ORDER BY COALESCE(stars,rating*5,0) DESC, COALESCE(sharpness,0) DESC, id) AS rank FROM images WHERE job_id=? AND status='done' AND rejected=0) WHERE rank=1)",[job]).map_err(err)?;
        tx.commit().map_err(err)?;
        task.finish();
        Ok(())
    }

    fn preview(&self, image: i64) -> Result<Value> {
        let output = self.root.join(format!("cache/native/view-{image}.jpg"));
        if !output.is_file() {
            let task = self.hub.start("Loading preview", 0);
            let path: String = self
                .db
                .lock()
                .unwrap()
                .query_row("SELECT path FROM images WHERE id=?", [image], |r| r.get(0))
                .map_err(err)?;
            let raw = conrod_io::raw::read(Path::new(&path))?;
            let rgb = Rgb::decode_jpeg(&raw.preview, 2)?.orient(raw.orientation);
            save_jpeg(&rgb, &output)?;
            task.finish();
        }
        Ok(json!(output))
    }
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
    fn delete_all_known_removes_every_vehicle() {
        let root =
            std::env::temp_dir().join(format!("conrod-known-{}-{}", std::process::id(), now()));
        let desktop = Desktop::open(root.clone()).unwrap();
        {
            let db = desktop.db.lock().unwrap();
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
                .unwrap()
                .query_row("SELECT count(*) FROM known_vehicles", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        drop(desktop);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manual_zero_and_region_label_undo_survive_reopen() {
        let root =
            std::env::temp_dir().join(format!("conrod-desktop-{}-{}", std::process::id(), now()));
        let desktop = Desktop::open(root.clone()).unwrap();
        let (job, image, detection) = {
            let db = desktop.db.lock().unwrap();
            let job =
                conrod_store::create_job(&db, &root, Some("Test"), &json!(Settings::default()))
                    .unwrap();
            conrod_store::add_images(&db, job, &[root.join("test.jpg")]).unwrap();
            let image: i64 = db
                .query_row("SELECT id FROM images WHERE job_id=?", [job], |r| r.get(0))
                .unwrap();
            db.execute("INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,features,region_type) VALUES(?,1,2,30,40,'person',0.9,?,'face')",params![image,serde_json::to_string(&vec![0.5;18]).unwrap()]).unwrap();
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
                .unwrap()
                .query_row("SELECT stars FROM native_labels", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            desktop
                .db
                .lock()
                .unwrap()
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
}
