//! The album's own sheet: rename it, summarise it, pick its cover. Ports
//! `POST /api/jobs/{id}`, `GET /api/jobs/{id}/summary` and `.../cover`.
//! Missing thumbnails are filled on demand when an album opens.
use crate::commands::{RenameArgs, UpdateJobSettingsArgs};
use crate::desktop::{rows, Desktop, Result};
use crate::err;
use crate::lock;
use rusqlite::params;
use serde_json::{json, Map, Value};
use std::path::Path;

/// A readable error, rather than SQLite's, for an album that is not there.
pub(crate) fn require_job(d: &Desktop, job: i64) -> Result<()> {
    let found = rows(&lock(&d.reader), "SELECT id FROM jobs WHERE id=?", [job])?;
    if found.is_empty() {
        Err("No such album".into())
    } else {
        Ok(())
    }
}

pub fn rename_job(d: &Desktop, a: &RenameArgs) -> Result<Value> {
    let label = a.label.as_deref().map(str::trim).filter(|l| !l.is_empty());
    let changed = lock(&d.db)
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
    let regroup_keys = ["burst_gap", "scan_profile", "overrides"];
    let wants_regroup = a.patch.keys().any(|k| regroup_keys.contains(&k.as_str()));
    let updated_json = {
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
        // An album stored before per-album gaps carries the old ignored 4.0;
        // rewrite it before the patch lands so a deliberate 4.0 sticks.
        conrod_core::settings::migrate_burst_gap(&mut map);
        // Drop an old album's stale vision copy first, so a choice made now is
        // the first the album has ever had.
        conrod_core::settings::follow_global_vlm(&mut map);

        for (k, v) in &a.patch {
            if k == "use_vlm" {
                // The album's own vision choice: a boolean, or null to follow
                // the global switch again. Anything else is ignored.
                match v {
                    Value::Bool(_) => {
                        map.insert(k.clone(), v.clone());
                    }
                    Value::Null => {
                        map.remove(k);
                    }
                    _ => {}
                }
                continue;
            }
            if k == "scan_profile" {
                if let Some(s) = v.as_str() {
                    let preset = conrod_core::profile::ShootPreset::parse(s);
                    // A new shoot type takes the photographer's defaults for
                    // that type; the album's own overrides stay as sent.
                    if map.get("scan_profile").and_then(Value::as_str) != Some(preset.name()) {
                        let defaults = lock(&d.settings).profile().defaults_for(preset);
                        map.insert("user_defaults".into(), Value::Object(defaults.to_map()));
                    }
                    map.insert(k.clone(), Value::String(preset.name().into()));
                    continue;
                }
            }
            map.insert(k.clone(), v.clone());
        }
        if a.patch.contains_key("blur_strictness") {
            map.insert("blur_explicit".into(), Value::Bool(true));
        }

        let updated_json = serde_json::to_string(&map).map_err(err)?;
        db.execute(
            "UPDATE jobs SET settings_json=? WHERE id=?",
            params![updated_json, a.job_id],
        )
        .map_err(err)?;
        updated_json
    };
    let map: Map<String, Value> = serde_json::from_str(&updated_json).unwrap_or_default();
    // A burst gap or preset change needs no rescan: regroup in place now, so the
    // album on screen changes as the control does. Not while a scan is writing
    // the same album: it regroups at the end anyway.
    let regroup = if wants_regroup && !d.scanning() {
        crate::desktop::Desktop::regroup(d, a.job_id).ok()
    } else {
        None
    };
    Ok(json!({ "ok": true, "jobId": a.job_id, "settings": map, "regroup": regroup }))
}

const VEHICLE: &str = "COALESCE(d.region_type,'vehicle')='vehicle'";

