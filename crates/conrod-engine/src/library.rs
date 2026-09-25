//! The album's own sheet: rename it, summarise it, pick its cover. Ports
//! `POST /api/jobs/{id}`, `GET /api/jobs/{id}/summary` and `.../cover`.
//! Missing thumbnails are filled on demand when an album opens.
use crate::commands::{RenameArgs, UpdateJobSettingsArgs};
use crate::desktop::{rows, Desktop, Result};
use rusqlite::params;
use serde_json::{json, Map, Value};
use std::path::Path;
use crate::lock;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// A readable error, rather than SQLite's, for an album that is not there.
pub(crate) fn require_job(d: &Desktop, job: i64) -> Result<()> {
    let found = rows(
        &lock(&d.reader),
        "SELECT id FROM jobs WHERE id=?",
        [job],
    )?;
    if found.is_empty() {
        Err("No such album".into())
    } else {
        Ok(())
    }
}

pub fn rename_job(d: &Desktop, a: &RenameArgs) -> Result<Value> {
    let label = a.label.as_deref().map(str::trim).filter(|l| !l.is_empty());
    let changed =
        lock(&d.db)
            .execute(
                "UPDATE jobs SET label=? WHERE id=?",
                params![label, a.job_id],
            )
            .map_err(err)?;
    if changed == 0 {
        return Err("No such album".into());
    }
    Ok(json!({ "label": label }))
}

pub fn update_job_settings(d: &Desktop, a: &UpdateJobSettingsArgs) -> Result<Value> {
    require_job(d, a.job_id)?;
    let db = lock(&d.db);
    let current_raw: Option<String> = db
        .query_row(
            "SELECT settings_json FROM jobs WHERE id=?",
            [a.job_id],
            |r| r.get(0),
        )
        .map_err(err)?;
    let mut map: Map<String, Value> = current_raw
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    for (k, v) in &a.patch {
        if k == "scan_profile" {
            if let Some(s) = v.as_str() {
                let preset = conrod_core::profile::ShootPreset::parse(s);
                map.insert(k.clone(), Value::String(preset.name().into()));
                continue;
            }
        }
        map.insert(k.clone(), v.clone());
    }

    let updated_json = serde_json::to_string(&map).map_err(err)?;
    db.execute(
        "UPDATE jobs SET settings_json=? WHERE id=?",
        params![updated_json, a.job_id],
    )
    .map_err(err)?;

    Ok(json!({ "ok": true, "jobId": a.job_id, "settings": map }))
}

const VEHICLE: &str = "COALESCE(d.region_type,'vehicle')='vehicle'";

/// What the album sheet shows: the album, how much is identified and how much
/// still wants a look, and which numbers and plates turned up.
pub fn summary(d: &Desktop, job: i64) -> Result<Value> {
    require_job(d, job)?;
    let settings = crate::operations::settings(d, job)?;
    let mapping = crate::catalog::mapping(&settings)?;
    let threshold = settings.ocr_accept_confidence;
    let db = lock(&d.reader);
    let album = rows(&db, "SELECT * FROM jobs WHERE id=?", [job])?
        .into_iter()
        .next()
        .ok_or("No such album")?;
    let first = |mut found: Vec<Value>| found.pop().unwrap_or(Value::Null);
    // Numbers, plates and the review count are about vehicles; a face has none.
    let counts = first(rows(
        &db,
        &format!(
            "SELECT COUNT(*) AS detections,
                    COALESCE(SUM(CASE WHEN {VEHICLE} AND d.number IS NOT NULL THEN 1 END),0) AS numbered,
                    COALESCE(SUM(CASE WHEN {VEHICLE} AND d.plate IS NOT NULL THEN 1 END),0) AS plated,
                    COALESCE(SUM(CASE WHEN d.rejected=1 THEN 1 END),0) AS rejected,
                    COALESCE(SUM(CASE WHEN d.reviewed=1 THEN 1 END),0) AS reviewed,
                    COALESCE(SUM(CASE WHEN {VEHICLE} AND _needs_review(d.number_conf,d.reviewed,d.rejected,d.uncertain,?2)=1 THEN 1 END),0) AS to_review
               FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?1"
        ),
        params![job, threshold],
    )?);
    let images = first(rows(
        &db,
        "SELECT COUNT(*) AS total,
                COALESCE(SUM(CASE WHEN status='done' THEN 1 END),0) AS scanned,
                COALESCE(SUM(CASE WHEN written_at IS NOT NULL THEN 1 END),0) AS written,
                COALESCE(SUM(CASE WHEN status='error' THEN 1 END),0) AS errors
           FROM images WHERE job_id=?",
        [job],
    )?);
    // Resolve race numbers through this album's entry list.
    let numbers: Vec<Value> = rows(
        &db,
        &format!("SELECT d.number AS number, COUNT(*) AS frames FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND {VEHICLE} AND d.number IS NOT NULL AND d.rejected=0 GROUP BY d.number ORDER BY frames DESC, d.number"),
        [job],
    )?
    .into_iter()
    .map(|mut n| {
        n["who"] = json!(mapping.as_ref().map(|m| m.describe(n["number"].as_str().unwrap_or_default())).unwrap_or_default());
        n
    })
    .collect();
    let plates = rows(
        &db,
        &format!("SELECT d.plate AS plate, COUNT(*) AS frames FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND {VEHICLE} AND d.plate IS NOT NULL AND d.rejected=0 GROUP BY d.plate ORDER BY frames DESC, d.plate"),
        [job],
    )?;
    let by_region: Map<String, Value> = rows(
        &db,
        "SELECT COALESCE(d.region_type,'vehicle') AS region, COUNT(*) AS n FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? GROUP BY region",
        [job],
    )?
    .into_iter()
    .map(|r| (r["region"].as_str().unwrap_or("vehicle").to_owned(), r["n"].clone()))
    .collect();
    Ok(json!({
        "job": album,
        "counts": counts,
        "images": images,
        "numbers": numbers,
        "plates": plates,
        "map_size": mapping.as_ref().map_or(0, |m| m.len()),
        "by_region": by_region,
    }))
}

