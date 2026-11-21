//! Application operations shared by Tauri and the CLI. Database writes and
//! scans stay here; the frontend receives data, never arbitrary SQL or paths.
use crate::commands::{Command, KnownArgs, MarkArgs, ScanArgs};
use crate::err;
use crate::lock;
use crate::now;
use crate::{FrameResult, Scan};
use conrod_core::{
    profile::{ScanProfile, ShootPreset},
    settings::Settings,
    tasks::TaskHub,
};
use conrod_vision::{imageops::Rgb, similarity};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub type Result<T> = std::result::Result<T, String>;
/// The single number a counting query returns.
fn scalar(db: &Connection, sql: &str, args: impl rusqlite::Params) -> Result<i64> {
    db.query_row(sql, args, |row| row.get(0)).map_err(err)
}
/// Does `subject` contain the centre of `af`? Both are fractions of the frame.
/// The same rule as `rescore::holds_middle`, which decides whether the camera
/// focused on a stored subject.
fn holds_middle(subject: [f64; 4], af: [f64; 4]) -> bool {
    let (cx, cy) = ((af[0] + af[2]) / 2.0, (af[1] + af[3]) / 2.0);
    cx >= subject[0] && cx <= subject[2] && cy >= subject[1] && cy <= subject[3]
}

/// The burst gap to group an album with: its own setting, or the one default.
fn burst_gap(setting: f64) -> f64 {
    conrod_core::settings::burst_gap_or_default(setting)
}

/// Whether a one-shot scan should run the identification lane at its end:
/// every workflow but "cull", and only when the shoot's preset identifies
/// subjects and reads numbers or plates. "cull" is today's scan without it.
fn wants_identify(mode: &str, profile_identifies: bool, reads_text: bool) -> bool {
    conrod_core::settings::workflow_mode_word(mode) != conrod_core::settings::WORKFLOW_CULL
        && profile_identifies
        && reads_text
}

/// The album setting that records how far its pipeline got, so Resume can
/// continue at that stage instead of starting over. Stored next to
/// `native_scan_stage` in the album's `settings_json`.
const RESUME_STAGE: &str = "native_resume_stage";

/// How far an album's pipeline got. Ordered: every stage before it is done.
///
/// A crash or Stop leaves the album in one of these; a resume starts at the
/// stored one. `Scan` also means "read the frames that are not done yet".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Tail {
    Scan,
    Group,
    Pick,
    Identify,
    Taste,
    Done,
}

impl Tail {
    fn parse(word: &str) -> Tail {
        match word {
            "group" => Tail::Group,
            "pick" => Tail::Pick,
            "identify" => Tail::Identify,
            "taste" => Tail::Taste,
            "done" => Tail::Done,
            _ => Tail::Scan,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Tail::Scan => "scan",
            Tail::Group => "group",
            Tail::Pick => "pick",
            Tail::Identify => "identify",
            Tail::Taste => "taste",
            Tail::Done => "done",
        }
    }
}

/// The tail a resume continues at: the album's own record, or its start.
fn stored_tail(settings: &Map<String, Value>) -> Tail {
    settings
        .get(RESUME_STAGE)
        .and_then(Value::as_str)
        .map_or(Tail::Scan, Tail::parse)
}

/// Where a resume that has no frames left to read picks the pipeline up. An
/// album that finished (`done`) is run again from the grouping, as before.
fn resume_tail(settings: &Map<String, Value>) -> Tail {
    match stored_tail(settings) {
        Tail::Done => Tail::Scan,
        tail => tail,
    }
}

/// The workflow a resume runs: the album's stored scan stage, never the
/// sheet's (a resume has no sheet). An album that was only indexed is culled.
fn stored_stage(settings: &Map<String, Value>) -> crate::commands::ScanStage {
    match settings.get("native_scan_stage").and_then(Value::as_str) {
        Some("all") => crate::commands::ScanStage::All,
        _ => crate::commands::ScanStage::Cull,
    }
}

/// The frames a resume still has to read: not done, not rejected by hand, not
/// a hidden copy. Frames already done are never read again.
fn unfinished_paths(db: &Connection, job: i64) -> Result<Vec<PathBuf>> {
    Ok(rows(
        db,
        "SELECT path FROM images WHERE job_id=? AND status!='done' AND COALESCE(rejected,0)=0 AND COALESCE(copy_of,0)=0",
        [job],
    )?
    .iter()
    .filter_map(|v| v["path"].as_str().map(PathBuf::from))
    .collect())
}

/// The frames a finished scan could not read: how many, and a few reasons in
/// the words the reader reported. One unreadable file never fails an album.
fn scan_errors(db: &Connection, job: i64) -> (i64, Vec<String>) {
    let count = scalar(
        db,
        "SELECT count(*) FROM images WHERE job_id=? AND status='error' AND COALESCE(rejected,0)=0 AND COALESCE(copy_of,0)=0",
        [job],
    )
    .unwrap_or(0);
    if count == 0 {
        return (0, Vec::new());
    }
    let samples = rows(
        db,
        "SELECT path,error FROM images WHERE job_id=? AND status='error' AND COALESCE(rejected,0)=0 AND COALESCE(copy_of,0)=0 ORDER BY id LIMIT 3",
        [job],
    )
    .map(|found| {
        found
            .iter()
            .filter_map(|row| {
                let path = row["path"].as_str()?;
                let name = Path::new(path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let reason = row["error"].as_str().unwrap_or("could not be read");
                Some(format!("{name}: {reason}"))
            })
            .collect()
    })
    .unwrap_or_default();
    (count, samples)
}

/// The status a finished scan leaves behind. Per-frame failures are
/// `done_errors` (Review opens, the album offers a retry); only a failed
/// pipeline is `error`.
fn album_status(
    stopped: bool,
    pipeline_failed: bool,
    incomplete: i64,
    errors: i64,
) -> &'static str {
    if stopped {
        "stopped"
    } else if pipeline_failed {
        "error"
    } else if incomplete > 0 && errors == incomplete {
        "done_errors"
    } else if incomplete > 0 {
        "error"
    } else {
        "done"
    }
}

/// What importing a folder would do to the library: the album that already
/// holds this root, the files already in *any* album (never imported twice),
/// and the files that are genuinely new.
struct ImportPlan {
    album: Option<i64>,
    duplicates: usize,
    fresh: Vec<PathBuf>,
}

