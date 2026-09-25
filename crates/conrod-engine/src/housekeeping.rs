//! The cache and the three resets. Ports `GET /api/cache`, `POST /api/cache/clear`
//! and `POST /api/reset/{identifications,detections}` / `POST /api/reset`.
//!
//! Nothing here reaches a photograph or anything the person made: only files
//! Conrod cut into its own `cache` folder (and only files named as it names
//! them, or found through the database and inside that folder), and rows of
//! the album tables. Known vehicles and the sharpness training labels are never
//! touched.
//!
//! The Rust cache is `<data>/cache/native/{thumb,view,crop}-<id>.jpg`:
//! `thumb` is the scan's contact-sheet frame (it cannot be made again without
//! scanning), `view` the full-size preview of a RAW (pulled again on demand),
//! `crop` a vehicle crop (it carries the plate and number reads and the look
//! embedding's source, so it goes only with its detection).
use crate::commands::CacheClearArgs;
use crate::desktop::{rows, Desktop, Result};
use crate::library::require_job;
use crate::lock;
use rusqlite::{params_from_iter, ToSql};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Thumb = 0,
    Preview = 1,
    Crop = 2,
}

struct Cached {
    kind: Kind,
    id: i64,
    path: PathBuf,
    bytes: u64,
}

fn cache_dir(d: &Desktop) -> PathBuf {
    d.root.join("cache/native")
}

/// The files of the cache named as Conrod names them; anything else is not ours.
fn cached(dir: &Path) -> Vec<Cached> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    read.filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".jpg")?;
            let (kind, id) = if let Some(id) = stem.strip_prefix("thumb-") {
                (Kind::Thumb, id)
            } else if let Some(id) = stem.strip_prefix("view-") {
                (Kind::Preview, id)
            } else {
                (Kind::Crop, stem.strip_prefix("crop-")?)
            };
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            Some(Cached {
                kind,
                id: id.parse().ok()?,
                path: e.path(),
                bytes: meta.len(),
            })
        })
        .collect()
}

/// The ids the database still refers to: frames, and detections with a crop.
fn live(d: &Desktop) -> Result<(HashSet<i64>, HashSet<i64>)> {
    let db = lock(&d.reader);
    let ids = |sql: &str| -> Result<HashSet<i64>> {
        Ok(rows(&db, sql, [])?
            .iter()
            .filter_map(|r| r["id"].as_i64())
            .collect())
    };
    Ok((
        ids("SELECT id FROM images")?,
        ids("SELECT id FROM detections WHERE crop_path IS NOT NULL")?,
    ))
}

fn orphaned(c: &Cached, images: &HashSet<i64>, crops: &HashSet<i64>) -> bool {
    match c.kind {
        Kind::Thumb | Kind::Preview => !images.contains(&c.id),
        Kind::Crop => !crops.contains(&c.id),
    }
}

/// What the cache holds, split by what dropping it would cost.
pub fn cache_info(d: &Desktop) -> Result<Value> {
    let dir = cache_dir(d);
    let (images, crops) = live(d)?;
    // thumbs, previews, crops that an album still uses; then the ones none does.
    let mut tally = [[0u64; 2]; 4];
    for c in cached(&dir) {
        let slot = if orphaned(&c, &images, &crops) {
            3
        } else {
            c.kind as usize
        };
        tally[slot][0] += 1;
        tally[slot][1] += c.bytes;
    }
    let entry = |t: [u64; 2]| json!({"files": t[0], "bytes": t[1]});
    let total = tally.iter().fold([0, 0], |s, t| [s[0] + t[0], s[1] + t[1]]);
    Ok(json!({
        "path": dir,
        "thumbs": entry(tally[0]),
        "previews": entry(tally[1]),
        "crops": entry(tally[2]),
        "orphaned": entry(tally[3]),
        "total": entry(total),
    }))
}

fn ensure_no_scan(d: &Desktop) -> Result<()> {
    if d.status()["activeJob"].is_null() {
        Ok(())
    } else {
        Err("A scan is running. Stop it first.".into())
    }
}

