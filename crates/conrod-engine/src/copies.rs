//! Fold the extra imports of one photo into a single visible frame.
//!
//! A folder that was copied twice under different names (as one of the Bathurst
//! shoots was: `20261008-IMGC2247.CR3`, `20261008_B1000_2-22.CR3` and
//! `IMGC2247.CR3` all at the same size and second) makes every burst look three
//! times as long as it is. The originals are never touched: the extra frames get
//! `copy_of` pointing at the one that is kept, and every part of the app that
//! lists, counts, picks or writes photos ignores them.
//!
//! The identification is deliberately conservative: the same capture time to the
//! millisecond, the same dimensions, the same file length, and the same bytes in
//! the first and last 64 KiB. A frame that merely looks alike is not touched. The
//! whole file is not hashed because an album is over 100 GB.
//!
//! Hand work is carried over at frame level only. A copy's detection-level
//! `reviewed`, `rejected` and `cull_reason` are not moved to the survivor; the
//! survivor choice instead prefers the frame with the most reviewed detections.

use crate::desktop::{rows, Desktop, Result};
use crate::lock;
use conrod_core::tasks::Task;
use rusqlite::params;
use serde_json::{json, Value};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::Hasher;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// How much of each end of a file is hashed. Enough to tell two different frames
/// apart while keeping a whole album's confirmation to a few seconds of I/O.
const SAMPLE: u64 = 64 * 1024;

fn sample_hash(path: &Path) -> Option<(u64, u64)> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let mut hasher = DefaultHasher::new();
    hasher.write_u64(len);
    let mut buf = vec![0u8; SAMPLE.min(len) as usize];
    file.read_exact(&mut buf).ok()?;
    hasher.write(&buf);
    if len > SAMPLE * 2 {
        file.seek(SeekFrom::End(-(SAMPLE as i64))).ok()?;
        let mut tail = vec![0u8; SAMPLE as usize];
        file.read_exact(&mut tail).ok()?;
        hasher.write(&tail);
    }
    Some((len, hasher.finish()))
}

/// A row that may be hidden: what the survivor choice and the hand work need.
struct Frame {
    id: i64,
    path: String,
    stars: i64,
    picked: i64,
    rejected: i64,
    reviewed: i64,
}

fn nothing(photos: usize) -> Value {
    json!({"photos": photos, "groups": 0, "copies": 0})
}