/// What the album sheet shows: the album, how much is identified and how much
/// still wants a look, and which numbers and plates turned up.
pub fn summary(d: &Desktop, job: i64) -> Result<Value> {
    require_job(d, job)?;
    let settings = crate::operations::settings(d, job)?;
    let mapping = crate::catalog::mapping(&d.root, &settings)?;
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
               FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?1 AND COALESCE(i.copy_of,0)=0"
        ),
        params![job, threshold],
    )?);
    let images = first(rows(
        &db,
        "SELECT COUNT(*) AS total,
                COALESCE(SUM(CASE WHEN status='done' THEN 1 END),0) AS scanned,
                COALESCE(SUM(CASE WHEN written_at IS NOT NULL THEN 1 END),0) AS written,
                COALESCE(SUM(CASE WHEN status='error' THEN 1 END),0) AS errors
           FROM images WHERE job_id=? AND COALESCE(copy_of,0)=0",
        [job],
    )?);
    // Resolve race numbers through this album's entry list.
    let numbers: Vec<Value> = rows(
        &db,
        &format!("SELECT d.number AS number, COUNT(*) AS frames FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND {VEHICLE} AND d.number IS NOT NULL AND d.rejected=0 AND COALESCE(i.copy_of,0)=0 GROUP BY d.number ORDER BY frames DESC, d.number"),
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
        &format!("SELECT d.plate AS plate, COUNT(*) AS frames FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND {VEHICLE} AND d.plate IS NOT NULL AND d.rejected=0 AND COALESCE(i.copy_of,0)=0 GROUP BY d.plate ORDER BY frames DESC, d.plate"),
        [job],
    )?;
    let by_region: Map<String, Value> = rows(
        &db,
        "SELECT COALESCE(d.region_type,'vehicle') AS region, COUNT(*) AS n FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(i.copy_of,0)=0 GROUP BY region",
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

/// A picture to stand for the album on its card: for a motorsport shoot, the
/// biggest sharp vehicle crop that is still on disk; otherwise (portraits,
/// events, mixed) a close-up crop is as likely to be a face, so we go
/// straight to the thumbnail of the best-rated full frame.
pub fn cover(d: &Desktop, job: i64) -> Result<Value> {
    require_job(d, job)?;
    let settings = crate::operations::settings(d, job)?;
    let vehicle_cover = settings.lanes().vehicles;
    let db = lock(&d.reader);
    let candidates = |sql: &str, column: &str| -> Result<Option<String>> {
        Ok(rows(&db, sql, [job])?
            .iter()
            .filter_map(|r| r[column].as_str())
            .find(|p| Path::new(p).is_file())
            .map(str::to_owned))
    };
    if vehicle_cover {
        if let Some(path) = candidates(
            &format!(
                "SELECT d.crop_path FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND d.crop_path IS NOT NULL AND d.rejected=0 AND COALESCE(i.copy_of,0)=0 AND {VEHICLE} ORDER BY d.conf * (d.x2-d.x1) * (d.y2-d.y1) * (1 + COALESCE(d.sharpness,0)) DESC LIMIT 20"
            ),
            "crop_path",
        )? {
            return Ok(json!({"path": path, "source": "crop"}));
        }
    }
    if let Some(path) = candidates(
        "SELECT thumb_path FROM images WHERE job_id=? AND thumb_path IS NOT NULL AND rejected=0 AND COALESCE(copy_of,0)=0 ORDER BY COALESCE(stars*0.2, rating, 0) DESC, id LIMIT 20",
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
    let todo = rows(&lock(&d.reader), "SELECT id,path FROM images WHERE job_id=? AND thumb_path IS NULL AND error IS NULL AND COALESCE(copy_of,0)=0 ORDER BY id", [job])?;
    if todo.is_empty() {
        return Ok(d.status());
    }
    crate::operations::launch(d, job, "Filling thumbnails", move |d, stop, task| {
        // Thumbnails are disk- and JPEG-bound, not cores: a small pool is
        // enough, and `preview_workers` sizes it (applies next scan/backfill).
        let workers = conrod_core::settings::clamp_workers(
            lock(&d.settings).preview_workers,
            conrod_core::settings::worker_caps().preview,
            4,
        ) as usize;
        let workers = workers.min(todo.len().max(1));
        let next = std::sync::atomic::AtomicUsize::new(0);
        let done = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..workers {
                let todo = &todo;
                let (next, done) = (&next, &done);
                let stop = &stop;
                scope.spawn(move || loop {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    let Some(row) =
                        todo.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                    else {
                        break;
                    };
                    let Ok(id) = row["id"].as_i64().ok_or("Missing image id") else {
                        break;
                    };
                    let Some(path) = row["path"].as_str().map(Path::new) else {
                        break;
                    };
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
                        let _ = lock(&d.db)
                            .execute("UPDATE images SET error=? WHERE id=?", params![e, id]);
                    }
                    let n = done.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    task.progress(n as u64, todo.len() as u64);
                });
            }
        });
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
    fn a_portrait_album_covers_with_the_thumbnail_not_a_face_crop() {
        let lib = Lib::new("cover-portrait");
        lib.run(
            "update_job_settings",
            json!({"jobId": lib.job, "patch": {"scan_profile": "portrait"}}),
        )
        .unwrap();
        let f1 = lib.frame("1.jpg", None);
        let thumb = lib.root.join("t1.jpg");
        std::fs::write(&thumb, b"x").unwrap();
        lib.sql(
            "UPDATE images SET thumb_path=?,rating=0.5 WHERE id=?",
            params![thumb.to_string_lossy(), f1],
        );
        let crop = lib.root.join("crop-1.jpg");
        std::fs::write(&crop, b"x").unwrap();
        let d1 = lib.detection(f1, "face", 0.99);
        lib.sql(
            "UPDATE detections SET crop_path=?,conf=? WHERE id=?",
            params![crop.to_string_lossy(), 0.99, d1],
        );
        let out = lib.run("cover", json!({"jobId": lib.job})).unwrap();
        assert_eq!(out["source"], "thumb", "a face crop is not a cover");
        assert_eq!(
            out["path"].as_str().map(std::path::Path::new),
            Some(thumb.as_path())
        );
    }

    #[test]
    fn changing_the_burst_gap_or_preset_regroups_the_album_in_place() {
        use crate::testkit::Lib;
        let lib = Lib::new("auto-regroup");
        for (i, name) in ["1.jpg", "2.jpg", "3.jpg"].iter().enumerate() {
            let image = lib.frame(name, None);
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=? WHERE id=?",
                rusqlite::params![f64::from(i as u32) * 1.0, image],
            );
            lib.detection(image, "vehicle", 0.5);
        }
        let out = lib
            .run(
                "update_job_settings",
                json!({"jobId": lib.job, "patch": {"burst_gap": 0.5}}),
            )
            .unwrap();
        assert_eq!(
            out["regroup"]["bursts"], 3,
            "one second apart, half a second gap"
        );
        assert_eq!(out["regroup"]["moments"], 3);
        let tight: i64 = lib.one(
            "SELECT count(DISTINCT burst_key) FROM images WHERE job_id=?",
            [lib.job],
        );
        assert_eq!(tight, 3);

        let out = lib
            .run(
                "update_job_settings",
                json!({"jobId": lib.job, "patch": {"burst_gap": 5.0}}),
            )
            .unwrap();
        assert_eq!(
            out["regroup"]["bursts"], 1,
            "five seconds swallows the lull"
        );

        let out = lib
            .run(
                "update_job_settings",
                json!({"jobId": lib.job, "patch": {"scan_profile": "portrait"}}),
            )
            .unwrap();
        assert_eq!(out["regroup"]["bursts"], 1, "a preset change regroups too");
        assert!(
            out["regroup"]["keepers"].is_number(),
            "and picks the keepers again: {out}"
        );

        let untouched = lib
            .run(
                "update_job_settings",
                json!({"jobId": lib.job, "patch": {"read_plates": false}}),
            )
            .unwrap();
        assert!(
            untouched["regroup"].is_null(),
            "a setting that needs no regrouping does not run the stage"
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
