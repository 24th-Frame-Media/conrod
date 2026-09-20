//! Passes over a whole album that work from what is already stored and read no
//! photograph: which frame of each pass is the keeper (`pick_of_pass`), and which
//! vehicles are one car (`consolidate`, behind the Group action). Ports
//! `pipeline.pick_of_pass`, `pipeline.embed_missing` and `grouping.consolidate`.
use crate::desktop::{rows, Desktop, Result};
use crate::edits::attributes_of;
use crate::library::require_job;
use crate::operations::{launch, settings};
use conrod_core::{
    grouping, models,
    profile::{ScanProfile, Subject},
    tasks::Task,
};
use conrod_vision::{detect::Device, imageops::Rgb, similarity};
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The `region_type` a profile's subject kind is stored under.
pub(crate) fn region_name(kind: Subject) -> &'static str {
    match kind {
        Subject::Eye => "eye",
        Subject::Face => "face",
        Subject::Person => "person",
        Subject::Vehicle => "vehicle",
        Subject::WholeFrame => "",
    }
}

/// Mark the one frame of each pass worth keeping.
///
/// The unit is the car in the pass: (burst, group), or the burst alone where
/// grouping has not run. A star given by hand (or already in the file) wins
/// outright; otherwise the measured rating decides, and a tie goes to the
/// earlier frame so a re-run picks the same one. Of a frame's subjects only
/// those of the kind the profile rates the frame by compete, as they do for
/// the frame's own rating.
pub fn pick_of_pass(conn: &Connection, job: i64, profile: ScanProfile) -> Result<Value> {
    let priority: Vec<&str> = profile.priority().iter().map(|k| region_name(*k)).collect();
    let rank = |region: &str| {
        priority
            .iter()
            .position(|p| *p == region)
            .unwrap_or(priority.len())
    };
    let found = rows(
        conn,
        "SELECT d.id, d.rating, d.sharpness, d.group_key, COALESCE(d.stars, i.rating_in_file, 0) AS by_hand, i.id AS image_id, i.burst_key, COALESCE(d.region_type,'vehicle') AS region FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.rejected,0)=0 AND COALESCE(d.bystander,0)=0 AND i.burst_key IS NOT NULL ORDER BY i.id, d.id",
        [job],
    )?;
    let region_of = |r: &Value| rank(r["region"].as_str().unwrap_or("vehicle"));
    let mut primary: HashMap<i64, usize> = HashMap::new();
    for r in &found {
        let best = primary
            .entry(r["image_id"].as_i64().unwrap_or_default())
            .or_insert(usize::MAX);
        *best = (*best).min(region_of(r));
    }
    // (burst, group) -> (star by hand, measured score, detection)
    let mut best: HashMap<(i64, Option<i64>), (i64, f64, i64)> = HashMap::new();
    for r in &found {
        if primary[&r["image_id"].as_i64().unwrap_or_default()] != region_of(r) {
            continue;
        }
        let Some(score) = r["rating"].as_f64().or_else(|| r["sharpness"].as_f64()) else {
            continue;
        };
        let unit = (
            r["burst_key"].as_i64().unwrap_or_default(),
            r["group_key"].as_i64(),
        );
        let by_hand = r["by_hand"].as_i64().unwrap_or(0);
        // Strictly greater: an equal score leaves the earlier frame in place.
        if best
            .get(&unit)
            .is_none_or(|(h, s, _)| by_hand > *h || (by_hand == *h && score > *s))
        {
            best.insert(unit, (by_hand, score, r["id"].as_i64().unwrap_or_default()));
        }
    }
    let picks: Vec<i64> = best.values().map(|b| b.2).collect();
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute(
        "UPDATE detections SET burst_pick=NULL WHERE image_id IN (SELECT id FROM images WHERE job_id=?)",
        [job],
    )
    .map_err(err)?;
    for id in &picks {
        tx.execute("UPDATE detections SET burst_pick=1 WHERE id=?", [id])
            .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    Ok(json!({"passes": best.len(), "picks": picks.len(), "considered": found.len()}))
}

/// The action: run the pick and report it as a task.
pub fn pick_keepers(d: &Desktop, job: i64) -> Result<Value> {
    require_job(d, job)?;
    let profile = ScanProfile::parse(&settings(d, job)?.scan_profile);
    let task = d.hub.start("Picking keepers", 0);
    match pick_of_pass(&d.db.lock().unwrap(), job, profile) {
        Ok(stats) => {
            task.detail(format!(
                "{} keepers from {} frames across {} passes",
                stats["picks"], stats["considered"], stats["passes"]
            ));
            task.finish();
            Ok(stats)
        }
        Err(e) => {
            task.fail(e.clone());
            Err(e)
        }
    }
}

/// The action: sort an album into one pile per car, as a background operation.
pub fn group(d: &Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    launch(d, job, "Grouping vehicles", move |d, stop, task| {
        let looked = embed_missing(d, job, stop, task)?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let (groups, vehicles) = consolidate(d, job, task)?;
        task.detail(format!(
            "{groups} cars from {vehicles} vehicles ({looked} looked at for the first time)"
        ));
        Ok(())
    })
}

/// Give every stored vehicle crop an embedding, for albums identified before
/// there were any. Skips crops that have one, so it is safe to run again.
fn embed_missing(d: &Desktop, job: i64, stop: &AtomicBool, task: &Task) -> Result<usize> {
    let todo = rows(
        &d.reader.lock().unwrap(),
        "SELECT d.id, d.crop_path FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND d.crop_path IS NOT NULL AND (d.embedding IS NULL OR d.embedding='') AND COALESCE(d.region_type,'vehicle')='vehicle' ORDER BY d.id",
        [job],
    )?;
    if todo.is_empty() {
        return Ok(0);
    }
    crate::setup::ensure(&d.hub, stop, crate::setup::SIMILARITY)?;
    let mut embedder =
        similarity::Embedder::load(&models::expected(models::SIMILARITY), Device::Cpu)?;
    let mut done = 0;
    for chunk in todo.chunks(50) {
        let mut found = Vec::new();
        for row in chunk {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let vector = row["crop_path"]
                .as_str()
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|bytes| Rgb::decode_jpeg(&bytes, 1).ok())
                .and_then(|crop| embedder.embed(&crop).ok());
            if let (Some(id), Some(v)) = (row["id"].as_i64(), vector) {
                found.push((id, similarity::pack(&v)));
            }
            done += 1;
            task.progress(done as u64, todo.len() as u64);
        }
        let db = d.db.lock().unwrap();
        let tx = db.unchecked_transaction().map_err(err)?;
        for (id, packed) in found {
            tx.execute(
                "UPDATE detections SET embedding=? WHERE id=?",
                params![packed, id],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        if stop.load(Ordering::Relaxed) {
            break;
        }
    }
    Ok(done)
}

/// The group's own answer if it has one, else what this frame's reader said.
fn agreed_or_own(agreed: &Option<String>, current: &Map<String, Value>, own: &str) -> Value {
    agreed
        .clone()
        .filter(|s| !s.is_empty())
        .map(Value::from)
        .unwrap_or_else(|| current.get(own).cloned().unwrap_or(Value::Null))
}

fn empty(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Null)) || value.and_then(Value::as_str) == Some("")
}