fn plan_import(db: &Connection, root: &Path, paths: &[PathBuf]) -> Result<ImportPlan> {
    let album: Option<i64> = db
        .query_row(
            "SELECT id FROM jobs WHERE root=?1 COLLATE NOCASE ORDER BY id LIMIT 1",
            [root.to_string_lossy().as_ref()],
            |r| r.get(0),
        )
        .optional()
        .map_err(err)?;
    // A photograph already in an album is not imported again, even under
    // another root: two rows would mean two scans of the same file.
    let mut probe = db
        .prepare("SELECT EXISTS(SELECT 1 FROM images WHERE path=?)")
        .map_err(err)?;
    let mut duplicates = 0;
    let mut fresh = Vec::with_capacity(paths.len());
    for path in paths {
        let text = path.to_string_lossy();
        let known: bool = probe
            .query_row([text.as_ref()], |r| r.get(0))
            .map_err(err)?;
        if known {
            duplicates += 1;
        } else {
            fresh.push(path.clone());
        }
    }
    Ok(ImportPlan {
        album,
        duplicates,
        fresh,
    })
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
fn keep_sets(images: &[Value]) -> Vec<Option<(i64, conrod_core::sets::SetKind)>> {
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
            let key = images[order[first as usize]]["id"]
                .as_i64()
                .unwrap_or_default();
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
    /// The identify run in progress (or just finished), for `identify-progress`
    /// events: the car being read, its prompt and reply, and its final fields.
    pub(crate) identify: Mutex<Option<Arc<crate::identify_live::IdentifyLive>>>,
    pub(crate) operations:
        Mutex<std::collections::BTreeMap<String, Arc<std::sync::atomic::AtomicBool>>>,
    /// What start-up is doing, for the splash (`startup-progress`, `startup_status`).
    pub startup: crate::startup::Tracker,
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
    let frames = rows_compact(db,"SELECT i.id,i.path,i.status,i.thumb_path,i.preview_path,i.width,i.height,i.rating,i.rejected,i.picked,i.burst_key,i.dupe_key,i.error, COALESCE(i.stars,(SELECT max(d.stars) FROM detections d WHERE d.image_id=i.id)) AS manual_stars, COALESCE((SELECT max(d.burst_pick) FROM detections d WHERE d.image_id=i.id AND d.burst_pick=1), i.burst_pick) AS burst_pick, i.set_id,i.set_kind,i.moments,i.exposure_verdict,i.highlight_clip,i.shadow_clip,i.dark_frame,i.tilt_deg,i.converge_deg,i.level_verdict,i.eye_open_min,i.smile_max,i.buck_score,i.taste,i.comp_placement,i.comp_mode,i.comp_balance_x,i.comp_balance_y,i.comp_symmetry,i.comp_clutter,i.comp_lines,i.comp_subject_share,i.comp_at,i.af_x1,i.af_y1,i.af_x2,i.af_y2,i.camera,i.lens,i.iso,i.file_size,i.exposure_time,i.exposure_bias,i.f_number,i.focal_length,i.focus_distance,i.taken_at FROM images i WHERE job_id=? AND COALESCE(i.copy_of,0)=0 ORDER BY path",[job])?;
    let mut detections = rows_compact(db,"SELECT d.id,d.image_id,d.cls,d.crop_path,d.x1,d.y1,d.x2,d.y2,d.sharpness,d.panning,d.number,d.plate,d.cull_reason,d.attributes,d.burst_pick,d.region_type,d.reviewed,d.rejected,d.bystander,d.stars,d.group_key,d.group_size,d.group_agreement,d.embedding,d.rating,d.rating_verdict,d.sharp_end,d.quality,d.cut_sides,d.subject_frac,d.lead_room,d.framing_score FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(i.copy_of,0)=0 ORDER BY d.image_id,d.id",[job])?;
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
        let startup = crate::startup::Tracker::new(crate::startup::TOTAL);
        startup.begin("library", "Opening your library", None);
        std::fs::create_dir_all(root.join("cache/native")).map_err(err)?;
        let db = conrod_store::connect(Some(&root.join("conrod.db"))).map_err(err)?;
        let reader = conrod_store::connect(Some(&root.join("conrod.db"))).map_err(err)?;
        let settings = Settings::load(&root.join("settings.json"));
        crate::decider::service().configure(&settings);
        task.finish();
        let albums: i64 = db
            .query_row("SELECT count(*) FROM jobs", [], |r| r.get(0))
            .unwrap_or(0);
        startup.finish(
            None,
            Some(match albums {
                0 => "No albums yet".to_string(),
                1 => "1 album".to_string(),
                n => format!("{n} albums"),
            }),
            None,
        );
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
            identify: Mutex::new(None),
            operations: Mutex::new(std::collections::BTreeMap::new()),
            startup,
        });
        crate::watching::restore(&desktop);
        // Before anything can offer to resume: an album a crash left mid-scan
        // still says `scanning`, which hides Resume. Mark it stopped instead;
        // every finished frame and its results are kept.
        desktop
            .startup
            .begin("albums", "Checking for albums a crash left open", None);
        match desktop.reconcile_interrupted() {
            Ok(0) => desktop
                .startup
                .finish(None, Some("None found".into()), None),
            Ok(n) => desktop.startup.finish(
                Some("Tidying albums a crash left open"),
                Some(format!(
                    "{n} {} marked stopped, ready to resume",
                    if n == 1 { "album" } else { "albums" }
                )),
                None,
            ),
            Err(e) => {
                desktop.startup.fail(format!("Could not check: {e}"));
                desktop
                    .hub
                    .note(format!("Could not reconcile interrupted albums: {e}"));
            }
        }
        Ok(desktop)
    }

    /// Mark every album a previous run left in an in-progress status as
    /// stopped, so it is resumable again. Returns how many were fixed.
    ///
    /// Statuses other than `scanning` are not written today, but they are the
    /// in-progress words a future stage could leave behind; treating them the
    /// same costs nothing and keeps a crash from stranding an album.
    fn reconcile_interrupted(&self) -> Result<usize> {
        lock(&self.db)
            .execute(
                "UPDATE jobs SET status='stopped' WHERE status IN ('scanning','indexing','grouping','picking','identifying')",
                [],
            )
            .map_err(err)
    }

    /// Record how far the album's pipeline got, so a later resume continues at
    /// that stage. Best effort: a failure is noted, never fatal to the scan.
    fn note_resume_stage(&self, job: i64, stage: Tail) {
        let db = lock(&self.db);
        let stored: String =
            match db.query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
                r.get(0)
            }) {
                Ok(stored) => stored,
                Err(e) => {
                    drop(db);
                    self.hub
                        .note(format!("Could not record the scan stage: {}", err(e)));
                    return;
                }
            };
        let mut map: Map<String, Value> = serde_json::from_str(&stored).unwrap_or_default();
        if map.get(RESUME_STAGE).and_then(Value::as_str) == Some(stage.word()) {
            return;
        }
        map.insert(RESUME_STAGE.into(), json!(stage.word()));
        let saved = match serde_json::to_string(&map) {
            Ok(saved) => saved,
            Err(_) => return,
        };
        if let Err(e) = db.execute(
            "UPDATE jobs SET settings_json=? WHERE id=?",
            params![saved, job],
        ) {
            drop(db);
            self.hub
                .note(format!("Could not record the scan stage: {}", err(e)));
        }
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

    /// The identify run in progress (or the one that just ended), if any.
    pub fn identify_live(&self) -> Option<Arc<crate::identify_live::IdentifyLive>> {
        lock(&self.identify).clone()
    }

    /// The `identify-progress` payload for this tick, throttled the same way as
    /// `scan-progress`: one per tick while the run is live, then one final
    /// snapshot. The last snapshot is what the screen shows as its summary.
    pub fn identify_snapshot(&self) -> Option<Value> {
        self.identify_live()?.emit_snapshot()
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
            overrides: None,
            scan_profile: None,
            blur_strictness: None,
            burst_gap: None,
            grouping_stacks: None,
            grouping_sensitivity: None,
            grouping_pause: None,
            grouping_keep_pans: None,
            grouping_merge: None,
            grouping_focus_anchor: None,
            workflow_mode: None,
            identify_scope: None,
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
                "reset_album",
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
            Command::StartupStatus {} => Ok(self.startup.snapshot()),
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
            Command::Identify(a) => crate::operations::identify(self, a.job_id, a.scope.as_deref()),
            Command::Identified(a) => self.identified(a.job_id),
            Command::ProviderModels(a) => Ok(
                match conrod_io::vlm::list_models(&a.provider, &a.host, &a.api_key) {
                    Ok(models) => json!({
                        "models": models.into_iter()
                            .map(|(id, name)| json!({"id": id, "name": name}))
                            .collect::<Vec<_>>(),
                    }),
                    Err(e) => json!({"models": [], "error": e}),
                },
            ),
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
            Command::DeciderStatus {} => Ok(crate::decider::service().status()),
            Command::InstallDecider {} => crate::decider::install_in_background(self),
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
            Command::Rescore(a) => crate::rescore::rescore(self, a.job_id),
            Command::Rescan(a) => crate::rescore::rescan(self, a.job_id),
            Command::PickKeepers(a) => crate::passes::pick_keepers(self, a.job_id),
            Command::FocusPoints(a) => crate::passes::focus_points(self, a.job_id),
            Command::Redetect(a) => crate::passes::redetect(self, a.job_id),
            Command::Composition(a) => crate::composition::measure_all(self, a.job_id),
            Command::CollapseCopies(a) => crate::passes::collapse_copies(self, a.job_id),
            Command::RestoreCopies(a) => crate::passes::restore_copies(self, a.job_id),
            Command::TasteRank(a) => crate::taste::taste_rank(self, a.job_id, a.force),
            Command::Group(a) => crate::passes::group(self, a.job_id),
            Command::Regroup(a) => self.regroup(a.job_id),
            Command::BulkEdit(a) => crate::edits::bulk_edit(self, &a),
            Command::RenameJob(a) => crate::library::rename_job(self, &a),
            Command::UpdateJobSettings(a) => {
                // A burst gap or preset is what the library regroups on; a
                // grouping choice needs the same, and it lives here so the
                // album on screen changes as the control does.
                const LIBRARY_REGROUPS: [&str; 2] = ["burst_gap", "scan_profile"];
                const GROUPING: [&str; 6] = [
                    "grouping_sensitivity",
                    "grouping_pause",
                    "grouping_keep_pans",
                    "grouping_merge",
                    "grouping_focus_anchor",
                    "grouping_stacks",
                ];
                let groupings = a.patch.keys().any(|k| GROUPING.contains(&k.as_str()));
                let already = a
                    .patch
                    .keys()
                    .any(|k| LIBRARY_REGROUPS.contains(&k.as_str()));
                let out = crate::library::update_job_settings(self, &a)?;
                if groupings && !already && !self.scanning() {
                    let _ = self.regroup(a.job_id);
                }
                Ok(out)
            }
            Command::SetIncludePeople(a) => {
                crate::passes::set_include_people(self, a.job_id, a.value)
            }
            Command::PresetPolicy(a) => {
                let preset = ShootPreset::parse(&a.preset);
                // An album carries its own copy of the photographer's defaults;
                // the import sheet shows what a new album would take.
                let user = match &a.user_defaults {
                    Some(map) => conrod_core::profile::Overrides::from_map(map),
                    None => lock(&self.settings).profile().defaults_for(preset),
                };
                Ok(conrod_core::profile::Lanes::resolve_with(
                    preset,
                    a.include_people,
                    &conrod_core::profile::Overrides::from_map(&a.overrides),
                    &user,
                )
                .summary())
            }
            Command::SetFeature(a) => {
                crate::passes::set_feature(self, a.job_id, &a.feature, a.value)
            }
            Command::Summary(a) => crate::library::summary(self, a.job_id),
            Command::ShootStats(a) => self.shoot_stats(a.job_id),
            Command::GlobalStats {} => self.global_stats(),
            Command::Filling(a) => crate::library::filling(self, a.job_id),
            Command::Cover(a) => crate::library::cover(self, a.job_id),
            Command::CacheInfo {} => crate::housekeeping::cache_info(self),
            Command::CacheClear(a) => crate::housekeeping::cache_clear(self, &a),
            Command::ResetIdentifications(a) => {
                crate::housekeeping::reset_identifications(self, a.job_id)
            }
            Command::ResetRatings(a) => crate::housekeeping::reset_ratings(self, a.job_id),
            Command::ResetDetections(a) => crate::housekeeping::reset_detections(self, a.job_id),
            Command::ResetAlbum(a) => match a.job_id {
                Some(job) => crate::housekeeping::reset_album(self, job),
                None => Err("reset_album needs a jobId: it resets one album".into()),
            },
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
            Command::LightroomExport(a) => crate::lightroom::export(self, a.job_id),
            Command::LightroomPending(a) => crate::lightroom::pending(self, a.dir.as_deref()),
            Command::LightroomImport(a) => {
                crate::lightroom::import(self, a.job_id, a.catalog.as_deref())
            }
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
        // The wizard's answers: known shoot types, known switches, nothing
        // else. An album's copy never lives in the global file.
        next.user_profile = conrod_core::user_profile::UserProfile::sanitize(&next.user_profile);
        next.user_defaults = Map::new();
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
        crate::decider::service().configure(&next);
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
        let mut rows = crate::setup::rows();
        if let Some(list) = rows.as_array_mut() {
            list.push(crate::decider::service().model_row());
        }
        rows
    }

    /// The worker count of every pool, its cap and whether a change applies
    /// live. No pool can be resized mid-scan (each is a scoped thread set), so
    /// every `live` is false and the UI says "applies next scan".
    pub fn worker_limits(&self) -> Value {
        let settings = lock(&self.settings);
        let caps = conrod_core::settings::worker_caps();
        let pool = |current: i64, max: i64, default: i64| json!({"current": current, "min": 1, "max": max, "default": default, "live": false});
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
        let (active_job, ended) = {
            let active = lock(&self.active);
            (
                active
                    .as_ref()
                    .filter(|(_, s)| !s.is_finished())
                    .map(|(j, _)| *j),
                active
                    .as_ref()
                    .filter(|(_, s)| s.is_finished())
                    .map(|(j, _)| *j),
            )
        };
        let operations: Vec<_> = lock(&self.operations).keys().cloned().collect();
        // Frames the scan that just ended could not read, with a few reasons:
        // the UI says "done with errors" from this instead of calling the album failed.
        let scan = ended.map(|job| {
            let (errors, error_samples) = scan_errors(&lock(&self.reader), job);
            json!({"job": job, "errors": errors, "error_samples": error_samples})
        });
        json!({"revision":self.hub.version(),"tasks":tasks,"log":self.hub.log(),"operations":operations,"activeJob":active_job,"scan":scan})
    }

    fn jobs(&self) -> Result<Vec<Value>> {
        let fallback_gap = lock(&self.settings).burst_gap;
        let reader = lock(&self.reader);
        let mut jobs = rows(&reader, "SELECT j.*, (SELECT count(*) FROM images i WHERE i.job_id=j.id AND COALESCE(i.copy_of,0)=0) AS total, (SELECT count(*) FROM images i WHERE i.job_id=j.id AND i.status='done' AND COALESCE(i.copy_of,0)=0) AS done, (SELECT count(*) FROM images i WHERE i.job_id=j.id AND i.status='error' AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0) AS errors, (SELECT count(*) FROM images i WHERE i.job_id=j.id AND COALESCE(i.copy_of,0)<>0) AS copies FROM jobs j ORDER BY j.id DESC", [])?;
        for job in &mut jobs {
            if job["errors"].as_i64().unwrap_or(0) > 0 {
                if let Some(id) = job["id"].as_i64() {
                    job["error_samples"] = json!(scan_errors(&reader, id).1);
                }
            }
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
        let keep = "COALESCE(i.copy_of,0)=0";
        let count = |extra: &str| -> Result<i64> {
            let sql = format!(
                "SELECT count(*) FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?1 AND ({read}) AND {keep} {extra}"
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
             WHERE i.job_id=?1 AND ({read}) AND {keep}
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
            // The album's own scope applies: the scan sheet's `identifyScope`
            // belongs to a new album's settings, and the identify command
            // carries the one-off override.
            return crate::operations::identify(
                self,
                a.job_id.ok_or("Identify requires an existing album")?,
                None,
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
        // How far this run's tail goes, which stage it reads at, and what the
        // import found. A resume keeps the album's own stage; a folder that is
        // already an album only gains the frames it does not have yet.
        let (job, paths, stage, from, existing, duplicates, added) = if let Some(job) = a.job_id {
            let db = lock(&self.db);
            let stored: String = db
                .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
                    r.get(0)
                })
                .map_err(err)?;
            let map = serde_json::from_str::<Map<String, Value>>(&stored).unwrap_or_default();
            settings = settings.apply(&map);
            let paths = unfinished_paths(&db, job)?;
            // Frames left to read start the pipeline at the scan again; with
            // none, continue at the stage the album stopped in.
            let from = if paths.is_empty() {
                resume_tail(&map)
            } else {
                Tail::Scan
            };
            (job, paths, stored_stage(&map), from, true, 0, 0)
        } else {
            // dunce: plain `canonicalize` returns a `\\?\` verbatim path on
            // Windows, which would be stored and then no longer match the
            // paths Python and exiftool use.
            let root = dunce::canonicalize(a.root.as_deref().ok_or("Choose a photo folder")?)
                .map_err(err)?;
            if !root.is_dir() {
                return Err("Choose a photo folder".into());
            }
            let found = crate::files_with_recursion(&root, a.recursive);
            if found.is_empty() {
                return Err("No CR2, CR3 or JPEG photos found in that folder".into());
            }
            let db = lock(&self.db);
            let plan = plan_import(&db, &root, &found)?;
            if let Some(job) = plan.album {
                // The folder already is an album: it gains only the frames it
                // has not seen, and keeps the settings it was created with.
                let added = plan.fresh.len();
                if added == 0 {
                    task.finish();
                    return Ok(json!({
                        "jobId": job,
                        "duplicates": plan.duplicates,
                        "added": 0,
                        "existing": true,
                        "scanning": false,
                    }));
                }
                let tx = db.unchecked_transaction().map_err(err)?;
                conrod_store::add_images(&tx, job, &plan.fresh).map_err(err)?;
                tx.commit().map_err(err)?;
                let stored: String = db
                    .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
                        r.get(0)
                    })
                    .map_err(err)?;
                let map = serde_json::from_str::<Map<String, Value>>(&stored).unwrap_or_default();
                settings = settings.apply(&map);
                // The sheet can still ask for an index-only import; otherwise
                // the new frames are read the way the album was created.
                let stage = if a.stage == ScanStage::Index {
                    ScanStage::Index
                } else {
                    stored_stage(&map)
                };
                (
                    job,
                    unfinished_paths(&db, job)?,
                    stage,
                    Tail::Scan,
                    true,
                    plan.duplicates,
                    added,
                )
            } else if plan.fresh.is_empty() {
                // Every frame is already in another album. Nothing is created:
                // report the album that holds them so the sheet can say so.
                let holder: Option<i64> = db
                    .query_row(
                        "SELECT job_id FROM images WHERE path=? LIMIT 1",
                        [found[0].to_string_lossy().as_ref()],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(err)?;
                task.finish();
                return Ok(json!({
                    "jobId": holder,
                    "duplicates": plan.duplicates,
                    "added": 0,
                    "existing": true,
                    "scanning": false,
                }));
            } else {
                // The sheet's choices belong to the album being created, not to the
                // global defaults: they are written once, into its settings_json.
                let profile = a
                    .scan_profile
                    .as_deref()
                    .or(a.profile.as_deref())
                    .unwrap_or(&settings.scan_profile);
                settings.scan_profile = ShootPreset::parse(profile).name().into();
                // The photographer's own defaults for this type, copied in now:
                // the album keeps them whatever the profile does later.
                settings.take_profile_defaults(a.blur_strictness.is_some());
                // The profile is global; the album keeps only its copy.
                settings.user_profile = Map::new();
                if let Some(read) = a.read_plates {
                    settings.read_plates = read;
                }
                if let Some(read) = a.read_numbers {
                    settings.read_numbers = read;
                }
                if let Some(people) = a.include_people {
                    settings.include_people = people;
                }
                if let Some(map) = &a.overrides {
                    // Only known features, only booleans.
                    settings.overrides = conrod_core::profile::Overrides::from_map(map).to_map();
                }
                if let Some(strictness) = a.blur_strictness.as_deref() {
                    if !["critical", "strict", "moderate", "lenient"].contains(&strictness) {
                        return Err(
                            "Strictness must be critical, strict, moderate or lenient".into()
                        );
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
                // The grouping sheet's choices go through the same normalisers the
                // settings loader and regroup use: an unknown word is "normal", and
                // a pause is clamped to the 0..=10 the cut honours (0 disables it).
                if let Some(stacks) = a.grouping_stacks {
                    settings.grouping_stacks = stacks;
                }
                if let Some(word) = a.grouping_sensitivity.as_deref() {
                    settings.grouping_sensitivity = conrod_core::bursts::sensitivity_word(word);
                }
                if let Some(pause) = a.grouping_pause {
                    settings.grouping_pause = conrod_core::bursts::clamped_pause(pause);
                }
                if let Some(keep) = a.grouping_keep_pans {
                    settings.grouping_keep_pans = keep;
                }
                if let Some(merge) = a.grouping_merge {
                    settings.grouping_merge = merge;
                }
                if let Some(anchor) = a.grouping_focus_anchor {
                    settings.grouping_focus_anchor = anchor;
                }
                if let Some(word) = a.workflow_mode.as_deref() {
                    settings.workflow_mode =
                        conrod_core::settings::workflow_mode_word(word).to_string();
                }
                if let Some(word) = a.identify_scope.as_deref() {
                    settings.identify_scope =
                        conrod_core::settings::identify_scope_word(word).to_string();
                }
                settings.extra.insert(
                    "native_scan_stage".into(),
                    json!(match a.stage {
                        ScanStage::Index => "index",
                        ScanStage::All => "all",
                        _ => "cull",
                    }),
                );
                let added = plan.fresh.len();
                let tx = db.unchecked_transaction().map_err(err)?;
                let job =
                    conrod_store::create_job(&tx, &root, a.label.as_deref(), &json!(settings))
                        .map_err(err)?;
                conrod_store::add_images(&tx, job, &plan.fresh).map_err(err)?;
                tx.commit().map_err(err)?;
                (
                    job,
                    plan.fresh,
                    a.stage,
                    Tail::Scan,
                    false,
                    plan.duplicates,
                    added,
                )
            }
        };
        if stage == ScanStage::Index {
            self.db
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .execute("UPDATE jobs SET status='indexed' WHERE id=?", [job])
                .map_err(err)?;
            task.finish();
            return Ok(
                json!({"jobId": job, "duplicates": duplicates, "added": added, "existing": existing, "scanning": false}),
            );
        }
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute("UPDATE jobs SET status='scanning' WHERE id=?", [job])
            .map_err(err)?;
        // A run that reads frames starts the pipeline at the scan; one that
        // only continues a stage keeps the stage it already recorded.
        if from == Tail::Scan {
            self.note_resume_stage(job, Tail::Scan);
        }
        task.finish();
        let desktop = self.clone();
        let eligibility = self.clone();
        // One progress object per scan: the pools write to it and the app
        // emits its snapshot as `scan-progress`.
        let faces_on = settings.lanes().faces;
        let progress = crate::progress::for_scan(job, &settings, faces_on);
        // Frames an earlier run of this album already read count as done, so a
        // resume reads `500 / 1000` and the counters add up to the album.
        let carried = scalar(
            &lock(&self.reader),
            "SELECT count(*) FROM images WHERE job_id=? AND status='done' AND COALESCE(rejected,0)=0 AND COALESCE(copy_of,0)=0",
            [job],
        )
        .unwrap_or(0)
        .max(0) as u64;
        for stage in ["read", "detect", "sharpness"]
            .into_iter()
            .chain(faces_on.then_some("faces"))
        {
            progress.carry(stage, carried);
        }
        // Grouping and picking each pass the whole album once, at the end.
        for stage in ["group", "pick"] {
            progress.plan(stage, carried + paths.len() as u64);
        }
        *lock(&self.progress) = Some(progress.clone());
        let scan = Arc::new(crate::scan_files_filtered(
            paths,
            settings.clone(),
            ScanProfile::parse(&settings.scan_profile),
            self.hub.clone(),
            Some(progress.clone()),
            move |path| {
                // The frame's row, which a hand reject or a missing row skips.
                let image_id = eligibility
                    .reader
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .query_row(
                        "SELECT id FROM images WHERE job_id=? AND path=? AND COALESCE(rejected,0)=0",
                        params![job, path.to_string_lossy()],
                        |r| r.get::<_, i64>(0),
                    )
                    .ok()?;
                Some(crate::Pickup {
                    image_id,
                    thumb: Some(
                        eligibility
                            .root
                            .join(format!("cache/native/thumb-{image_id}.jpg")),
                    ),
                })
            },
            move |result| match result {
                Ok(frame) => {
                    if let Err(e) = desktop.persist_frame(job, &frame, &settings, true) {
                        desktop.hub.start("Saving scan result", 0).fail(e);
                    }
                }
                Err((path, e)) => {
                    let saved = lock(&desktop.db).execute(
                        "UPDATE images SET status='error',error=? WHERE job_id=? AND path=?",
                        params![e, job, path.to_string_lossy()],
                    );
                    if let Err(e) = saved {
                        desktop.hub.start("Saving scan result", 0).fail(err(e));
                    }
                }
            },
        ));
        self.finishing.store(true, Ordering::Release);
        *active = Some((job, scan.clone()));
        let desktop = self.clone();
        // A one-shot scan runs every stage to the end: bursts and keepers, then
        // identification when the album's preset reads numbers or plates.
        let one_shot = stage == ScanStage::All;
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
            let mut final_status = if stopped {
                "stopped"
            } else if lock(&scan.error).is_some() {
                "error"
            } else {
                "done"
            };
            if !stopped {
                // Only the stages that had not finished run again: a resume
                // continues where the album stopped instead of starting over.
                if from <= Tail::Group {
                    // An identify-only album was not scanned to cull: the automatic
                    // cull the scan just measured is cleared before regrouping picks
                    // keepers, and before the identification lane, so a rescan
                    // cannot put it back. A hand reject, the stars and the measured
                    // scores are left exactly as they are.
                    if one_shot && final_status == "done" {
                        if let Err(e) = desktop.clear_automatic_culls_if_identify(job) {
                            desktop.hub.start("Clearing automatic culls", 0).fail(e);
                        }
                    }
                    desktop.note_resume_stage(job, Tail::Group);
                    if let Some(p) = &desktop.progress() {
                        p.phase("group");
                    }
                    if let Err(e) = desktop.regroup(job) {
                        desktop.hub.start("Grouping album", 0).fail(e);
                        final_status = "error";
                    }
                    desktop.note_resume_stage(job, Tail::Pick);
                }
                if from <= Tail::Pick {
                    if let Some(p) = &desktop.progress() {
                        p.phase("pick");
                    }
                    // Identify when this shoot's presets read numbers or plates:
                    // identification is the slow lane, so it runs last and the
                    // scan is not "done" until it is.
                    let wants_identify = one_shot
                        && final_status == "done"
                        && crate::operations::settings(&desktop, job).is_ok_and(|cfg| {
                            wants_identify(
                                cfg.workflow(),
                                cfg.lanes().identifies()
                                    || cfg.lanes().names
                                    || cfg.lanes().moments_detect,
                                cfg.read_plates
                                    || cfg.read_numbers
                                    || cfg.lanes().names
                                    || cfg.lanes().moments_detect,
                            )
                        });
                    if wants_identify {
                        desktop.note_resume_stage(job, Tail::Identify);
                        // No scope argument: the album's own setting applies
                        // (keepers unless it chose everything).
                        if let Err(e) = crate::operations::identify(&desktop, job, None) {
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
                }
                // The learned taste order comes last, after the pick it sits
                // beside: without a head file it is skipped without a word.
                if final_status == "done" && from <= Tail::Taste {
                    desktop.note_resume_stage(job, Tail::Taste);
                    if let Some(p) = &desktop.progress() {
                        p.phase("pick");
                    }
                    crate::taste::after_scan(&desktop, job);
                }
            }
            // Frames that could not be read are counted, with a few reasons:
            // one unreadable file finishes the album done-with-errors instead
            // of failing it, and a retry of those frames resumes the scan.
            let (incomplete, errors) = {
                let db = lock(&desktop.db);
                // A count that cannot be read counts as incomplete: the scan is
                // not called done on a guess.
                let incomplete: i64 = db
                    .query_row(
                        "SELECT count(*) FROM images WHERE job_id=? AND status!='done' AND COALESCE(rejected,0)=0 AND COALESCE(copy_of,0)=0",
                        [job],
                        |r| r.get(0),
                    )
                    .unwrap_or_else(|e| {
                        desktop.hub.start("Checking scan result", 0).fail(err(e));
                        1
                    });
                let (errors, _) = scan_errors(&db, job);
                (incomplete, errors)
            };
            let outcome = album_status(stopped, final_status == "error", incomplete, errors);
            if !stopped {
                desktop.note_resume_stage(
                    job,
                    match outcome {
                        "done" => Tail::Done,
                        // The failed frames are a scan again, even though the
                        // rest of the pipeline finished.
                        "done_errors" => Tail::Scan,
                        _ => from,
                    },
                );
            }
            if let Err(e) = lock(&desktop.db)
                .execute("UPDATE jobs SET status=? WHERE id=?", params![outcome, job])
            {
                desktop.hub.start("Saving scan status", 0).fail(err(e));
            }
            if let Some(progress) = desktop.progress() {
                progress.finish(outcome);
            }
            desktop.finishing.store(false, Ordering::Release);
            desktop.hub.note(if stopped {
                "Scan stopped"
            } else {
                "Album processing updated"
            });
        });
        Ok(
            json!({"jobId": job, "duplicates": duplicates, "added": added, "existing": existing, "scanning": true}),
        )
    }

    /// The shoot's own numbers, for the statistics card in the settings sheet:
    /// what it was taken with, how it was exposed, how the frames and their
    /// detections came out, and how much space the files take.
    ///
    /// An album scanned before the lens/ISO/file-size columns existed is
    /// backfilled here, once: the headers are read with a small disk-bound
    /// pool and each row is written as it is read. A file that has moved is
    /// skipped, never failing the card. Nothing is rounded; the frontend
    /// formats.
    pub fn shoot_stats(&self, job: i64) -> Result<Value> {
        crate::library::require_job(self, job)?;
        let settings = crate::operations::settings(self, job)?;
        let backfilled = self.backfill_shot_details(job)?;
        let db = lock(&self.reader);
        // What every count is about: the album's frames that are done and are
        // not hidden copies of another import.
        const VISIBLE: &str = "i.job_id=? AND i.status='done' AND COALESCE(i.copy_of,0)=0";
        let frames = scalar(
            &db,
            &format!("SELECT COUNT(*) FROM images i WHERE {VISIBLE}"),
            [job],
        )?;
        let copies_hidden = scalar(
            &db,
            "SELECT COUNT(*) FROM images WHERE job_id=? AND COALESCE(copy_of,0)<>0",
            [job],
        )?;
        let totals = rows(
            &db,
            &format!(
                "SELECT COALESCE(SUM(i.file_size),0) AS size_bytes,
                        MIN(i.taken_at) AS taken_from, MAX(i.taken_at) AS taken_to
                   FROM images i WHERE {VISIBLE}"
            ),
            [job],
        )?
        .pop()
        .unwrap_or(Value::Null);
        let cameras = rows(
            &db,
            &format!(
                "SELECT i.camera AS name, COUNT(*) AS count FROM images i
                  WHERE {VISIBLE} AND i.camera IS NOT NULL AND i.camera<>''
                  GROUP BY i.camera ORDER BY count DESC, name"
            ),
            [job],
        )?;
        let lenses = rows(
            &db,
            &format!(
                "SELECT i.lens AS name, COUNT(*) AS count FROM images i
                  WHERE {VISIBLE} AND i.lens IS NOT NULL AND i.lens<>''
                  GROUP BY i.lens ORDER BY count DESC, name"
            ),
            [job],
        )?;
        // The most common value of a setting, or null when the album records
        // none. Ties go to the smaller value, so the card does not flutter
        // between two equally used lenses.
        let mode = |column: &str, skip_blank: bool| -> Result<Value> {
            let blank = if skip_blank {
                format!(" AND i.{column}<>''")
            } else {
                String::new()
            };
            let sql = format!(
                "SELECT i.{column} AS value FROM images i
                  WHERE {VISIBLE} AND i.{column} IS NOT NULL{blank}
                  GROUP BY i.{column} ORDER BY COUNT(*) DESC, value LIMIT 1"
            );
            Ok(rows(&db, &sql, [job])?
                .pop()
                .and_then(|mut row| {
                    row.as_object_mut()
                        .and_then(|object| object.remove("value"))
                })
                .unwrap_or(Value::Null))
        };
        let histogram = |column: &str| -> Result<Vec<Value>> {
            rows(
                &db,
                &format!(
                    "SELECT i.{column} AS value, COUNT(*) AS count FROM images i
                      WHERE {VISIBLE} AND i.{column} IS NOT NULL
                      GROUP BY i.{column} ORDER BY count DESC, value"
                ),
                [job],
            )
        };
        let pans = scalar(
            &db,
            &format!(
                "SELECT COUNT(DISTINCT d.image_id) FROM detections d
                   JOIN images i ON i.id=d.image_id
                  WHERE {VISIBLE} AND COALESCE(d.panning,0)=1"
            ),
            [job],
        )?;
        // A burst is the time burst grouping found: frames shot within the
        // album's gap of each other, per camera. It is what `regroup` reports.
        let bursts = scalar(
            &db,
            &format!(
                "SELECT COUNT(DISTINCT i.time_burst) FROM images i
                  WHERE {VISIBLE} AND i.time_burst IS NOT NULL"
            ),
            [job],
        )?;
        let verdicts = rows(
            &db,
            &format!(
                "SELECT d.sharpness_verdict AS verdict, COUNT(*) AS count
                   FROM detections d JOIN images i ON i.id=d.image_id
                  WHERE {VISIBLE} AND d.sharpness_verdict IS NOT NULL
                  GROUP BY d.sharpness_verdict"
            ),
            [job],
        )?;
        let verdict = |word: &str| -> i64 {
            verdicts
                .iter()
                .find(|row| row["verdict"].as_str() == Some(word))
                .and_then(|row| row["count"].as_i64())
                .unwrap_or(0)
        };
        let af_missing = scalar(
            &db,
            &format!("SELECT COUNT(*) FROM images i WHERE {VISIBLE} AND i.af_x1 IS NULL"),
            [job],
        )?;
        let af_soft = scalar(
            &db,
            &format!(
                "SELECT COUNT(*) FROM images i WHERE {VISIBLE}
                    AND i.af_sharpness IS NOT NULL AND i.af_sharpness < ?"
            ),
            params![job, settings.blurred_below],
        )?;
        // Whether the camera's focus area landed on a subject: the same centre
        // rule `rescore.rs` uses. Geometry needs the preview's size, which a
        // scanned frame always has.
        let af_boxes = rows(
            &db,
            &format!(
                "SELECT i.id, i.af_x1, i.af_y1, i.af_x2, i.af_y2, i.width, i.height,
                        d.x1 AS dx1, d.y1 AS dy1, d.x2 AS dx2, d.y2 AS dy2
                   FROM images i
              LEFT JOIN detections d ON d.image_id=i.id
                    AND COALESCE(d.region_type,'vehicle') IN ('vehicle','person')
                  WHERE {VISIBLE} AND i.af_x1 IS NOT NULL AND i.af_y1 IS NOT NULL
                    AND i.af_x2 IS NOT NULL AND i.af_y2 IS NOT NULL"
            ),
            [job],
        )?;
        let mut found: std::collections::HashMap<i64, (bool, bool)> =
            std::collections::HashMap::new();
        for row in af_boxes {
            let id = row["id"].as_i64().unwrap_or_default();
            let entry = found.entry(id).or_insert((false, false));
            let (Some(dx1), Some(dy1), Some(dx2), Some(dy2)) = (
                row["dx1"].as_f64(),
                row["dy1"].as_f64(),
                row["dx2"].as_f64(),
                row["dy2"].as_f64(),
            ) else {
                continue;
            };
            entry.0 = true;
            let af = [
                row["af_x1"].as_f64().unwrap_or_default(),
                row["af_y1"].as_f64().unwrap_or_default(),
                row["af_x2"].as_f64().unwrap_or_default(),
                row["af_y2"].as_f64().unwrap_or_default(),
            ];
            let (w, h) = (
                row["width"].as_f64().unwrap_or_default(),
                row["height"].as_f64().unwrap_or_default(),
            );
            if w > 0.0 && h > 0.0 && holds_middle([dx1 / w, dy1 / h, dx2 / w, dy2 / h], af) {
                entry.1 = true;
            }
        }
        let af_no_subject = found.values().filter(|(detected, _)| !detected).count() as i64;
        let af_off_subject = found
            .values()
            .filter(|(detected, holds)| *detected && !holds)
            .count() as i64;
        let keepers = scalar(
            &db,
            &format!(
                "SELECT COUNT(*) FROM images i WHERE {VISIBLE}
                    AND (COALESCE(i.burst_pick,0)=1
                     OR EXISTS (SELECT 1 FROM detections d
                                 WHERE d.image_id=i.id AND COALESCE(d.burst_pick,0)=1))"
            ),
            [job],
        )?;
        let rejected = scalar(
            &db,
            &format!("SELECT COUNT(*) FROM images i WHERE {VISIBLE} AND COALESCE(i.rejected,0)=1"),
            [job],
        )?;
        // A hand star wins over any subject's; no star anywhere is zero, which
        // is a value the card shows rather than an absence.
        let stars = rows(
            &db,
            &format!(
                "SELECT COALESCE(i.stars,
                                (SELECT MAX(d.stars) FROM detections d WHERE d.image_id=i.id),
                                0) AS value,
                        COUNT(*) AS count
                   FROM images i WHERE {VISIBLE} GROUP BY value ORDER BY value"
            ),
            [job],
        )?;
        let most_used = json!({
            "camera": mode("camera", true)?,
            "lens": mode("lens", true)?,
            "f_number": mode("f_number", false)?,
            "shutter": mode("exposure_time", false)?,
            "iso": mode("iso", false)?,
            "focal_length": mode("focal_length", false)?,
        });
        Ok(json!({
            "frames": frames,
            "copies_hidden": copies_hidden,
            "size_bytes": totals["size_bytes"].clone(),
            "backfilled": backfilled,
            "cameras": cameras,
            "lenses": lenses,
            "most_used": most_used,
            "f_numbers": histogram("f_number")?,
            "shutters": histogram("exposure_time")?,
            "isos": histogram("iso")?,
            "focal_lengths": histogram("focal_length")?,
            "pans": pans,
            "bursts": bursts,
            "blurred": verdict("blurred"),
            "soft": verdict("soft"),
            "sharp": verdict("sharp"),
            "af": {
                "frames": frames,
                "missing": af_missing,
                "off_subject": af_off_subject,
                "no_subject": af_no_subject,
                "soft": af_soft,
            },
            "keepers": keepers,
            "rejected": rejected,
            "stars": stars,
            "taken_from": totals["taken_from"].clone(),
            "taken_to": totals["taken_to"].clone(),
            "catalog_match": crate::lightroom::catalog_match(&db, job)?,
        }))
    }

    /// The whole library in one set of numbers: every album's frames, space,
    /// keepers and quality, and -- across the albums that record all of them --
    /// what was used and how it was exposed.
    ///
    /// An album is "complete" when every one of its visible frames has a
    /// non-empty camera and lens and a non-null ISO, f-number, shutter and
    /// focal length. Gear and exposure lists count complete albums only, so an
    /// album whose headers lacked a lens cannot skew the library's camera
    /// list; every other number covers all albums. Nothing is read from disk:
    /// only the columns a scan already stored are aggregated.
    pub fn global_stats(&self) -> Result<Value> {
        // The blur bar is the app's global one: an album's own strictness is
        // not stored per frame, so it cannot be applied library-wide.
        let blurred_below = lock(&self.settings).blurred_below;
        let db = lock(&self.reader);
        // What every count is about: a frame the scan finished, never a hidden
        // copy of another import.
        const VISIBLE: &str = "i.status='done' AND COALESCE(i.copy_of,0)=0";
        let albums = scalar(
            &db,
            &format!("SELECT COUNT(DISTINCT i.job_id) FROM images i WHERE {VISIBLE}"),
            [],
        )?;
        let frames = scalar(
            &db,
            &format!("SELECT COUNT(*) FROM images i WHERE {VISIBLE}"),
            [],
        )?;
        let copies_hidden = scalar(
            &db,
            "SELECT COUNT(*) FROM images WHERE COALESCE(copy_of,0)<>0",
            [],
        )?;
        let size_bytes = scalar(
            &db,
            &format!("SELECT COALESCE(SUM(i.file_size),0) FROM images i WHERE {VISIBLE}"),
            [],
        )?;
        // The albums whose visible frames all record what a gear list needs.
        // An album with no visible frame is not in the table at all.
        let complete: Vec<i64> = rows(
            &db,
            &format!(
                "SELECT i.job_id AS job FROM images i WHERE {VISIBLE}
                  GROUP BY i.job_id
                 HAVING SUM(CASE WHEN i.camera IS NULL OR i.camera='' THEN 1 ELSE 0 END)=0
                    AND SUM(CASE WHEN i.lens IS NULL OR i.lens='' THEN 1 ELSE 0 END)=0
                    AND SUM(CASE WHEN i.iso IS NULL THEN 1 ELSE 0 END)=0
                    AND SUM(CASE WHEN i.f_number IS NULL THEN 1 ELSE 0 END)=0
                    AND SUM(CASE WHEN i.exposure_time IS NULL THEN 1 ELSE 0 END)=0
                    AND SUM(CASE WHEN i.focal_length IS NULL THEN 1 ELSE 0 END)=0"
            ),
            [],
        )?
        .iter()
        .filter_map(|row| row["job"].as_i64())
        .collect();
        // Only these job ids narrow a gear query; an empty list must never
        // become an empty `IN ()` (which SQLite rejects).
        let scope = if complete.is_empty() {
            String::new()
        } else {
            format!(
                " AND i.job_id IN ({})",
                complete
                    .iter()
                    .map(i64::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let complete_frames = if complete.is_empty() {
            0
        } else {
            scalar(
                &db,
                &format!("SELECT COUNT(*) FROM images i WHERE {VISIBLE}{scope}"),
                [],
            )?
        };
        let (cameras, lenses, most_used, f_numbers, shutters, isos, focal_lengths) =
            if complete.is_empty() {
                (
                    Vec::new(),
                    Vec::new(),
                    json!({
                        "camera": Value::Null,
                        "lens": Value::Null,
                        "f_number": Value::Null,
                        "shutter": Value::Null,
                        "iso": Value::Null,
                        "focal_length": Value::Null,
                    }),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )
            } else {
                let cameras = rows(
                    &db,
                    &format!(
                        "SELECT i.camera AS name, COUNT(*) AS count FROM images i
                          WHERE {VISIBLE}{scope} AND i.camera IS NOT NULL AND i.camera<>''
                          GROUP BY i.camera ORDER BY count DESC, name"
                    ),
                    [],
                )?;
                let lenses = rows(
                    &db,
                    &format!(
                        "SELECT i.lens AS name, COUNT(*) AS count FROM images i
                          WHERE {VISIBLE}{scope} AND i.lens IS NOT NULL AND i.lens<>''
                          GROUP BY i.lens ORDER BY count DESC, name"
                    ),
                    [],
                )?;
                // The most common value of a setting. Ties go to the smaller
                // value, as `shoot_stats` does.
                let mode = |column: &str| -> Result<Value> {
                    let sql = format!(
                        "SELECT i.{column} AS value FROM images i
                          WHERE {VISIBLE}{scope} AND i.{column} IS NOT NULL
                          GROUP BY i.{column} ORDER BY COUNT(*) DESC, value LIMIT 1"
                    );
                    Ok(rows(&db, &sql, [])?
                        .pop()
                        .and_then(|mut row| {
                            row.as_object_mut()
                                .and_then(|object| object.remove("value"))
                        })
                        .unwrap_or(Value::Null))
                };
                let histogram = |column: &str| -> Result<Vec<Value>> {
                    rows(
                        &db,
                        &format!(
                            "SELECT i.{column} AS value, COUNT(*) AS count FROM images i
                              WHERE {VISIBLE}{scope} AND i.{column} IS NOT NULL
                              GROUP BY i.{column} ORDER BY count DESC, value"
                        ),
                        [],
                    )
                };
                let most_used = json!({
                    "camera": mode("camera")?,
                    "lens": mode("lens")?,
                    "f_number": mode("f_number")?,
                    "shutter": mode("exposure_time")?,
                    "iso": mode("iso")?,
                    "focal_length": mode("focal_length")?,
                });
                (
                    cameras,
                    lenses,
                    most_used,
                    histogram("f_number")?,
                    histogram("exposure_time")?,
                    histogram("iso")?,
                    histogram("focal_length")?,
                )
            };
        let pans = scalar(
            &db,
            &format!(
                "SELECT COUNT(DISTINCT d.image_id) FROM detections d
                   JOIN images i ON i.id=d.image_id
                  WHERE {VISIBLE} AND COALESCE(d.panning,0)=1"
            ),
            [],
        )?;
        // A burst is one `(album, time_burst)` row: the same burst number in
        // two albums is two bursts. SQLite has no tuple DISTINCT, hence the
        // subquery.
        let bursts = scalar(
            &db,
            &format!(
                "SELECT COUNT(*) FROM (
                     SELECT DISTINCT i.job_id, i.time_burst FROM images i
                      WHERE {VISIBLE} AND i.time_burst IS NOT NULL
                 )"
            ),
            [],
        )?;
        let verdicts = rows(
            &db,
            &format!(
                "SELECT d.sharpness_verdict AS verdict, COUNT(*) AS count
                   FROM detections d JOIN images i ON i.id=d.image_id
                  WHERE {VISIBLE} AND d.sharpness_verdict IS NOT NULL
                  GROUP BY d.sharpness_verdict"
            ),
            [],
        )?;
        let verdict = |word: &str| -> i64 {
            verdicts
                .iter()
                .find(|row| row["verdict"].as_str() == Some(word))
                .and_then(|row| row["count"].as_i64())
                .unwrap_or(0)
        };
        let af_missing = scalar(
            &db,
            &format!("SELECT COUNT(*) FROM images i WHERE {VISIBLE} AND i.af_x1 IS NULL"),
            [],
        )?;
        let af_soft = scalar(
            &db,
            &format!(
                "SELECT COUNT(*) FROM images i WHERE {VISIBLE}
                    AND i.af_sharpness IS NOT NULL AND i.af_sharpness < ?"
            ),
            params![blurred_below],
        )?;
        // Whether the camera's focus area landed on a subject: the same centre
        // rule and per-image grouping `shoot_stats` uses, over every album.
        let af_boxes = rows(
            &db,
            &format!(
                "SELECT i.id, i.af_x1, i.af_y1, i.af_x2, i.af_y2, i.width, i.height,
                        d.x1 AS dx1, d.y1 AS dy1, d.x2 AS dx2, d.y2 AS dy2
                   FROM images i
              LEFT JOIN detections d ON d.image_id=i.id
                    AND COALESCE(d.region_type,'vehicle') IN ('vehicle','person')
                  WHERE {VISIBLE} AND i.af_x1 IS NOT NULL AND i.af_y1 IS NOT NULL
                    AND i.af_x2 IS NOT NULL AND i.af_y2 IS NOT NULL"
            ),
            [],
        )?;
        let mut found: std::collections::HashMap<i64, (bool, bool)> =
            std::collections::HashMap::new();
        for row in af_boxes {
            let id = row["id"].as_i64().unwrap_or_default();
            let entry = found.entry(id).or_insert((false, false));
            let (Some(dx1), Some(dy1), Some(dx2), Some(dy2)) = (
                row["dx1"].as_f64(),
                row["dy1"].as_f64(),
                row["dx2"].as_f64(),
                row["dy2"].as_f64(),
            ) else {
                continue;
            };
            entry.0 = true;
            let af = [
                row["af_x1"].as_f64().unwrap_or_default(),
                row["af_y1"].as_f64().unwrap_or_default(),
                row["af_x2"].as_f64().unwrap_or_default(),
                row["af_y2"].as_f64().unwrap_or_default(),
            ];
            let (w, h) = (
                row["width"].as_f64().unwrap_or_default(),
                row["height"].as_f64().unwrap_or_default(),
            );
            if w > 0.0 && h > 0.0 && holds_middle([dx1 / w, dy1 / h, dx2 / w, dy2 / h], af) {
                entry.1 = true;
            }
        }
        let af_no_subject = found.values().filter(|(detected, _)| !detected).count() as i64;
        let af_off_subject = found
            .values()
            .filter(|(detected, holds)| *detected && !holds)
            .count() as i64;
        // The effective keeper, as identification reads it: a hand pick, or --
        // when no frame of its burst was picked by hand -- the engine's
        // suggestion. A NULL burst key matches no other NULL, so burstless
        // frames never suppress each other.
        let keepers = scalar(
            &db,
            &format!(
                "SELECT COUNT(*) FROM images i WHERE {VISIBLE}
                    AND (COALESCE(i.picked,0)=1
                     OR (NOT EXISTS (SELECT 1 FROM images p
                                      WHERE p.job_id=i.job_id AND p.burst_key=i.burst_key
                                        AND COALESCE(p.picked,0)=1)
                         AND (COALESCE(i.burst_pick,0)=1
                           OR EXISTS (SELECT 1 FROM detections d
                                       WHERE d.image_id=i.id
                                         AND COALESCE(d.burst_pick,0)=1))))"
            ),
            [],
        )?;
        let rejected = scalar(
            &db,
            &format!("SELECT COUNT(*) FROM images i WHERE {VISIBLE} AND COALESCE(i.rejected,0)=1"),
            [],
        )?;
        // Time in the field: the span from a job's first to its last frame,
        // for the jobs that have at least two timed frames. `taken_at` is wall
        // clock encoded as an epoch, and only differences are used.
        let spans: Vec<f64> = rows(
            &db,
            &format!(
                "SELECT i.job_id AS job, COUNT(i.taken_at) AS timed,
                        MIN(i.taken_at) AS first, MAX(i.taken_at) AS last
                   FROM images i WHERE {VISIBLE} AND i.taken_at IS NOT NULL
                  GROUP BY i.job_id"
            ),
            [],
        )?
        .iter()
        .filter_map(|row| {
            if row["timed"].as_i64().unwrap_or_default() < 2 {
                return None;
            }
            Some(row["last"].as_f64()? - row["first"].as_f64()?)
        })
        .collect();
        let total_span: f64 = spans.iter().sum();
        let timed_albums = spans.len() as i64;
        Ok(json!({
            "albums": albums,
            "complete_albums": complete.len(),
            "frames": frames,
            "complete_frames": complete_frames,
            "copies_hidden": copies_hidden,
            "size_bytes": size_bytes,
            "cameras": cameras,
            "lenses": lenses,
            "most_used": most_used,
            "f_numbers": f_numbers,
            "shutters": shutters,
            "isos": isos,
            "focal_lengths": focal_lengths,
            "pans": pans,
            "bursts": bursts,
            "blurred": verdict("blurred"),
            "soft": verdict("soft"),
            "sharp": verdict("sharp"),
            "af": {
                "frames": frames,
                "missing": af_missing,
                "off_subject": af_off_subject,
                "no_subject": af_no_subject,
                "soft": af_soft,
            },
            "keepers": keepers,
            "rejected": rejected,
            "shooting_seconds": total_span.round() as i64,
            "average_shoot_seconds": if timed_albums == 0 {
                0
            } else {
                (total_span / timed_albums as f64).round() as i64
            },
            "longest_shoot_seconds": spans.iter().copied().fold(0.0, f64::max).round() as i64,
            "timed_albums": timed_albums,
        }))
    }

    /// Read the lens, ISO and file size of the frames that are missing them.
    /// Resumable by design: every row is written as it is read, so an
    /// interrupted backfill leaves the rest for the next `shoot_stats`.
    fn backfill_shot_details(&self, job: i64) -> Result<i64> {
        let todo = rows(
            &lock(&self.reader),
            "SELECT id, path, lens, iso, file_size FROM images
              WHERE job_id=? AND status='done' AND COALESCE(copy_of,0)=0
                AND (lens IS NULL OR iso IS NULL OR file_size IS NULL)
              ORDER BY id",
            [job],
        )?;
        if todo.is_empty() {
            return Ok(0);
        }
        // Disk-bound, like the preview pool: a little under the cores.
        let workers = std::thread::available_parallelism()
            .map_or(4, |n| n.get() * 4 / 5)
            .clamp(2, 16)
            .min(todo.len());
        let next = AtomicUsize::new(0);
        let done = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let todo = &todo;
                let (next, done) = (&next, &done);
                scope.spawn(move || loop {
                    let n = next.fetch_add(1, Ordering::Relaxed);
                    let Some(row) = todo.get(n) else { break };
                    let Some(id) = row["id"].as_i64() else {
                        continue;
                    };
                    let Some(path) = row["path"].as_str().map(Path::new) else {
                        continue;
                    };
                    let Ok(meta) = std::fs::metadata(path) else {
                        // Moved, renamed or on an unplugged card: skip it and
                        // let a later attempt try again.
                        continue;
                    };
                    let file_size = meta.len() as i64;
                    let exposure = conrod_io::raw::exposure_meta(path).ok();
                    let lens = exposure.as_ref().and_then(|e| e.lens.as_deref());
                    let iso = exposure.as_ref().and_then(|e| e.iso);
                    // Only a value that actually fills a missing column counts:
                    // a header with no lens or ISO leaves those null, and the
                    // card reports the frames it really learned something from.
                    let new_lens = row["lens"].is_null() && lens.is_some();
                    let new_iso = row["iso"].is_null() && iso.is_some();
                    let new_size = row["file_size"].is_null();
                    if !(new_lens || new_iso || new_size) {
                        continue;
                    }
                    let changed = lock(&self.db)
                        .execute(
                            "UPDATE images
                                SET lens=COALESCE(?, lens), iso=COALESCE(?, iso),
                                    file_size=COALESCE(?, file_size)
                              WHERE id=?",
                            params![
                                exposure.as_ref().and_then(|e| e.lens.as_deref()),
                                exposure.as_ref().and_then(|e| e.iso),
                                file_size,
                                id
                            ],
                        )
                        .unwrap_or(0);
                    if changed != 0 {
                        done.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });
        Ok(done.load(Ordering::Relaxed) as i64)
    }

    /// Save one scanned frame. `report` tells the scan screen, which a one-off
    /// pass such as a re-detect must not: its counts are not a scan's.
    pub(crate) fn persist_frame(
        &self,
        job: i64,
        f: &FrameResult,
        settings: &Settings,
        report: bool,
    ) -> Result<()> {
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
        // The scan's lane may have written it already, to show the frame early.
        if !f.thumb_saved {
            save_jpeg(&f.thumb, &thumb)?;
        }
        let db = lock(&self.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        tx.execute("DELETE FROM detections WHERE image_id=?", [image])
            .map_err(err)?;
        for s in &f.subjects {
            insert_subject(&tx, image, s)?;
        }
        let rating = f.rating;
        tx.execute("UPDATE images SET status='done',error=NULL,width=?,height=?,camera=?,taken_at=?,sharpness=?,rating=?,sharpness_whole=?,pan_share=?,thumb_path=?,exposure_verdict=?,highlight_clip=?,shadow_clip=?,dark_frame=?,tilt_deg=?,converge_deg=?,level_verdict=?,eye_open_min=?,smile_max=?,buck_score=?,exposure_time=?,exposure_bias=?,f_number=?,focal_length=?,focus_distance=?,lens=?,iso=?,file_size=?,af_x1=COALESCE(?,af_x1),af_y1=COALESCE(?,af_y1),af_x2=COALESCE(?,af_x2),af_y2=COALESCE(?,af_y2),af_mode=CASE WHEN ? THEN ? ELSE af_mode END,af_sharpness=CASE WHEN ? THEN ? ELSE af_sharpness END WHERE id=?",params![f.size.0 as i64,f.size.1 as i64,f.camera,f.taken,f.whole,rating,f.whole_raw,f.pan_share,thumb.to_string_lossy(),f.exposure.as_ref().map(|e| e.verdict),f.exposure.as_ref().map(|e| e.highlight_clip),f.exposure.as_ref().map(|e| e.shadow_clip),f.exposure.as_ref().map(|e| e.dark_frame),f.level.as_ref().and_then(|l| l.tilt_deg),f.level.as_ref().and_then(|l| l.verticals_converge_deg),f.level.as_ref().map(|l| l.verdict),f.eye_open_min,f.smile_max,f.buck,f.exif.exposure_time,f.exif.exposure_bias,f.exif.f_number,f.exif.focal_length,f.exif.focus_distance,f.lens,f.exif.iso,f.file_size,f.af.map(|a| a.x1),f.af.map(|a| a.y1),f.af.map(|a| a.x2),f.af.map(|a| a.y2),f.af.is_some(),f.af.and_then(|a| a.mode).map(i64::from),f.af.is_some(),f.af_sharpness,image]).map_err(err)?;
        tx.commit().map_err(err)?;
        if report {
            self.report_frame(image, &thumb, f, settings);
        }
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
        let verdict = conrod_vision::sharpness::verdict_for(
            f.rating,
            settings.sharp_at,
            settings.blurred_below,
        );
        let subject = f
            .subjects
            .iter()
            .max_by(|a, b| {
                let area = |s: &crate::Subject| (s.bbox[2] - s.bbox[0]) * (s.bbox[3] - s.bbox[1]);
                area(a).total_cmp(&area(b))
            })
            .map_or("frame", |s| s.region_type);
        // "Soft" in the found counts is anything short of sharp, as it always
        // was; the strip itself can now say blurred.
        let soft = verdict != "sharp";
        let closed = f
            .subjects
            .iter()
            .filter(|s| s.cull_reason == crate::EYES_CLOSED)
            .count();
        progress.add_found(|found| {
            found.subjects += f.subjects.iter().filter(|s| !s.bystander).count() as u64;
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
            file: f
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned()),
            stars: Some(f.stars),
            boxes: f.kept.clone(),
            af: f.af.map(|a| [a.x1, a.y1, a.x2, a.y2]),
            af_kind: f.af.map(|a| crate::progress::af_kind(a.kind)),
        });
    }

    /// In an "identify" album, clear the automatic cull a scan measured: the
    /// photographer asked for identification, not culling. Only `cull_reason`
    /// is cleared -- a hand reject (`images.rejected`/`detections.rejected`),
    /// the stars, the framing and the measured scores stay as they were.
    /// Returns the number of detections the clear let back in.
    pub(crate) fn clear_automatic_culls(&self, job: i64) -> Result<usize> {
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .execute(
                "UPDATE detections SET cull_reason=NULL WHERE image_id IN (SELECT id FROM images WHERE job_id=?)",
                [job],
            )
            .map_err(err)
    }

    /// The scan's finish path: after a one-shot scan of an "identify" album has
    /// finished, drop the automatic cull. "both" and "cull" albums keep it.
    /// Returns whether anything was cleared.
    pub(crate) fn clear_automatic_culls_if_identify(&self, job: i64) -> Result<bool> {
        let cfg = crate::operations::settings(self, job)?;
        if cfg.workflow() != conrod_core::settings::WORKFLOW_IDENTIFY {
            return Ok(false);
        }
        self.clear_automatic_culls(job)?;
        Ok(true)
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
        let gap = burst_gap(cfg.burst_gap);
        // What this album's stacking rules are: the sensitivity ladder plus its
        // own pause and pan choices.
        let rules = conrod_core::bursts::MomentRules::from_sensitivity(
            &cfg.grouping_sensitivity,
            cfg.grouping_pause,
            cfg.grouping_keep_pans,
        );
        // The grouping is worked out on the reader connection, so the writer
        // is only held to store the result.
        let db = lock(&self.reader);
        let images = rows(
            &db,
            "SELECT id,path,camera,taken_at,rating,width,height,exposure_time,exposure_bias,f_number,focal_length,focus_distance FROM images WHERE job_id=? AND status='done' AND COALESCE(copy_of,0)=0 ORDER BY path",
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
        // Frames taken as a pan: one continuous action, so their moments merge.
        let mut pans: std::collections::HashSet<i64> = std::collections::HashSet::new();
        // Images whose largest subject is a vehicle: only liveries of vehicles
        // tell one car from another.
        let mut vehicles: std::collections::HashSet<i64> = std::collections::HashSet::new();
        // Frames whose detections the camera's focus area made bystanders: the
        // AF box is their subject, whatever the album's anchor setting says.
        let mut af_subject: std::collections::HashSet<i64> = std::collections::HashSet::new();
        for d in rows(
            &db,
            "SELECT d.image_id AS image,d.x1,d.y1,d.x2,d.y2,d.panning AS pan,COALESCE(d.region_type,'vehicle') AS region,COALESCE(d.bystander,0) AS bystander,COALESCE(d.af_bystander,0) AS af_bystander FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?",
            [job],
        )? {
            let image = d["image"].as_i64().unwrap_or_default();
            // A bystander is not what the frame is of: it neither anchors the
            // moment nor marks the frame as a pan.
            if d["bystander"].as_i64() == Some(1) {
                if d["af_bystander"].as_i64() == Some(1) {
                    af_subject.insert(image);
                }
                continue;
            }
            let bbox = ["x1", "y1", "x2", "y2"].map(|k| d[k].as_f64().unwrap_or_default());
            let area = (bbox[2] - bbox[0]) * (bbox[3] - bbox[1]);
            if d["pan"].as_i64() == Some(1) {
                pans.insert(image);
            }
            if main.get(&image).is_none_or(|&(seen, _)| area > seen) {
                main.insert(image, (area, bbox));
                if d["region"].as_str() == Some("vehicle") {
                    vehicles.insert(image);
                } else {
                    vehicles.remove(&image);
                }
            }
        }
        // Each image's main subject as fractions of the frame.
        let mut fractions: std::collections::HashMap<i64, [f64; 4]> = images
            .iter()
            .filter_map(|v| {
                let id = v["id"].as_i64()?;
                let (w, h) = (v["width"].as_f64()?, v["height"].as_f64()?);
                let &(_, [x1, y1, x2, y2]) = main.get(&id)?;
                (w > 0.0 && h > 0.0).then_some((id, [x1 / w, y1 / h, x2 / w, y2 / h]))
            })
            .collect();
        // A frame the detector missed but the camera focused: the AF box (already
        // fractions of the upright frame) is the moment's anchor, so such a frame
        // stays in its car's moment instead of standing beside it, unless the
        // album turned `grouping_focus_anchor` off. Only a frame with no usable
        // detection box falls back to it. The looks below keep to
        // detection boxes on purpose: an AF-only frame's livery is still the
        // car, but measuring a look from a box the detector never confirmed is a
        // larger change than this needs.
        let mut af_boxes: std::collections::HashMap<i64, [f64; 4]> =
            std::collections::HashMap::new();
        for v in rows(
            &db,
            "SELECT id,af_x1,af_y1,af_x2,af_y2 FROM images WHERE job_id=? AND af_x1 IS NOT NULL",
            [job],
        )? {
            let (Some(id), Some(x1), Some(y1), Some(x2), Some(y2)) = (
                v["id"].as_i64(),
                v["af_x1"].as_f64(),
                v["af_y1"].as_f64(),
                v["af_x2"].as_f64(),
                v["af_y2"].as_f64(),
            ) else {
                continue;
            };
            af_boxes.insert(id, [x1, y1, x2, y2]);
        }
        // A frame the focus area rates has no detection box to measure a look
        // from: it is measured on the AF box instead.
        for id in &af_subject {
            if let (false, Some(af)) = (fractions.contains_key(id), af_boxes.get(id)) {
                fractions.insert(*id, *af);
            }
        }
        // What each frame looks like, measured from its thumbnail once and kept.
        drop(db);
        let looks = crate::looks::load(&self.reader, &self.db, job, &fractions)?;
        let mut order: Vec<usize> = (0..frames.len()).collect();
        order.sort_by(|&a, &b| {
            frames[a]
                .taken
                .unwrap_or_default()
                .total_cmp(&frames[b].taken.unwrap_or_default())
                .then_with(|| frames[a].path.cmp(&frames[b].path))
        });
        let mut moment_frames: Vec<_> = {
            // An album-local id per camera: two bodies' bursts are never joined.
            let mut camera_ids: std::collections::HashMap<String, u32> =
                std::collections::HashMap::new();
            let mut next_camera = 0u32;
            order
                .iter()
                .map(|&i| {
                    let (w, h) = (images[i]["width"].as_f64(), images[i]["height"].as_f64());
                    let id = images[i]["id"].as_i64().unwrap_or_default();
                    let subject = match (main.get(&id), w, h) {
                        (Some(&(_, [x1, y1, x2, y2])), Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                            Some([x1 / w, y1 / h, x2 / w, y2 / h])
                        }
                        // The album can turn the focus-area anchor off, leaving a
                        // frame the detector missed with no subject at all.
                        _ if cfg.grouping_focus_anchor || af_subject.contains(&id) => {
                            af_boxes.get(&id).copied()
                        }
                        _ => None,
                    };
                    let camera = *camera_ids
                        .entry(frames[i].camera.clone())
                        .or_insert_with(|| {
                            next_camera += 1;
                            next_camera - 1
                        });
                    conrod_core::bursts::MomentFrame {
                        burst: i64::from(frames[i].burst),
                        camera,
                        portrait: h.is_some_and(|h| w.is_some_and(|w| h > w)),
                        taken: frames[i].taken,
                        look: looks.get(&id).map(|look| conrod_core::bursts::Look {
                            vehicle: vehicles.contains(&id),
                            ..*look
                        }),
                        subject,
                        panning: pans.contains(&id),
                    }
                })
                .collect()
        };
        // The time cut is a fixed gap: join the neighbouring time bursts that
        // are the same subject a few seconds apart before moments are cut, so
        // two stacks of one car become one. The joined group is one time burst
        // from here on, which is what the album shows as its stack.
        if cfg.grouping_merge {
            for (position, &key) in conrod_core::bursts::merge_bursts(&moment_frames)
                .iter()
                .enumerate()
            {
                moment_frames[position].burst = key;
                frames[order[position]].burst = key as u32;
            }
        }
        let moment_keys = conrod_core::bursts::split_moments_with(&moment_frames, &rules);
        let mut key = vec![0; frames.len()];
        for (&i, moment) in order.iter().zip(moment_keys) {
            key[i] = moment;
        }
        // A bracket, focus stack or interval is one stack in every preset: the
        // frames were shot to be judged together, whatever the shoot type.
        for (i, set) in sets.iter().enumerate() {
            if let Some((first, _)) = set {
                key[i] = SET_KEY_BASE + first;
            }
        }
        if !cfg.grouping_stacks {
            // Stacking turned off for this album: every frame is its own stack,
            // even a bracket's. The key is the frame's place in the order the
            // moments are numbered (time, then path), so it is dense from 1 and
            // the same on every run for the same album.
            for (position, &i) in order.iter().enumerate() {
                key[i] = position as i64 + 1;
            }
        }
        let bursts = frames
            .iter()
            .map(|frame| frame.burst)
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        let moments = key.iter().collect::<std::collections::BTreeSet<_>>().len();
        let db = lock(&self.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        for (i, frame) in frames.iter().enumerate() {
            let set = sets[i];
            tx.execute(
                "UPDATE images SET burst_key=?,time_burst=?,set_id=?,set_kind=? WHERE job_id=? AND path=?",
                params![
                    key[i],
                    i64::from(frame.burst),
                    set.map(|(id, _)| id),
                    set.map(|(_, kind)| set_kind_name(kind)),
                    job,
                    frame.path
                ],
            )
            .map_err(err)?;
        }
        // A hidden copy keeps no key of its own: nothing may see it as a burst
        // or moment member, even if it is shown again before the next regroup.
        tx.execute(
            "UPDATE images SET burst_key=NULL,time_burst=NULL,set_id=NULL,set_kind=NULL WHERE job_id=? AND COALESCE(copy_of,0)<>0",
            [job],
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        if let Some(progress) = self.progress() {
            progress.settle("group", frames.len() as u64);
        }
        // A frame with no subject in a burst where another frame has one is a
        // shot the detector missed: rate it on the whole frame before the
        // keepers below are picked on the ratings.
        crate::passes::rate_missed_frames(&db, job)?;
        let keepers = if cfg.grouping_stacks {
            // The same rule the identify stage uses, so the keeper regroup
            // leaves is the one a later pick would choose anyway.
            let picks = crate::passes::pick_of_pass(&db, job, &cfg.lanes(), cfg.use_quality_score)?;
            picks["picks"].as_u64().unwrap_or_default()
        } else {
            // Stacking is off: every frame is its own stack, so "best of the
            // stack" would mark every frame as a keeper, which tells the
            // photographer nothing. Suggest none, and take back the picks a
            // regroup made while stacking was still on.
            db.execute(
                "UPDATE detections SET burst_pick=0 WHERE image_id IN (SELECT id FROM images WHERE job_id=?)",
                [job],
            )
            .map_err(err)?;
            db.execute("UPDATE images SET burst_pick=0 WHERE job_id=?", [job])
                .map_err(err)?;
            0
        };
        task.detail(format!(
            "{bursts} bursts · {moments} moments · {keepers} keepers"
        ));
        if let Some(progress) = self.progress() {
            progress.add_found(|found| {
                found.bursts = bursts as u64;
                found.keepers = keepers;
            });
            progress.settle("pick", frames.len() as u64);
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
    db.execute("INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,sharpness,rating,rating_verdict,panning,sharp_end,cull_reason,features,heuristic,region_type,quality,attributes,cut_sides,subject_frac,lead_room,framing_score,bystander,af_bystander) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",params![image,s.bbox[0],s.bbox[1],s.bbox[2],s.bbox[3],s.class,s.conf,s.sharpness,s.rating,conrod_vision::sharpness::rating_for(s.rating,0.825,0.606),s.panning,s.sharp_end,s.cull_reason,serde_json::to_string(&s.features).map_err(err)?,s.heuristic,s.region_type,s.quality,eyes_json(s.eyes_open),s.cut_sides,s.subject_frac,s.lead_room,s.framing_score,s.bystander.then_some(1),s.bystander.then_some(1)]).map_err(err)?;
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
        let kind: String = lib.one("SELECT set_kind FROM images WHERE id=?", [images[0]]);
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
    fn a_time_burst_is_the_frames_shot_within_the_gap_per_camera() {
        use crate::testkit::Lib;
        let lib = Lib::new("time-burst");
        let (a, b, c) = (
            lib.frame("1.jpg", None),
            lib.frame("2.jpg", None),
            lib.frame("3.jpg", None),
        );
        for (taken, image) in [(100.0, a), (100.2, b), (105.0, c)] {
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![taken, image],
            );
            lib.detection(image, "vehicle", 0.6);
        }
        lib.d().regroup(lib.job).unwrap();
        let time =
            |id: i64| -> Option<i64> { lib.one("SELECT time_burst FROM images WHERE id=?", [id]) };
        assert!(time(a).is_some(), "a regrouped frame has a time burst");
        assert_eq!(time(a), time(b), "0.2 s apart is one time burst");
        assert_ne!(
            time(a),
            time(c),
            "5 s later is another time burst: {:?} vs {:?}",
            time(a),
            time(c)
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
            .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
                r.get(0)
            })
            .unwrap();
        let map: Map<String, Value> = serde_json::from_str(&stored).unwrap();
        assert!(
            map["scan_profile"]
                .as_str()
                .unwrap()
                .starts_with("portrait"),
            "{map:?}"
        );
        assert_eq!(map["blur_strictness"], "lenient");
        assert_eq!(
            map["blur_explicit"], true,
            "a sheet choice must beat the preset's default"
        );
        assert_eq!(map["burst_gap"], 2.5);

        let after = lock(&desktop.settings).clone();
        assert_eq!(after, before, "the sheet never writes the global defaults");

        // The album's own settings report what the sheet chose.
        let effective = crate::operations::settings(&desktop, job).unwrap();
        assert_eq!(effective.blur_strictness, "lenient");
        assert_eq!(effective.burst_gap, 2.5);

        // Nonsense is refused before an album exists (a folder that already is
        // an album only gains frames and takes no sheet choices).
        let fresh = root.join("fresh");
        std::fs::create_dir_all(&fresh).unwrap();
        std::fs::write(fresh.join("b.jpg"), b"y").unwrap();
        for args in [
            json!({"root": fresh.to_string_lossy(), "stage": "index", "burstGap": 0}),
            json!({"root": fresh.to_string_lossy(), "stage": "index", "blurStrictness": "loud"}),
        ] {
            assert!(desktop.dispatch("scan", args).is_err());
        }
        drop(desktop);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_new_album_copies_the_photographers_defaults_and_keeps_them_when_the_profile_changes() {
        let root = std::env::temp_dir().join(format!(
            "conrod-user-profile-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let first = root.join("first");
        let second = root.join("second");
        for dir in [&first, &second] {
            std::fs::create_dir_all(dir).unwrap();
            std::fs::write(dir.join("a.jpg"), dir.to_string_lossy().as_bytes()).unwrap();
        }
        let desktop = Desktop::open(root.clone()).unwrap();
        let saved = desktop
            .dispatch(
                "save_settings",
                json!({"user_profile": {
                    "done": true,
                    "default_preset": "motorsport-track",
                    "blur": "strict",
                    "presets": {"motorsport-track": {"faces": true}, "nonsense": {"faces": true}},
                    "junk": 1,
                }, "user_defaults": {"faces": true}}),
            )
            .unwrap();
        // Unknown keys are dropped; the global file never carries an album's copy.
        assert_eq!(
            saved["user_profile"]["presets"],
            json!({"motorsport-track": {"faces": true}})
        );
        assert!(saved["user_profile"].get("junk").is_none());
        assert_eq!(saved["user_defaults"], json!({}));
        let scan = |dir: &std::path::Path, extra: Value| {
            let mut args = json!({"root": dir.to_string_lossy(), "stage": "index", "scanProfile": "motorsport-track"});
            args.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            desktop.dispatch("scan", args).unwrap()["jobId"]
                .as_i64()
                .unwrap()
        };
        let job = scan(&first, json!({}));
        let effective = crate::operations::settings(&desktop, job).unwrap();
        assert_eq!(effective.user_defaults["faces"], true);
        assert!(
            effective.user_profile.is_empty(),
            "an album keeps a copy, not the profile"
        );
        // The photographer's default for the type: faces on at a track day.
        assert!(effective.lanes().faces && effective.lanes().people_judged);
        // The profile's blur level applies where the sheet chose none.
        assert_eq!(effective.effective_blur(), "strict");

        // The profile changes; the existing album does not.
        desktop
            .dispatch(
                "save_settings",
                json!({"user_profile": {"presets": {"motorsport-track": {"faces": false}}}}),
            )
            .unwrap();
        let effective = crate::operations::settings(&desktop, job).unwrap();
        assert!(effective.lanes().faces, "{:?}", effective.user_defaults);
        // A new album follows the new profile; a sheet choice beats the profile's blur.
        let job2 = scan(&second, json!({"blurStrictness": "lenient"}));
        let effective = crate::operations::settings(&desktop, job2).unwrap();
        assert!(!effective.lanes().faces);
        assert_eq!(effective.effective_blur(), "lenient");
        // The album's own override still wins, both ways.
        let on = crate::passes::set_feature(&desktop, job2, "faces", Some(true)).unwrap();
        assert_eq!(on["lanes"]["detects"][2]["on"], true);
        assert_eq!(on["lanes"]["detects"][2]["default"], false);
        let off = crate::passes::set_feature(&desktop, job, "faces", Some(false)).unwrap();
        assert_eq!(off["lanes"]["detects"][2]["on"], false);
        assert_eq!(off["lanes"]["detects"][2]["default"], true);
        // The policy command reports the saved profile for the import sheet.
        let sheet = desktop
            .dispatch("preset_policy", json!({"preset": "motorsport-track"}))
            .unwrap();
        assert_eq!(sheet["detects"][2]["on"], false);
        drop(desktop);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_new_album_takes_the_sheets_grouping_choices_and_normalises_them() {
        let root = std::env::temp_dir().join(format!(
            "conrod-grouping-sheet-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.jpg"), b"x").unwrap();
        let desktop = Desktop::open(root.clone()).unwrap();

        let out = desktop
            .dispatch(
                "scan",
                json!({
                    "root": root.to_string_lossy(),
                    "label": "Grouping album",
                    "stage": "index",
                    "groupingSensitivity": " EAGER ",
                    "groupingPause": 25.0,
                    "groupingKeepPans": false,
                    "groupingFocusAnchor": false,
                    "groupingStacks": false,
                }),
            )
            .unwrap();
        let job = out["jobId"].as_i64().unwrap();

        let settings = crate::operations::settings(&desktop, job).unwrap();
        assert_eq!(
            settings.grouping_sensitivity, "eager",
            "the word is normalised the way the cut reads it"
        );
        assert_eq!(
            settings.grouping_pause, 10.0,
            "a pause is clamped to the 0..=10 the cut honours"
        );
        assert!(!settings.grouping_keep_pans);
        assert!(!settings.grouping_focus_anchor);
        assert!(!settings.grouping_stacks);
        // The album stores them, so a later load keeps the sheet's choices.
        let stored: String = lock(&desktop.db)
            .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
                r.get(0)
            })
            .unwrap();
        let map: Map<String, Value> = serde_json::from_str(&stored).unwrap();
        assert_eq!(map["grouping_sensitivity"], "eager");
        assert_eq!(map["grouping_pause"], 10.0);
        assert_eq!(map["grouping_stacks"], false);

        // A sheet that sends none of them leaves today's behaviour.
        let plain_root = root.join("plain");
        std::fs::create_dir_all(&plain_root).unwrap();
        std::fs::write(plain_root.join("b.jpg"), b"x").unwrap();
        let plain = desktop
            .dispatch(
                "scan",
                json!({"root": plain_root.to_string_lossy(), "stage": "index"}),
            )
            .unwrap();
        let plain =
            crate::operations::settings(&desktop, plain["jobId"].as_i64().unwrap()).unwrap();
        assert_eq!(plain.grouping_sensitivity, "normal");
        assert_eq!(plain.grouping_pause, conrod_core::bursts::MOMENT_PAUSE);
        assert!(plain.grouping_keep_pans);
        assert!(plain.grouping_focus_anchor);
        assert!(plain.grouping_stacks);
        drop(desktop);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_album_left_scanning_by_a_crash_is_stopped_when_the_app_opens() {
        let root = std::env::temp_dir().join(format!(
            "conrod-stale-scan-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let desktop = Desktop::open(root.clone()).unwrap();
        let job = conrod_store::create_job(
            &lock(&desktop.db),
            &root,
            Some("Interrupted"),
            &json!(Settings::default()),
        )
        .unwrap();
        // One frame finished before the crash, one still waiting: the crash
        // leaves the album in `scanning`.
        lock(&desktop.db)
            .execute(
                "INSERT INTO images(job_id,path,status) VALUES(?,?,'done'),(?,?,'pending')",
                params![
                    job,
                    root.join("done.jpg").to_string_lossy(),
                    job,
                    root.join("waiting.jpg").to_string_lossy()
                ],
            )
            .unwrap();
        desktop.note_resume_stage(job, Tail::Identify);
        lock(&desktop.db)
            .execute("UPDATE jobs SET status='scanning' WHERE id=?", [job])
            .unwrap();
        drop(desktop);

        let reopened = Desktop::open(root.clone()).unwrap();
        let (status, settings): (String, String) = lock(&reopened.db)
            .query_row(
                "SELECT status,settings_json FROM jobs WHERE id=?",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "stopped", "so the Library offers Resume again");
        let settings: Map<String, Value> = serde_json::from_str(&settings).unwrap();
        assert_eq!(stored_tail(&settings), Tail::Identify, "the stage is kept");
        let (frames, done): (i64, i64) = lock(&reopened.db)
            .query_row(
                "SELECT count(*), COALESCE(SUM(status='done'),0) FROM images WHERE job_id=?",
                [job],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((frames, done), (2, 1), "nothing is deleted or re-read");
        drop(reopened);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_scan_that_could_not_read_some_frames_is_done_with_errors() {
        assert_eq!(album_status(false, false, 0, 0), "done");
        assert_eq!(
            album_status(false, false, 2, 2),
            "done_errors",
            "per-frame failures do not fail the album"
        );
        assert_eq!(album_status(true, false, 3, 1), "stopped");
        assert_eq!(
            album_status(false, false, 3, 1),
            "error",
            "frames left unread beside the failed ones are not a clean finish"
        );
        assert_eq!(album_status(false, true, 1, 0), "error");
        assert_eq!(
            album_status(false, false, 2, 0),
            "error",
            "frames left without a reason are not a clean finish"
        );

        // The payload carries the count and a few reasons for the UI.
        use crate::testkit::Lib;
        let lib = Lib::new("scan-errors");
        let bad = lib.frame("bad.cr3", None);
        let worse = lib.frame("worse.cr3", None);
        lib.sql(
            "UPDATE images SET status='error',error='unsupported RAW' WHERE id=?",
            [bad],
        );
        lib.sql(
            "UPDATE images SET status='error',error='file is missing' WHERE id=?",
            [worse],
        );
        let (errors, samples) = scan_errors(&lib.db(), lib.job);
        assert_eq!(errors, 2);
        assert_eq!(samples.len(), 2);
        assert!(samples.iter().any(|s| s.contains("unsupported RAW")));
        let jobs = lib.run("jobs", json!({})).unwrap();
        assert_eq!(jobs[0]["errors"], 2);
        assert_eq!(jobs[0]["error_samples"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_resume_keeps_the_album_stage_and_skips_finished_frames() {
        use crate::commands::ScanStage;
        let one_shot = json!({"native_scan_stage": "all", "native_resume_stage": "identify"});
        let one_shot = one_shot.as_object().unwrap();
        assert_eq!(stored_stage(one_shot), ScanStage::All);
        assert_eq!(
            stored_tail(one_shot),
            Tail::Identify,
            "a vision run that stopped resumes at vision"
        );
        let indexed = json!({"native_scan_stage": "index"});
        assert_eq!(
            stored_stage(indexed.as_object().unwrap()),
            ScanStage::Cull,
            "an album that was only indexed resumes as a cull"
        );
        let fresh = Map::new();
        assert_eq!(stored_tail(&fresh), Tail::Scan);
        assert_eq!(stored_stage(&fresh), ScanStage::Cull);
        let finished = json!({"native_resume_stage": "done"});
        assert_eq!(
            resume_tail(finished.as_object().unwrap()),
            Tail::Scan,
            "a finished album resumes from the grouping, as before"
        );

        // Only frames that are not done are read again.
        use crate::testkit::Lib;
        let lib = Lib::new("resume-stage");
        lib.frame("done.jpg", None);
        let waiting = lib.frame("waiting.jpg", None);
        lib.sql("UPDATE images SET status='pending' WHERE id=?", [waiting]);
        let paths = unfinished_paths(&lib.db(), lib.job).unwrap();
        assert_eq!(paths, vec![lib.root.join("waiting.jpg")]);
    }

    #[test]
    fn importing_a_folder_that_is_already_an_album_adds_only_new_frames() {
        let root =
            std::env::temp_dir().join(format!("conrod-reimport-{}-{}", std::process::id(), now()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.jpg"), b"x").unwrap();
        std::fs::write(root.join("b.jpg"), b"x").unwrap();
        let desktop = Desktop::open(root.clone()).unwrap();
        let scan = |desktop: &Arc<Desktop>| {
            desktop
                .dispatch(
                    "scan",
                    json!({"root": root.to_string_lossy(), "stage": "index"}),
                )
                .unwrap()
        };

        let first = scan(&desktop);
        let job = first["jobId"].as_i64().unwrap();
        assert_eq!(first["duplicates"], 0);
        assert_eq!(first["added"], 2);

        // The same folder again: no second album, nothing added, and the
        // duplicate count the sheet needs to say so.
        let again = scan(&desktop);
        assert_eq!(again["jobId"], job, "the album that already holds it");
        assert_eq!(again["duplicates"], 2);
        assert_eq!(again["added"], 0);
        assert_eq!(again["existing"], true);
        assert_eq!(again["scanning"], false);
        // Asking for a full scan changes nothing either: no frame is new.
        let culled = desktop
            .dispatch("scan", json!({"root": root.to_string_lossy()}))
            .unwrap();
        assert_eq!(
            (culled["added"].as_u64(), culled["scanning"].as_bool()),
            (Some(0), Some(false))
        );
        assert_eq!(culled["duplicates"], 2);
        assert!(desktop.progress().is_none(), "no scan was started");
        assert_eq!(
            lock(&desktop.db)
                .query_row("SELECT count(*) FROM jobs", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            lock(&desktop.db)
                .query_row("SELECT count(*) FROM images WHERE job_id=?", [job], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            2
        );

        // A frame that arrived since joins the same album.
        std::fs::write(root.join("c.jpg"), b"x").unwrap();
        let third = scan(&desktop);
        assert_eq!(third["jobId"], job);
        assert_eq!(third["duplicates"], 2);
        assert_eq!(third["added"], 1);
        assert_eq!(
            lock(&desktop.db)
                .query_row("SELECT count(*) FROM images WHERE job_id=?", [job], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            3
        );
        drop(desktop);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn only_cull_mode_skips_the_identification_lane() {
        assert!(wants_identify("both", true, true));
        assert!(wants_identify("identify", true, true));
        assert!(
            !wants_identify("cull", true, true),
            "cull mode is today's scan without identification"
        );
        assert!(
            !wants_identify(" CULL ", true, true),
            "the word is normalised before it is judged"
        );
        assert!(
            !wants_identify("identify", false, true),
            "the preset says so"
        );
        assert!(
            !wants_identify("identify", true, false),
            "identification without plates or numbers reads nothing"
        );
        assert!(wants_identify("nonsense", true, true), "unknown is both");
    }

    #[test]
    fn the_identify_lane_stays_on_the_workflow_gate_whatever_the_scope() {
        // Scope picks which subjects the lane works on, never whether it runs:
        // a "cull" album runs neither, an "everything" album still runs.
        for scope in ["keepers", "everything", " EVERYTHING "] {
            let settings = Settings {
                identify_scope: scope.into(),
                ..Default::default()
            };
            assert!(wants_identify(settings.workflow(), true, true));
            assert!(
                !wants_identify("cull", true, true),
                "{scope} does not turn identification on for a cull album"
            );
            assert_eq!(
                settings.identify_scope(),
                if scope.trim().eq_ignore_ascii_case("everything") {
                    "everything"
                } else {
                    "keepers"
                }
            );
        }
    }

    #[test]
    fn an_identify_album_stores_its_workflow_and_clears_the_automatic_cull() {
        let root = std::env::temp_dir().join(format!(
            "conrod-workflow-sheet-{}-{}",
            std::process::id(),
            now()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.jpg"), b"x").unwrap();
        let desktop = Desktop::open(root.clone()).unwrap();

        let out = desktop
            .dispatch(
                "scan",
                json!({
                    "root": root.to_string_lossy(),
                    "label": "Identify album",
                    "stage": "index",
                    "workflowMode": " IDENTIFY ",
                    "identifyScope": " Everything ",
                }),
            )
            .unwrap();
        let job = out["jobId"].as_i64().unwrap();
        let stored: String = lock(&desktop.db)
            .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| {
                r.get(0)
            })
            .unwrap();
        let map: Map<String, Value> = serde_json::from_str(&stored).unwrap();
        assert_eq!(
            map["workflow_mode"], "identify",
            "the sheet's word is stored normalised"
        );
        assert_eq!(
            map["identify_scope"], "everything",
            "the sheet's scope is stored normalised, and it is separate from the workflow"
        );
        assert_eq!(
            crate::operations::settings(&desktop, job)
                .unwrap()
                .identify_scope(),
            "everything"
        );
        assert_eq!(
            crate::operations::settings(&desktop, job)
                .unwrap()
                .workflow(),
            "identify"
        );

        // What the scan measured: one soft subject culled automatically, one
        // subject a person rejected by hand.
        let image: i64 = lock(&desktop.db)
            .query_row("SELECT id FROM images WHERE job_id=?", [job], |r| r.get(0))
            .unwrap();
        let (soft, rejected) = {
            let db = lock(&desktop.db);
            db.execute(
                "INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,sharpness,rating,region_type,cull_reason) VALUES(?,1000,1000,3000,2000,'car',0.9,0.2,0.2,'vehicle','too blurred')",
                [image],
            )
            .unwrap();
            let soft = db.last_insert_rowid();
            db.execute(
                "INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,sharpness,rating,region_type,rejected) VALUES(?,1000,1000,3000,2000,'car',0.9,0.9,0.9,'vehicle',1)",
                [image],
            )
            .unwrap();
            (soft, db.last_insert_rowid())
        };

        // The scan's finish path, before regrouping and the identify lane.
        assert!(
            desktop.clear_automatic_culls_if_identify(job).unwrap(),
            "identify mode clears the automatic cull"
        );
        assert_eq!(
            lock(&desktop.db)
                .query_row(
                    "SELECT cull_reason FROM detections WHERE id=?",
                    [soft],
                    |r| { r.get::<_, Option<String>>(0) }
                )
                .unwrap(),
            None,
            "no detection is left culled"
        );
        assert_eq!(
            lock(&desktop.db)
                .query_row(
                    "SELECT rejected FROM detections WHERE id=?",
                    [rejected],
                    |r| { r.get::<_, i64>(0) }
                )
                .unwrap(),
            1,
            "a hand reject is not an automatic cull"
        );

        // A sheet that sends no workflow leaves today's pipeline: a measured
        // cull stays.
        let plain_root = root.join("plain");
        std::fs::create_dir_all(&plain_root).unwrap();
        std::fs::write(plain_root.join("b.jpg"), b"x").unwrap();
        let plain = desktop
            .dispatch(
                "scan",
                json!({"root": plain_root.to_string_lossy(), "stage": "index"}),
            )
            .unwrap();
        let plain_job = plain["jobId"].as_i64().unwrap();
        assert_eq!(
            crate::operations::settings(&desktop, plain_job)
                .unwrap()
                .workflow(),
            "both",
            "missing is today's full pipeline"
        );
        let plain_image: i64 = lock(&desktop.db)
            .query_row("SELECT id FROM images WHERE job_id=?", [plain_job], |r| {
                r.get(0)
            })
            .unwrap();
        let plain_soft = {
            let db = lock(&desktop.db);
            db.execute(
                "INSERT INTO detections(image_id,x1,y1,x2,y2,cls,conf,sharpness,rating,region_type,cull_reason) VALUES(?,1000,1000,3000,2000,'car',0.9,0.2,0.2,'vehicle','too blurred')",
                [plain_image],
            )
            .unwrap();
            db.last_insert_rowid()
        };
        assert!(!desktop
            .clear_automatic_culls_if_identify(plain_job)
            .unwrap());
        assert_eq!(
            lock(&desktop.db)
                .query_row(
                    "SELECT cull_reason FROM detections WHERE id=?",
                    [plain_soft],
                    |r| r.get::<_, Option<String>>(0)
                )
                .unwrap(),
            Some("too blurred".into()),
            "both keeps the scan's cull"
        );

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
            lib.detection(f2, "vehicle", 0.86),
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
        let lanes = Settings::default().lanes();
        crate::passes::pick_of_pass(&lock(&lib.d().db), lib.job, &lanes, false).unwrap();
        assert_eq!(
            lib.one::<i64>("SELECT id FROM detections WHERE burst_pick=1", []),
            strong
        );
        let _ = weak;
    }

    #[test]
    fn regroup_splits_a_burst_where_the_subject_cuts_and_picks_once_per_moment() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-moments");
        // Eight frames, 0.2 s apart, in one burst: four of a car centred in the
        // frame, then four of another far off to one side -- a cut, not a drift.
        for i in 0..8 {
            let image = lib.frame(&format!("{i}.jpg"), None);
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![f64::from(i) * 0.2, image],
            );
            let detection = lib.detection(image, "vehicle", 0.8);
            let scale = 0.1_f64.sqrt();
            let (w, h) = (6000.0 * scale, 4000.0 * scale);
            let (cx, cy) = if i < 4 {
                (3000.0, 2000.0)
            } else {
                (800.0, 3200.0)
            };
            lib.sql(
                "UPDATE detections SET x1=?,y1=?,x2=?,y2=? WHERE id=?",
                params![
                    cx - w / 2.0,
                    cy - h / 2.0,
                    cx + w / 2.0,
                    cy + h / 2.0,
                    detection
                ],
            );
        }
        lib.d().regroup(lib.job).unwrap();
        let keys: i64 = lib.one(
            "SELECT count(DISTINCT burst_key) FROM images WHERE job_id=?",
            [lib.job],
        );
        assert_eq!(keys, 2, "a cut must split the burst in two");
        let picks: i64 = lib.one("SELECT count(*) FROM detections WHERE burst_pick=1", []);
        assert_eq!(picks, keys, "one pick per burst_key");
        let one_each: i64 = lib.one(
            "SELECT count(*) FROM (SELECT i.burst_key FROM detections d JOIN images i ON i.id=d.image_id WHERE d.burst_pick=1 GROUP BY i.burst_key HAVING count(*)=1)",
            [],
        );
        assert_eq!(one_each, keys, "every key gets exactly one pick");
    }

    #[test]
    fn regroup_uses_the_albums_own_grouping_sensitivity() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-sensitivity");
        // One time burst, 0.2 s apart: four frames of a car centred in the
        // frame, then four of another far off to one side -- a cut.
        for i in 0..8 {
            let image = lib.frame(&format!("{i}.jpg"), None);
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![f64::from(i) * 0.2, image],
            );
            let detection = lib.detection(image, "vehicle", 0.8);
            let scale = 0.1_f64.sqrt();
            let (w, h) = (6000.0 * scale, 4000.0 * scale);
            let (cx, cy) = if i < 4 {
                (3000.0, 2000.0)
            } else {
                (800.0, 3200.0)
            };
            lib.sql(
                "UPDATE detections SET x1=?,y1=?,x2=?,y2=? WHERE id=?",
                params![
                    cx - w / 2.0,
                    cy - h / 2.0,
                    cx + w / 2.0,
                    cy + h / 2.0,
                    detection
                ],
            );
        }
        let keys = |lib: &Lib| -> i64 {
            lib.one(
                "SELECT count(DISTINCT burst_key) FROM images WHERE job_id=?",
                [lib.job],
            )
        };
        let choose = |lib: &Lib, word: &str| {
            lib.run(
                "update_job_settings",
                json!({"jobId": lib.job, "patch": {"grouping_sensitivity": word}}),
            )
            .unwrap();
        };
        choose(&lib, "eager");
        lib.d().regroup(lib.job).unwrap();
        assert_eq!(keys(&lib), 2, "eager splits the burst at the other car");
        choose(&lib, "off");
        lib.d().regroup(lib.job).unwrap();
        assert_eq!(keys(&lib), 1, "off stacks the whole time burst");
        let stored: String = lib.one("SELECT settings_json FROM jobs WHERE id=?", [lib.job]);
        assert!(
            stored.contains("\"grouping_sensitivity\":\"off\""),
            "the album stores its own choice: {stored}"
        );
    }

    #[test]
    fn look_alike_bursts_join_unless_the_album_turns_it_off() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-merge");
        // Two frames 4 s apart: the time cut (1.5 s) makes two bursts, but the
        // same camera, size, scene, box and livery say one subject.
        let thumb = lib.root.join("thumb.jpg");
        let image = image::RgbImage::from_fn(420, 280, |x, y| {
            let inside = (150..270).contains(&x) && (110..170).contains(&y);
            image::Rgb(if inside { [200, 30, 30] } else { [90, 90, 92] })
        });
        image.save(&thumb).unwrap();
        let mut frames = Vec::new();
        for (i, taken) in [(0, 0.0), (1, 4.0)] {
            let id = lib.frame(&format!("{i}.jpg"), None);
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=?,thumb_path=? WHERE id=?",
                params![taken, thumb.to_string_lossy(), id],
            );
            lib.detection(id, "vehicle", 0.8);
            frames.push(id);
        }
        let key =
            |image: i64| -> i64 { lib.one("SELECT burst_key FROM images WHERE id=?", [image]) };

        lib.d().regroup(lib.job).unwrap();
        assert_eq!(
            key(frames[0]),
            key(frames[1]),
            "look-alikes join by default"
        );

        lib.run(
            "update_job_settings",
            json!({"jobId": lib.job, "patch": {"grouping_merge": false}}),
        )
        .unwrap();
        assert_ne!(
            key(frames[0]),
            key(frames[1]),
            "the album can keep the time cut final"
        );
        let stored: String = lib.one("SELECT settings_json FROM jobs WHERE id=?", [lib.job]);
        assert!(
            stored.contains("\"grouping_merge\":false"),
            "the album stores its own choice: {stored}"
        );
    }

    #[test]
    fn turning_stacking_off_gives_every_frame_its_own_stack_and_no_keeper() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-no-stacks");
        // Three frames a half second apart: one time burst, one moment.
        let mut frames = Vec::new();
        for i in 0..3 {
            let image = lib.frame(&format!("{i}.jpg"), None);
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![f64::from(i) * 0.5, image],
            );
            lib.detection(image, "vehicle", 0.8);
            frames.push(image);
        }
        let keys = || -> Vec<i64> {
            frames
                .iter()
                .map(|&image| lib.one("SELECT burst_key FROM images WHERE id=?", [image]))
                .collect()
        };
        let stacks = |on: bool| {
            lib.run(
                "update_job_settings",
                json!({"jobId": lib.job, "patch": {"grouping_stacks": on}}),
            )
            .unwrap();
        };
        stacks(false);
        let counts = lib.d().regroup(lib.job).unwrap();
        let off = keys();
        assert_eq!(
            counts["moments"], 3,
            "every frame is its own stack: {counts}"
        );
        assert_eq!(
            off,
            vec![1, 2, 3],
            "dense keys in the order the moments are numbered"
        );
        assert_eq!(
            lib.one::<i64>("SELECT count(*) FROM detections WHERE burst_pick=1", []),
            0,
            "no keeper is suggested when nothing is stacked"
        );
        assert_eq!(
            lib.one::<i64>(
                "SELECT count(*) FROM images WHERE job_id=? AND COALESCE(burst_pick,0)<>0",
                [lib.job]
            ),
            0
        );
        // The same album regrouped again numbers the same frames the same way.
        lib.d().regroup(lib.job).unwrap();
        assert_eq!(keys(), off, "the keys are deterministic");
        // Stacking back on: one stack, and a keeper.
        stacks(true);
        let counts = lib.d().regroup(lib.job).unwrap();
        assert_eq!(counts["moments"], 1, "{counts}");
        assert_eq!(
            lib.one::<i64>(
                "SELECT count(DISTINCT burst_key) FROM images WHERE job_id=?",
                [lib.job]
            ),
            1
        );
        assert!(counts["keepers"].as_u64().unwrap() >= 1, "{counts}");
        assert_eq!(
            lib.one::<i64>("SELECT count(*) FROM detections WHERE burst_pick=1", []),
            1,
            "one stack, one keeper"
        );
    }

    #[test]
    fn the_camera_focus_area_keeps_a_missed_frame_in_its_cars_moment() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-af");
        // One burst: car A centred, a frame the detector missed, then car B far
        // off to one side. The missed frame's AF box sits where B is, which is
        // what the camera focused on.
        let a = lib.frame("a.jpg", None);
        let bare = lib.frame("bare.jpg", None);
        let b = lib.frame("b.jpg", None);
        for (taken, image) in [(0.0, a), (0.2, bare), (0.4, b)] {
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![taken, image],
            );
        }
        lib.detection(a, "vehicle", 0.8);
        lib.detection(b, "vehicle", 0.8);
        lib.sql(
            "UPDATE detections SET x1=3600,y1=2400,x2=4800,y2=3200 WHERE image_id=?",
            [b],
        );
        lib.sql(
            "UPDATE images SET af_x1=0.6,af_y1=0.6,af_x2=0.8,af_y2=0.8 WHERE id=?",
            [bare],
        );

        lib.d().regroup(lib.job).unwrap();
        let key =
            |image: i64| -> i64 { lib.one("SELECT burst_key FROM images WHERE id=?", [image]) };
        assert_ne!(key(a), key(b), "two cars, two moments");
        assert_eq!(
            key(bare),
            key(b),
            "the AF box anchors the missed frame to the car it focused on"
        );

        // Without the AF box the missed frame has no subject to be anchored by:
        // it joins the moment open before it, and B's car stands apart from it.
        lib.sql(
            "UPDATE images SET af_x1=NULL,af_y1=NULL,af_x2=NULL,af_y2=NULL WHERE id=?",
            [bare],
        );
        lib.d().regroup(lib.job).unwrap();
        assert_eq!(key(bare), key(a), "a box-less frame joins the open moment");
        assert_ne!(key(bare), key(b), "and stays apart from the car it was of");
    }

    #[test]
    fn the_albums_focus_anchor_choice_decides_where_a_missed_frame_goes() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-af-off");
        let a = lib.frame("a.jpg", None);
        let bare = lib.frame("bare.jpg", None);
        let b = lib.frame("b.jpg", None);
        for (taken, image) in [(0.0, a), (0.2, bare), (0.4, b)] {
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![taken, image],
            );
        }
        lib.detection(a, "vehicle", 0.8);
        lib.detection(b, "vehicle", 0.8);
        lib.sql(
            "UPDATE detections SET x1=3600,y1=2400,x2=4800,y2=3200 WHERE image_id=?",
            [b],
        );
        lib.sql(
            "UPDATE images SET af_x1=0.6,af_y1=0.6,af_x2=0.8,af_y2=0.8 WHERE id=?",
            [bare],
        );
        lib.run(
            "update_job_settings",
            json!({"jobId": lib.job, "patch": {"grouping_focus_anchor": false}}),
        )
        .unwrap();
        lib.d().regroup(lib.job).unwrap();
        let key =
            |image: i64| -> i64 { lib.one("SELECT burst_key FROM images WHERE id=?", [image]) };
        assert_eq!(
            key(bare),
            key(a),
            "anchor off: the missed frame has no subject and joins the open moment"
        );
        assert_ne!(
            key(bare),
            key(b),
            "and is not anchored to the car the camera focused on"
        );
    }

    #[test]
    fn a_frame_the_focus_area_rates_is_anchored_by_it_not_by_its_bystander() {
        use crate::testkit::Lib;
        let lib = Lib::new("regroup-af-bystander");
        let a = lib.frame("a.jpg", None);
        let x = lib.frame("x.jpg", None);
        let b = lib.frame("b.jpg", None);
        for (taken, image) in [(0.0, a), (0.2, x), (0.4, b)] {
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                params![taken, image],
            );
        }
        lib.detection(a, "vehicle", 0.8);
        lib.detection(b, "vehicle", 0.8);
        lib.sql(
            "UPDATE detections SET x1=3600,y1=2400,x2=4800,y2=3200 WHERE image_id=?",
            [b],
        );
        // A detection that looks like car A's, but the camera focused on B's.
        let truck = lib.detection(x, "vehicle", 0.8);
        lib.sql(
            "UPDATE detections SET bystander=1,af_bystander=1 WHERE id=?",
            [truck],
        );
        lib.sql(
            "UPDATE images SET af_x1=0.6,af_y1=0.6,af_x2=0.8,af_y2=0.8 WHERE id=?",
            [x],
        );
        // The album's anchor choice is about frames the detector missed; a
        // frame whose subject the camera named is anchored by it regardless.
        lib.run(
            "update_job_settings",
            json!({"jobId": lib.job, "patch": {"grouping_focus_anchor": false}}),
        )
        .unwrap();
        lib.d().regroup(lib.job).unwrap();
        let key =
            |image: i64| -> i64 { lib.one("SELECT burst_key FROM images WHERE id=?", [image]) };
        assert_eq!(key(x), key(b), "the AF box is the subject");
        assert_ne!(key(x), key(a), "not the bystander that looks like car A");
    }

    #[test]
    fn the_review_payload_carries_the_camera_focus_box() {
        use crate::testkit::Lib;
        let lib = Lib::new("review-af");
        let focused = lib.frame("focused.jpg", None);
        let bare = lib.frame("bare.jpg", None);
        lib.sql(
            "UPDATE images SET af_x1=0.05,af_y1=0.1,af_x2=0.45,af_y2=0.55 WHERE id=?",
            [focused],
        );

        let review = lib.run("review", json!({"jobId": lib.job})).unwrap();
        let frames = review["frames"].as_array().unwrap();
        let shown = |id: i64| {
            frames
                .iter()
                .find(|frame| frame["id"].as_i64() == Some(id))
                .unwrap_or_else(|| panic!("frame {id} is in the payload: {review}"))
        };
        let box_ = shown(focused);
        assert_eq!(box_["af_x1"].as_f64(), Some(0.05));
        assert_eq!(box_["af_y1"].as_f64(), Some(0.1));
        assert_eq!(box_["af_x2"].as_f64(), Some(0.45));
        assert_eq!(box_["af_y2"].as_f64(), Some(0.55));
        let none = shown(bare);
        for column in ["af_x1", "af_y1", "af_x2", "af_y2"] {
            assert!(
                none[column].is_null(),
                "a frame the camera's focus area was not read for has no {column}: {none}"
            );
        }
    }

    #[test]
    fn a_hidden_copy_stays_out_of_the_review_payload_and_the_album_counts() {
        use crate::testkit::Lib;
        let lib = Lib::new("review-copy");
        let frame = lib.frame("frame.jpg", Some(1));
        let copy = lib.frame("copy.jpg", Some(1));
        lib.detection(frame, "vehicle", 0.9);
        lib.detection(copy, "vehicle", 0.8);
        lib.sql(
            "UPDATE images SET copy_of=? WHERE id=?",
            params![frame, copy],
        );

        let review = lib.run("review", json!({"jobId": lib.job})).unwrap();
        let frames = review["frames"].as_array().unwrap();
        assert_eq!(frames.len(), 1, "{review}");
        assert_eq!(frames[0]["id"].as_i64(), Some(frame));
        let detections = review["detections"].as_array().unwrap();
        assert!(
            detections
                .iter()
                .all(|d| d["image_id"].as_i64() == Some(frame)),
            "the copy's detection is hidden too: {review}"
        );

        let jobs = lib.run("jobs", json!({})).unwrap();
        let album = jobs
            .as_array()
            .unwrap()
            .iter()
            .find(|j| j["id"].as_i64() == Some(lib.job))
            .unwrap();
        assert_eq!(
            album["total"].as_i64(),
            Some(1),
            "the copy is not a frame of the album: {album}"
        );
        assert_eq!(album["copies"].as_i64(), Some(1), "{album}");
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
        const FRAME_SQL: &str = "SELECT i.id,i.path,i.status,i.thumb_path,i.preview_path,i.width,i.height,i.rating,i.rejected,i.picked,i.burst_key,i.dupe_key,i.error, COALESCE(i.stars,(SELECT max(d.stars) FROM detections d WHERE d.image_id=i.id)) AS manual_stars, COALESCE((SELECT max(d.burst_pick) FROM detections d WHERE d.image_id=i.id AND d.burst_pick=1), i.burst_pick) AS burst_pick, i.set_id,i.set_kind,i.moments,i.exposure_verdict,i.highlight_clip,i.shadow_clip,i.dark_frame,i.tilt_deg,i.converge_deg,i.level_verdict,i.eye_open_min,i.smile_max,i.buck_score,i.taste,i.comp_placement,i.comp_mode,i.comp_balance_x,i.comp_balance_y,i.comp_symmetry,i.comp_clutter,i.comp_lines,i.comp_subject_share,i.comp_at,i.af_x1,i.af_y1,i.af_x2,i.af_y2,i.camera,i.lens,i.iso,i.file_size,i.exposure_time,i.exposure_bias,i.f_number,i.focal_length,i.focus_distance,i.taken_at FROM images i WHERE job_id=? AND COALESCE(i.copy_of,0)=0 ORDER BY path";
        const DET_SQL: &str = "SELECT d.id,d.image_id,d.cls,d.crop_path,d.x1,d.y1,d.x2,d.y2,d.sharpness,d.panning,d.number,d.plate,d.cull_reason,d.attributes,d.burst_pick,d.region_type,d.reviewed,d.rejected,d.bystander,d.stars,d.group_key,d.group_size,d.group_agreement,d.embedding,d.rating,d.rating_verdict,d.sharp_end,d.quality,d.cut_sides,d.subject_frac,d.lead_room,d.framing_score FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(i.copy_of,0)=0 ORDER BY d.image_id,d.id";

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

    #[test]
    fn the_review_payload_carries_the_shot_details() {
        use crate::testkit::Lib;
        let lib = Lib::new("review-shot-details");
        let image = lib.frame("1.jpg", None);
        lib.sql(
            "UPDATE images SET camera='Canon EOS R7 358',lens='EF100-400mm F4.5-5.6L IS II USM',
                    iso=400,file_size=123456,exposure_time=0.001,exposure_bias=-0.3,
                    f_number=5.6,focal_length=400,focus_distance=7.5,taken_at=1760000000
              WHERE id=?",
            params![image],
        );
        let out = lib.run("review", json!({"jobId": lib.job})).unwrap();
        let frame = &out["frames"][0];
        assert_eq!(frame["camera"], "Canon EOS R7 358");
        assert_eq!(frame["lens"], "EF100-400mm F4.5-5.6L IS II USM");
        assert_eq!(frame["iso"], 400);
        assert_eq!(frame["file_size"], 123456);
        assert_eq!(frame["exposure_time"].as_f64(), Some(0.001));
        assert_eq!(frame["exposure_bias"].as_f64(), Some(-0.3));
        assert_eq!(frame["f_number"].as_f64(), Some(5.6));
        assert_eq!(frame["focal_length"].as_f64(), Some(400.0));
        assert_eq!(frame["focus_distance"].as_f64(), Some(7.5));
        assert_eq!(frame["taken_at"].as_f64(), Some(1760000000.0));
    }

    #[test]
    fn shoot_stats_counts_the_album_and_backfills_an_old_row() {
        use crate::testkit::Lib;
        let lib = Lib::new("shoot-stats");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(2)),
            lib.frame("3.jpg", Some(2)),
        );
        lib.sql(
            "UPDATE images SET camera='Canon EOS R7 358',lens='EF100-400mm',iso=400,
                    file_size=1000,f_number=5.6,exposure_time=0.001,focal_length=400,
                    taken_at=100,stars=5,af_x1=0.20,af_y1=0.30,af_x2=0.30,af_y2=0.40,
                    af_sharpness=0.9,time_burst=1
              WHERE id=?",
            params![f1],
        );
        lib.sql(
            "UPDATE images SET camera='Nikon Z9',lens='NIKKOR Z 400mm',iso=800,
                    file_size=2000,f_number=4.0,exposure_time=0.002,focal_length=400,
                    taken_at=200,rejected=1,af_x1=0.70,af_y1=0.70,af_x2=0.80,af_y2=0.80,
                    af_sharpness=0.3,time_burst=2
              WHERE id=?",
            params![f2],
        );
        lib.sql(
            "UPDATE images SET camera='Canon EOS R7 358',lens='EF100-400mm',iso=400,
                    file_size=4000,f_number=5.6,exposure_time=0.001,focal_length=400,
                    burst_pick=1,af_x1=0.10,af_y1=0.10,af_x2=0.20,af_y2=0.20,
                    time_burst=2
              WHERE id=?",
            params![f3],
        );
        // A hidden copy is counted apart and never in the shoot's own totals.
        let copy = lib.frame("copy.jpg", Some(1));
        lib.sql(
            "UPDATE images SET copy_of=?,camera='Canon EOS R7 358',file_size=9999 WHERE id=?",
            params![f1, copy],
        );
        // A row an older scan left without shot details, backed by a real file.
        std::fs::write(lib.root.join("old.jpg"), b"backfill").unwrap();
        let old = lib.frame("old.jpg", None);
        let (d1, d2, d3) = (
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f2, "vehicle", 0.9),
            lib.detection(f1, "vehicle", 0.9),
        );
        lib.sql(
            "UPDATE detections SET panning=1,sharpness_verdict='sharp',burst_pick=1,stars=4 WHERE id=?",
            [d1],
        );
        lib.sql(
            "UPDATE detections SET sharpness_verdict='soft',stars=2 WHERE id=?",
            [d2],
        );
        lib.sql(
            "UPDATE detections SET sharpness_verdict='blurred',stars=3 WHERE id=?",
            [d3],
        );

        let out = lib.run("shoot_stats", json!({"jobId": lib.job})).unwrap();
        assert_eq!(out["frames"], 4, "four visible done originals");
        assert_eq!(out["copies_hidden"], 1);
        assert_eq!(out["size_bytes"], 7008, "the old row's real 8-byte file");
        assert_eq!(out["backfilled"], 1, "only the old row had to be read");
        assert_eq!(
            out["cameras"],
            json!([
                {"name": "Canon EOS R7 358", "count": 2},
                {"name": "Nikon Z9", "count": 1}
            ])
        );
        assert_eq!(
            out["lenses"],
            json!([
                {"name": "EF100-400mm", "count": 2},
                {"name": "NIKKOR Z 400mm", "count": 1}
            ])
        );
        assert_eq!(
            out["most_used"],
            json!({
                "camera": "Canon EOS R7 358",
                "lens": "EF100-400mm",
                "f_number": 5.6,
                "shutter": 0.001,
                "iso": 400,
                "focal_length": 400.0
            })
        );
        assert_eq!(
            out["f_numbers"],
            json!([{"value": 5.6, "count": 2}, {"value": 4.0, "count": 1}])
        );
        assert_eq!(
            out["isos"],
            json!([{"value": 400, "count": 2}, {"value": 800, "count": 1}])
        );
        assert_eq!(out["focal_lengths"], json!([{"value": 400.0, "count": 3}]));
        assert_eq!(
            out["shutters"],
            json!([{"value": 0.001, "count": 2}, {"value": 0.002, "count": 1}])
        );
        assert_eq!(out["pans"], 1, "one frame has a panning detection");
        assert_eq!(out["bursts"], 2, "two distinct burst keys");
        assert_eq!(out["blurred"], 1);
        assert_eq!(out["soft"], 1);
        assert_eq!(out["sharp"], 1);
        assert_eq!(
            out["af"],
            json!({"frames": 4, "missing": 1, "off_subject": 1, "no_subject": 1, "soft": 1})
        );
        assert_eq!(out["keepers"], 2, "a picked frame and a picked subject");
        assert_eq!(out["rejected"], 1);
        assert_eq!(
            out["stars"],
            json!([
                {"value": 0, "count": 2},
                {"value": 2, "count": 1},
                {"value": 5, "count": 1}
            ]),
            "a hand star outranks a subject's, and both outrank nothing"
        );
        assert_eq!(out["taken_from"].as_f64(), Some(100.0));
        assert_eq!(out["taken_to"].as_f64(), Some(200.0));

        // The old row now knows its size, and a second call has nothing to do.
        let (size, lens, iso): (i64, Option<String>, Option<i64>) = lib
            .db()
            .query_row(
                "SELECT file_size, lens, iso FROM images WHERE id=?",
                [old],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((size, lens, iso), (8, None, None));
        let again = lib.run("shoot_stats", json!({"jobId": lib.job})).unwrap();
        assert_eq!(again["backfilled"], 0);
        assert_eq!(again["size_bytes"], out["size_bytes"]);
    }

    #[test]
    fn global_stats_aggregates_every_album_and_only_complete_gear() {
        use crate::testkit::Lib;
        let lib = Lib::new("global-stats");
        // `Lib` only opens one album; the second is this library's own next job
        // in the same database.
        let job2 = conrod_store::create_job(
            &lib.db(),
            &lib.root,
            Some("Second"),
            &json!(Settings::default()),
        )
        .unwrap();
        let frame = |job: i64, name: &str, burst: Option<i64>| -> i64 {
            let db = lib.db();
            db.execute(
                "INSERT INTO images(job_id,path,status,width,height,burst_key,time_burst)
                 VALUES(?,?,'done',6000,4000,?,?)",
                params![job, lib.root.join(name).to_string_lossy(), burst, burst],
            )
            .unwrap();
            db.last_insert_rowid()
        };
        let job1 = lib.job;
        let (a1, a2, a3, a4, a5, a6) = (
            frame(job1, "a1.cr3", Some(1)),
            frame(job1, "a2.cr3", Some(2)),
            frame(job1, "a3.cr3", Some(2)),
            frame(job1, "a4.cr3", None),
            frame(job1, "a5.cr3", None),
            frame(job1, "a6.cr3", Some(3)),
        );
        let (b1, b2) = (
            frame(job2, "b1.cr3", Some(1)),
            frame(job2, "b2.cr3", Some(1)),
        );

        // Album 1 records the whole gear chain on every visible frame, so it is
        // the one album the gear and exposure lists may count.
        let canon = "UPDATE images SET camera='Canon EOS R7 358', lens='RF-S 18-150mm',
                iso=?, f_number=?, exposure_time=?, focal_length=?, file_size=?,
                taken_at=?, stars=? WHERE id=?";
        lib.sql(canon, params![100, 6.3, 0.008, 50.0, 1000, 1000, 0, a1]);
        lib.sql(canon, params![100, 6.3, 0.008, 50.0, 1000, 1010, 0, a2]);
        lib.sql(canon, params![100, 6.3, 0.008, 50.0, 1000, 1020, 0, a3]);
        lib.sql(canon, params![100, 6.3, 0.008, 150.0, 1000, 1030, 0, a4]);
        lib.sql(canon, params![200, 8.0, 0.004, 150.0, 1000, 1040, 0, a5]);
        lib.sql(canon, params![200, 8.0, 0.004, 150.0, 1000, 1050, 5, a6]);
        // Album 2 is incomplete: its second frame lost the lens in the header,
        // so none of its gear or exposure may reach the library lists.
        let nikon = "UPDATE images SET camera='Nikon Z9', lens=?, iso=800, f_number=4.0,
                exposure_time=0.002, focal_length=400.0, file_size=?, taken_at=?,
                rejected=? WHERE id=?";
        lib.sql(nikon, params!["NIKKOR Z 400mm", 2000, 2000, 0, b1]);
        lib.sql(nikon, params![None::<String>, 3000, 2300, 1, b2]);

        // Picks: a3 overrides the engine's suggestion in its burst (a2); a4 is
        // picked with no burst at all; a5 is suggested in the same NULL burst
        // and must not be suppressed by a4; a6 is only hand-starred.
        lib.sql("UPDATE images SET picked=1 WHERE id=?", [a3]);
        lib.sql("UPDATE images SET picked=1 WHERE id=?", [a4]);
        // A hidden copy is counted apart and never in the library's totals.
        let copy = frame(job1, "a1-copy.cr3", None);
        lib.sql(
            "UPDATE images SET copy_of=?, camera='Canon EOS R7 358', lens='RF-S 18-150mm',
                    iso=100, f_number=6.3, exposure_time=0.008, focal_length=50.0,
                    file_size=9999, taken_at=5000 WHERE id=?",
            params![a1, copy],
        );

        // Autofocus: a1/a3/a5/a6/b1 inside their subject, a2 beside it, b2 with
        // no subject at all, a4 without an AF area.
        let af_on = "UPDATE images SET af_x1=0.20, af_y1=0.30, af_x2=0.30, af_y2=0.40,
                af_sharpness=? WHERE id=?";
        lib.sql(af_on, params![0.3, a1]);
        lib.sql(af_on, params![0.9, a3]);
        lib.sql(af_on, params![None::<f64>, a5]);
        lib.sql(af_on, params![None::<f64>, a6]);
        lib.sql(af_on, params![0.9, b1]);
        lib.sql(af_on, params![None::<f64>, b2]);
        lib.sql(
            "UPDATE images SET af_x1=0.70, af_y1=0.70, af_x2=0.80, af_y2=0.80 WHERE id=?",
            [a2],
        );

        let (d1, d2, d3, d4, d5, d6) = (
            lib.detection(a1, "vehicle", 0.9),
            lib.detection(a2, "vehicle", 0.9),
            lib.detection(a3, "vehicle", 0.9),
            lib.detection(a5, "vehicle", 0.9),
            lib.detection(a6, "vehicle", 0.9),
            lib.detection(b1, "vehicle", 0.9),
        );
        lib.sql(
            "UPDATE detections SET sharpness_verdict='sharp', panning=1 WHERE id=?",
            [d1],
        );
        lib.sql(
            "UPDATE detections SET sharpness_verdict='soft', burst_pick=1 WHERE id=?",
            [d2],
        );
        lib.sql(
            "UPDATE detections SET sharpness_verdict='sharp' WHERE id=?",
            [d3],
        );
        lib.sql(
            "UPDATE detections SET sharpness_verdict='soft', burst_pick=1 WHERE id=?",
            [d4],
        );
        lib.sql(
            "UPDATE detections SET sharpness_verdict='blurred' WHERE id=?",
            [d5],
        );
        lib.sql(
            "UPDATE detections SET sharpness_verdict='sharp', burst_pick=1 WHERE id=?",
            [d6],
        );

        let out = lib.run("global_stats", json!({})).unwrap();
        assert_eq!(
            out,
            json!({
                "albums": 2,
                "complete_albums": 1,
                "frames": 8,
                "complete_frames": 6,
                "copies_hidden": 1,
                "size_bytes": 11000,
                "cameras": [{"name": "Canon EOS R7 358", "count": 6}],
                "lenses": [{"name": "RF-S 18-150mm", "count": 6}],
                "most_used": {
                    "camera": "Canon EOS R7 358",
                    "lens": "RF-S 18-150mm",
                    "f_number": 6.3,
                    "shutter": 0.008,
                    "iso": 100,
                    "focal_length": 50.0,
                },
                "f_numbers": [
                    {"value": 6.3, "count": 4},
                    {"value": 8.0, "count": 2}
                ],
                "shutters": [
                    {"value": 0.008, "count": 4},
                    {"value": 0.004, "count": 2}
                ],
                "isos": [
                    {"value": 100, "count": 4},
                    {"value": 200, "count": 2}
                ],
                "focal_lengths": [
                    {"value": 50.0, "count": 3},
                    {"value": 150.0, "count": 3}
                ],
                "pans": 1,
                "bursts": 4,
                "blurred": 1,
                "soft": 2,
                "sharp": 3,
                "af": {
                    "frames": 8,
                    "missing": 1,
                    "off_subject": 1,
                    "no_subject": 1,
                    "soft": 1
                },
                "keepers": 4,
                "rejected": 1,
                "shooting_seconds": 350,
                "average_shoot_seconds": 175,
                "longest_shoot_seconds": 300,
                "timed_albums": 2
            })
        );
    }

    #[test]
    fn global_stats_without_a_complete_album_has_no_gear() {
        use crate::testkit::Lib;
        let lib = Lib::new("global-stats-empty");
        let frame = lib.frame("only.cr3", Some(1));
        // One missing lens is enough to keep the album out of the gear lists.
        lib.sql(
            "UPDATE images SET camera='Canon EOS R7 358', lens=NULL, iso=100, f_number=6.3,
                    exposure_time=0.008, focal_length=50.0, file_size=500, taken_at=10,
                    time_burst=1
              WHERE id=?",
            [frame],
        );
        lib.detection(frame, "vehicle", 0.9);
        let out = lib.run("global_stats", json!({})).unwrap();
        assert_eq!(
            out,
            json!({
                "albums": 1,
                "complete_albums": 0,
                "frames": 1,
                "complete_frames": 0,
                "copies_hidden": 0,
                "size_bytes": 500,
                "cameras": [],
                "lenses": [],
                "most_used": {
                    "camera": null,
                    "lens": null,
                    "f_number": null,
                    "shutter": null,
                    "iso": null,
                    "focal_length": null,
                },
                "f_numbers": [],
                "shutters": [],
                "isos": [],
                "focal_lengths": [],
                "pans": 0,
                "bursts": 1,
                "blurred": 0,
                "soft": 0,
                "sharp": 0,
                "af": {
                    "frames": 1,
                    "missing": 1,
                    "off_subject": 0,
                    "no_subject": 0,
                    "soft": 0
                },
                "keepers": 0,
                "rejected": 0,
                "shooting_seconds": 0,
                "average_shoot_seconds": 0,
                "longest_shoot_seconds": 0,
                "timed_albums": 0
            })
        );
    }
}
