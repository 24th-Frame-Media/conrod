//! Hand edits to detections, one at a time and in bulk. Ports
//! `POST /api/detections/{id}` and `/api/detections/bulk` of `conrod/server.py`.
//!
//! What a person types is ground truth: a number becomes digits with source
//! `manual` and confidence 1, a plate becomes upper-case letters and digits, a
//! team they typed no longer needs corroborating, sponsors lose blanks and
//! repeats. The attributes are edited as the raw JSON, not through
//! `VehicleAnalysis`, so the keys grouping keeps there (`own_make`,
//! `group_make`, ...) survive an edit.
use crate::commands::{BulkArgs, EditArgs};
use crate::desktop::{Desktop, Result};
use conrod_core::{analysis::VehicleAnalysis, keywords};
use crate::lock;
use conrod_vision::sharpness;
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Map, Value};
use std::collections::HashSet;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The digits of `text`, or `None` when there are none.
fn digits(text: &str) -> Option<String> {
    let kept: String = text.chars().filter(char::is_ascii_digit).collect();
    (!kept.is_empty()).then_some(kept)
}

/// Letters and digits only, upper-cased, or `None` when nothing is left.
fn tidy_plate(text: &str) -> Option<String> {
    let kept: String = text
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect();
    (!kept.is_empty()).then_some(kept)
}

