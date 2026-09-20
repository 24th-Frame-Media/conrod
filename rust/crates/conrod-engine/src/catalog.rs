//! Entry lists and the cross-album vehicle registry.
use crate::desktop::{rows, Desktop, Result};
use conrod_core::{
    mapping::NumberMap,
    registry::{self, Member, Reading, Row},
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::BTreeMap;
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
fn row(v: &Value) -> Row {
    let text = |k: &str| v[k].as_str().map(str::to_owned);
    Row {
        plate: text("plate").unwrap_or_default(),
        make: text("make"),
        model: text("model"),
        colour: text("colour"),
        body_type: text("body_type"),
        team: text("team"),
        sponsors: text("sponsors"),
        race_number: text("race_number"),
    }
}
fn put(db: &Connection, r: &Row) -> Result<()> {
    db.execute("INSERT INTO known_vehicles(plate,make,model,colour,body_type,team,sponsors,race_number,updated_at) VALUES(?,?,?,?,?,?,?,?,unixepoch('now')) ON CONFLICT(plate) DO UPDATE SET make=excluded.make,model=excluded.model,colour=excluded.colour,body_type=excluded.body_type,team=excluded.team,sponsors=excluded.sponsors,race_number=excluded.race_number,updated_at=excluded.updated_at", params![r.plate,r.make,r.model,r.colour,r.body_type,r.team,r.sponsors,r.race_number]).map_err(err)?;
    Ok(())
}
pub fn export(d: &Desktop) -> Result<Value> {
    Ok(json!(registry::to_csv(
        &rows(
            &d.reader.lock().unwrap(),
            "SELECT * FROM known_vehicles ORDER BY plate",
            []
        )?
        .iter()
        .map(row)
        .collect::<Vec<_>>()
    )))
}
pub fn import(d: &Desktop, csv: &str) -> Result<Value> {
    let (incoming, skipped) = registry::parse_csv(csv)?;
    let db = d.db.lock().unwrap();
    let tx = db.unchecked_transaction().map_err(err)?;
    for r in &incoming {
        let existing = rows(
            &tx,
            "SELECT * FROM known_vehicles WHERE plate=?",
            [&r.plate],
        )?
        .first()
        .map(row);
        put(&tx, &registry::merge_csv_row(r, existing.as_ref()))?;
    }
    tx.commit().map_err(err)?;
    Ok(json!({"written": incoming.len(), "skipped": skipped}))
}
pub fn seed(d: &Desktop, job: Option<i64>) -> Result<Value> {
    if let Some(job) = job {
        crate::library::require_job(d, job)?;
    }
    let db = d.db.lock().unwrap();
    let tx = db.unchecked_transaction().map_err(err)?;
    let found = rows(&tx, "SELECT d.*,i.job_id FROM detections d JOIN images i ON i.id=d.image_id WHERE (?1 IS NULL OR i.job_id=?1) AND d.plate IS NOT NULL AND d.plate!='' AND d.attributes IS NOT NULL ORDER BY d.id", [job])?;
    let mut cars: BTreeMap<(i64, String), Vec<Member>> = BTreeMap::new();
    let mut loose = Vec::new();
    let mut order = Vec::new();
    for v in &found {
        let Some(attrs) = v["attributes"]
            .as_str()
            .and_then(|s| serde_json::from_str::<serde_json::Map<String, Value>>(s).ok())
        else {
            continue;
        };
        let plate = v["plate"].as_str().unwrap_or_default();
        if v["group_key"].is_null() {
            loose.push(Reading::from_parsed(plate, &attrs, v["number"].as_str()));
        } else {
            let key = (
                v["job_id"].as_i64().unwrap_or_default(),
                v["group_key"].to_string(),
            );
            if !cars.contains_key(&key) {
                order.push(key.clone());
            }
            cars.entry(key).or_default().push(Member {
                plate: Some(plate.into()),
                attributes: attrs,
            });
        }
    }
    let mut readings = Vec::new();
    for key in order {
        let members = &cars[&key];
        if let Some(r) = registry::agreed(members) {
            readings.push(r);
        }
    }
    readings.extend(loose);
    let mut written = 0;
    for r in &readings {
        let plate = registry::normalise(Some(&r.plate));
        if plate.is_empty() {
            continue;
        }
        let fresh = Row {
            plate: plate.clone(),
            make: r.make.clone(),
            model: r.model.clone(),
            colour: r.colour.clone(),
            body_type: r.body_type.clone(),
            team: r.team.clone(),
            sponsors: r
                .sponsors
                .as_ref()
                .map(|s| s.join(", "))
                .filter(|s| !s.is_empty()),
            race_number: r.race_number.clone(),
        };
        if [
            &fresh.make,
            &fresh.model,
            &fresh.colour,
            &fresh.body_type,
            &fresh.team,
            &fresh.sponsors,
            &fresh.race_number,
        ]
        .iter()
        .all(|v| v.as_deref().is_none_or(str::is_empty))
        {
            continue;
        }
        let existing = rows(&tx, "SELECT * FROM known_vehicles WHERE plate=?", [&plate])?;
        // Seeding fills blanks; a CSV import deliberately replaces supplied fields.
        let merged = existing.first().map_or_else(
            || fresh.clone(),
            |v| registry::merge_csv_row(&row(v), Some(&fresh)),
        );
        let mut aliases: std::collections::BTreeSet<String> = r
            .aliases
            .iter()
            .map(|a| registry::normalise(Some(a)))
            .collect();
        if let Some(old) = existing.first().and_then(|v| v["aliases"].as_str()) {
            aliases.extend(old.split(',').map(|a| registry::normalise(Some(a))));
        }
        aliases.remove("");
        aliases.remove(&plate);
        let aliases = aliases.into_iter().collect::<Vec<_>>().join(", ");
        if existing.first().is_some_and(|old| {
            row(old) == merged && old["aliases"].as_str().unwrap_or_default() == aliases
        }) {
            continue;
        }
        put(&tx, &merged)?;
        tx.execute(
            "UPDATE known_vehicles SET aliases=? WHERE plate=?",
            params![aliases, plate],
        )
        .map_err(err)?;
        written += 1;
    }
    let known: i64 = tx
        .query_row("SELECT count(*) FROM known_vehicles", [], |r| r.get(0))
        .map_err(err)?;
    tx.commit().map_err(err)?;
    Ok(
        json!({"looked_at": found.len(), "cars": readings.len(), "written": written, "known": known}),
    )
}
pub fn entries(d: &Desktop, text: &str) -> Result<Value> {
    let map = NumberMap::parse(text)?;
    if map.is_empty() {
        return Err("The entry list contains no race numbers".into());
    }
    let dir = d.root.join("entries");
    std::fs::create_dir_all(&dir).map_err(err)?;
    let path = dir.join(format!(
        "entries-{}.csv",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::write(&path, text).map_err(err)?;
    let mut settings = d.settings.lock().unwrap();
    settings.extra.insert("map_path".into(), json!(path));
    settings.save(&d.root.join("settings.json")).map_err(err)?;
    Ok(json!({"path": path, "count": map.len()}))
}
pub fn mapping(settings: &conrod_core::settings::Settings) -> Result<Option<NumberMap>> {
    settings
        .extra
        .get("map_path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(|path| NumberMap::parse(&std::fs::read_to_string(path).map_err(err)?))
        .transpose()
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;
    #[test]
    fn csv_round_trip_and_entries_feed_keywords() {
        let lib = Lib::new("catalog");
        import(lib.d(), "plate,make,model\nABC123,Ford,Falcon\n").unwrap();
        import(lib.d(), "plate,make,team\nABC123,,Test team\n").unwrap();
        let text = export(lib.d()).unwrap();
        assert!(text.as_str().unwrap().contains("Ford,Falcon"));
        let out = entries(lib.d(), "number,driver\n07,Alex\n").unwrap();
        assert_eq!(out["count"], 1);
        let map = mapping(&lib.d().settings.lock().unwrap()).unwrap().unwrap();
        assert!(map.keywords_for("7", "").contains(&"Alex".into()));
        assert!(entries(lib.d(), "driver\nAlex\n").is_err());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::testkit::Lib;
    #[test]
    fn seeding_is_idempotent_and_preserves_a_manual_identity() {
        let lib = Lib::new("seed");
        let frame = lib.frame("1.jpg", Some(1));
        let det = lib.detection(frame, "vehicle", 0.9);
        lib.sql(
            "UPDATE detections SET plate='ABC123',attributes=? WHERE id=?",
            params![json!({"make":"Ford","model":"Falcon"}).to_string(), det],
        );
        assert_eq!(seed(lib.d(), Some(lib.job)).unwrap()["written"], 1);
        assert_eq!(seed(lib.d(), Some(lib.job)).unwrap()["written"], 0);
        import(lib.d(), "plate,model\nABC123,Mustang\n").unwrap();
        seed(lib.d(), Some(lib.job)).unwrap();
        assert!(export(lib.d())
            .unwrap()
            .as_str()
            .unwrap()
            .contains("Mustang"));
    }
    #[test]
    fn index_respects_subfolders_and_write_dry_run_touches_no_photos() {
        let lib = Lib::new("index");
        let photos = lib.root.join("photos");
        std::fs::create_dir_all(photos.join("nested")).unwrap();
        std::fs::write(photos.join("a.jpg"), b"test original").unwrap();
        std::fs::write(photos.join("nested/b.jpg"), b"another original").unwrap();
        let out = lib
            .run(
                "scan",
                json!({"root": photos, "stage":"index", "recursive":false}),
            )
            .unwrap();
        let job = out["jobId"].as_i64().unwrap();
        assert_eq!(
            lib.one::<i64>("SELECT count(*) FROM images WHERE job_id=?", [job]),
            1
        );
        assert!(!lib.d().scanning());
        lib.sql(
            "UPDATE images SET status='done',rating=0.9 WHERE job_id=?",
            [job],
        );
        lib.run("write", json!({"jobId":job,"dryRun":true}))
            .unwrap();
        lib.wait_idle();
        assert_eq!(
            lib.one::<i64>(
                "SELECT count(*) FROM images WHERE written_at IS NOT NULL",
                []
            ),
            0
        );
        assert_eq!(
            std::fs::read(photos.join("a.jpg")).unwrap(),
            b"test original"
        );
        assert!(!photos.join("a.xmp").exists());
        assert_eq!(lib.task("Writing metadata")["state"], "done");
    }
}
