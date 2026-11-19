//! The composition pass: read every visible frame's thumbnail, measure where
//! the subject sits and how the picture is built, and store it beside the frame.
//!
//! The measurement is `conrod_vision::composition`'s: placement of the main
//! subject against the frame (centre, thirds, golden), horizontal and vertical
//! balance, symmetry, background clutter, how many leading lines there are and
//! what share of the frame the subject covers. The subject box comes from the
//! frame's largest detection, as fractions of the upright frame, so the pass
//! and the review grid agree on where the car is; a frame with no detection is
//! measured on the whole picture (`subject: None`).
//!
//! Decoding the thumbnail is the cost, so frames are decoded in parallel and
//! written in one transaction per batch. A thumbnail that is missing or cannot
//! be decoded is skipped, never failed: one bad cache file must not make the
//! album's measurements unusable. Re-running the pass rewrites every column --
//! it is a measure, not a snapshot, and `comp_at` says how fresh it is.

use crate::desktop::{rows, Desktop, Result};
use crate::err;
use crate::library::require_job;
use crate::lock;
use crate::now;
use crate::operations::launch;
use conrod_core::tasks::Task;
use conrod_vision::{composition, imageops::Rgb};
use rayon::prelude::*;
use rusqlite::params;
use serde_json::{json, Value};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Thumbnails decoded between two transactions. Big enough that the write lock
/// is taken once per couple of seconds of work, small enough that a cancelled
/// pass still keeps what it measured.
const BATCH: usize = 200;

/// One frame to measure: its thumbnail, and its main subject as fractions of
/// the frame (`None` when no detection boxed anything).
struct Frame {
    id: i64,
    thumb: String,
    subject: Option<[f64; 4]>,
}

/// What measuring one frame produced.
enum Outcome {
    Measured {
        id: i64,
        value: composition::Composition,
    },
    Skipped,
    Failed,
}

/// The action: measure composition for the whole album, in the background.
pub fn measure_all(d: &Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    launch(d, job, "Measuring composition", move |d, stop, task| {
        let counts = measure(d, job, stop, task)?;
        task.detail(detail(&counts));
        Ok(())
    })
}

/// The pass itself, split from the launch so the counts are a plain value.
/// Returns `measured`, `skipped`, `failed` and `ms_per_frame`.
fn measure(d: &Desktop, job: i64, stop: &AtomicBool, task: &Task) -> Result<Value> {
    let frames = frames(d, job)?;
    let total = frames.len();
    task.progress(0, total as u64);
    let (mut measured, mut skipped, mut failed) = (0usize, 0usize, 0usize);
    let started = Instant::now();
    let mut done = 0usize;
    for chunk in frames.chunks(BATCH) {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // Decode and measure in parallel; the collect keeps the batch's order
        // so a write and a progress tick line up with what was measured.
        let outcomes: Vec<Outcome> = chunk.par_iter().map(one).collect();
        let mut pending = Vec::new();
        for outcome in outcomes {
            match outcome {
                Outcome::Measured { id, value } => {
                    pending.push((id, value));
                    measured += 1;
                }
                Outcome::Skipped => skipped += 1,
                Outcome::Failed => failed += 1,
            }
        }
        if !pending.is_empty() {
            write(d, &pending, now())?;
        }
        done += chunk.len();
        task.progress(done as u64, total as u64);
        task.detail(format!(
            "{measured} measured · {skipped} skipped · {failed} failed · {:.0} ms/frame",
            started.elapsed().as_secs_f64() * 1000.0 / measured.max(1) as f64
        ));
    }
    Ok(json!({
        "measured": measured,
        "skipped": skipped,
        "failed": failed,
        "ms_per_frame": started.elapsed().as_secs_f64() * 1000.0 / measured.max(1) as f64,
    }))
}

fn detail(counts: &Value) -> String {
    format!(
        "{} measured · {} skipped · {} failed · {:.0} ms/frame",
        counts["measured"].as_u64().unwrap_or_default(),
        counts["skipped"].as_u64().unwrap_or_default(),
        counts["failed"].as_u64().unwrap_or_default(),
        counts["ms_per_frame"].as_f64().unwrap_or_default(),
    )
}

