//! Port of `conrod/store.py`.
//!
//! A shoot is a *job*: a folder full of frames, each with zero or more
//! detected vehicles, each of which may have a number. The database is what
//! the review UI reads and writes, and what the XMP writer consumes at the
//! end, so a run can be interrupted and resumed without redoing work.
//!
//! This opens the exact same `conrod.db` the Python app uses, with the same
//! schema text, the same column-diffing migrations, and the same pragmas --
//! a file this crate touches stays fully usable by Python afterwards.

use conrod_core::bursts::Frame;
use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OptionalExtension, Result, Row};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS jobs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    root          TEXT NOT NULL,
    label         TEXT,
    created_at    REAL NOT NULL,
    status        TEXT NOT NULL DEFAULT 'scanning',
    settings_json TEXT
);

CREATE TABLE IF NOT EXISTS images (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id       INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    path         TEXT NOT NULL,
    preview_path TEXT,
    width        INTEGER,
    height       INTEGER,
    status       TEXT NOT NULL DEFAULT 'pending',
    error        TEXT,
    written_at   REAL,
    stars        INTEGER,
    rejected     INTEGER NOT NULL DEFAULT 0,
    thumb_path   TEXT,
    UNIQUE (job_id, path)
);

CREATE TABLE IF NOT EXISTS detections (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    image_id      INTEGER NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    x1 REAL, y1 REAL, x2 REAL, y2 REAL,
    cls           TEXT,
    conf          REAL,
    crop_path     TEXT,
    number        TEXT,
    number_source TEXT,          -- 'ocr' | 'roundel' | 'vlm' | 'manual' | pairs
    number_conf   REAL,
    plate         TEXT,
    plate_state   TEXT,
    plate_conf    REAL,
    -- The full VehicleAnalysis as JSON: make, model, colour, team, sponsors,
    -- text. Kept as one blob because the fields are read and written together
    -- and the shape is still moving.
    attributes    TEXT,
    reviewed      INTEGER NOT NULL DEFAULT 0,
    rejected      INTEGER NOT NULL DEFAULT 0,
    features      TEXT,
    heuristic     REAL,
    region_type   TEXT DEFAULT 'vehicle'
);

-- Cars this photographer has already met, keyed by plate. Not scoped to a
-- job: the point is that the same cars turn up at the same meets, so what
-- one album worked out is what the next one starts from. See registry.py.
CREATE TABLE IF NOT EXISTS known_vehicles (
    plate       TEXT PRIMARY KEY,
    make        TEXT,
    model       TEXT,
    colour      TEXT,
    body_type   TEXT,
    team        TEXT,
    sponsors    TEXT,          -- comma separated, as a CSV column would be
    race_number TEXT,
    -- Other spellings of this plate seen on the same car, comma separated.
    -- Not guesses: grouping joins 43111J to 73111J only on visual evidence
    -- inside one burst, so these are readings of a plate this car was
    -- actually wearing. That makes a lookup on a misread safe here in a way
    -- that fuzzy-matching an arbitrary plate never is.
    aliases     TEXT,
    updated_at  REAL
);

-- Sharpness rated by hand on the Train screen. Keyed by the frame and the box
-- rather than by detection id: an album scanned again gets new detection ids
-- and the rating is about the photograph, not about the row. The features are
-- kept here, taken when the rating was given, so training does not depend on
-- a crop still being in the cache -- see sharp_model.
CREATE TABLE IF NOT EXISTS sharpness_labels (
    path            TEXT NOT NULL,
    x1 REAL NOT NULL, y1 REAL NOT NULL, x2 REAL NOT NULL, y2 REAL NOT NULL,
    stars           INTEGER NOT NULL,   -- 1..5 of the subject, 0 = cannot tell
    pan             INTEGER NOT NULL DEFAULT 0,
    heur_pan        INTEGER,            -- what the measure said, for scoring it
    features        TEXT,
    feature_version INTEGER,
    created_at      REAL NOT NULL,
    PRIMARY KEY (path, x1, y1, x2, y2)
);

CREATE INDEX IF NOT EXISTS idx_images_job    ON images(job_id, status);
CREATE INDEX IF NOT EXISTS idx_det_image     ON detections(image_id);
CREATE INDEX IF NOT EXISTS idx_det_number    ON detections(number);
";

