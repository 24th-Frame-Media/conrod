//! The taste pass: score every frame's cached preview with the learned head and
//! store it in `images.taste`.
//!
//! This is a *sort*, never a keeper: the experiment behind it ranks the
//! photographer's delivered frame in its burst's top three 63.7 % of the time
//! but is exact only 26.5 % of the time (report in `target/taste/report.md`).
//! Nothing here touches `burst_pick`.
//!
//! The pass is optional twice over: no `taste-head.json` means no head, and the
//! scan's post-pick step silently does nothing. Running the command without a
//! head is not an error either -- it reports `"head": false` and spends no time.

use crate::desktop::{rows, Desktop, Result};
use crate::err;
use crate::library::require_job;
use crate::lock;
use crate::operations::launch;
use conrod_core::{models, tasks::Task};
use conrod_vision::{detect::Device, imageops::Rgb, taste::TasteModel};
use rusqlite::params;
use serde_json::{json, Value};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Both halves on disk: the tower and the head. The tower alone is not enough.
pub fn available() -> bool {
    models::find(models::TASTE_TOWER).is_some() && models::find(models::TASTE_HEAD).is_some()
}

/// The head file alone, which is what the review payload advertises: a head
/// without its tower is a broken install, and taste stays hidden.
pub fn head_available() -> bool {
    models::find(models::TASTE_HEAD).is_some()
}

/// The action: score this album in the background, as an operation.
pub fn taste_rank(d: &Arc<Desktop>, job: i64, force: bool) -> Result<Value> {
    require_job(d, job)?;
    if !available() {
        return Ok(json!({
            "head": false,
            "operation": Value::Null,
            "detail": format!(
                "no taste head: put {} and {} in {}",
                models::TASTE_HEAD,
                models::TASTE_TOWER,
                models::expected(models::TASTE_HEAD)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .display()
            ),
        }));
    }
    launch(d, job, "Scoring taste", move |d, stop, task| {
        let stats = rank(d, job, force, stop, task)?;
        task.detail(format!(
            "{} scored · {} already had one · {} unreadable · {:.0} ms/frame",
            stats.scored, stats.skipped, stats.failed, stats.ms_per_frame
        ));
        Ok(())
    })?;
    Ok(json!({"head": true}))
}

/// The scan's step after the pick. Quiet by design: no head, no task, no error.
pub fn after_scan(d: &Desktop, job: i64) -> Option<Value> {
    if !available() {
        return None;
    }
    let task = d.hub.start("Scoring taste", 0);
    let stop = AtomicBool::new(false);
    match rank(d, job, false, &stop, &task) {
        Ok(stats) => {
            task.detail(format!(
                "{} frames scored, {:.0} ms/frame",
                stats.scored, stats.ms_per_frame
            ));
            task.finish();
            Some(json!({
                "scored": stats.scored,
                "skipped": stats.skipped,
                "failed": stats.failed,
                "msPerFrame": stats.ms_per_frame,
            }))
        }
        Err(e) => {
            // A missing or broken head must never fail the scan that just
            // finished; taste is an extra order, not a stage the album needs.
            task.fail(e);
            None
        }
    }
}

struct Stats {
    scored: usize,
    skipped: usize,
    failed: usize,
    ms_per_frame: f64,
}

/// Score every frame that has no taste yet (or all of them with `force`).
fn rank(d: &Desktop, job: i64, force: bool, stop: &AtomicBool, task: &Task) -> Result<Stats> {
    let tower = models::find(models::TASTE_TOWER)
        .ok_or_else(|| format!("missing model {}", models::TASTE_TOWER))?;
    let head = models::find(models::TASTE_HEAD)
        .ok_or_else(|| format!("missing {}", models::TASTE_HEAD))?;
    let sql = format!(
        "SELECT id, preview_path, thumb_path FROM images WHERE job_id=? AND status='done' AND COALESCE(copy_of,0)=0{} ORDER BY id",
        if force { "" } else { " AND taste IS NULL" }
    );
    let frames = rows(&lock(&d.reader), &sql, [job])?;
    let total = frames.len();
    task.progress(0, total as u64);
    if total == 0 {
        return Ok(Stats {
            scored: 0,
            skipped: 0,
            failed: 0,
            ms_per_frame: 0.0,
        });
    }
    let mut model = TasteModel::load(&tower, &head, Device::Cpu)?;
    let (mut scored, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    let mut pending: Vec<(i64, f64)> = Vec::new();
    let started = Instant::now();
    for (i, row) in frames.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Some(id) = row["id"].as_i64() else {
            continue;
        };
        // The full-size preview when the viewer has cached one, else the
        // scan's 420px thumbnail. Both are normalised to 512px before scoring.
        let path = ["preview_path", "thumb_path"]
            .iter()
            .filter_map(|key| row[*key].as_str())
            .find(|p| !p.is_empty() && Path::new(p).is_file());
        let Some(path) = path else {
            skipped += 1;
            continue;
        };
        let image = std::fs::read(path)
            .ok()
            .and_then(|bytes| Rgb::decode_jpeg(&bytes, 1).ok());
        match image.and_then(|image| model.score(&image)) {
            Some(score) => {
                pending.push((id, f64::from(score)));
                scored += 1;
            }
            None => failed += 1,
        }
        if pending.len() >= 32 {
            write(d, &mut pending)?;
        }
        task.progress(i as u64 + 1, total as u64);
        if i % 64 == 0 {
            task.detail(format!(
                "{scored} scored · {} ms/frame",
                (started.elapsed().as_millis() as f64 / scored.max(1) as f64).round()
            ));
        }
    }
    write(d, &mut pending)?;
    let ms_per_frame = started.elapsed().as_secs_f64() * 1000.0 / scored.max(1) as f64;
    Ok(Stats {
        scored,
        skipped,
        failed,
        ms_per_frame,
    })
}

fn write(d: &Desktop, pending: &mut Vec<(i64, f64)>) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    for (id, score) in pending.drain(..) {
        tx.execute("UPDATE images SET taste=? WHERE id=?", params![score, id])
            .map_err(err)?;
    }
    tx.commit().map_err(err)
}

#[cfg(test)]
mod tests {
    use crate::testkit::Lib;
    use serde_json::json;

    #[test]
    fn the_column_and_the_command_exist_and_a_missing_album_still_errors() {
        let lib = Lib::new("taste");
        let frame = lib.frame("1.jpg", Some(1));
        // The column is added by the migration and starts empty.
        let taste: Option<f64> = lib.one("SELECT taste FROM images WHERE id=?", [frame]);
        assert_eq!(taste, None);
        // Whatever this machine has installed, the command answers instead of
        // crashing: no head here means `head: false` and no operation.
        let out = lib.run("taste_rank", json!({"jobId": lib.job})).unwrap();
        assert!(out["head"].is_boolean(), "{out}");
        assert!(lib.run("taste_rank", json!({"jobId": 999})).is_err());
        assert!(lib.run("taste_rank", json!({})).is_err());
    }

    #[test]
    fn the_review_payload_says_whether_taste_is_installed_and_carries_its_score() {
        let lib = Lib::new("taste-review");
        let frame = lib.frame("1.jpg", Some(1));
        lib.sql("UPDATE images SET taste=0.8125 WHERE id=?", [frame]);
        let review = lib.run("review", json!({"jobId": lib.job})).unwrap();
        assert!(review["tasteHead"].is_boolean(), "{review}");
        let shown = &review["frames"][0];
        assert_eq!(shown["id"].as_i64(), Some(frame));
        assert!((shown["taste"].as_f64().unwrap() - 0.8125).abs() < 1e-9);
    }
}
