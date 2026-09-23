//! Sharpness training operations used by the desktop front end.
//!
//! Labels are snapshots: the feature vector is copied from the detection when
//! a person rates it, so training remains reproducible after crops are cleared
//! or the detector is run again. Fitting happens after the database mutex is
//! released because ridge fitting is deliberately independent of SQLite.

use crate::desktop::{Desktop, Result};
use conrod_core::ridge::{self, SharpModel};
use conrod_store::SharpnessLabelRow;
use conrod_vision::sharpness;
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

const FOLDS: usize = 5;
/// `fit_sharp` needs 60 rows. With five folds, each training split must keep
/// at least 60 rows, hence 75 is the honest minimum for validation.
const VALIDATION_MIN_LABELS: usize = ridge::SHARP_MIN_LABELS * FOLDS / (FOLDS - 1);

fn task(d: &Desktop, label: &str) -> conrod_core::tasks::Task {
    d.hub.start(label, 0)
}

fn model_path(d: &Desktop) -> PathBuf {
    d.root.join("models").join("sharpness.json")
}

fn current_model(d: &Desktop) -> Option<SharpModel> {
    let text = fs::read_to_string(model_path(d)).ok()?;
    let model: SharpModel = serde_json::from_str(&text).ok()?;
    (model.version == ridge::FEATURE_VERSION
        && !model.weights.is_empty()
        && model.mean.len() == model.spread.len()
        && model.mean.len() == model.weights.len()
        && model
            .mean
            .iter()
            .chain(&model.spread)
            .chain(&model.weights)
            .all(|v| v.is_finite())
        && model.spread.iter().all(|v| *v > 0.0)
        && model.intercept.is_finite())
    .then_some(model)
}

fn counts(d: &Desktop) -> Result<(usize, usize, [usize; 5])> {
    let db =
        d.db.lock()
            .map_err(|_| "database lock poisoned".to_string())?;
    let mut by_stars = [0; 5];
    let mut rated = 0;
    let mut unsure = 0;
    let mut stmt = db
        .prepare("SELECT stars, count(*) FROM sharpness_labels GROUP BY stars")
        .map_err(|e| e.to_string())?;
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
        let stars: i64 = row.get(0).map_err(|e| e.to_string())?;
        let n: usize = row.get::<_, i64>(1).map_err(|e| e.to_string())? as usize;
        match stars {
            0 => unsure += n,
            1..=5 => {
                rated += n;
                by_stars[stars as usize - 1] = n;
            }
            _ => {}
        }
    }
    Ok((rated, unsure, by_stars))
}

fn status_value(d: &Desktop) -> Result<Value> {
    let (rated, unsure, by_stars) = counts(d)?;
    let model = current_model(d);
    Ok(json!({
        "labels": rated + unsure,
        "rated": rated,
        "unsure": unsure,
        "by_stars": {
            "1": by_stars[0], "2": by_stars[1], "3": by_stars[2],
            "4": by_stars[3], "5": by_stars[4]
        },
        "needed": ridge::SHARP_MIN_LABELS,
        "validation_needed": VALIDATION_MIN_LABELS,
        "active": model.is_some(),
        "model": model.map(|m| json!({"trained_on": m.trained_on, "version": m.version})),
    }))
}

/// Current labels and whether the learned model is active.
pub fn status(d: &Desktop) -> Result<Value> {
    status_value(d)
}

fn detection_id(args: &Value) -> Result<i64> {
    args.get("detectionId")
        .or_else(|| args.get("det"))
        .and_then(Value::as_i64)
        .filter(|id| *id > 0)
        .ok_or_else(|| "Missing detectionId".to_string())
}