/// Columns added after the first release. SQLite has no "ADD COLUMN IF NOT
/// EXISTS", so they are applied against the existing table and skipped when
/// already present -- simpler and safer than a version table for a local tool.
const MIGRATIONS: &[(&str, &str, &str)] = &[
    ("detections", "plate", "TEXT"),
    ("detections", "plate_state", "TEXT"),
    ("detections", "plate_conf", "REAL"),
    ("detections", "attributes", "TEXT"),
    ("detections", "signature", "TEXT"),
    ("detections", "group_key", "INTEGER"),
    ("detections", "group_size", "INTEGER"),
    ("detections", "group_agreement", "REAL"),
    ("detections", "colour_hex", "TEXT"),
    ("detections", "group_colour_hex", "TEXT"),
    // Which body shot it and which burst it belongs to -- see bursts.py.
    ("images", "camera", "TEXT"),
    ("images", "burst_key", "INTEGER"),
    ("images", "taken_at", "REAL"),
    // Measured on the crop, not the frame, so a panning shot is not marked
    // down for the blur that makes it good.
    ("detections", "sharpness", "REAL"),
    ("detections", "sharpness_verdict", "TEXT"),
    // Why a detection was cut before it was ever identified.
    ("detections", "cull_reason", "TEXT"),
    // How many frame edges the subject runs off, and the rating combining
    // that with focus. Kept apart from sharpness, a pure focus measure.
    ("detections", "clipped", "INTEGER"),
    ("detections", "rating", "REAL"),
    ("detections", "rating_verdict", "TEXT"),
    // A star given by hand, which outranks the measured one everywhere. Its
    // own column so re-culling an album cannot quietly erase it.
    ("detections", "stars", "INTEGER"),
    // A vehicle in the photograph that is not what the photograph is of.
    ("detections", "bystander", "INTEGER"),
    // Where the sharpness is, not how much: a held pan has a sharp subject
    // against a smeared background.
    ("detections", "panning", "INTEGER"),
    ("detections", "background", "REAL"),
    ("detections", "sharp_end", "TEXT"),
    // A close-call cull that a person should still see.
    ("detections", "uncertain", "INTEGER"),
    // What the file already said before Conrod looked at it.
    ("images", "rating_in_file", "INTEGER"),
    ("images", "label_in_file", "TEXT"),
    // What the crop looks like to the similarity model, for re-grouping a
    // whole shoot without opening a crop again.
    ("detections", "embedding", "TEXT"),
    // The star this photographer would probably give, learned from the ones
    // already given. Stored because the review grid sorts on it in SQL.
    ("detections", "predicted_stars", "INTEGER"),
    // 1 on the one frame of a pass worth keeping. Stored for the same reason.
    ("detections", "burst_pick", "INTEGER"),
    // Other readings of the same car's plate. See known_vehicles.
    ("known_vehicles", "aliases", "TEXT"),
    ("images", "sharpness", "REAL"),
    ("images", "rating", "REAL"),
    ("images", "rating_verdict", "TEXT"),
    ("images", "stars", "INTEGER"),
    ("images", "rejected", "INTEGER NOT NULL DEFAULT 0"),
    ("images", "thumb_path", "TEXT"),
    ("detections", "features", "TEXT"),
    ("detections", "heuristic", "REAL"),
    ("detections", "region_type", "TEXT DEFAULT 'vehicle'"),
];

fn now_unix() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}

// --- paths, mirroring config.py's _data_root --------------------------------

/// Where the database (and everything else churny) lives. Deliberately NOT
/// %LOCALAPPDATA%: a sandboxed or Store-packaged host redirects that into a
/// per-app container the venv and models are not in. `CONROD_HOME` overrides
/// it; otherwise it's `%USERPROFILE%\.conrod`.
pub fn data_root() -> PathBuf {
    if let Ok(over) = std::env::var("CONROD_HOME") {
        if !over.is_empty() {
            return PathBuf::from(over);
        }
    }
    let base = std::env::var("USERPROFILE")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOME").ok())
        .unwrap_or_default();
    PathBuf::from(base).join(".conrod")
}

/// Path to the same `conrod.db` the Python app opens.
pub fn db_path() -> PathBuf {
    data_root().join("conrod.db")
}

// --- connecting --------------------------------------------------------------

/// Which database files this process has already prepared (schema +
/// migrations + WAL pragmas), keyed by the resolved path. Matches store.py's
/// module-level `_prepared` set: those are writes, so doing them on every
/// connection queued every request behind the analysis workers' write lock.
fn prepared() -> &'static Mutex<HashSet<String>> {
    static PREPARED: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    PREPARED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Open a connection, creating the schema and running migrations at most once
/// per process per path.
pub fn connect(path: Option<&Path>) -> Result<Connection> {
    let target = path.map(PathBuf::from).unwrap_or_else(db_path);
    let conn = Connection::open(&target)?;
    // Autocommit (rusqlite's default -- no BEGIN is ever issued here) is the
    // point: Python's isolation_level=None avoided holding the single write
    // lock for the whole of a multi-second analysis batch. Every statement
    // below is its own transaction, which is what WAL is for.
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA busy_timeout=30000;")?;
    register_needs_review(&conn)?;

    let key = target.to_string_lossy().into_owned();
    let mut done = prepared().lock().unwrap();
    if !done.contains(&key) {
        // WAL lets the UI read while a scan writes; NORMAL is the matching
        // durability setting -- a crash can lose the last commits, which for
        // a re-runnable scan is a fair trade for not fsyncing every frame.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch(SCHEMA)?;
        migrate(&conn)?;
        done.insert(key);
    }
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<()> {
    for (table, column, kind) in MIGRATIONS {
        let mut existing = HashSet::new();
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            existing.insert(row.get::<_, String>(1)?);
        }
        if !existing.contains(*column) {
            conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {kind}"))?;
        }
    }
    Ok(())
}

/// Open a connection, run `f`, and close it. Autocommit means every write in
/// `f` is already durable by the time it returns; this exists for parity with
/// store.py's `session()` context manager.
pub fn session<T>(path: Option<&Path>, f: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
    let conn = connect(path)?;
    f(&conn)
}

// --- the `_needs_review` SQL function, from conrod/server.py ---------------

/// Register `_needs_review(number_conf, reviewed, rejected, uncertain,
/// threshold)` on this connection, as `conrod/server.py` does on every
/// connection it uses -- so a query written against the Python schema still
/// runs unchanged here.
pub fn register_needs_review(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "_needs_review",
        5,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            Ok(needs_review(
                ctx.get(0)?,
                ctx.get(1)?,
                ctx.get(2)?,
                ctx.get(3)?,
                ctx.get(4)?,
            ))
        },
    )
}

