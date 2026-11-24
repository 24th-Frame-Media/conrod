//! Seed the work units from capture time before the cull lane reads a pixel.
//!
//! The pass reads each new frame's EXIF header, fills `taken_at`/`camera` (and
//! the lens, ISO and file size the stats card would otherwise backfill later),
//! then cuts the album into units at [`bursts::MERGE_GAP`] with
//! [`bursts::cut_units`]. A final stack can never cross a unit, so the units are
//! a safe visibility and resume boundary. Frames are hidden until the scan's
//! last stage finishes; see [`reveal_units`].

use crate::desktop::{rows, Desktop};
use crate::lock;
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/// Where a seeded frame's provisional `burst_key` / `time_burst` start: above
/// the moment keys a regroup writes (dense from 1) and the keep-set keys
/// (`desktop::SET_KEY_BASE` plus a frame index).
pub(crate) const SEED_KEY_BASE: i64 = 2_000_000;

/// Read the headers of the album's frames and cut its units, stopping early
/// between files when `stop` is raised. Best-effort per file: an unreadable or
/// moved header is skipped, never a failed scan.
pub(crate) fn seed_cancellable(
    d: &Desktop,
    job: i64,
    stop: Option<&AtomicBool>,
) -> Result<usize, String> {
    let todo = rows(
        &lock(&d.reader),
        "SELECT id,path,camera,taken_at,lens,iso,file_size,rating_in_file FROM images
          WHERE job_id=? AND COALESCE(copy_of,0)=0
            AND (camera IS NULL OR taken_at IS NULL OR lens IS NULL OR iso IS NULL OR file_size IS NULL)
          ORDER BY id",
        [job],
    )?;
    if !todo.is_empty() {
        // `import_existing_ratings` (on by default): a star already in the file
        // is read into `rating_in_file` and counts as a hand star in the pick.
        let import = crate::operations::settings(d, job).is_ok_and(|s| s.import_existing_ratings);
        read_headers(d, &todo, stop, import)?;
    }
    if stop.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
        return Ok(0);
    }
    assign_units(&lock(&d.db), job)
}

/// The EXIF-only part, in a small disk-bound pool: each row writes only the
/// columns it actually filled, so a re-seed costs no more than a header read.
fn read_headers(
    d: &Desktop,
    todo: &[serde_json::Value],
    stop: Option<&AtomicBool>,
    import_ratings: bool,
) -> Result<(), String> {
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get() * 4 / 5)
        .clamp(2, 16)
        .min(todo.len().max(1));
    let next = AtomicUsize::new(0);
    let updates = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            let (todo, next, updates) = (todo, &next, &updates);
            scope.spawn(move || {
                let mut mine = Vec::new();
                loop {
                    if stop.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
                        break;
                    }
                    let n = next.fetch_add(1, Ordering::Relaxed);
                    let Some(row) = todo.get(n) else { break };
                    let Some(id) = row["id"].as_i64() else {
                        continue;
                    };
                    let Some(path) = row["path"].as_str().map(Path::new) else {
                        continue;
                    };
                    let Ok(meta) = std::fs::metadata(path) else {
                        continue;
                    };
                    let Ok(exposure) = conrod_io::raw::exposure_meta(path) else {
                        continue;
                    };
                    let camera = row["camera"].is_null().then_some(exposure.camera).flatten();
                    let taken = row["taken_at"]
                        .is_null()
                        .then_some(exposure.taken)
                        .flatten();
                    let lens = row["lens"].is_null().then_some(exposure.lens).flatten();
                    let iso = row["iso"].is_null().then_some(exposure.iso).flatten();
                    let size = row["file_size"].is_null().then_some(meta.len() as i64);
                    // The star a RAW or its sidecar already carries, when the
                    // album asks for it: the photographer's own word.
                    let rating = if import_ratings && row["rating_in_file"].is_null() {
                        conrod_io::raw::file_rating(path)
                    } else {
                        None
                    };
                    if camera.is_some()
                        || taken.is_some()
                        || lens.is_some()
                        || iso.is_some()
                        || size.is_some()
                        || rating.is_some()
                    {
                        mine.push((id, camera, taken, lens, iso, size, rating));
                    }
                }
                lock(updates).extend(mine);
            });
        }
    });
    let updates = updates.into_inner().unwrap_or_default();
    if !updates.is_empty() {
        // The worker pool has joined; write every filled column in one
        // transaction.
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(crate::err)?;
        for (id, camera, taken, lens, iso, size, rating) in updates {
            tx.execute(
                "UPDATE images SET camera=COALESCE(camera,?),taken_at=COALESCE(taken_at,?),
                    lens=COALESCE(lens,?),iso=COALESCE(iso,?),file_size=COALESCE(file_size,?),
                    rating_in_file=COALESCE(rating_in_file,?)
                  WHERE id=?",
                params![camera, taken, lens, iso, size, rating, id],
            )
            .map_err(crate::err)?;
        }
        tx.commit().map_err(crate::err)?;
    }
    Ok(())
}