/// Every visible done frame with a thumbnail, and its largest detection box.
/// The subquery picks that box by area, whatever its `region_type`; a frame
/// with none keeps NULLs and is measured whole.
fn frames(d: &Desktop, job: i64) -> Result<Vec<Frame>> {
    const SQL: &str = "SELECT i.id, i.thumb_path, i.width, i.height, d.x1, d.y1, d.x2, d.y2 \
         FROM images i LEFT JOIN detections d ON d.id = ( \
             SELECT d2.id FROM detections d2 WHERE d2.image_id=i.id \
             ORDER BY (COALESCE(d2.x2,0)-COALESCE(d2.x1,0)) * (COALESCE(d2.y2,0)-COALESCE(d2.y1,0)) DESC \
             LIMIT 1) \
         WHERE i.job_id=? AND i.status='done' AND COALESCE(i.copy_of,0)=0 \
           AND i.thumb_path IS NOT NULL AND i.thumb_path<>'' \
         ORDER BY i.id";
    Ok(rows(&lock(&d.reader), SQL, [job])?
        .iter()
        .filter_map(frame_of)
        .collect())
}

fn frame_of(value: &Value) -> Option<Frame> {
    let id = value["id"].as_i64()?;
    let thumb = value["thumb_path"].as_str()?.to_string();
    let (width, height) = (value["width"].as_f64()?, value["height"].as_f64()?);
    let subject = match (
        value["x1"].as_f64(),
        value["y1"].as_f64(),
        value["x2"].as_f64(),
        value["y2"].as_f64(),
    ) {
        (Some(x1), Some(y1), Some(x2), Some(y2)) if width > 0.0 && height > 0.0 => {
            let (x1, x2) = (x1.min(x2), x1.max(x2));
            let (y1, y2) = (y1.min(y2), y1.max(y2));
            Some([
                (x1 / width).clamp(0.0, 1.0),
                (y1 / height).clamp(0.0, 1.0),
                (x2 / width).clamp(0.0, 1.0),
                (y2 / height).clamp(0.0, 1.0),
            ])
        }
        _ => None,
    };
    Some(Frame { id, thumb, subject })
}

/// Decode one thumbnail and measure it. A missing or unreadable file is
/// skipped; a measurement that panics or comes back non-finite is failed, so a
/// bad frame is counted rather than taking the pass down with it.
fn one(frame: &Frame) -> Outcome {
    let path = Path::new(&frame.thumb);
    if !path.is_file() {
        return Outcome::Skipped;
    }
    let Ok(image) = Rgb::open(path) else {
        return Outcome::Skipped;
    };
    match catch_unwind(AssertUnwindSafe(|| {
        composition::measure(&image, frame.subject)
    })) {
        Ok(value) if finite(&value) => Outcome::Measured {
            id: frame.id,
            value,
        },
        _ => Outcome::Failed,
    }
}

fn finite(c: &composition::Composition) -> bool {
    [
        c.placement,
        c.balance_x,
        c.balance_y,
        c.symmetry,
        c.clutter,
        c.subject_share,
    ]
    .iter()
    .all(|v| v.is_finite())
}