/// A detection a human should still look at.
///
/// A culled frame normally leaves review: it has been dealt with. The
/// exception is a cull the measurement was not sure about -- a pan held on
/// one end of the car reads as blurred when averaged over the whole vehicle,
/// and a shoot that silently loses those is worse than one that culls
/// nothing. Those come back into review until someone has looked.
fn needs_review(
    number_conf: Option<f64>,
    reviewed: Option<i64>,
    rejected: Option<i64>,
    uncertain: Option<i64>,
    threshold: Option<f64>,
) -> i64 {
    if reviewed.unwrap_or(0) != 0 {
        return 0;
    }
    if rejected.unwrap_or(0) != 0 {
        return i64::from(uncertain.unwrap_or(0) != 0);
    }
    let threshold = match threshold {
        Some(t) if t != 0.0 => t,
        _ => 0.8,
    };
    i64::from(number_conf.is_none_or(|c| c < threshold))
}

// --- row types ---------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    pub id: i64,
    pub root: String,
    pub label: Option<String>,
    pub created_at: f64,
    pub status: String,
    pub settings_json: Option<Value>,
}

pub fn job_from_row(row: &Row<'_>) -> Result<Job> {
    Ok(Job {
        id: row.get("id")?,
        root: row.get("root")?,
        label: row.get("label")?,
        created_at: row.get("created_at")?,
        status: row.get("status")?,
        settings_json: opt_json(row.get("settings_json")?),
    })
}

/// A job plus the counts `list_jobs` computes alongside it.
#[derive(Debug, Clone, PartialEq)]
pub struct JobSummary {
    pub id: i64,
    pub root: String,
    pub label: Option<String>,
    pub created_at: f64,
    pub status: String,
    pub settings_json: Option<Value>,
    pub image_count: i64,
    pub detection_count: i64,
    /// Frames nobody has looked at yet -- not "status != 'detected'", which
    /// would count unreadable frames as still to come forever.
    pub unfinished_count: i64,
    pub failed_count: i64,
    /// Whether grouping has ever run: an album that was culled and stopped
    /// has none, and review needs to say that.
    pub grouped_count: i64,
}