/// Cut and store the units for every frame of the album, hiding them until the
/// scan's last stage. Returns the unit count. Pure enough to test without files:
/// it uses the `taken_at`/`camera` columns the header pass has just filled.
pub(crate) fn assign_units(db: &Connection, job: i64) -> Result<usize, String> {
    let rows = rows(
        db,
        "SELECT id,path,camera,taken_at FROM images
          WHERE job_id=? AND COALESCE(copy_of,0)=0 ORDER BY path",
        [job],
    )?;
    let mut frames: Vec<conrod_core::bursts::Frame> = rows
        .iter()
        .map(|row| conrod_core::bursts::Frame {
            path: row["path"].as_str().unwrap_or_default().into(),
            camera: row["camera"].as_str().unwrap_or_default().into(),
            taken: row["taken_at"].as_f64(),
            burst: 0,
            unit: 0,
        })
        .collect();
    let count = conrod_core::bursts::cut_units(&mut frames, conrod_core::bursts::MERGE_GAP);
    let tx = db.unchecked_transaction().map_err(crate::err)?;
    for (frame, row) in frames.iter().zip(&rows) {
        let Some(id) = row["id"].as_i64() else {
            continue;
        };
        tx.execute(
            // The seeded group is also the frame's provisional stack identity
            // (until the regroup refines it with looks and subjects), so a
            // stopped or interrupted album still shows sensible stacks.
            // Already-done frames stay visible and keep a key a regroup gave
            // them when resuming or adding newly-arrived frames to an album.
            //
            // A seeded key lives in its own number space (`SEED_KEY_BASE`),
            // apart from the moment keys a regroup writes, and is re-derived
            // from the unit every time: units are renumbered over all frames
            // whenever files are added, so a key kept from an earlier run
            // could equal another unit's new number and join two unrelated
            // shots into one burst.
            "UPDATE images
                SET unit_id=?1,
                    burst_key=CASE WHEN burst_key IS NULL OR burst_key>=?2 THEN ?2+?1 ELSE burst_key END,
                    time_burst=CASE WHEN time_burst IS NULL OR time_burst>=?2 THEN ?2+?1 ELSE time_burst END,
                    visible=CASE WHEN status='done' THEN 1 ELSE 0 END
              WHERE id=?3",
            params![i64::from(frame.unit), SEED_KEY_BASE, id],
        )
        .map_err(crate::err)?;
    }
    tx.commit().map_err(crate::err)?;
    Ok(count)
}

/// Flip each unit visible in its own transaction: one burst becomes visible
/// only when its last stage has finished. Returns how many units were flipped.
pub(crate) fn reveal_units(d: &Desktop, job: i64) -> Result<usize, String> {
    let units: Vec<i64> = rows(
        &lock(&d.reader),
        "SELECT DISTINCT unit_id FROM images WHERE job_id=? AND unit_id IS NOT NULL ORDER BY unit_id",
        [job],
    )?
    .iter()
    .filter_map(|row| row["unit_id"].as_i64())
    .collect();
    let db = lock(&d.db);
    for unit in &units {
        let tx = db.unchecked_transaction().map_err(crate::err)?;
        tx.execute(
            "UPDATE images SET visible=1 WHERE job_id=? AND unit_id=?",
            params![job, unit],
        )
        .map_err(crate::err)?;
        tx.commit().map_err(crate::err)?;
    }
    Ok(units.len())
}

/// Show every frame again: a stopped, failed or interrupted album must never
/// look empty just because its units did not finish.
pub(crate) fn reveal_all(d: &Desktop, job: i64) -> Result<usize, String> {
    lock(&d.db)
        .execute("UPDATE images SET visible=1 WHERE job_id=?", params![job])
        .map_err(crate::err)
}

#[cfg(test)]
mod tests {
    use crate::testkit::Lib;