/// Group an album's vehicles by how their crops look and settle on one identity
/// per car, written back beside (never over) what each frame's reader said.
/// Returns (cars, vehicles). Then re-picks the keepers, which are per car.
pub fn consolidate(d: &Desktop, job: i64, task: &Task) -> Result<(usize, usize)> {
    task.detail("Sorting vehicles into cars");
    let found = rows(
        &d.reader.lock().unwrap(),
        "SELECT d.id, d.attributes, d.colour_hex, d.plate, d.embedding, i.id AS image_id, i.burst_key FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND d.rejected=0 AND COALESCE(d.bystander,0)=0 AND COALESCE(d.region_type,'vehicle')='vehicle' ORDER BY i.id, d.id",
        [job],
    )?;
    if found.is_empty() {
        return Ok((0, 0));
    }
    let mut attributes: HashMap<i64, Map<String, Value>> = HashMap::new();
    let mut looks = Vec::new();
    for row in &found {
        let id = row["id"].as_i64().unwrap_or_default();
        let mut parsed = attributes_of(row["attributes"].as_str());
        grouping::use_own_reading(&mut parsed);
        parsed.insert("colour_hex".into(), row["colour_hex"].clone());
        attributes.insert(id, parsed);
        looks.push(grouping::LookRow {
            det_id: id,
            vector: row["embedding"]
                .as_str()
                .and_then(similarity::unpack)
                .map(|v| v.into_iter().map(f64::from).collect()),
            frame_index: row["image_id"].as_i64().unwrap_or_default(),
            burst: row["burst_key"].as_i64(),
            plate: row["plate"].as_str().map(str::to_owned),
        });
    }
    // Python falls back to a cruder shape-and-colour measure when the look model
    // has not seen most of the album; there are no crop signatures here, so the
    // album has to be looked at first.
    let usable = looks.iter().filter(|l| l.vector.is_some()).count();
    if usable < (looks.len() / 2).max(1) {
        return Err(format!(
            "Only {usable} of {} vehicles have been looked at by the grouping model, too few to group. Install the model and group again.",
            looks.len()
        ));
    }
    let assignment = grouping::cluster_by_look(&looks, grouping::SAME_CAR);
    let mut members: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for look in &looks {
        if let Some(key) = assignment.get(&look.det_id) {
            members.entry(*key).or_default().push(look.det_id);
        }
    }

    // (detection, attributes, group key, size, agreement, sampled colour)
    let mut writes = Vec::new();
    for (key, ids) in &members {
        let group: Vec<_> = ids.iter().map(|i| attributes[i].clone()).collect();
        let agreed = grouping::consensus(&group);
        for id in ids {
            let mut current = attributes[id].clone();
            // What this frame's reader said is a measurement and is kept; the
            // group's answer goes alongside and everything downstream prefers it.
            grouping::remember_own_reading(&mut current);
            current.insert("group_make".into(), json!(agreed.make));
            current.insert("group_model".into(), json!(agreed.model));
            let make = agreed_or_own(&agreed.make, &current, "own_make");
            let model = agreed_or_own(&agreed.model, &current, "own_model");
            current.insert("make".into(), make);
            current.insert("model".into(), model);
            if agreed.colour.as_deref().is_some_and(|c| !c.is_empty()) {
                current.insert("colour".into(), json!(agreed.colour));
            }
            if agreed.plate.is_some() && empty(current.get("plate")) {
                current.insert("plate".into(), json!(agreed.plate));
                current.insert("plate_conf".into(), json!(0.0));
            }
            if agreed.race_number.is_some() && empty(current.get("race_number")) {
                current.insert("race_number".into(), json!(agreed.race_number));
            }
            if agreed.team.is_some() && empty(current.get("team")) {
                current.insert("team".into(), json!(agreed.team));
            }
            if !agreed.sponsors.is_empty() {
                current.insert("sponsors".into(), json!(agreed.sponsors));
            }
            if !agreed.livery_text.is_empty() {
                current.insert("livery_text".into(), json!(agreed.livery_text));
            }
            current.insert("group_disputed".into(), json!(agreed.disputed));
            current.insert("group_second_look".into(), json!(agreed.second_look));
            writes.push((
                *id,
                Value::Object(current).to_string(),
                *key,
                agreed.size as i64,
                (agreed.agreement * 1000.0).round() / 1000.0,
                agreed.colour_hex.clone(),
            ));
        }
    }
    let profile = ScanProfile::parse(&settings(d, job)?.scan_profile);
    let db = d.db.lock().unwrap();
    let tx = db.unchecked_transaction().map_err(err)?;
    for (id, attrs, key, size, agreement, hex) in &writes {
        // group_colour_hex, never colour_hex: the per-frame sample is a
        // measurement and a second regroup must not average averages.
        tx.execute(
            "UPDATE detections SET attributes=?,group_key=?,group_size=?,group_agreement=?,group_colour_hex=? WHERE id=?",
            params![attrs, key, size, agreement, hex, id],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    pick_of_pass(&db, job, profile)?;
    Ok((members.len(), looks.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;

    fn picks(lib: &Lib) -> Vec<i64> {
        let db = lib.db();
        let mut stmt = db
            .prepare("SELECT id FROM detections WHERE burst_pick=1 ORDER BY id")
            .unwrap();
        let ids = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        ids
    }

    #[test]
    fn the_sharpest_frame_of_each_pass_is_the_keeper_and_a_hand_star_beats_it() {
        let lib = Lib::new("pick");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        let g1 = lib.frame("4.jpg", Some(2));
        let g2 = lib.frame("5.jpg", Some(2));
        let none = lib.frame("6.jpg", None);
        let (a, b, c) = (
            lib.detection(f1, "vehicle", 0.60),
            lib.detection(f2, "vehicle", 0.80),
            lib.detection(f3, "vehicle", 0.80),
        );
        let (d1, d2) = (
            lib.detection(g1, "vehicle", 0.70),
            lib.detection(g2, "vehicle", 0.55),
        );
        lib.detection(none, "vehicle", 0.99);
        let out = lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(
            (
                out["passes"].as_i64(),
                out["picks"].as_i64(),
                out["considered"].as_i64()
            ),
            (Some(2), Some(2), Some(5)),
            "a frame with no burst has no pass"
        );
        assert_eq!(picks(&lib), vec![b, d1], "a tie goes to the earlier frame");
        let unpicked: Option<i64> = lib.one("SELECT burst_pick FROM detections WHERE id=?", [a]);
        assert_eq!(unpicked, None, "not a keeper is NULL, as Python leaves it");

        // A hand star outranks a better measure; a hand reject leaves the pass.
        lib.sql("UPDATE detections SET stars=3 WHERE id=?", [a]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(picks(&lib), vec![a, d1]);
        lib.sql("UPDATE detections SET rejected=1 WHERE id=?", [a]);
        lib.sql("UPDATE detections SET bystander=1 WHERE id=?", [d1]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(picks(&lib), vec![b, d2]);
        let _ = c;
        assert!(lib.run("pick_keepers", json!({"jobId": 999})).is_err());
        assert!(lib.task("Picking keepers")["detail"]
            .as_str()
            .unwrap()
            .contains("keepers"));
    }

    #[test]
    fn two_cars_in_one_burst_each_keep_a_frame_and_only_the_rated_kind_competes() {
        let lib = Lib::new("pick-groups");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let (car_a1, car_b1) = (
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f1, "vehicle", 0.5),
        );
        let (car_a2, car_b2) = (
            lib.detection(f2, "vehicle", 0.7),
            lib.detection(f2, "vehicle", 0.6),
        );
        // A face in the frame does not compete with the cars under Motorsport.
        let face = lib.detection(f1, "face", 0.99);
        for (det, group) in [(car_a1, 1), (car_a2, 1), (car_b1, 2), (car_b2, 2)] {
            lib.sql(
                "UPDATE detections SET group_key=? WHERE id=?",
                params![group, det],
            );
        }
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(picks(&lib), vec![car_a1, car_b2]);
        assert!(!picks(&lib).contains(&face));
    }

    fn look(lib: &Lib, det: i64, vector: [f32; 3]) {
        lib.sql(
            "UPDATE detections SET embedding=? WHERE id=?",
            params![similarity::pack(&vector), det],
        );
    }

    #[test]
    fn group_sorts_vehicles_into_cars_and_keeps_each_readers_own_answer() {
        let lib = Lib::new("group");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        let (a1, a2) = (
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f2, "vehicle", 0.8),
        );
        let other = lib.detection(f3, "vehicle", 0.7);
        look(&lib, a1, [1.0, 0.0, 0.0]);
        look(&lib, a2, [0.99, 0.14, 0.0]);
        look(&lib, other, [0.0, 1.0, 0.0]);
        lib.sql(
            "UPDATE detections SET attributes=? WHERE id=?",
            params![json!({"make": "Ford", "model": "Focus"}).to_string(), a1],
        );
        lib.sql(
            "UPDATE detections SET attributes=? WHERE id=?",
            params![
                json!({"make": "Ford", "model": "Focus", "team": "Falcon"}).to_string(),
                a2
            ],
        );
        // A hand-rejected vehicle is not grouped at all.
        let gone = lib.detection(f3, "vehicle", 0.4);
        lib.sql("UPDATE detections SET rejected=1 WHERE id=?", [gone]);

        let started = lib.run("group", json!({"jobId": lib.job})).unwrap();
        assert!(started["operation"]
            .as_str()
            .unwrap()
            .starts_with("Grouping vehicles"));
        lib.wait_idle();

        let key = |det: i64| -> Option<i64> {
            lib.one("SELECT group_key FROM detections WHERE id=?", [det])
        };
        assert!(
            key(a1).is_some() && key(a1) == key(a2),
            "one car in two frames"
        );
        assert_ne!(key(other), key(a1));
        assert_eq!(key(gone), None);
        let size: i64 = lib.one("SELECT group_size FROM detections WHERE id=?", [a1]);
        assert_eq!(size, 2);
        let raw: String = lib.one("SELECT attributes FROM detections WHERE id=?", [a1]);
        let attrs: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(attrs["group_make"], "Ford");
        assert_eq!(attrs["own_make"], "Ford");
        assert_eq!(
            attrs["team"], "Falcon",
            "a team one frame saw is shared by the group"
        );
        assert_eq!(lib.task("Grouping vehicles")["state"], "done");
        assert!(lib.task("Grouping vehicles")["detail"]
            .as_str()
            .unwrap()
            .contains("2 cars from 3 vehicles"));
        // Grouping re-picked: one keeper per car.
        assert_eq!(picks(&lib), vec![a1, other]);
    }

    #[test]
    fn group_keeps_apart_crops_alike_only_below_pythons_same_car_threshold() {
        // Job 39 of the real library: at 0.8 Rust merged cars Python (0.90) keeps apart.
        let lib = Lib::new("group-threshold");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        let (a, b, c) = (
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f2, "vehicle", 0.8),
            lib.detection(f3, "vehicle", 0.7),
        );
        look(&lib, a, [1.0, 0.0, 0.0]);
        look(&lib, b, [0.85, 0.526_782_7, 0.0]); // cosine 0.85 with `a`: a different car
        look(&lib, c, [0.95, -0.312_249_9, 0.0]); // cosine 0.95 with `a` (and far from `b`): the same car
        lib.run("group", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        let key = |det: i64| -> Option<i64> {
            lib.one("SELECT group_key FROM detections WHERE id=?", [det])
        };
        assert_eq!(key(a), key(c), "0.95 alike is one car");
        assert_ne!(key(a), key(b), "0.85 alike is two cars at Python's 0.90");
    }

    #[test]
    fn group_says_so_when_the_album_has_not_been_looked_at() {
        let lib = Lib::new("group-blind");
        let f = lib.frame("1.jpg", Some(1));
        lib.detection(f, "vehicle", 0.9);
        lib.detection(f, "vehicle", 0.9);
        lib.run("group", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        let task = lib.task("Grouping vehicles");
        assert_eq!(task["state"], "failed");
        assert!(task["error"].as_str().unwrap().contains("Only 0 of 2"));
        assert!(lib.run("group", json!({"jobId": 4242})).is_err());
    }
}