/// A picture to stand for the album on its card: the surest vehicle crop that
/// is still on disk, else the thumbnail of its best-rated frame.
pub fn cover(d: &Desktop, job: i64) -> Result<Value> {
    require_job(d, job)?;
    let db = lock(&d.reader);
    let candidates = |sql: &str, column: &str| -> Result<Option<String>> {
        Ok(rows(&db, sql, [job])?
            .iter()
            .filter_map(|r| r[column].as_str())
            .find(|p| Path::new(p).is_file())
            .map(str::to_owned))
    };
    if let Some(path) = candidates(
        "SELECT d.crop_path FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND d.crop_path IS NOT NULL AND d.rejected=0 ORDER BY d.conf DESC LIMIT 20",
        "crop_path",
    )? {
        return Ok(json!({"path": path, "source": "crop"}));
    }
    if let Some(path) = candidates(
        "SELECT thumb_path FROM images WHERE job_id=? AND thumb_path IS NOT NULL AND rejected=0 ORDER BY COALESCE(stars*0.2, rating, 0) DESC, id LIMIT 20",
        "thumb_path",
    )? {
        return Ok(json!({"path": path, "source": "thumb"}));
    }
    Ok(Value::Null)
}

/// Index-only and older libraries acquire thumbnails without invoking detection.
pub fn filling(d: &std::sync::Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    if d.scanning() || !lock(&d.operations).is_empty() {
        return Ok(d.status());
    }
    let todo = rows(&lock(&d.reader), "SELECT id,path FROM images WHERE job_id=? AND thumb_path IS NULL AND error IS NULL ORDER BY id", [job])?;
    if todo.is_empty() {
        return Ok(d.status());
    }
    crate::operations::launch(d, job, "Filling thumbnails", move |d, stop, task| {
        for (index, row) in todo.iter().enumerate() {
            if stop.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
            let id = row["id"].as_i64().ok_or("Missing image id")?;
            let path = Path::new(row["path"].as_str().ok_or("Missing image path")?);
            let result = (|| -> Result<()> {
                let raw = conrod_io::raw::read(path)?;
                let rgb = conrod_vision::imageops::Rgb::decode_jpeg(&raw.preview, 1)?
                    .orient(raw.orientation);
                let output = d.root.join(format!("cache/native/thumb-{id}.jpg"));
                crate::desktop::save_jpeg(&crate::thumbnail(&rgb), &output)?;
                lock(&d.db)
                    .execute(
                        "UPDATE images SET thumb_path=?,width=?,height=? WHERE id=?",
                        params![
                            output.to_string_lossy(),
                            rgb.width as i64,
                            rgb.height as i64,
                            id
                        ],
                    )
                    .map_err(err)?;
                Ok(())
            })();
            if let Err(e) = result {
                lock(&d.db)
                    .execute("UPDATE images SET error=? WHERE id=?", params![e, id])
                    .map_err(err)?;
            }
            task.progress((index + 1) as u64, todo.len() as u64);
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use crate::testkit::Lib;
    use rusqlite::params;
    use serde_json::json;

    #[test]
    fn rename_trims_and_a_blank_label_goes_back_to_the_folder_name() {
        let lib = Lib::new("rename");
        let out = lib
            .run(
                "rename_job",
                json!({"jobId": lib.job, "label": "  Bathurst Sunday "}),
            )
            .unwrap();
        assert_eq!(out["label"], "Bathurst Sunday");
        let label: Option<String> = lib.one("SELECT label FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(label.as_deref(), Some("Bathurst Sunday"));
        lib.run("rename_job", json!({"jobId": lib.job, "label": "   "}))
            .unwrap();
        let label: Option<String> = lib.one("SELECT label FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(label, None);
        lib.run("rename_job", json!({"jobId": lib.job, "label": "x"}))
            .unwrap();
        lib.run("rename_job", json!({"jobId": lib.job, "label": null}))
            .unwrap();
        let label: Option<String> = lib.one("SELECT label FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(label, None);
        assert!(lib
            .run("rename_job", json!({"jobId": 99, "label": "x"}))
            .unwrap_err()
            .contains("No such album"));
    }

    #[test]
    fn the_summary_counts_vehicles_and_lists_numbers_and_plates() {
        let lib = Lib::new("summary");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        lib.sql("UPDATE images SET written_at=1 WHERE id=?", [f1]);
        lib.sql(
            "INSERT INTO images(job_id,path,status) VALUES(?,'bad.jpg','error')",
            [lib.job],
        );
        let (a, b, c) = (
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f2, "vehicle", 0.9),
            lib.detection(f2, "vehicle", 0.9),
        );
        let face = lib.detection(f2, "face", 0.9);
        // a: confident number, plate; b: same number, low confidence; c: rejected, unnumbered.
        lib.sql(
            "UPDATE detections SET number='7',number_conf=0.95,plate='ABC123' WHERE id=?",
            [a],
        );
        lib.sql(
            "UPDATE detections SET number='7',number_conf=0.3 WHERE id=?",
            [b],
        );
        lib.sql(
            "UPDATE detections SET rejected=1,reviewed=1 WHERE id=?",
            [c],
        );
        let out = lib.run("summary", json!({"jobId": lib.job})).unwrap();
        assert_eq!(out["job"]["label"], "Test");
        let counts = &out["counts"];
        assert_eq!(counts["detections"], 4);
        assert_eq!(
            (counts["numbered"].as_i64(), counts["plated"].as_i64()),
            (Some(2), Some(1))
        );
        assert_eq!(
            (counts["rejected"].as_i64(), counts["reviewed"].as_i64()),
            (Some(1), Some(1))
        );
        assert_eq!(
            counts["to_review"], 1,
            "only b: a is sure, c was dealt with, a face is not a vehicle"
        );
        assert_eq!(
            out["images"],
            json!({"total": 3, "scanned": 2, "written": 1, "errors": 1})
        );
        assert_eq!(
            out["numbers"],
            json!([{"number": "7", "frames": 2, "who": ""}])
        );
        assert_eq!(out["plates"], json!([{"plate": "ABC123", "frames": 1}]));
        assert_eq!(out["by_region"], json!({"vehicle": 3, "face": 1}));
        let _ = face;
        assert!(lib
            .run("summary", json!({"jobId": 55}))
            .unwrap_err()
            .contains("No such album"));
    }

    #[test]
    fn the_cover_is_a_crop_that_exists_else_a_thumbnail_else_nothing() {
        let lib = Lib::new("cover");
        assert!(lib
            .run("cover", json!({"jobId": lib.job}))
            .unwrap()
            .is_null());
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let (thumb1, thumb2) = (lib.root.join("t1.jpg"), lib.root.join("t2.jpg"));
        std::fs::write(&thumb2, b"x").unwrap();
        lib.sql(
            "UPDATE images SET thumb_path=?,rating=0.5 WHERE id=?",
            params![thumb1.to_string_lossy(), f1],
        );
        lib.sql(
            "UPDATE images SET thumb_path=?,rating=0.4 WHERE id=?",
            params![thumb2.to_string_lossy(), f2],
        );
        let out = lib.run("cover", json!({"jobId": lib.job})).unwrap();
        assert_eq!(out["source"], "thumb");
        assert_eq!(
            out["path"].as_str().map(std::path::Path::new),
            Some(thumb2.as_path()),
            "the best thumbnail that is still there"
        );

        let (missing, present, rejected) = (
            lib.root.join("missing.jpg"),
            lib.root.join("crop-2.jpg"),
            lib.root.join("crop-3.jpg"),
        );
        std::fs::write(&present, b"x").unwrap();
        std::fs::write(&rejected, b"x").unwrap();
        let (d1, d2, d3) = (
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f2, "vehicle", 0.9),
        );
        for (det, path, conf, rej) in [
            (d1, &missing, 0.99, 0),
            (d2, &present, 0.8, 0),
            (d3, &rejected, 0.95, 1),
        ] {
            lib.sql(
                "UPDATE detections SET crop_path=?,conf=?,rejected=? WHERE id=?",
                params![path.to_string_lossy(), conf, rej, det],
            );
        }
        let out = lib.run("cover", json!({"jobId": lib.job})).unwrap();
        assert_eq!(out["source"], "crop");
        assert_eq!(
            out["path"].as_str().map(std::path::Path::new),
            Some(present.as_path())
        );
    }

    #[test]
    fn update_job_settings_persists_preset_and_toggles() {
        let lib = Lib::new("update_settings");
        let res = lib
            .run(
                "update_job_settings",
                json!({
                    "jobId": lib.job,
                    "patch": {
                        "scan_profile": "motorsport-track",
                        "read_numbers": false,
                        "read_plates": false
                    }
                }),
            )
            .unwrap();
        assert_eq!(res["ok"], true);
        assert_eq!(res["settings"]["scan_profile"], "motorsport-track");
        assert_eq!(res["settings"]["read_numbers"], false);
        assert_eq!(res["settings"]["read_plates"], false);

        let settings = crate::operations::settings(lib.d(), lib.job).unwrap();
        assert_eq!(settings.scan_profile, "motorsport-track");
        assert!(!settings.read_numbers);
        assert!(!settings.read_plates);
    }
}