    #[test]
    fn seeded_keys_follow_the_units_and_never_meet_a_regrouped_moment_key() {
        let lib = Lib::new("seed-keys");
        let timed = |name: &str, taken: f64| {
            let id = lib.frame(name, None);
            lib.sql(
                "UPDATE images SET camera='cam',taken_at=?,burst_key=NULL,time_burst=NULL WHERE id=?",
                rusqlite::params![taken, id],
            );
            id
        };
        let (a, b) = (timed("1.jpg", 100.0), timed("2.jpg", 200.0));
        super::assign_units(&lib.db(), lib.job).unwrap();
        let key = |id: i64| -> i64 { lib.one("SELECT burst_key FROM images WHERE id=?", [id]) };
        let unit = |id: i64| -> i64 { lib.one("SELECT unit_id FROM images WHERE id=?", [id]) };
        assert_eq!(key(a), super::SEED_KEY_BASE + unit(a));
        // Files added later, shot earlier: every unit is renumbered. A key kept
        // from the first run would now equal another unit's number.
        let early = timed("0.jpg", 10.0);
        // One frame was regrouped meanwhile: its moment key is kept as it is.
        lib.sql("UPDATE images SET burst_key=1,time_burst=1 WHERE id=?", [b]);
        super::assign_units(&lib.db(), lib.job).unwrap();
        assert_eq!(key(early), super::SEED_KEY_BASE + unit(early));
        assert_eq!(
            key(a),
            super::SEED_KEY_BASE + unit(a),
            "a seeded key is re-derived from the new unit"
        );
        assert_eq!(key(b), 1, "a regrouped moment keeps its key");
        let keys = [key(early), key(a), key(b)];
        assert_eq!(
            keys.iter().collect::<std::collections::BTreeSet<_>>().len(),
            3,
            "three different shots, three different keys: {keys:?}"
        );
    }

    #[test]
    fn units_are_cut_from_capture_time_and_revealed_one_burst_at_a_time() {
        let lib = Lib::new("seed-units");
        let (a, b, c) = (
            lib.frame("1.jpg", None),
            lib.frame("2.jpg", None),
            lib.frame("3.jpg", None),
        );
        for (taken, image) in [(100.0, a), (105.0, b), (120.0, c)] {
            lib.sql(
                "UPDATE images SET status='pending',camera='cam',taken_at=? WHERE id=?",
                rusqlite::params![taken, image],
            );
        }
        let count = super::assign_units(&lib.db(), lib.job).unwrap();
        assert_eq!(count, 2, "a 15 s gap starts a second unit");
        let unit = |id: i64| -> i64 { lib.one("SELECT unit_id FROM images WHERE id=?", [id]) };
        assert_eq!(unit(a), unit(b));
        assert_ne!(unit(a), unit(c));
        let hidden: i64 = lib.one(
            "SELECT count(*) FROM images WHERE job_id=? AND visible=0",
            [lib.job],
        );
        assert_eq!(
            hidden, 3,
            "pending frames wait for reveal before the last stage"
        );

        assert_eq!(super::reveal_units(lib.d(), lib.job).unwrap(), 2);
        let shown: i64 = lib.one(
            "SELECT count(*) FROM images WHERE job_id=? AND visible=1",
            [lib.job],
        );
        assert_eq!(shown, 3);

        // Resuming an album with already-done frames must keep those frames
        // visible and preserve their grouped burst_key.
        lib.sql(
            "UPDATE images SET status='done',burst_key=99,visible=1 WHERE id=?",
            [a],
        );
        lib.sql(
            "UPDATE images SET status='pending',burst_key=NULL WHERE id=?",
            [c],
        );
        super::assign_units(&lib.db(), lib.job).unwrap();
        let (vis_a, bk_a): (i64, i64) = lib
            .db()
            .query_row(
                "SELECT visible,burst_key FROM images WHERE id=?",
                [a],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(vis_a, 1, "already-done frames stay visible on resume");
        assert_eq!(bk_a, 99, "already-grouped burst_key is preserved on resume");
        let vis_c: i64 = lib.one("SELECT visible FROM images WHERE id=?", [c]);
        assert_eq!(vis_c, 0, "pending frames start hidden until finished");

        // A stop or a crash shows the frames again instead of an empty album.
        lib.sql("UPDATE images SET visible=0 WHERE job_id=?", [lib.job]);
        super::reveal_all(lib.d(), lib.job).unwrap();
        let shown: i64 = lib.one(
            "SELECT count(*) FROM images WHERE job_id=? AND visible=1",
            [lib.job],
        );
        assert_eq!(shown, 3);
    }
}