/// Resets also refuse while a background operation is working on the album.
fn ensure_idle(d: &Desktop, job: Option<i64>) -> Result<()> {
    if !d.status()["activeJob"].is_null() {
        return Err("A scan is running. Stop it first, then reset.".into());
    }
    let busy = d
        .operations
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .keys()
        .any(|k| job.is_none_or(|j| k.ends_with(&format!(":{j}"))));
    if busy {
        Err("An operation is running on this album. Cancel it or wait for it, then reset.".into())
    } else {
        Ok(())
    }
}

/// Drop what was chosen and nothing else. All off by default.
pub fn cache_clear(d: &Desktop, a: &CacheClearArgs) -> Result<Value> {
    ensure_no_scan(d)?;
    let (mut chosen, mut in_album) = (a.previews, HashSet::new());
    if let Some(job) = a.job_id {
        require_job(d, job)?;
        in_album = rows(
            &lock(&d.reader),
            "SELECT id FROM images WHERE job_id=?",
            [job],
        )?
        .iter()
        .filter_map(|r| r["id"].as_i64())
        .collect();
        chosen |= !in_album.is_empty();
    }
    if !(a.orphaned || chosen) {
        return Ok(json!({"removed": 0, "freed": 0}));
    }
    let task = d.hub.start("Clearing cache", 0);
    let (images, crops) = live(d)?;
    let doomed: Vec<_> = cached(&cache_dir(d))
        .into_iter()
        .filter(|c| {
            (a.orphaned && orphaned(c, &images, &crops))
                || (c.kind == Kind::Preview && (a.previews || in_album.contains(&c.id)))
        })
        .collect();
    let (mut removed, mut freed) = (0u64, 0u64);
    for (n, c) in doomed.iter().enumerate() {
        if std::fs::remove_file(&c.path).is_ok() {
            removed += 1;
            freed += c.bytes;
        }
        task.progress(n as u64 + 1, doomed.len() as u64);
    }
    task.detail(format!("Removed {removed} files"));
    task.finish();
    Ok(json!({"removed": removed, "freed": freed}))
}

/// Delete crops, and only crops inside the cache folder: the paths come from
/// the database, and a reset must not be a way to delete anything else.
fn remove_crops(d: &Desktop, paths: &[String]) -> usize {
    let cache = d.root.join("cache");
    paths
        .iter()
        .map(Path::new)
        .filter(|p| {
            p.starts_with(&cache)
                && !p.components().any(|c| c == Component::ParentDir)
                && std::fs::remove_file(p).is_ok()
        })
        .count()
}

/// `WHERE` for the images of one album, or of all of them, with its argument.
fn scope(job: Option<i64>) -> (&'static str, Vec<i64>) {
    match job {
        Some(job) => ("WHERE i.job_id = ?", vec![job]),
        None => ("", vec![]),
    }
}

fn crop_paths(d: &Desktop, job: Option<i64>) -> Result<Vec<String>> {
    let (filter, args) = scope(job);
    let joined = if filter.is_empty() { "WHERE" } else { "AND" };
    Ok(rows(
        &lock(&d.reader),
        &format!("SELECT d.crop_path FROM detections d JOIN images i ON i.id=d.image_id {filter} {joined} d.crop_path IS NOT NULL"),
        params_from_iter(&args),
    )?
    .iter()
    .filter_map(|r| r["crop_path"].as_str().map(str::to_owned))
    .collect())
}

/// Forget what the vision model said and keep everything else: every crop,
/// star, measurement, plate, hand-typed number and reject. The read-back groups
/// go with it, and so do the identities of detections nobody has reviewed:
/// those are what Identify reads again. A reviewed detection keeps its
/// attributes (a person may have typed them, and Identify skips it).
pub fn reset_identifications(d: &Desktop, job: Option<i64>) -> Result<Value> {
    ensure_idle(d, job)?;
    if let Some(job) = job {
        require_job(d, job)?;
    }
    let task = d.hub.start("Resetting identifications", 0);
    let (filter, args) = scope(job);
    let inside = format!("image_id IN (SELECT i.id FROM images i {filter})");
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    let run =
        |sql: String| -> Result<usize> { tx.execute(&sql, params_from_iter(&args)).map_err(err) };
    run(format!("UPDATE detections SET group_key=NULL,group_size=NULL,group_agreement=NULL,group_colour_hex=NULL WHERE {inside}"))?;
    let cleared = run(format!(
        "UPDATE detections SET attributes=NULL WHERE reviewed=0 AND {inside}"
    ))?;
    // A number the vision model read goes with it; one the OCR or a person
    // found does not.
    run(format!("UPDATE detections SET number=NULL,number_source=NULL,number_conf=NULL WHERE number_source='vlm' AND {inside}"))?;
    let kept: i64 = tx
        .query_row(
            &format!("SELECT COUNT(*) FROM detections WHERE reviewed=1 AND {inside}"),
            params_from_iter(&args),
            |r| r.get(0),
        )
        .map_err(err)?;
    tx.commit().map_err(err)?;
    task.finish();
    Ok(json!({"ok": true, "identifications_cleared": cleared, "kept_reviewed": kept}))
}