/// Store one batch in a single transaction. Every column is rewritten, so a
/// re-run overwrites an older measurement instead of keeping it.
fn write(d: &Desktop, pending: &[(i64, composition::Composition)], at: f64) -> Result<()> {
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    for (id, c) in pending {
        tx.execute(
            "UPDATE images SET comp_placement=?1, comp_mode=?2, comp_balance_x=?3, \
             comp_balance_y=?4, comp_symmetry=?5, comp_clutter=?6, comp_lines=?7, \
             comp_subject_share=?8, comp_at=?9 WHERE id=?10",
            params![
                c.placement,
                c.mode,
                c.balance_x,
                c.balance_y,
                c.symmetry,
                c.clutter,
                c.lines,
                c.subject_share,
                at,
                id
            ],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::Command;
    use crate::testkit::Lib;
    use std::time::Duration;

    /// A 64x64 JPEG thumbnail: grey road with a bright block where the subject
    /// sits. The pass measures the picture, not the box, so any real image will
    /// do; this one gives the line and symmetry detectors an edge to find.
    fn thumbnail(lib: &Lib, name: &str) -> String {
        let (w, h) = (64u32, 64u32);
        let mut data = Vec::with_capacity((w * h * 3) as usize);
        for y in 0..h {
            for x in 0..w {
                let subject = (10..54).contains(&x) && (18..46).contains(&y);
                data.extend(if subject {
                    [220u8, 210, 60]
                } else {
                    [70, 85, 95]
                });
            }
        }
        let path = lib.root.join(name);
        image::RgbImage::from_raw(w, h, data)
            .unwrap()
            .save(&path)
            .unwrap();
        path.to_string_lossy().into_owned()
    }

    /// Run the command and wait for the background task to end.
    fn run(lib: &Lib) -> Value {
        lib.run("composition", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        lib.task("Measuring composition")
    }

    /// The eight columns plus `comp_at`, all of them non-null.
    type Row = (f64, String, f64, f64, f64, f64, i64, f64, f64);
    fn columns(lib: &Lib, id: i64) -> Row {
        lib.db()
            .query_row(
                "SELECT comp_placement,comp_mode,comp_balance_x,comp_balance_y,comp_symmetry,\
                 comp_clutter,comp_lines,comp_subject_share,comp_at FROM images WHERE id=?",
                [id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ))
                },
            )
            .unwrap()
    }

    #[test]
    fn a_frame_with_a_subject_gets_every_column_and_the_box_share() {
        let lib = Lib::new("composition-subject");
        let frame = lib.frame("1.jpg", None);
        let thumb = thumbnail(&lib, "thumb-1.jpg");
        lib.sql(
            "UPDATE images SET thumb_path=? WHERE id=?",
            params![thumb, frame],
        );
        // The testkit box is 1000,1000..3000,2000 on a 6000x4000 frame.
        lib.detection(frame, "vehicle", 0.9);

        let task = run(&lib);
        assert_eq!(task["state"], "done", "{task}");

        let (placement, mode, bx, by, symmetry, clutter, lines, share, at) = columns(&lib, frame);
        assert!((0.0..=1.0).contains(&placement), "{placement}");
        assert!(
            ["centre", "thirds", "phi"].contains(&mode.as_str()),
            "{mode}"
        );
        assert!((-1.0..=1.0).contains(&bx), "{bx}");
        assert!((-1.0..=1.0).contains(&by), "{by}");
        assert!((0.0..=1.0).contains(&symmetry), "{symmetry}");
        assert!((0.0..=1.0).contains(&clutter), "{clutter}");
        assert!((0..=8).contains(&lines), "{lines}");
        assert!(
            (share - 2000.0 * 1000.0 / (6000.0 * 4000.0)).abs() < 0.01,
            "the subject share is the box's area: {share}"
        );
        assert!(at > 0.0, "{at}");
        assert!(
            task["detail"]
                .as_str()
                .is_some_and(|d| d.starts_with("1 measured · 0 skipped · 0 failed")),
            "{task}"
        );
    }

    #[test]
    fn a_frame_without_a_subject_is_still_measured_for_the_whole_picture() {
        let lib = Lib::new("composition-no-subject");
        let frame = lib.frame("1.jpg", None);
        let thumb = thumbnail(&lib, "thumb-1.jpg");
        lib.sql(
            "UPDATE images SET thumb_path=? WHERE id=?",
            params![thumb, frame],
        );

        let task = run(&lib);
        assert_eq!(task["state"], "done", "{task}");

        let (_, _, _, _, _, clutter, _, share, _) = columns(&lib, frame);
        assert!(share <= f64::EPSILON, "no box means no share: {share}");
        assert!((0.0..=1.0).contains(&clutter), "{clutter}");
    }

    #[test]
    fn a_frame_whose_thumbnail_is_gone_is_skipped_not_failed() {
        let lib = Lib::new("composition-missing");
        let gone = lib.frame("gone.jpg", None);
        lib.sql(
            "UPDATE images SET thumb_path=? WHERE id=?",
            params![lib.root.join("no-such-thumb.jpg").to_string_lossy(), gone],
        );
        let good = lib.frame("1.jpg", None);
        let thumb = thumbnail(&lib, "thumb-1.jpg");
        lib.sql(
            "UPDATE images SET thumb_path=? WHERE id=?",
            params![thumb, good],
        );
        lib.detection(good, "vehicle", 0.9);

        let task = run(&lib);
        assert_eq!(task["state"], "done", "{task}");
        let detail = task["detail"].as_str().unwrap_or_default();
        assert!(
            detail.starts_with("1 measured · 1 skipped · 0 failed"),
            "{detail}"
        );
        assert!(
            lib.one::<Option<f64>>("SELECT comp_placement FROM images WHERE id=?", [gone])
                .is_none(),
            "a missing thumbnail leaves the frame unmeasured"
        );
        assert!(
            lib.one::<Option<f64>>("SELECT comp_placement FROM images WHERE id=?", [good])
                .is_some(),
            "one missing file does not stop the rest"
        );
    }

    #[test]
    fn a_second_run_rewrites_the_same_rows_and_refreshes_comp_at() {
        fn count(lib: &Lib) -> (i64, f64) {
            lib.db()
                .query_row(
                    "SELECT count(*), max(comp_at) FROM images WHERE job_id=? AND comp_placement IS NOT NULL",
                    [lib.job],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .unwrap()
        }

        let lib = Lib::new("composition-again");
        let frame = lib.frame("1.jpg", None);
        let thumb = thumbnail(&lib, "thumb-1.jpg");
        lib.sql(
            "UPDATE images SET thumb_path=? WHERE id=?",
            params![thumb, frame],
        );
        lib.detection(frame, "vehicle", 0.9);

        run(&lib);
        let (rows, first) = count(&lib);
        assert_eq!(rows, 1);
        std::thread::sleep(Duration::from_millis(30));
        run(&lib);
        let (again, second) = count(&lib);
        assert_eq!(again, rows, "a re-run adds no rows");
        assert!(second > first, "comp_at is refreshed: {first} -> {second}");
    }

    #[test]
    fn the_review_payload_carries_the_composition_columns() {
        let lib = Lib::new("composition-review");
        let frame = lib.frame("1.jpg", None);
        lib.sql(
            "UPDATE images SET comp_placement=0.625, comp_mode='thirds', comp_balance_x=0.125, \
             comp_balance_y=-0.25, comp_symmetry=0.5, comp_clutter=0.375, comp_lines=3, \
             comp_subject_share=0.25, comp_at=1700000000.5 WHERE id=?",
            [frame],
        );

        let review = lib.run("review", json!({"jobId": lib.job})).unwrap();
        let shown = &review["frames"][0];
        assert_eq!(shown["id"].as_i64(), Some(frame));
        assert_eq!(shown["comp_placement"].as_f64(), Some(0.625));
        assert_eq!(shown["comp_mode"], "thirds");
        assert_eq!(shown["comp_balance_x"].as_f64(), Some(0.125));
        assert_eq!(shown["comp_balance_y"].as_f64(), Some(-0.25));
        assert_eq!(shown["comp_symmetry"].as_f64(), Some(0.5));
        assert_eq!(shown["comp_clutter"].as_f64(), Some(0.375));
        assert_eq!(shown["comp_lines"].as_i64(), Some(3));
        assert_eq!(shown["comp_subject_share"].as_f64(), Some(0.25));
        assert_eq!(shown["comp_at"].as_f64(), Some(1700000000.5));
    }

    #[test]
    fn the_command_takes_a_positive_job_id() {
        assert!(matches!(
            Command::parse("composition", json!({"jobId": 2})),
            Ok(Command::Composition(_))
        ));
        assert!(Command::parse("composition", json!({})).is_err());
        assert!(Command::parse("composition", json!({"jobId": 0})).is_err());
    }
}