fn trimmed(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

/// The stored attributes as an object; a blank or broken record is an empty one.
pub(crate) fn attributes_of(raw: Option<&str>) -> Map<String, Value> {
    match raw.and_then(|s| serde_json::from_str::<Value>(s).ok()) {
        Some(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

/// A person typed the number: it is certain, whatever read it before.
fn manual_number(attrs: &mut Map<String, Value>, number: &Option<String>) {
    attrs.insert("race_number".into(), json!(number));
    attrs.insert("number_source".into(), json!("manual"));
    attrs.insert("number_conf".into(), json!(1.0));
}

struct Row {
    attributes: Option<String>,
    cls: Option<String>,
    number: Option<String>,
    source: Option<String>,
    conf: Option<f64>,
    plate: Option<String>,
    plate_state: Option<String>,
    rejected: i64,
    bystander: i64,
    stars: Option<i64>,
    predicted: Option<i64>,
    rating: Option<f64>,
    embedding: Option<String>,
    region: Option<String>,
}

pub fn edit_detection(d: &Desktop, a: &EditArgs) -> Result<Value> {
    if let Some(Some(stars)) = a.stars {
        if !(0..=5).contains(&stars) {
            return Err("Stars must be 1-5, 0 to clear the hand rating, or null".into());
        }
    }
    let options = {
        let s = lock(&d.settings);
        keywords::KeywordOptions {
            prefix: s.keyword_prefix.clone(),
            write_plate: s.write_plate_keyword,
        }
    };
    let db = lock(&d.db);
    let row = db
        .query_row(
            "SELECT attributes,cls,number,number_source,number_conf,plate,plate_state,COALESCE(rejected,0),COALESCE(bystander,0),stars,predicted_stars,rating,embedding,region_type FROM detections WHERE id=?",
            [a.detection_id],
            |r| {
                Ok(Row {
                    attributes: r.get(0)?,
                    cls: r.get(1)?,
                    number: r.get(2)?,
                    source: r.get(3)?,
                    conf: r.get(4)?,
                    plate: r.get(5)?,
                    plate_state: r.get(6)?,
                    rejected: r.get(7)?,
                    bystander: r.get(8)?,
                    stars: r.get(9)?,
                    predicted: r.get(10)?,
                    rating: r.get(11)?,
                    embedding: r.get(12)?,
                    region: r.get(13)?,
                })
            },
        )
        .optional()
        .map_err(err)?
        .ok_or("No such detection")?;

    let mut attrs = attributes_of(row.attributes.as_deref());
    let (mut number, mut source, mut conf) = (row.number, row.source, row.conf);
    let (mut plate, mut plate_state) = (row.plate, row.plate_state);
    for (key, value) in a.changes() {
        match key {
            "race_number" => {
                number = value.and_then(digits);
                source = Some("manual".into());
                conf = Some(1.0);
                manual_number(&mut attrs, &number);
            }
            "plate" => {
                plate = value.and_then(tidy_plate);
                attrs.insert("plate".into(), json!(plate));
                let certainty = if plate.is_some() { 1.0 } else { 0.0 };
                attrs.insert("plate_conf".into(), json!(certainty));
            }
            "plate_state" => {
                plate_state = trimmed(value);
                attrs.insert("plate_state".into(), json!(plate_state));
            }
            _ => {
                let text = trimmed(value);
                if key == "team" && text.is_some() {
                    attrs.insert("team_corroborated".into(), json!(true));
                }
                attrs.insert(key.into(), json!(text));
            }
        }
    }
    if let Some(list) = &a.sponsors {
        let mut seen = HashSet::new();
        let mut kept = Vec::new();
        for raw in list.iter().flatten() {
            let text = raw.trim();
            if !text.is_empty() && seen.insert(text.to_uppercase()) {
                kept.push(text.to_owned());
            }
        }
        attrs.insert("sponsors".into(), json!(kept));
    }
    let rejected = a.rejected.map_or(row.rejected, i64::from);
    let bystander = a.bystander.map_or(row.bystander, i64::from);
    let stars = match a.stars {
        None => row.stars,
        Some(None) | Some(Some(0)) => None,
        Some(Some(n)) => Some(n),
    };
    db.execute(
        "UPDATE detections SET number=?,number_source=?,number_conf=?,plate=?,plate_state=?,attributes=?,rejected=?,reviewed=?,stars=?,bystander=? WHERE id=?",
        params![number, source, conf, plate, plate_state, Value::Object(attrs.clone()).to_string(), rejected, a.reviewed, stars, bystander, a.detection_id],
    )
    .map_err(err)?;
    if row.region.as_deref() == Some("face") {
        if let (Some(name), Some(embedding)) = (
            attrs
                .get("person_name")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty()),
            row.embedding.as_deref().filter(|s| !s.is_empty()),
        ) {
            let country = attrs.get("country").and_then(Value::as_str);
            db.execute(
                "INSERT INTO known_people(name,country,embedding,updated_at) VALUES(?,?,?,unixepoch('now')) ON CONFLICT(name) DO UPDATE SET country=COALESCE(excluded.country,known_people.country),embedding=excluded.embedding,updated_at=excluded.updated_at",
                params![name.trim(), country, embedding],
            )
            .map_err(err)?;
        }
    }
    drop(db);

    let mut analysis = VehicleAnalysis::from_map(&attrs);
    if let Some(kind) = row.cls.filter(|c| !c.is_empty()) {
        analysis.kind = kind;
    }
    // The star the card shows: the hand one, else the learned one, else the measured one.
    let shown = stars
        .or(row.predicted)
        .or_else(|| row.rating.map(|r| i64::from(sharpness::stars_for(r))));
    Ok(json!({
        "ok": true,
        "id": a.detection_id,
        "number": number,
        "plate": plate,
        "bystander": bystander != 0,
        "stars": shown,
        "by_hand": stars.is_some(),
        // The entry list is not in the Rust engine yet, so nobody is looked up.
        "who": "",
        "title": analysis.title(),
        "keywords": keywords::for_vehicle(&analysis, &options, None),
        "attributes": attrs,
    }))
}

pub fn bulk_edit(d: &Desktop, a: &BulkArgs) -> Result<Value> {
    let number = a.number.as_deref().map(digits);
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    let mut updated = 0;
    for &id in &a.ids {
        let Some(raw) = tx
            .query_row("SELECT attributes FROM detections WHERE id=?", [id], |r| {
                r.get::<_, Option<String>>(0)
            })
            .optional()
            .map_err(err)?
        else {
            continue;
        };
        updated += 1;
        if let Some(number) = &number {
            let mut attrs = attributes_of(raw.as_deref());
            manual_number(&mut attrs, number);
            tx.execute(
                "UPDATE detections SET number=?,number_source='manual',number_conf=1.0,reviewed=1,attributes=? WHERE id=?",
                params![number, Value::Object(attrs).to_string(), id],
            )
            .map_err(err)?;
        }
        if let Some(rejected) = a.rejected {
            tx.execute(
                "UPDATE detections SET rejected=?,reviewed=1 WHERE id=?",
                params![rejected, id],
            )
            .map_err(err)?;
        }
        if let Some(bystander) = a.bystander {
            tx.execute(
                "UPDATE detections SET bystander=?,reviewed=1 WHERE id=?",
                params![bystander, id],
            )
            .map_err(err)?;
        }
        if let Some(reviewed) = a.reviewed {
            tx.execute(
                "UPDATE detections SET reviewed=? WHERE id=?",
                params![reviewed, id],
            )
            .map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    Ok(json!({"ok": true, "updated": updated}))
}

#[cfg(test)]
mod tests {
    use crate::testkit::Lib;
    use rusqlite::params;
    use serde_json::json;

    fn one_detection(lib: &Lib) -> i64 {
        let image = lib.frame("a.jpg", Some(1));
        lib.detection(image, "vehicle", 0.9)
    }

    #[test]
    fn a_typed_number_and_plate_are_normalised_and_certain() {
        let lib = Lib::new("edit-number");
        let det = one_detection(&lib);
        let out = lib
            .run(
                "edit_detection",
                json!({"detectionId": det, "number": " #4 7a", "plate": "ab-12 cd", "team": " Falcon ", "sponsors": ["Shell", " shell ", "", "Pirelli"]}),
            )
            .unwrap();
        assert_eq!(out["number"], "47");
        assert_eq!(out["plate"], "AB12CD");
        assert_eq!(out["attributes"]["team"], "Falcon");
        assert_eq!(out["attributes"]["team_corroborated"], true);
        assert_eq!(out["attributes"]["sponsors"], json!(["Shell", "Pirelli"]));
        assert_eq!(out["attributes"]["plate_conf"], 1.0);
        assert_eq!(out["attributes"]["number_conf"], 1.0);
        assert_eq!(out["title"], "#47 Car");
        assert!(out["keywords"].as_array().unwrap().len() >= 3);
        let (source, conf, reviewed): (String, f64, i64) = lib
            .db()
            .query_row(
                "SELECT number_source,number_conf,reviewed FROM detections WHERE id=?",
                [det],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((source.as_str(), conf, reviewed), ("manual", 1.0, 1));
        // A number that is all letters clears it, still as the person's word.
        let out = lib
            .run(
                "edit_detection",
                json!({"detectionId": det, "number": "abc"}),
            )
            .unwrap();
        assert!(out["number"].is_null());
    }

    #[test]
    fn editing_one_field_leaves_the_rest_and_the_grouping_keys_alone() {
        let lib = Lib::new("edit-keep");
        let det = one_detection(&lib);
        lib.sql(
            "UPDATE detections SET attributes=?,number='9',number_source='ocr',number_conf=0.5 WHERE id=?",
            params![json!({"make": "Ford", "own_make": "Fod", "group_make": "Ford"}).to_string(), det],
        );
        let out = lib
            .run(
                "edit_detection",
                json!({"detectionId": det, "colour": "red"}),
            )
            .unwrap();
        assert_eq!(out["attributes"]["own_make"], "Fod");
        assert_eq!(out["attributes"]["colour"], "red");
        assert_eq!(out["number"], "9");
        let source: String = lib.one("SELECT number_source FROM detections WHERE id=?", [det]);
        assert_eq!(source, "ocr", "no one typed the number");
        assert!(lib
            .run("edit_detection", json!({"detectionId": det + 100}))
            .unwrap_err()
            .contains("No such detection"));
    }

    #[test]
    fn naming_an_embedded_face_remembers_it_for_later_suggestions() {
        let lib = Lib::new("edit-known-person");
        let image = lib.frame("portrait.jpg", Some(1));
        let face = lib.detection(image, "face", 0.9);
        lib.sql(
            "UPDATE detections SET region_type='face',embedding=? WHERE id=?",
            params!["0.1,0.2,0.3", face],
        );
        lib.run(
            "edit_detection",
            json!({"detectionId": face, "personName": " Alex Smith ", "country": "AU"}),
        )
        .unwrap();
        let (country, embedding): (String, String) = lib
            .db()
            .query_row(
                "SELECT country,embedding FROM known_people WHERE name='Alex Smith'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            (country.as_str(), embedding.as_str()),
            ("AU", "0.1,0.2,0.3")
        );
    }

    #[test]
    fn stars_reject_and_bystander_follow_the_recorded_rules() {
        let lib = Lib::new("edit-stars");
        let det = one_detection(&lib);
        lib.sql("UPDATE detections SET predicted_stars=2 WHERE id=?", [det]);
        let out = lib
            .run(
                "edit_detection",
                json!({"detectionId": det, "stars": 4, "rejected": true}),
            )
            .unwrap();
        assert_eq!(
            (out["stars"].as_i64(), out["by_hand"].as_bool()),
            (Some(4), Some(true))
        );
        let (stars, rejected): (i64, i64) = lib
            .db()
            .query_row(
                "SELECT stars,rejected FROM detections WHERE id=?",
                [det],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((stars, rejected), (4, 1));
        // Left out: nothing changes. Zero: back to the measured or learned star.
        let out = lib
            .run(
                "edit_detection",
                json!({"detectionId": det, "bystander": true}),
            )
            .unwrap();
        assert_eq!(out["stars"], 4);
        assert_eq!(out["bystander"], true);
        let out = lib
            .run(
                "edit_detection",
                json!({"detectionId": det, "stars": 0, "reviewed": false}),
            )
            .unwrap();
        assert_eq!(
            (out["stars"].as_i64(), out["by_hand"].as_bool()),
            (Some(2), Some(false))
        );
        let (stars, reviewed): (Option<i64>, i64) = lib
            .db()
            .query_row(
                "SELECT stars,reviewed FROM detections WHERE id=?",
                [det],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((stars, reviewed), (None, 0));
        assert!(lib
            .run("edit_detection", json!({"detectionId": det, "stars": 9}))
            .is_err());
    }

    #[test]
    fn a_bulk_edit_sets_a_number_and_a_reject_on_every_named_detection() {
        let lib = Lib::new("edit-bulk");
        let image = lib.frame("a.jpg", Some(1));
        let (a, b, c) = (
            lib.detection(image, "vehicle", 0.9),
            lib.detection(image, "vehicle", 0.8),
            lib.detection(image, "vehicle", 0.7),
        );
        lib.sql(
            "UPDATE detections SET attributes=? WHERE id=?",
            params![json!({"make": "Kia"}).to_string(), a],
        );
        let out = lib
            .run(
                "bulk_edit",
                json!({"ids": [a, b, 9999], "number": "no. 12", "rejected": true}),
            )
            .unwrap();
        assert_eq!(out["updated"], 2, "only the detections that exist count");
        let (number, source, rejected, reviewed, attrs): (String, String, i64, i64, String) = lib
            .db()
            .query_row(
                "SELECT number,number_source,rejected,reviewed,attributes FROM detections WHERE id=?",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .unwrap();
        assert_eq!(
            (number.as_str(), source.as_str(), rejected, reviewed),
            ("12", "manual", 1, 1)
        );
        let attrs: serde_json::Value = serde_json::from_str(&attrs).unwrap();
        assert_eq!(
            (attrs["make"].as_str(), attrs["race_number"].as_str()),
            (Some("Kia"), Some("12"))
        );
        let untouched: i64 = lib.one("SELECT reviewed+rejected FROM detections WHERE id=?", [c]);
        assert_eq!(untouched, 0);
        // No ids is a no-op; a number with no digits clears.
        assert_eq!(
            lib.run("bulk_edit", json!({"ids": []})).unwrap()["updated"],
            0
        );
        lib.run("bulk_edit", json!({"ids": [a], "number": "-"}))
            .unwrap();
        let cleared: Option<String> = lib.one("SELECT number FROM detections WHERE id=?", [a]);
        assert_eq!(cleared, None);
    }
}