/// Clear manual star ratings and rejection flags across the album, restoring
/// frames and detections to the automated baseline measurements.
pub fn reset_ratings(d: &Desktop, job: Option<i64>) -> Result<Value> {
    ensure_idle(d, job)?;
    if let Some(job) = job {
        require_job(d, job)?;
    }
    let task = d.hub.start("Resetting ratings", 0);
    let (filter, args) = scope(job);
    let inside = format!("image_id IN (SELECT i.id FROM images i {filter})");
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    let run =
        |sql: String| -> Result<usize> { tx.execute(&sql, params_from_iter(&args)).map_err(err) };
    let frames_cleared = if let Some(j) = job {
        tx.execute(
            "UPDATE images SET stars=NULL, rejected=0 WHERE job_id=?",
            [j],
        )
        .map_err(err)?
    } else {
        tx.execute("UPDATE images SET stars=NULL, rejected=0", [])
            .map_err(err)?
    };
    let dets_cleared = run(format!(
        "UPDATE detections SET stars=NULL, rejected=0 WHERE {inside}"
    ))?;
    tx.commit().map_err(err)?;
    drop(db);
    task.finish();
    if let Some(j) = job {
        let _ = crate::passes::pick_keepers(d, j);
    }
    Ok(json!({"ok": true, "frames_cleared": frames_cleared, "detections_cleared": dets_cleared}))
}

/// Throw away every detection and identification, keeping the albums and the
/// frames in them. The frames go back to waiting, so scanning the album again
/// finds the subjects afresh without indexing the folder again.
pub fn reset_detections(d: &Desktop, job: Option<i64>) -> Result<Value> {
    ensure_idle(d, job)?;
    if let Some(job) = job {
        require_job(d, job)?;
    }
    let task = d.hub.start("Resetting detections", 0);
    let crops = crop_paths(d, job)?;
    let (filter, args) = scope(job);
    let gone = {
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        let gone = tx
            .execute(
                &format!(
                    "DELETE FROM detections WHERE image_id IN (SELECT i.id FROM images i {filter})"
                ),
                params_from_iter(&args),
            )
            .map_err(err)?;
        // What the scan measured goes; where and when the frame was shot stays.
        let images = filter.replace("i.job_id", "job_id");
        tx.execute(
            &format!("UPDATE images SET status='pending',error=NULL,sharpness=NULL,rating=NULL,burst_key=NULL {images}"),
            params_from_iter(&args),
        )
        .map_err(err)?;
        let jobs = job.map_or("", |_| "WHERE id = ?");
        tx.execute(
            &format!("UPDATE jobs SET status='stopped' {jobs}"),
            params_from_iter(&args),
        )
        .map_err(err)?;
        tx.commit().map_err(err)?;
        gone
    };
    let removed = remove_crops(d, &crops);
    task.finish();
    Ok(json!({"ok": true, "detections_removed": gone, "crops_removed": removed}))
}

