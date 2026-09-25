//! Region labels supplement, rather than replace, Python's vehicle labels.
use crate::commands::{LabelArgs, Region};
use crate::desktop::{rows, Desktop, Result};
use conrod_core::ridge;
use crate::lock;
use conrod_vision::{sharpness, similarity};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::PathBuf;
const REGIONS: [&str; 4] = ["vehicle", "person", "face", "eye"];
fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
pub(crate) fn path(d: &Desktop, r: &str) -> PathBuf {
    d.root.join("models").join(if r == "vehicle" {
        "sharpness.json".into()
    } else {
        format!("sharpness-{r}.json")
    })
}
pub fn prepare(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS native_labels(path TEXT NOT NULL,x1 REAL NOT NULL,y1 REAL NOT NULL,x2 REAL NOT NULL,y2 REAL NOT NULL,region TEXT NOT NULL,stars INTEGER NOT NULL,pan INTEGER NOT NULL,heur_pan INTEGER NOT NULL,features TEXT,version INTEGER NOT NULL,created_at REAL NOT NULL,PRIMARY KEY(path,x1,y1,x2,y2,region)); CREATE TABLE IF NOT EXISTS native_label_undo(id INTEGER PRIMARY KEY AUTOINCREMENT,current_label TEXT NOT NULL,previous_label TEXT);") .map_err(err)?;
    db.execute("INSERT OR IGNORE INTO native_labels SELECT path,x1,y1,x2,y2,'vehicle',stars,pan,COALESCE(heur_pan,0),features,COALESCE(feature_version,0),created_at FROM sharpness_labels",[]).map_err(err)?;
    Ok(())
}
pub fn status(d: &Desktop) -> Result<Value> {
    let data = rows(
        &lock(&d.db),
        "SELECT region,count(*) AS labels,sum(stars>0) AS rated FROM native_labels GROUP BY region",
        [],
    )?;
    let total: i64 = data.iter().filter_map(|v| v["labels"].as_i64()).sum();
    let regions:Vec<_>=REGIONS.iter().map(|r|json!({"region":r,"labels":data.iter().find(|v|v["region"]==*r).and_then(|v|v["labels"].as_i64()).unwrap_or(0),"active":path(d,r).is_file()})).collect();
    Ok(
        json!({"labels":total,"regions":regions,"active":REGIONS.iter().any(|r|path(d,r).is_file()),"validation_needed":75}),
    )
}
fn put(db: &Connection, v: &Value) -> Result<()> {
    db.execute(
        "INSERT OR REPLACE INTO native_labels VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
        params![
            v["path"].as_str(),
            v["x1"].as_f64(),
            v["y1"].as_f64(),
            v["x2"].as_f64(),
            v["y2"].as_f64(),
            v["region"].as_str(),
            v["stars"].as_i64(),
            v["pan"].as_i64(),
            v["heur_pan"].as_i64(),
            v["features"].as_str(),
            v["version"].as_i64(),
            v["created_at"].as_f64()
        ],
    )
    .map_err(err)?;
    if v["region"] == "vehicle" {
        db.execute("INSERT OR REPLACE INTO sharpness_labels(path,x1,y1,x2,y2,stars,pan,heur_pan,features,feature_version,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)",params![v["path"].as_str(),v["x1"].as_f64(),v["y1"].as_f64(),v["x2"].as_f64(),v["y2"].as_f64(),v["stars"].as_i64(),v["pan"].as_i64(),v["heur_pan"].as_i64(),v["features"].as_str(),v["version"].as_i64(),v["created_at"].as_f64()]).map_err(err)?;
    }
    Ok(())
}
pub fn label(d: &Desktop, a: &LabelArgs) -> Result<Value> {
    let id = a.detection_id;
    let stars = Some(a.stars)
        .filter(|n| (0..=5).contains(n))
        .ok_or("Stars must be 0–5")?;
    let task = d.hub.start("Saving subject rating", 0);
    {
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        let mut v=rows(&tx,"SELECT i.path,d.x1,d.y1,d.x2,d.y2,COALESCE(d.region_type,'vehicle') AS region,d.features,COALESCE(d.panning,0) AS heur_pan,unixepoch('now') AS created_at FROM detections d JOIN images i ON i.id=d.image_id WHERE d.id=?",[id])?.pop().ok_or("Detection not found")?;
        if !REGIONS.contains(&v["region"].as_str().unwrap_or_default()) {
            return Err("This subject type cannot be trained".into());
        }
        if stars > 0 {
            let features: Vec<f64> = serde_json::from_str(
                v["features"]
                    .as_str()
                    .ok_or("Re-measure this photo before rating it")?,
            )
            .map_err(err)?;
            if features.len() != sharpness::FEATURE_NAMES.len()
                || features.iter().any(|v| !v.is_finite())
            {
                return Err("Invalid feature snapshot".into());
            }
        }
        let old=rows(&tx,"SELECT * FROM native_labels WHERE path=? AND x1=? AND y1=? AND x2=? AND y2=? AND region=?",params![v["path"].as_str(),v["x1"].as_f64(),v["y1"].as_f64(),v["x2"].as_f64(),v["y2"].as_f64(),v["region"].as_str()])?.pop();
        v["stars"] = json!(stars);
        v["pan"] = json!(i64::from(a.pan));
        v["version"] = json!(ridge::FEATURE_VERSION);
        tx.execute(
            "INSERT INTO native_label_undo(current_label,previous_label) VALUES(?,?)",
            params![v.to_string(), old.map(|v| v.to_string())],
        )
        .map_err(err)?;
        put(&tx, &v)?;
        tx.commit().map_err(err)?;
    }
    task.finish();
    status(d)
}
pub fn undo(d: &Desktop) -> Result<Value> {
    let task = d.hub.start("Undoing subject rating", 0);
    {
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        if let Some(last) = rows(
            &tx,
            "SELECT * FROM native_label_undo ORDER BY id DESC LIMIT 1",
            [],
        )?
        .pop()
        {
            let v: Value =
                serde_json::from_str(last["current_label"].as_str().ok_or("Invalid undo entry")?)
                    .map_err(err)?;
            tx.execute("DELETE FROM native_labels WHERE path=? AND x1=? AND y1=? AND x2=? AND y2=? AND region=?",params![v["path"].as_str(),v["x1"].as_f64(),v["y1"].as_f64(),v["x2"].as_f64(),v["y2"].as_f64(),v["region"].as_str()]).map_err(err)?;
            if v["region"] == "vehicle" {
                tx.execute(
                    "DELETE FROM sharpness_labels WHERE path=? AND x1=? AND y1=? AND x2=? AND y2=?",
                    params![
                        v["path"].as_str(),
                        v["x1"].as_f64(),
                        v["y1"].as_f64(),
                        v["x2"].as_f64(),
                        v["y2"].as_f64()
                    ],
                )
                .map_err(err)?;
            }
            if let Some(old) = last["previous_label"].as_str() {
                put(&tx, &serde_json::from_str::<Value>(old).map_err(err)?)?;
            }
            tx.execute(
                "DELETE FROM native_label_undo WHERE id=?",
                [last["id"].as_i64()],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
    }
    task.finish();
    status(d)
}
pub fn train(d: &Desktop, region: Region) -> Result<Value> {
    let region = region.as_str();
    let task = d.hub.start(format!("Training {region} sharpness"), 0);
    let data = rows(
        &lock(&d.db),
        "SELECT * FROM native_labels WHERE region=? AND stars>0 AND version=? ORDER BY path,x1,y1",
        params![region, ridge::FEATURE_VERSION],
    )?;
    let mut vectors = Vec::new();
    let mut stars = Vec::new();
    for row in &data {
        let v: Vec<f64> = serde_json::from_str(row["features"].as_str().ok_or("Missing features")?)
            .map_err(err)?;
        if v.len() != 18 || v.iter().any(|n| !n.is_finite()) {
            return Err("Invalid training features".into());
        }
        vectors.push(v);
        stars.push(row["stars"].as_f64().ok_or("Invalid rating")?);
    }
    let baseline: Vec<f64> = vectors
        .iter()
        .map(|v| f64::from(sharpness::stars_for(v[0])))
        .collect();
    let validation = crate::training::validation(&vectors, &stars, &baseline).ok_or_else(|| {
        format!(
            "Need at least 75 varied {region} ratings for held-out validation ({} available)",
            stars.len()
        )
    })?;
    let better =
        validation["model"]["mean_error"].as_f64() < validation["measure"]["mean_error"].as_f64();
    if better {
        let model = ridge::fit_sharp(&vectors, &stars).ok_or("Model could not be fitted")?;
        std::fs::create_dir_all(d.root.join("models")).map_err(err)?;
        let target = path(d, region);
        let temp = target.with_extension("json.tmp");
        std::fs::write(&temp, serde_json::to_vec(&model).map_err(err)?).map_err(err)?;
        std::fs::rename(temp, target).map_err(err)?;
    }
    task.finish();
    let mut state = status(d)?;
    state["validation"] = validation;
    state["accepted"] = json!(better);
    state["region"] = json!(region);
    Ok(state)
}
pub fn forget(d: &Desktop, region: Region) -> Result<Value> {
    let r = region.as_str();
    let task = d.hub.start(format!("Forgetting {r} model"), 0);
    let target = path(d, r);
    if target.exists() {
        std::fs::remove_file(target).map_err(err)?;
    }
    task.finish();
    status(d)
}

pub fn taste(d: &Desktop) -> Result<Value> {
    let task = d.hub.start("Learning review preferences", 0);
    let rows=rows(&lock(&d.db),"SELECT embedding,CASE WHEN rejected=1 THEN 1 ELSE stars END AS stars FROM detections WHERE embedding IS NOT NULL AND (stars IS NOT NULL OR (reviewed=1 AND rejected=1))",[])?;
    let pairs: Vec<_> = rows
        .iter()
        .filter_map(|r| {
            Some((
                similarity::unpack(r["embedding"].as_str()?)?
                    .into_iter()
                    .map(f64::from)
                    .collect::<Vec<_>>(),
                r["stars"].as_f64()?.clamp(1.0, 5.0),
            ))
        })
        .collect();
    let vectors: Vec<_> = pairs.iter().map(|p| p.0.clone()).collect();
    let stars: Vec<_> = pairs.iter().map(|p| p.1).collect();
    let model = ridge::fit_taste(&vectors, &stars)
        .ok_or("Need 200 varied review ratings with embeddings to learn preferences")?;
    std::fs::create_dir_all(d.root.join("models")).map_err(err)?;
    std::fs::write(
        d.root.join("models/taste.json"),
        serde_json::to_vec(&model).map_err(err)?,
    )
    .map_err(err)?;
    let db = lock(&d.db);
    for row in crate::desktop::rows(
        &db,
        "SELECT id,embedding FROM detections WHERE embedding IS NOT NULL",
        [],
    )? {
        if let Some(v) = row["embedding"].as_str().and_then(similarity::unpack) {
            let vector: Vec<_> = v.into_iter().map(f64::from).collect();
            db.execute(
                "UPDATE detections SET predicted_stars=? WHERE id=?",
                params![ridge::predict_taste(&model, &vector), row["id"].as_i64()],
            )
            .map_err(err)?;
        }
    }
    task.finish();
    Ok(json!({"trained_on":model.trained_on}))
}