/// Mark every copy of a frame in the album, then regroup and re-pick so bursts
/// and keepers use the survivor only. Returns the counts.
pub(crate) fn collapse(d: &Desktop, job: i64, stop: &AtomicBool, task: &Task) -> Result<Value> {
    let found = rows(
        &lock(&d.reader),
        "SELECT id, path, taken_at, width, height, COALESCE(stars,0) AS stars, COALESCE(picked,0) AS picked, \
         COALESCE(rejected,0) AS rejected, \
         (SELECT count(*) FROM detections d WHERE d.image_id=images.id AND COALESCE(d.reviewed,0)<>0) AS reviewed \
         FROM images WHERE job_id=? AND COALESCE(copy_of,0)=0 ORDER BY id",
        [job],
    )?;
    let frames: Vec<Frame> = found
        .iter()
        .filter_map(|row| {
            Some(Frame {
                id: row["id"].as_i64()?,
                path: row["path"].as_str()?.to_string(),
                stars: row["stars"].as_i64().unwrap_or_default(),
                picked: row["picked"].as_i64().unwrap_or_default(),
                rejected: row["rejected"].as_i64().unwrap_or_default(),
                reviewed: row["reviewed"].as_i64().unwrap_or_default(),
            })
        })
        .collect();

    // Candidates only: frames taken in the same millisecond at the same size.
    // Two different shots of one moment are one or two hundredths of a second
    // apart, so this cannot join distinct frames of a burst.
    let mut buckets: HashMap<(i64, i64, i64), Vec<usize>> = HashMap::new();
    for (i, row) in found.iter().enumerate() {
        let Some(taken) = row["taken_at"].as_f64() else {
            continue;
        };
        let key = (
            (taken * 1000.0).round() as i64,
            row["width"].as_i64().unwrap_or_default(),
            row["height"].as_i64().unwrap_or_default(),
        );
        buckets.entry(key).or_default().push(i);
    }
    // Progress is over the frames that can actually be compared.
    let eligible: u64 = buckets
        .values()
        .filter(|g| g.len() > 1)
        .map(|g| g.len() as u64)
        .sum();
    task.progress(0, eligible);
    task.detail(format!(
        "Comparing {} photos in {} groups",
        eligible,
        buckets.values().filter(|g| g.len() > 1).count()
    ));

    let mut merged = Vec::new(); // (survivor, stars, picked, every member rejected)
    let mut copies = Vec::new(); // (copy, survivor)
    let mut done = 0u64;
    for group in buckets.values().filter(|g| g.len() > 1) {
        if stop.load(Ordering::Relaxed) {
            return Ok(nothing(frames.len()));
        }
        let mut by_content: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
        for &i in group {
            if let Some(key) = sample_hash(Path::new(&frames[i].path)) {
                by_content.entry(key).or_default().push(i);
            }
            done += 1;
        }
        task.progress(done, eligible);
        for same in by_content.values().filter(|g| g.len() > 1) {
            // One frame of the group carries the photographer's decisions: a
            // pick first, then a frame not rejected, then stars, reviews, and
            // the oldest import as the tiebreak.
            let survivor = *same
                .iter()
                .max_by_key(|&&i| {
                    let f = &frames[i];
                    (
                        f.picked,
                        i64::from(f.rejected == 0),
                        f.stars,
                        f.reviewed,
                        -f.id,
                    )
                })
                .expect("a non-empty group");
            let stars = same
                .iter()
                .map(|&i| frames[i].stars)
                .max()
                .unwrap_or_default();
            let picked = same
                .iter()
                .map(|&i| frames[i].picked)
                .max()
                .unwrap_or_default();
            let all_rejected = same.iter().all(|&i| frames[i].rejected != 0);
            merged.push((frames[survivor].id, stars, picked, all_rejected));
            for &i in same.iter() {
                if i != survivor {
                    copies.push((frames[i].id, frames[survivor].id));
                }
            }
        }
    }
    if stop.load(Ordering::Relaxed) {
        return Ok(nothing(frames.len()));
    }

    {
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
        for (id, stars, picked, all_rejected) in &merged {
            // A pick and a reject are mutually exclusive; a group with a pick
            // was not thrown away.
            let rejected = if *picked == 1 {
                0
            } else {
                i64::from(*all_rejected)
            };
            tx.execute(
                "UPDATE images SET stars=CASE WHEN ?1>0 THEN ?1 ELSE stars END, picked=?2, rejected=?3 WHERE id=?4",
                params![stars, picked, rejected, id],
            )
            .map_err(|e| e.to_string())?;
            // The keeper pick reads the detection's star, not the frame's.
            if *stars > 0 {
                tx.execute(
                    "UPDATE detections SET stars=?1 WHERE image_id=?2 AND stars IS NULL",
                    params![stars, id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
        for (copy, survivor) in &copies {
            tx.execute(
                "UPDATE images SET copy_of=?, burst_pick=NULL, picked=0 WHERE id=?",
                params![survivor, copy],
            )
            .map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE detections SET burst_pick=NULL WHERE image_id=?",
                [copy],
            )
            .map_err(|e| e.to_string())?;
            // Nothing may point at a row that is now hidden.
            tx.execute(
                "UPDATE images SET copy_of=?1 WHERE copy_of=?2",
                params![survivor, copy],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())?;
    }
    // Bursts and keepers are built from the frames that remain visible.
    if !copies.is_empty() {
        d.regroup(job)?;
    }
    task.detail(format!(
        "{} copies of {} photos hidden",
        copies.len(),
        merged.len()
    ));
    Ok(json!({
        "photos": frames.len(),
        "groups": merged.len(),
        "copies": copies.len(),
    }))
}

/// Show every hidden copy again, then rebuild the bursts, moments and keepers
/// from the whole album.
pub(crate) fn restore(d: &Desktop, job: i64, task: &Task) -> Result<Value> {
    let shown = lock(&d.db)
        .execute(
            "UPDATE images SET copy_of=NULL WHERE job_id=? AND copy_of IS NOT NULL",
            [job],
        )
        .map_err(|e| e.to_string())?;
    d.regroup(job)?;
    task.detail(format!("{shown} copies shown again"));
    Ok(json!({"shown": shown}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;
    use std::path::PathBuf;

    fn photos(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("conrod-copies-{tag}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn write(dir: &Path, name: &str, body: &[u8]) -> String {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path.to_string_lossy().to_string()
    }

    /// A done 6000x4000 frame of this album, at `taken` (or with no capture time).
    fn shot(lib: &Lib, path: String, taken: Option<f64>) -> i64 {
        match taken {
            Some(t) => lib.one(
                "INSERT INTO images(job_id,path,status,width,height,taken_at) VALUES(?1,?2,'done',6000,4000,?3) RETURNING id",
                params![lib.job, path, t],
            ),
            None => lib.one(
                "INSERT INTO images(job_id,path,status,width,height,taken_at) VALUES(?1,?2,'done',6000,4000,NULL) RETURNING id",
                params![lib.job, path],
            ),
        }
    }

    fn run_collapse(lib: &Lib) {
        lib.run("collapse_copies", json!({"jobId": lib.job}))
            .unwrap();
        lib.wait_idle();
    }

    /// (hidden id, survivor id) for every hidden copy of the album.
    fn hidden(lib: &Lib) -> Vec<(i64, i64)> {
        let db = lib.db();
        let mut stmt = db
            .prepare(
                "SELECT id, copy_of FROM images WHERE job_id=? AND copy_of IS NOT NULL ORDER BY id",
            )
            .unwrap();
        let found = stmt
            .query_map([lib.job], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        found
    }

    #[test]
    fn copies_are_hidden_and_a_different_shot_in_the_same_instant_stays() {
        let lib = Lib::new("copies-identical");
        let dir = photos("identical");
        let body = vec![7u8; 200 * 1024];
        let ids: Vec<i64> = ["a.CR3", "b.CR3", "c.CR3"]
            .iter()
            .map(|name| shot(&lib, write(&dir, name, &body), Some(1791446678.54)))
            .collect();
        // A different shot in the same millisecond: same size, different bytes.
        let other = shot(
            &lib,
            write(&dir, "d.CR3", &vec![9u8; 200 * 1024]),
            Some(1791446678.54),
        );

        run_collapse(&lib);

        let found = hidden(&lib);
        assert_eq!(found.len(), 2, "two copies hidden: {found:?}");
        assert!(
            found.iter().all(|(_, survivor)| *survivor == ids[0]),
            "the copies point at the oldest frame: {found:?}"
        );
        assert_eq!(
            lib.one::<Option<i64>>("SELECT copy_of FROM images WHERE id=?", [other]),
            None,
            "the different shot is untouched"
        );
    }

    #[test]
    fn hand_work_on_a_hidden_copy_moves_to_the_survivor() {
        let lib = Lib::new("copies-hand-work");
        let dir = photos("hand-work");
        let body = vec![3u8; 200 * 1024];
        let ids: Vec<i64> = ["a.CR3", "b.CR3", "c.CR3"]
            .iter()
            .map(|name| shot(&lib, write(&dir, name, &body), Some(1791446678.54)))
            .collect();
        // The star and the pick sit on different rows, neither the oldest.
        lib.sql("UPDATE images SET stars=4 WHERE id=?", [ids[1]]);
        lib.sql("UPDATE images SET picked=1 WHERE id=?", [ids[2]]);
        let detection = lib.detection(ids[2], "vehicle", 0.9);

        run_collapse(&lib);

        assert_eq!(
            lib.one::<i64>("SELECT stars FROM images WHERE id=?", [ids[2]]),
            4,
            "the group's star moves to the picked survivor"
        );
        assert_eq!(
            lib.one::<i64>("SELECT picked FROM images WHERE id=?", [ids[2]]),
            1
        );
        assert_eq!(
            lib.one::<Option<i64>>("SELECT stars FROM detections WHERE id=?", [detection]),
            Some(4),
            "the keeper pick reads the detection's star"
        );
        assert_eq!(hidden(&lib), vec![(ids[0], ids[2]), (ids[1], ids[2])]);
    }

    #[test]
    fn a_shot_is_rejected_only_when_every_copy_was() {
        let lib = Lib::new("copies-rejected");
        let dir = photos("rejected");
        let all_gone = vec![1u8; 200 * 1024];
        let one_gone = vec![2u8; 200 * 1024];
        let mut first = Vec::new();
        for name in ["a1.CR3", "a2.CR3", "a3.CR3"] {
            let id = shot(&lib, write(&dir, name, &all_gone), Some(1791446678.54));
            lib.sql("UPDATE images SET rejected=1 WHERE id=?", [id]);
            first.push(id);
        }
        let mut second = Vec::new();
        for name in ["b1.CR3", "b2.CR3", "b3.CR3"] {
            second.push(shot(
                &lib,
                write(&dir, name, &one_gone),
                Some(1791446679.54),
            ));
        }
        lib.sql("UPDATE images SET rejected=1 WHERE id=?", [second[0]]);

        run_collapse(&lib);

        assert_eq!(
            lib.one::<i64>("SELECT rejected FROM images WHERE id=?", [first[0]]),
            1,
            "every copy was rejected, so the survivor is too"
        );
        assert_eq!(
            lib.one::<i64>("SELECT rejected FROM images WHERE id=?", [second[1]]),
            0,
            "one live copy keeps the shot"
        );
        assert_eq!(
            lib.one::<Option<i64>>("SELECT copy_of FROM images WHERE id=?", [second[1]]),
            None,
            "the survivor is the frame that was not rejected"
        );
        assert_eq!(
            lib.one::<Option<i64>>("SELECT copy_of FROM images WHERE id=?", [second[0]]),
            Some(second[1])
        );
    }

    #[test]
    fn collapsing_twice_changes_nothing() {
        let lib = Lib::new("copies-twice");
        let dir = photos("twice");
        let body = vec![5u8; 200 * 1024];
        for name in ["a.CR3", "b.CR3", "c.CR3"] {
            shot(&lib, write(&dir, name, &body), Some(1791446678.54));
        }

        run_collapse(&lib);
        let once = hidden(&lib);
        run_collapse(&lib);
        let twice = hidden(&lib);

        assert_eq!(once.len(), 2);
        assert_eq!(twice, once, "a second run hides nothing more");
    }

    #[test]
    fn a_missing_file_is_skipped() {
        let lib = Lib::new("copies-missing");
        let dir = photos("missing");
        let body = vec![6u8; 200 * 1024];
        let first = shot(&lib, write(&dir, "a.CR3", &body), Some(1791446678.54));
        let missing = shot(
            &lib,
            dir.join("gone.CR3").to_string_lossy().to_string(),
            Some(1791446678.54),
        );
        let last = shot(&lib, write(&dir, "c.CR3", &body), Some(1791446678.54));

        run_collapse(&lib);

        assert_eq!(hidden(&lib), vec![(last, first)]);
        assert_eq!(
            lib.one::<Option<i64>>("SELECT copy_of FROM images WHERE id=?", [missing]),
            None,
            "an unreadable file is never hidden"
        );
    }

    #[test]
    fn frames_with_no_capture_time_are_left_alone() {
        let lib = Lib::new("copies-no-time");
        let dir = photos("no-time");
        let body = vec![4u8; 200 * 1024];
        for name in ["a.CR3", "b.CR3"] {
            shot(&lib, write(&dir, name, &body), None);
        }

        run_collapse(&lib);

        assert!(hidden(&lib).is_empty());
    }

    #[test]
    fn nothing_points_at_a_hidden_row() {
        let lib = Lib::new("copies-chain");
        let dir = photos("chain");
        let body = vec![8u8; 200 * 1024];
        let a = shot(&lib, write(&dir, "a.CR3", &body), Some(1791446678.54));
        let b = shot(&lib, write(&dir, "b.CR3", &body), Some(1791446678.54));
        run_collapse(&lib);
        assert_eq!(hidden(&lib), vec![(b, a)]);

        // A copy imported later, picked by hand, becomes the survivor and takes
        // over the chain the first run left.
        let c = shot(&lib, write(&dir, "c.CR3", &body), Some(1791446678.54));
        lib.sql("UPDATE images SET picked=1 WHERE id=?", [c]);
        run_collapse(&lib);

        assert_eq!(
            lib.one::<Option<i64>>("SELECT copy_of FROM images WHERE id=?", [a]),
            Some(c)
        );
        assert_eq!(
            lib.one::<Option<i64>>("SELECT copy_of FROM images WHERE id=?", [b]),
            Some(c),
            "a row hidden before points at the new survivor"
        );
    }

    #[test]
    fn restore_copies_shows_them_again() {
        let lib = Lib::new("copies-restore");
        let dir = photos("restore");
        let body = vec![2u8; 200 * 1024];
        for name in ["a.CR3", "b.CR3", "c.CR3"] {
            shot(&lib, write(&dir, name, &body), Some(1791446678.54));
        }
        run_collapse(&lib);
        assert_eq!(hidden(&lib).len(), 2);

        lib.run("restore_copies", json!({"jobId": lib.job}))
            .unwrap();
        lib.wait_idle();

        assert!(hidden(&lib).is_empty());
        assert_eq!(lib.task("Restoring copies")["state"], "done");
        let detail = lib.task("Restoring copies")["detail"].clone();
        assert_eq!(detail.as_str(), Some("2 copies shown again"));
    }
}