/// Store one hand rating. An unusable positive rating is remembered as an
/// unsure label so it is not offered again, matching the Python app.
pub fn label(d: &Desktop, args: &Value) -> Result<Value> {
    let det_id = detection_id(args)?;
    let stars = args.get("stars").and_then(Value::as_i64).unwrap_or(0);
    if !(0..=5).contains(&stars) {
        return Err("Stars must be 0–5".into());
    }
    let pan = args.get("pan").and_then(Value::as_bool).unwrap_or(false);

    let (frame, bbox, features, heur_pan) = {
        let db =
            d.db.lock()
                .map_err(|_| "database lock poisoned".to_string())?;
        let row = db
            .query_row(
                "SELECT i.path, d.x1, d.y1, d.x2, d.y2, d.features, d.panning
                   FROM detections d JOIN images i ON i.id=d.image_id WHERE d.id=?",
                [det_id],
                |row| {
                    let features: Option<String> = row.get(5)?;
                    Ok((
                        row.get::<_, String>(0)?,
                        [
                            row.get::<_, f64>(1)?,
                            row.get::<_, f64>(2)?,
                            row.get::<_, f64>(3)?,
                            row.get::<_, f64>(4)?,
                        ],
                        features,
                        row.get::<_, Option<i64>>(6)?.unwrap_or(0) != 0,
                    ))
                },
            )
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => "no such detection".to_string(),
                _ => e.to_string(),
            })?;
        let parsed = row
            .2
            .and_then(|text| serde_json::from_str::<Vec<f64>>(&text).ok());
        if !row.1.iter().all(|value| value.is_finite()) {
            return Err("detection has invalid bounds".to_string());
        }
        let usable = parsed.as_ref().is_some_and(|v| {
            v.len() == sharpness::FEATURE_NAMES.len() && v.iter().all(|x| x.is_finite())
        });
        (row.0, row.1, parsed.filter(|_| usable), row.3)
    };

    let stored_stars = if stars > 0 && features.is_none() {
        0
    } else {
        stars
    };
    let db =
        d.db.lock()
            .map_err(|_| "database lock poisoned".to_string())?;
    conrod_store::add_sharpness_label(
        &db,
        &frame,
        bbox,
        stored_stars,
        pan,
        heur_pan,
        features.as_deref(),
        ridge::FEATURE_VERSION as i64,
    )
    .map_err(|e| e.to_string())?;
    drop(db);
    Ok(json!({"stored": stars == 0 || features.is_some(), "state": status_value(d)?}))
}

pub fn undo(d: &Desktop) -> Result<Value> {
    let t = task(d, "Undoing last training label");
    let result = {
        let db =
            d.db.lock()
                .map_err(|_| "database lock poisoned".to_string())?;
        conrod_store::undo_sharpness_label(&db).map_err(|e| e.to_string())?;
        drop(db);
        status_value(d)
    };
    t.finish();
    result
}

fn load_training_rows(d: &Desktop) -> Result<Vec<SharpnessLabelRow>> {
    let db =
        d.db.lock()
            .map_err(|_| "database lock poisoned".to_string())?;
    conrod_store::sharpness_labels(&db, ridge::FEATURE_VERSION as i64).map_err(|e| e.to_string())
}

fn valid_training_data(rows: &[SharpnessLabelRow]) -> Option<(Vec<Vec<f64>>, Vec<f64>)> {
    let mut vectors = Vec::with_capacity(rows.len());
    let mut stars = Vec::with_capacity(rows.len());
    for row in rows {
        let vector = row.features.as_ref()?.as_array()?;
        let vector: Vec<f64> = vector.iter().map(Value::as_f64).collect::<Option<_>>()?;
        if vector.len() != sharpness::FEATURE_NAMES.len() || !vector.iter().all(|v| v.is_finite()) {
            return None;
        }
        vectors.push(vector);
        stars.push(row.stars as f64);
    }
    Some((vectors, stars))
}

fn score(predicted: &[f64], actual: &[f64]) -> Value {
    let rounded: Vec<f64> = predicted
        .iter()
        .map(|v| v.round_ties_even().clamp(1.0, 5.0))
        .collect();
    let n = actual.len() as f64;
    let exact = rounded.iter().zip(actual).filter(|(a, b)| a == b).count() as f64 / n;
    let within_one = rounded
        .iter()
        .zip(actual)
        .filter(|(a, b)| (*a - *b).abs() <= 1.0)
        .count() as f64
        / n;
    let mean_error = rounded
        .iter()
        .zip(actual)
        .map(|(a, b)| (*a - *b).abs())
        .sum::<f64>()
        / n;
    json!({"exact": exact, "within_one": within_one, "mean_error": mean_error})
}