fn job_summary_from_row(row: &Row<'_>) -> Result<JobSummary> {
    Ok(JobSummary {
        id: row.get("id")?,
        root: row.get("root")?,
        label: row.get("label")?,
        created_at: row.get("created_at")?,
        status: row.get("status")?,
        settings_json: opt_json(row.get("settings_json")?),
        image_count: row.get("image_count")?,
        detection_count: row.get("detection_count")?,
        unfinished_count: row.get("unfinished_count")?,
        failed_count: row.get("failed_count")?,
        grouped_count: row.get("grouped_count")?,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub id: i64,
    pub job_id: i64,
    pub path: String,
    pub preview_path: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub status: String,
    pub error: Option<String>,
    pub written_at: Option<f64>,
    pub stars: Option<i64>,
    pub rejected: bool,
    pub thumb_path: Option<String>,
    pub camera: Option<String>,
    pub burst_key: Option<i64>,
    pub taken_at: Option<f64>,
    pub rating_in_file: Option<i64>,
    pub label_in_file: Option<String>,
    pub sharpness: Option<f64>,
    pub rating: Option<f64>,
    pub rating_verdict: Option<String>,
}

pub fn image_from_row(row: &Row<'_>) -> Result<Image> {
    Ok(Image {
        id: row.get("id")?,
        job_id: row.get("job_id")?,
        path: row.get("path")?,
        preview_path: row.get("preview_path")?,
        width: row.get("width")?,
        height: row.get("height")?,
        status: row.get("status")?,
        error: row.get("error")?,
        written_at: row.get("written_at")?,
        stars: row.get("stars")?,
        rejected: row.get::<_, i64>("rejected")? != 0,
        thumb_path: row.get("thumb_path")?,
        camera: row.get("camera")?,
        burst_key: row.get("burst_key")?,
        taken_at: row.get("taken_at")?,
        rating_in_file: row.get("rating_in_file")?,
        label_in_file: row.get("label_in_file")?,
        sharpness: row.get("sharpness")?,
        rating: row.get("rating")?,
        rating_verdict: row.get("rating_verdict")?,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub id: i64,
    pub image_id: i64,
    pub x1: Option<f64>,
    pub y1: Option<f64>,
    pub x2: Option<f64>,
    pub y2: Option<f64>,
    pub cls: Option<String>,
    pub conf: Option<f64>,
    pub crop_path: Option<String>,
    pub number: Option<String>,
    pub number_source: Option<String>,
    pub number_conf: Option<f64>,
    pub plate: Option<String>,
    pub plate_state: Option<String>,
    pub plate_conf: Option<f64>,
    pub attributes: Option<Value>,
    pub reviewed: bool,
    pub rejected: bool,
    pub signature: Option<String>,
    pub group_key: Option<i64>,
    pub group_size: Option<i64>,
    pub group_agreement: Option<f64>,
    pub colour_hex: Option<String>,
    pub group_colour_hex: Option<String>,
    pub sharpness: Option<f64>,
    pub sharpness_verdict: Option<String>,
    pub cull_reason: Option<String>,
    pub clipped: Option<i64>,
    pub rating: Option<f64>,
    pub rating_verdict: Option<String>,
    pub stars: Option<i64>,
    pub bystander: Option<i64>,
    pub panning: Option<i64>,
    pub background: Option<f64>,
    pub sharp_end: Option<String>,
    pub uncertain: Option<i64>,
    pub embedding: Option<String>,
    pub predicted_stars: Option<i64>,
    pub burst_pick: Option<i64>,
    pub features: Option<Value>,
    pub heuristic: Option<f64>,
    pub region_type: Option<String>,
}

pub fn detection_from_row(row: &Row<'_>) -> Result<Detection> {
    Ok(Detection {
        id: row.get("id")?,
        image_id: row.get("image_id")?,
        x1: row.get("x1")?,
        y1: row.get("y1")?,
        x2: row.get("x2")?,
        y2: row.get("y2")?,
        cls: row.get("cls")?,
        conf: row.get("conf")?,
        crop_path: row.get("crop_path")?,
        number: row.get("number")?,
        number_source: row.get("number_source")?,
        number_conf: row.get("number_conf")?,
        plate: row.get("plate")?,
        plate_state: row.get("plate_state")?,
        plate_conf: row.get("plate_conf")?,
        attributes: opt_json(row.get("attributes")?),
        reviewed: row.get("reviewed")?,
        rejected: row.get("rejected")?,
        signature: row.get("signature")?,
        group_key: row.get("group_key")?,
        group_size: row.get("group_size")?,
        group_agreement: row.get("group_agreement")?,
        colour_hex: row.get("colour_hex")?,
        group_colour_hex: row.get("group_colour_hex")?,
        sharpness: row.get("sharpness")?,
        sharpness_verdict: row.get("sharpness_verdict")?,
        cull_reason: row.get("cull_reason")?,
        clipped: row.get("clipped")?,
        rating: row.get("rating")?,
        rating_verdict: row.get("rating_verdict")?,
        stars: row.get("stars")?,
        bystander: row.get("bystander")?,
        panning: row.get("panning")?,
        background: row.get("background")?,
        sharp_end: row.get("sharp_end")?,
        uncertain: row.get("uncertain")?,
        embedding: row.get("embedding")?,
        predicted_stars: row.get("predicted_stars")?,
        burst_pick: row.get("burst_pick")?,
        features: opt_json(row.get("features")?),
        heuristic: row.get("heuristic")?,
        region_type: row.get("region_type")?,
    })
}

/// A candidate handed to the Train screen by [`next_to_rate`].
#[derive(Debug, Clone, PartialEq)]
pub struct NextToRate {
    pub id: i64,
    pub crop_path: Option<String>,
    pub x1: Option<f64>,
    pub y1: Option<f64>,
    pub x2: Option<f64>,
    pub y2: Option<f64>,
    pub frame: String,
    pub preview_path: Option<String>,
}

fn next_to_rate_from_row(row: &Row<'_>) -> Result<NextToRate> {
    Ok(NextToRate {
        id: row.get("id")?,
        crop_path: row.get("crop_path")?,
        x1: row.get("x1")?,
        y1: row.get("y1")?,
        x2: row.get("x2")?,
        y2: row.get("y2")?,
        frame: row.get("frame")?,
        preview_path: row.get("preview_path")?,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct SharpnessLabelRow {
    pub stars: i64,
    pub pan: i64,
    pub heur_pan: Option<i64>,
    pub features: Option<Value>,
}

fn sharpness_label_from_row(row: &Row<'_>) -> Result<SharpnessLabelRow> {
    Ok(SharpnessLabelRow {
        stars: row.get("stars")?,
        pan: row.get("pan")?,
        heur_pan: row.get("heur_pan")?,
        features: opt_json(row.get("features")?),
    })
}

/// What `set_analysis` stores against a detection -- the same fields
/// `conrod.analyze.VehicleAnalysis` carries, plus its own JSON serialisation
/// as `attributes` (kept as a JSON `Value` here rather than a fixed struct:
/// the shape is still moving on the Python side, so this crate only knows it
/// as a blob, exactly what the `attributes` column has always been).
#[derive(Debug, Clone, PartialEq)]
pub struct Analysis {
    pub race_number: Option<String>,
    pub number_source: Option<String>,
    pub number_conf: Option<f64>,
    pub plate: Option<String>,
    pub plate_state: Option<String>,
    pub plate_conf: Option<f64>,
    pub attributes: Value,
}

/// Parse a JSON column leniently: a malformed or absent value reads as
/// absent rather than an error, matching every `json.loads(...)` the Python
/// side wraps in a `try`/`except`.
fn opt_json(text: Option<String>) -> Option<Value> {
    text.and_then(|t| serde_json::from_str(&t).ok())
}

// --- jobs --------------------------------------------------------------------

pub fn create_job(
    conn: &Connection,
    root: &Path,
    label: Option<&str>,
    settings: &Value,
) -> Result<i64> {
    let label = label
        .map(str::to_string)
        .or_else(|| root.file_name().map(|n| n.to_string_lossy().into_owned()));
    conn.execute(
        "INSERT INTO jobs (root, label, created_at, settings_json) VALUES (?,?,?,?)",
        (
            root.to_string_lossy().as_ref(),
            label,
            now_unix(),
            serde_json::to_string(settings).unwrap(),
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn set_job_status(conn: &Connection, job_id: i64, status: &str) -> Result<()> {
    conn.execute("UPDATE jobs SET status=? WHERE id=?", (status, job_id))?;
    Ok(())
}

pub fn latest_job(conn: &Connection) -> Result<Option<Job>> {
    conn.query_row(
        "SELECT * FROM jobs ORDER BY id DESC LIMIT 1",
        [],
        job_from_row,
    )
    .optional()
}

pub fn list_jobs(conn: &Connection) -> Result<Vec<JobSummary>> {
    let mut stmt = conn.prepare(
        "SELECT j.*,
               (SELECT COUNT(*) FROM images i WHERE i.job_id = j.id) AS image_count,
               (SELECT COUNT(*) FROM detections d
                  JOIN images i2 ON i2.id = d.image_id
                 WHERE i2.job_id = j.id) AS detection_count,
               (SELECT COUNT(*) FROM images i3
                 WHERE i3.job_id = j.id AND i3.status = 'pending')
                 AS unfinished_count,
               (SELECT COUNT(*) FROM images i5
                 WHERE i5.job_id = j.id AND i5.status = 'error')
                 AS failed_count,
               (SELECT COUNT(*) FROM detections d2
                  JOIN images i4 ON i4.id = d2.image_id
                 WHERE i4.job_id = j.id AND d2.group_key IS NOT NULL)
                 AS grouped_count
          FROM jobs j ORDER BY j.id DESC",
    )?;
    let rows = stmt.query_map([], job_summary_from_row)?;
    rows.collect()
}

// --- images ------------------------------------------------------------------

pub fn add_images(conn: &Connection, job_id: i64, paths: &[PathBuf]) -> Result<()> {
    let mut stmt = conn.prepare("INSERT OR IGNORE INTO images (job_id, path) VALUES (?,?)")?;
    for p in paths {
        stmt.execute((job_id, p.to_string_lossy().as_ref()))?;
    }
    Ok(())
}

pub fn pending_images(conn: &Connection, job_id: i64, status: &str) -> Result<Vec<Image>> {
    let mut stmt = conn.prepare("SELECT * FROM images WHERE job_id=? AND status=? ORDER BY id")?;
    let rows = stmt.query_map((job_id, status), image_from_row)?;
    rows.collect()
}

/// List an album's frames in contact-sheet order.
pub fn list_images(
    conn: &Connection,
    job_id: i64,
    limit: Option<i64>,
    offset: i64,
) -> Result<Vec<Image>> {
    let limit = limit.unwrap_or(500).clamp(1, 5000);
    let mut stmt =
        conn.prepare("SELECT * FROM images WHERE job_id=? ORDER BY id LIMIT ? OFFSET ?")?;
    let rows = stmt.query_map((job_id, limit, offset.max(0)), image_from_row)?;
    rows.collect()
}

pub fn get_image(conn: &Connection, image_id: i64) -> Result<Option<Image>> {
    conn.query_row(
        "SELECT * FROM images WHERE id=?",
        [image_id],
        image_from_row,
    )
    .optional()
}

/// Apply the hand review state for a frame, including frames with no subjects.
/// `None` leaves the corresponding value unchanged; `Some(0)` clears stars.
pub fn update_image_review(
    conn: &Connection,
    image_id: i64,
    stars: Option<i64>,
    rejected: Option<bool>,
) -> Result<bool> {
    let changed = match (stars, rejected) {
        (Some(stars), Some(rejected)) => conn.execute(
            "UPDATE images SET stars=?, rejected=? WHERE id=?",
            (
                if stars == 0 { None } else { Some(stars) },
                rejected as i64,
                image_id,
            ),
        )?,
        (Some(stars), None) => conn.execute(
            "UPDATE images SET stars=? WHERE id=?",
            (if stars == 0 { None } else { Some(stars) }, image_id),
        )?,
        (None, Some(rejected)) => conn.execute(
            "UPDATE images SET rejected=? WHERE id=?",
            (rejected as i64, image_id),
        )?,
        (None, None) => 0,
    };
    Ok(changed != 0)
}

pub fn delete_image(conn: &Connection, image_id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM images WHERE id=?", [image_id])? != 0)
}

pub fn set_image_result(
    conn: &Connection,
    image_id: i64,
    status: &str,
    preview_path: Option<&str>,
    width: Option<i64>,
    height: Option<i64>,
    error: Option<&str>,
) -> Result<()> {
    conn.execute(
        "UPDATE images
              SET status=?, preview_path=COALESCE(?, preview_path),
                  width=COALESCE(?, width), height=COALESCE(?, height), error=?
            WHERE id=?",
        (status, preview_path, width, height, error, image_id),
    )?;
    Ok(())
}

/// Store frame-level focus/rating for images where no detection supplied a
/// subject score. This keeps no-subject frames reviewable and sortable.
pub fn set_image_quality(
    conn: &Connection,
    image_id: i64,
    sharpness: Option<f64>,
    rating: Option<f64>,
    rating_verdict: Option<&str>,
    stars: Option<i64>,
    rejected: Option<bool>,
) -> Result<bool> {
    if conn
        .query_row("SELECT 1 FROM images WHERE id=?", [image_id], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .is_none()
    {
        return Ok(false);
    }
    if sharpness.is_some() || rating.is_some() || rating_verdict.is_some() {
        conn.execute(
            "UPDATE images SET sharpness=COALESCE(?, sharpness), rating=COALESCE(?, rating),
             rating_verdict=COALESCE(?, rating_verdict) WHERE id=?",
            (sharpness, rating, rating_verdict, image_id),
        )?;
    }
    if let Some(stars) = stars {
        conn.execute(
            "UPDATE images SET stars=? WHERE id=?",
            (if stars == 0 { None } else { Some(stars) }, image_id),
        )?;
    }
    if let Some(rejected) = rejected {
        conn.execute(
            "UPDATE images SET rejected=? WHERE id=?",
            (rejected as i64, image_id),
        )?;
    }
    Ok(true)
}

// --- detections --------------------------------------------------------------

pub fn add_detection(
    conn: &Connection,
    image_id: i64,
    bbox: [f64; 4],
    cls: &str,
    conf: f64,
    crop_path: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO detections (image_id, x1, y1, x2, y2, cls, conf, crop_path)
           VALUES (?,?,?,?,?,?,?,?)",
        (
            image_id, bbox[0], bbox[1], bbox[2], bbox[3], cls, conf, crop_path,
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn set_number(
    conn: &Connection,
    det_id: i64,
    number: Option<&str>,
    source: &str,
    confidence: f64,
) -> Result<()> {
    conn.execute(
        "UPDATE detections SET number=?, number_source=?, number_conf=? WHERE id=?",
        (number, source, confidence, det_id),
    )?;
    Ok(())
}

/// Store a completed [`Analysis`] against a detection.
pub fn set_analysis(
    conn: &Connection,
    det_id: i64,
    analysis: &Analysis,
    colour_hex: Option<&str>,
    sharpness: Option<f64>,
    sharpness_verdict: Option<&str>,
) -> Result<()> {
    let attributes = serde_json::to_string(&analysis.attributes).unwrap();
    conn.execute(
        "UPDATE detections
              SET number=?, number_source=?, number_conf=?,
                  plate=?, plate_state=?, plate_conf=?, attributes=?,
                  colour_hex=COALESCE(?, colour_hex),
                  sharpness=COALESCE(?, sharpness),
                  sharpness_verdict=COALESCE(?, sharpness_verdict)
            WHERE id=?",
        (
            &analysis.race_number,
            &analysis.number_source,
            analysis.number_conf,
            &analysis.plate,
            &analysis.plate_state,
            analysis.plate_conf,
            attributes,
            colour_hex,
            sharpness,
            sharpness_verdict,
            det_id,
        ),
    )?;
    Ok(())
}

/// Record which camera took each frame and which burst it belongs to.
///
/// Written after the folder is read, before any frame is analysed, so
/// grouping and the review screen can both lean on it. Frames the scan has
/// never heard of are ignored: this only annotates, it does not decide what
/// is in the job. Returns how many images were matched.
pub fn set_frame_origin(conn: &Connection, job_id: i64, frames: &[Frame]) -> Result<usize> {
    let known = images_by_path_key(conn, job_id)?;
    let mut update =
        conn.prepare("UPDATE images SET camera=?, burst_key=?, taken_at=? WHERE id=?")?;
    let mut n = 0;
    for frame in frames {
        if let Some(&image_id) = known.get(&path_key(&frame.path)) {
            update.execute((&frame.camera, frame.burst as i64, frame.taken, image_id))?;
            n += 1;
        }
    }
    Ok(n)
}

/// Record the rating and colour label each file already carried.
///
/// `marks` is keyed by path, holding `(rating, label)`. A rating of zero is
/// not a rating: every camera writes 0 for "not rated", so treating it as a
/// deliberate one star would put a floor under the whole shoot. Returns how
/// many images got a rating (as opposed to just a label, or nothing).
pub fn set_existing_marks(
    conn: &Connection,
    job_id: i64,
    marks: &HashMap<String, (Option<i64>, Option<String>)>,
) -> Result<usize> {
    let known = images_by_path_key(conn, job_id)?;
    let mut update =
        conn.prepare("UPDATE images SET rating_in_file=?, label_in_file=? WHERE id=?")?;
    let mut with_rating = 0;
    for (path, (rating, label)) in marks {
        let Some(&image_id) = known.get(&path_key(path)) else {
            continue;
        };
        let stars = rating.filter(|r| (1..=5).contains(r));
        let label = label.as_deref().map(str::trim).filter(|s| !s.is_empty());
        update.execute((stars, label, image_id))?;
        if stars.is_some() {
            with_rating += 1;
        }
    }
    Ok(with_rating)
}

fn images_by_path_key(conn: &Connection, job_id: i64) -> Result<HashMap<String, i64>> {
    let mut stmt = conn.prepare("SELECT id, path FROM images WHERE job_id=?")?;
    let mut rows = stmt.query([job_id])?;
    let mut known = HashMap::new();
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        let path: String = row.get(1)?;
        known.insert(path_key(&path), id);
    }
    Ok(known)
}

/// Matches `os.path.normcase(os.path.normpath(path))` closely enough for the
/// absolute Windows paths this tool ever sees: forward slashes become
/// backslashes, repeated separators collapse, and case folds.
///
/// exiftool reports `SourceFile` with forward slashes and the database holds
/// what Windows gave us, so comparing the strings directly matched nothing at
/// all -- see `set_frame_origin` and `set_existing_marks` in store.py.
///
/// ponytail: does not resolve "." or ".." components. Every caller hands this
/// an absolute path from exiftool or a directory walk, neither of which ever
/// produces one, so a full lexical normpath would be dead code here.
fn path_key(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut prev_sep = false;
    for ch in path.chars() {
        let ch = if ch == '/' { '\\' } else { ch };
        if ch == '\\' {
            if prev_sep {
                continue;
            }
            prev_sep = true;
        } else {
            prev_sep = false;
        }
        out.push(ch);
    }
    if out.len() > 3 && out.ends_with('\\') {
        out.pop();
    }
    out.to_lowercase()
}

/// Everything known about the picture, as opposed to what is in it. Python's
/// defaults: `panning = false`, `sharp_end = "even"`, `background = -1.0`,
/// `uncertain = false`.
// A direct port of store.py's keyword-argument setter; splitting it into a
// builder or an options struct would be more code for the same one call site.
#[allow(clippy::too_many_arguments)]
pub fn set_quality(
    conn: &Connection,
    det_id: i64,
    sharpness: f64,
    sharpness_verdict: &str,
    clipped: i64,
    rating: f64,
    rating_verdict: &str,
    panning: bool,
    sharp_end: &str,
    background: f64,
    uncertain: bool,
) -> Result<()> {
    conn.execute(
        "UPDATE detections
              SET sharpness=?, sharpness_verdict=?, clipped=?,
                  rating=?, rating_verdict=?, panning=?, background=?,
                  sharp_end=?, uncertain=?
            WHERE id=?",
        (
            sharpness,
            sharpness_verdict,
            clipped,
            rating,
            rating_verdict,
            panning as i64,
            background,
            sharp_end,
            uncertain as i64,
            det_id,
        ),
    )?;
    Ok(())
}

/// Cut a detection before it is identified, and say why.
///
/// Rejected rather than deleted: the crop and its score stay, so the
/// Rejected view can show what was cut and put anything back that should not
/// have been. `uncertain` marks a cull that was a close call, so review can
/// surface it rather than leaving it to be found by someone counting frames.
pub fn cull_detection(conn: &Connection, det_id: i64, reason: &str, uncertain: bool) -> Result<()> {
    conn.execute(
        "UPDATE detections SET rejected=1, cull_reason=?, uncertain=? WHERE id=?",
        (reason, uncertain as i64, det_id),
    )?;
    Ok(())
}

pub fn unread_detections(conn: &Connection, job_id: i64) -> Result<Vec<Detection>> {
    let mut stmt = conn.prepare(
        "SELECT d.* FROM detections d
             JOIN images i ON i.id = d.image_id
            WHERE i.job_id=? AND d.number_source IS NULL
            ORDER BY d.id",
    )?;
    let rows = stmt.query_map([job_id], detection_from_row)?;
    rows.collect()
}

/// List all detections in an album. Callers can filter rejected rows in SQL
/// when they need the review, rejected, or all views.
pub fn list_detections(conn: &Connection, job_id: i64) -> Result<Vec<Detection>> {
    let mut stmt = conn.prepare(
        "SELECT d.* FROM detections d JOIN images i ON i.id=d.image_id
         WHERE i.job_id=? ORDER BY d.id",
    )?;
    let rows = stmt.query_map([job_id], detection_from_row)?;
    rows.collect()
}

pub fn get_detection(conn: &Connection, det_id: i64) -> Result<Option<Detection>> {
    conn.query_row(
        "SELECT * FROM detections WHERE id=?",
        [det_id],
        detection_from_row,
    )
    .optional()
}

/// Update fields a reviewer can change without requiring a Python
/// `VehicleAnalysis` implementation in this crate.
#[allow(clippy::too_many_arguments)]
pub fn update_detection_review(
    conn: &Connection,
    det_id: i64,
    number: Option<Option<&str>>,
    plate: Option<Option<&str>>,
    attributes: Option<&Value>,
    rejected: Option<bool>,
    reviewed: Option<bool>,
    stars: Option<Option<i64>>,
    bystander: Option<bool>,
) -> Result<bool> {
    if conn
        .query_row("SELECT 1 FROM detections WHERE id=?", [det_id], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .is_none()
    {
        return Ok(false);
    }
    if let Some(number) = number {
        conn.execute(
            "UPDATE detections SET number=?, number_source='manual', number_conf=CASE WHEN ? IS NULL THEN NULL ELSE 1.0 END WHERE id=?",
            (number, number, det_id),
        )?;
    }
    if let Some(plate) = plate {
        conn.execute("UPDATE detections SET plate=? WHERE id=?", (plate, det_id))?;
    }
    if let Some(attributes) = attributes {
        conn.execute(
            "UPDATE detections SET attributes=? WHERE id=?",
            (serde_json::to_string(attributes).unwrap(), det_id),
        )?;
    }
    if let Some(rejected) = rejected {
        conn.execute(
            "UPDATE detections SET rejected=? WHERE id=?",
            (rejected as i64, det_id),
        )?;
    }
    if let Some(reviewed) = reviewed {
        conn.execute(
            "UPDATE detections SET reviewed=? WHERE id=?",
            (reviewed as i64, det_id),
        )?;
    }
    if let Some(stars) = stars {
        conn.execute("UPDATE detections SET stars=? WHERE id=?", (stars, det_id))?;
    }
    if let Some(bystander) = bystander {
        conn.execute(
            "UPDATE detections SET bystander=? WHERE id=?",
            (bystander as i64, det_id),
        )?;
    }
    Ok(true)
}

pub fn delete_detection(conn: &Connection, det_id: i64) -> Result<bool> {
    Ok(conn.execute("DELETE FROM detections WHERE id=?", [det_id])? != 0)
}

/// Store grouping's per-detection result. Passing `None` clears a previous
/// grouping, which is how regroup/reset returns an album to its initial state.
pub fn set_detection_group(
    conn: &Connection,
    det_id: i64,
    group_key: Option<i64>,
    group_size: Option<i64>,
    agreement: Option<f64>,
    group_colour_hex: Option<&str>,
) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE detections SET group_key=?, group_size=?, group_agreement=?, group_colour_hex=? WHERE id=?",
        (group_key, group_size, agreement, group_colour_hex, det_id),
    )? != 0)
}

pub fn clear_groups(conn: &Connection, job_id: Option<i64>) -> Result<usize> {
    let changed = match job_id {
        Some(job_id) => conn.execute(
            "UPDATE detections SET group_key=NULL, group_size=NULL, group_agreement=NULL,
             group_colour_hex=NULL WHERE image_id IN (SELECT id FROM images WHERE job_id=?)",
            [job_id],
        )?,
        None => conn.execute(
            "UPDATE detections SET group_key=NULL, group_size=NULL, group_agreement=NULL,
             group_colour_hex=NULL",
            [],
        )?,
    };
    Ok(changed)
}

/// Persist cull/training measurements produced by the Rust engine.
pub fn set_detection_measurement(
    conn: &Connection,
    det_id: i64,
    features: Option<&[f64]>,
    heuristic: Option<f64>,
    region_type: Option<&str>,
) -> Result<bool> {
    Ok(conn.execute(
        "UPDATE detections SET features=?, heuristic=?, region_type=COALESCE(?, region_type) WHERE id=?",
        (features.map(|f| serde_json::to_string(f).unwrap()), heuristic, region_type, det_id),
    )? != 0)
}

/// What the crop looks like to the similarity model.
pub fn set_embedding(conn: &Connection, det_id: i64, packed: &str) -> Result<()> {
    conn.execute(
        "UPDATE detections SET embedding=? WHERE id=?",
        (packed, det_id),
    )?;
    Ok(())
}

// --- sharpness training -------------------------------------------------------

/// A crop nobody has rated yet, from one slice of the measured range.
///
/// The caller picks the slice: a random crop of a shoot is mostly blur and
/// would spend the whole session on the easy cases, so asking for every
/// slice in turn is what puts the borderline frames in front of a person.
pub fn next_to_rate(
    conn: &Connection,
    pan: i64,
    low: f64,
    high: f64,
    job_id: Option<i64>,
) -> Result<Option<NextToRate>> {
    conn.query_row(
        "SELECT d.id, d.crop_path, d.x1, d.y1, d.x2, d.y2,
                  i.path AS frame, i.preview_path
             FROM detections d
             JOIN images i ON i.id = d.image_id
        LEFT JOIN sharpness_labels l
               ON l.path = i.path AND l.x1 = d.x1 AND l.y1 = d.y1
              AND l.x2 = d.x2 AND l.y2 = d.y2
            WHERE l.path IS NULL AND d.crop_path IS NOT NULL
              AND d.sharpness IS NOT NULL
              AND COALESCE(d.panning, 0) = ?
              AND d.sharpness >= ? AND d.sharpness < ?
              AND (? IS NULL OR i.job_id = ?)
            ORDER BY random() LIMIT 1",
        (pan, low, high, job_id, job_id),
        next_to_rate_from_row,
    )
    .optional()
}

// Same call as `set_quality` above: a direct port of a keyword-argument setter.
#[allow(clippy::too_many_arguments)]
pub fn add_sharpness_label(
    conn: &Connection,
    frame: &str,
    bbox: [f64; 4],
    stars: i64,
    pan: bool,
    heur_pan: bool,
    features: Option<&[f64]>,
    version: i64,
) -> Result<()> {
    let features_json = features
        .filter(|f| !f.is_empty())
        .map(|f| serde_json::to_string(f).unwrap());
    conn.execute(
        "INSERT OR REPLACE INTO sharpness_labels
               (path, x1, y1, x2, y2, stars, pan, heur_pan, features,
                feature_version, created_at)
           VALUES (?,?,?,?,?,?,?,?,?,?,?)",
        (
            frame,
            bbox[0],
            bbox[1],
            bbox[2],
            bbox[3],
            stars,
            pan as i64,
            heur_pan as i64,
            features_json,
            version,
            now_unix(),
        ),
    )?;
    Ok(())
}

pub fn undo_sharpness_label(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM sharpness_labels WHERE rowid = \
         (SELECT max(rowid) FROM sharpness_labels)",
        [],
    )?;
    Ok(())
}

/// Every rating that can be learned from: rated, and measured the way the
/// current features are measured.
pub fn sharpness_labels(conn: &Connection, version: i64) -> Result<Vec<SharpnessLabelRow>> {
    let mut stmt = conn.prepare(
        "SELECT stars, pan, heur_pan, features FROM sharpness_labels
            WHERE stars > 0 AND features IS NOT NULL AND feature_version = ?",
    )?;
    let rows = stmt.query_map([version], sharpness_label_from_row)?;
    rows.collect()
}

pub fn sharpness_label_counts(conn: &Connection) -> Result<HashMap<i64, i64>> {
    let mut stmt =
        conn.prepare("SELECT stars, count(*) AS n FROM sharpness_labels GROUP BY stars")?;
    let rows = stmt.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?;
    rows.collect()
}