/// Forget every album and everything read from it, and start again. Settings,
/// known vehicles and the training labels are kept: they are how the next scan
/// is set up, not a result of the last one.
pub fn reset_all(d: &Desktop) -> Result<Value> {
    ensure_idle(d, None)?;
    let task = d.hub.start("Resetting library", 0);
    let crops = crop_paths(d, None)?;
    let (jobs, frames) = {
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        let count = |table: &str| -> Result<i64> {
            tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .map_err(err)
        };
        let counts = (count("jobs")?, count("images")?);
        // In the order the cascade would take them, so it does not matter
        // whether this connection enforces foreign keys.
        for table in ["detections", "images", "jobs"] {
            tx.execute(&format!("DELETE FROM {table}"), [])
                .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        counts
    };
    let removed = remove_crops(d, &crops);
    task.finish();
    Ok(
        json!({"ok": true, "scans_removed": jobs, "frames_removed": frames, "crops_removed": removed}),
    )
}

// `ToSql` is named for the `params_from_iter(&Vec<i64>)` calls above.
#[allow(dead_code)]
fn _params_are_to_sql(_: &dyn ToSql) {}

#[cfg(test)]
mod tests {
    use crate::testkit::Lib;
    use rusqlite::params;
    use serde_json::json;
    use std::path::PathBuf;

    fn cache(lib: &Lib, name: &str, bytes: usize) -> PathBuf {
        let dir = lib.root.join("cache/native");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), vec![0u8; bytes]).unwrap();
        dir.join(name)
    }

    /// One frame with a thumbnail, a preview and a crop, plus the strays.
    fn stocked(lib: &Lib) -> (i64, i64) {
        let image = lib.frame("a.jpg", Some(1));
        let det = lib.detection(image, "vehicle", 0.9);
        let thumb = cache(lib, &format!("thumb-{image}.jpg"), 10);
        cache(lib, &format!("view-{image}.jpg"), 100);
        let crop = cache(lib, &format!("crop-{det}.jpg"), 1000);
        lib.sql(
            "UPDATE images SET thumb_path=? WHERE id=?",
            params![thumb.to_string_lossy(), image],
        );
        lib.sql(
            "UPDATE detections SET crop_path=? WHERE id=?",
            params![crop.to_string_lossy(), det],
        );
        cache(lib, "thumb-9999.jpg", 20);
        cache(lib, "view-9999.jpg", 200);
        cache(lib, "crop-9999.jpg", 2000);
        cache(lib, "notes.txt", 5);
        (image, det)
    }

    #[test]
    fn the_cache_survey_splits_in_use_from_orphaned_and_ignores_strangers() {
        let lib = Lib::new("cache-info");
        stocked(&lib);
        let out = lib.run("cache_info", json!({})).unwrap();
        assert_eq!(out["thumbs"], json!({"files": 1, "bytes": 10}));
        assert_eq!(out["previews"], json!({"files": 1, "bytes": 100}));
        assert_eq!(out["crops"], json!({"files": 1, "bytes": 1000}));
        assert_eq!(out["orphaned"], json!({"files": 3, "bytes": 2220}));
        assert_eq!(out["total"], json!({"files": 6, "bytes": 3330}));
    }

    #[test]
    fn clearing_drops_only_what_was_asked_for() {
        let lib = Lib::new("cache-clear");
        let (image, det) = stocked(&lib);
        let dir = lib.root.join("cache/native");
        let gone = |name: &str| !dir.join(name).exists();
        // Nothing chosen, nothing dropped.
        assert_eq!(
            lib.run("cache_clear", json!({})).unwrap(),
            json!({"removed": 0, "freed": 0})
        );
        assert!(!gone("view-9999.jpg"));

        let out = lib.run("cache_clear", json!({"orphaned": true})).unwrap();
        assert_eq!(out, json!({"removed": 3, "freed": 2220}));
        assert!(gone("thumb-9999.jpg") && gone("view-9999.jpg") && gone("crop-9999.jpg"));
        assert!(
            dir.join(format!("view-{image}.jpg")).exists(),
            "a preview in use stays unless asked"
        );
        assert!(dir.join("notes.txt").exists(), "not ours");

        // One album's previews; then every preview. Thumbnails and crops never.
        let out = lib.run("cache_clear", json!({"jobId": lib.job})).unwrap();
        assert_eq!(out, json!({"removed": 1, "freed": 100}));
        cache(&lib, &format!("view-{image}.jpg"), 100);
        let out = lib.run("cache_clear", json!({"previews": true})).unwrap();
        assert_eq!(out["removed"], 1);
        assert!(dir.join(format!("thumb-{image}.jpg")).exists());
        assert!(dir.join(format!("crop-{det}.jpg")).exists());
        assert!(lib
            .run("cache_clear", json!({"jobId": 404}))
            .unwrap_err()
            .contains("No such album"));
    }

    #[test]
    fn reset_identifications_forgets_the_model_and_keeps_the_work() {
        let lib = Lib::new("reset-ids");
        let image = lib.frame("a.jpg", Some(1));
        let (unread, reviewed, typed) = (
            lib.detection(image, "vehicle", 0.9),
            lib.detection(image, "vehicle", 0.8),
            lib.detection(image, "vehicle", 0.7),
        );
        lib.sql(
            "INSERT INTO known_vehicles(plate,make) VALUES('KEEP1','Kia')",
            [],
        );
        lib.sql(
            "UPDATE detections SET attributes='{\"make\":\"Ford\"}',number='7',number_source='vlm',number_conf=0.9,plate='ABC',stars=4,embedding='e',group_key=3,group_size=2,group_agreement=0.9,group_colour_hex='#fff' WHERE id=?",
            [unread],
        );
        lib.sql("UPDATE detections SET attributes='{\"make\":\"Typed\"}',reviewed=1,number='8',number_source='manual' WHERE id=?", [reviewed]);
        lib.sql("UPDATE detections SET attributes='{\"make\":\"Ocr\"}',number='9',number_source='ocr' WHERE id=?", [typed]);
        lib.sql("UPDATE jobs SET status='done' WHERE id=?", [lib.job]);

        let out = lib
            .run("reset_identifications", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            out,
            json!({"ok": true, "identifications_cleared": 2, "kept_reviewed": 1})
        );
        type Identification = (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
        );
        let row = |det: i64| -> Identification {
            lib.db().query_row(
                "SELECT attributes,number,number_source,group_key,stars FROM detections WHERE id=?",
                [det],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap()
        };
        assert_eq!(
            row(unread),
            (None, None, None, None, Some(4)),
            "the model's answers go, the star stays"
        );
        let plate: String = lib.one("SELECT plate FROM detections WHERE id=?", [unread]);
        let embedding: String = lib.one("SELECT embedding FROM detections WHERE id=?", [unread]);
        assert_eq!((plate.as_str(), embedding.as_str()), ("ABC", "e"));
        let kept = row(reviewed);
        assert_eq!(
            (kept.0.as_deref(), kept.1.as_deref()),
            (Some("{\"make\":\"Typed\"}"), Some("8"))
        );
        assert_eq!(
            row(typed).1.as_deref(),
            Some("9"),
            "an OCR number never involved the model"
        );
        let known: i64 = lib.one("SELECT COUNT(*) FROM known_vehicles", []);
        assert_eq!(known, 1);
        let status: String = lib.one("SELECT status FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(status, "done");
        assert!(lib
            .run("reset_identifications", json!({"jobId": 88}))
            .is_err());
        // Without a job it is every album.
        assert_eq!(
            lib.run("reset_identifications", json!({})).unwrap()["ok"],
            true
        );
    }

    #[test]
    fn reset_detections_removes_detections_and_our_crops_but_never_an_original() {
        let lib = Lib::new("reset-dets");
        let (image, det) = stocked(&lib);
        let other = lib.d();
        let _ = other;
        let original = lib.root.join("photo.cr3");
        std::fs::write(&original, b"raw").unwrap();
        let second = lib.detection(image, "vehicle", 0.5);
        lib.sql(
            "UPDATE detections SET crop_path=? WHERE id=?",
            params![original.to_string_lossy(), second],
        );
        lib.sql("UPDATE images SET rating=0.8,sharpness=0.7,burst_key=4,camera='EOS',stars=3 WHERE id=?", [image]);
        lib.sql("INSERT INTO known_vehicles(plate) VALUES('KEEP1')", []);
        lib.sql("INSERT INTO sharpness_labels(path,x1,y1,x2,y2,stars,created_at) VALUES('p',1,2,3,4,3,1)", []);
        lib.sql("UPDATE jobs SET status='done' WHERE id=?", [lib.job]);

        let out = lib
            .run("reset_detections", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            out,
            json!({"ok": true, "detections_removed": 2, "crops_removed": 1})
        );
        assert!(
            original.exists(),
            "a path outside the cache is never deleted"
        );
        assert!(!lib
            .root
            .join(format!("cache/native/crop-{det}.jpg"))
            .exists());
        assert!(lib
            .root
            .join(format!("cache/native/thumb-{image}.jpg"))
            .exists());
        let left: i64 = lib.one("SELECT COUNT(*) FROM detections", []);
        assert_eq!(left, 0);
        let (status, rating, burst, camera, stars): (
            String,
            Option<f64>,
            Option<i64>,
            String,
            i64,
        ) = lib
            .db()
            .query_row(
                "SELECT status,rating,burst_key,camera,stars FROM images WHERE id=?",
                [image],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(
            (status.as_str(), rating, burst, camera.as_str(), stars),
            ("pending", None, None, "EOS", 3)
        );
        let job: String = lib.one("SELECT status FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(job, "stopped", "so the album can be scanned again");
        let kept: i64 = lib.one(
            "SELECT (SELECT COUNT(*) FROM known_vehicles)+(SELECT COUNT(*) FROM sharpness_labels)",
            [],
        );
        assert_eq!(kept, 2);
    }

    #[test]
    fn reset_all_empties_the_library_but_not_the_people_s_data_and_waits_for_operations() {
        let lib = Lib::new("reset-all");
        let (image, det) = stocked(&lib);
        lib.sql("INSERT INTO known_vehicles(plate) VALUES('KEEP1')", []);
        lib.sql("INSERT INTO sharpness_labels(path,x1,y1,x2,y2,stars,created_at) VALUES('p',1,2,3,4,3,1)", []);
        crate::lock(&lib.d().operations)
            .insert(format!("Grouping vehicles:{}", lib.job), Default::default());
        let err = lib.run("reset_all", json!({})).unwrap_err();
        assert!(err.contains("operation is running"), "{err}");
        assert!(lib
            .run("reset_detections", json!({"jobId": lib.job}))
            .is_err());
        crate::lock(&lib.d().operations).clear();

        let out = lib.run("reset_all", json!({})).unwrap();
        assert_eq!(
            out,
            json!({"ok": true, "scans_removed": 1, "frames_removed": 1, "crops_removed": 1})
        );
        let counts: (i64, i64, i64, i64, i64) = lib
            .db()
            .query_row(
                "SELECT (SELECT COUNT(*) FROM jobs),(SELECT COUNT(*) FROM images),(SELECT COUNT(*) FROM detections),(SELECT COUNT(*) FROM known_vehicles),(SELECT COUNT(*) FROM sharpness_labels)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 0, 1, 1));
        assert!(!lib
            .root
            .join(format!("cache/native/crop-{det}.jpg"))
            .exists());
        // The scan's thumbnails are left for the cache clear to find as orphans.
        assert!(lib
            .root
            .join(format!("cache/native/thumb-{image}.jpg"))
            .exists());
    }

    #[test]
    fn reset_ratings_clears_manual_stars_and_rejections() {
        let lib = Lib::new("reset-ratings");
        let (image, det) = stocked(&lib);
        lib.sql("UPDATE images SET stars=5, rejected=1 WHERE id=?", [image]);
        lib.sql(
            "UPDATE detections SET stars=5, rejected=1 WHERE id=?",
            [det],
        );

        let out = lib.run("reset_ratings", json!({"jobId": lib.job})).unwrap();
        assert_eq!(out["ok"], true);
        assert_eq!(out["frames_cleared"], 1);
        assert_eq!(out["detections_cleared"], 1);

        let (stars, rejected): (Option<i64>, i64) = lib
            .db()
            .query_row(
                "SELECT stars, rejected FROM images WHERE id=?",
                [image],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(stars, None);
        assert_eq!(rejected, 0);

        let (det_stars, det_rejected): (Option<i64>, i64) = lib
            .db()
            .query_row(
                "SELECT stars, rejected FROM detections WHERE id=?",
                [det],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(det_stars, None);
        assert_eq!(det_rejected, 0);
    }
}