/// Five deterministic held-out folds. Keeping this here makes the safety
/// decision testable without opening a database or writing a model file.
pub(super) fn validation(vectors: &[Vec<f64>], stars: &[f64], baseline: &[f64]) -> Option<Value> {
    if vectors.len() < VALIDATION_MIN_LABELS
        || vectors.len() != stars.len()
        || vectors.len() != baseline.len()
    {
        return None;
    }
    let mut predicted = vec![0.0; stars.len()];
    for fold in 0..FOLDS {
        let mut train_vectors = Vec::new();
        let mut train_stars = Vec::new();
        for i in 0..stars.len() {
            if i % FOLDS != fold {
                train_vectors.push(vectors[i].clone());
                train_stars.push(stars[i]);
            }
        }
        let model = ridge::fit_sharp(&train_vectors, &train_stars)?;
        for i in 0..stars.len() {
            if i % FOLDS == fold {
                predicted[i] = ridge::predict_sharp(&model, &vectors[i])?;
            }
        }
    }
    Some(json!({
        "n": stars.len(),
        "model": score(&predicted, stars),
        "measure": score(baseline, stars),
    }))
}

pub fn train(d: &Desktop) -> Result<Value> {
    let t = task(d, "Training sharpness model");
    let result = (|| {
        let rows = load_training_rows(d)?;
        let (vectors, stars) = valid_training_data(&rows)
            .ok_or_else(|| "Training labels contain invalid feature data".to_string())?;
        if vectors.len() < VALIDATION_MIN_LABELS {
            return Err(format!(
                "{VALIDATION_MIN_LABELS} ratings are needed for five-fold validation, and there are {}",
                vectors.len()
            ));
        }
        let baseline: Vec<f64> = vectors
            .iter()
            .map(|v| sharpness::stars_for(v[0]) as f64)
            .collect();
        let validation = validation(&vectors, &stars, &baseline)
            .ok_or_else(|| "Those ratings are too alike or cannot be fitted".to_string())?;
        let model = ridge::fit_sharp(&vectors, &stars)
            .ok_or_else(|| "Those ratings are too alike or cannot be fitted".to_string())?;
        let model_error = validation["model"]["mean_error"]
            .as_f64()
            .unwrap_or(f64::INFINITY);
        let baseline_error = validation["measure"]["mean_error"]
            .as_f64()
            .unwrap_or(f64::INFINITY);
        let better = model_error < baseline_error;
        if better {
            let path = model_path(d);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::write(
                &path,
                serde_json::to_vec(&model).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        }
        let active = current_model(d).is_some();
        Ok(json!({
            "active": active,
            "accepted": better,
            "validation": validation,
            "trained_on": model.trained_on,
            "state": status_value(d)?,
        }))
    })();
    match result {
        Ok(value) => {
            t.finish();
            Ok(value)
        }
        Err(error) => {
            t.fail(error.clone());
            Err(error)
        }
    }
}

pub fn forget(d: &Desktop) -> Result<Value> {
    let t = task(d, "Forgetting sharpness model");
    let path = model_path(d);
    if path.exists() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    t.finish();
    status_value(d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(n: usize) -> (Vec<Vec<f64>>, Vec<f64>, Vec<f64>) {
        let mut vectors = Vec::new();
        let mut stars = Vec::new();
        let mut baseline = Vec::new();
        for i in 0..n {
            let star = (i % 5 + 1) as f64;
            let mut v = vec![0.0; sharpness::FEATURE_NAMES.len()];
            v[0] = star / 5.0;
            v[1] = star;
            vectors.push(v);
            stars.push(star);
            baseline.push(1.0);
        }
        (vectors, stars, baseline)
    }

    #[test]
    fn validation_requires_each_fit_to_have_sixty_rows() {
        let (vectors, stars, baseline) = data(VALIDATION_MIN_LABELS - 1);
        assert!(validation(&vectors, &stars, &baseline).is_none());
    }

    #[test]
    fn validation_is_held_out_and_reports_model_and_baseline() {
        let (vectors, stars, baseline) = data(VALIDATION_MIN_LABELS);
        let report = validation(&vectors, &stars, &baseline).unwrap();
        assert_eq!(report["n"], VALIDATION_MIN_LABELS);
        assert!(
            report["model"]["mean_error"].as_f64().unwrap()
                < report["measure"]["mean_error"].as_f64().unwrap()
        );
    }
}
