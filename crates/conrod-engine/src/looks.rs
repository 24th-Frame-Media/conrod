//! What each frame looks like, kept in `images.look` for the moment splitter.
//!
//! `regroup` asks for the looks of a whole album. A frame whose stored look is
//! missing, of another version, or taken from a different subject box than the
//! one it has now is measured again from its cached thumbnail (decode-bound, so
//! it runs on the rayon pool, as `near_duplicates` hashes thumbnails) and
//! written back, so an album scanned before this existed works without a
//! re-scan, and every later regroup only reads text.
use crate::desktop::{rows, Result};
use conrod_core::bursts::Look;
use conrod_vision::{imageops::Rgb, look};
use rayon::prelude::*;
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::path::Path;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The look of every visible frame of the album that has a readable
/// thumbnail. `boxes` holds each frame's main subject box as fractions of the
/// frame; a frame not in it has no subject. The returned looks carry
/// `vehicle: false`: whether the subject is a vehicle is the caller's to say.
pub(crate) fn load(
    conn: &Connection,
    job: i64,
    boxes: &HashMap<i64, [f64; 4]>,
) -> Result<HashMap<i64, Look>> {
    let stored = rows(
        conn,
        "SELECT id,thumb_path,look FROM images WHERE job_id=? AND status='done' AND COALESCE(copy_of,0)=0 AND thumb_path IS NOT NULL",
        [job],
    )?;
    let mut have: HashMap<i64, look::Stored> = HashMap::new();
    let mut todo: Vec<(i64, String, Option<[f64; 4]>)> = Vec::new();
    for row in &stored {
        let Some(id) = row["id"].as_i64() else {
            continue;
        };
        let bbox = boxes.get(&id).copied();
        match row["look"].as_str().and_then(look::unpack) {
            Some(known) if known.bbox == bbox.map(look::quantise_box) => {
                have.insert(id, known);
            }
            _ => {
                if let Some(path) = row["thumb_path"].as_str() {
                    todo.push((id, path.to_string(), bbox));
                }
            }
        }
    }
    let measured: Vec<(i64, look::Stored)> = todo
        .par_iter()
        .filter_map(|(id, path, bbox)| {
            let image = Rgb::open(Path::new(path)).ok()?;
            Some((*id, look::measure(&image, *bbox)))
        })
        .collect();
    if !measured.is_empty() {
        let tx = conn.unchecked_transaction().map_err(err)?;
        for (id, stored) in &measured {
            tx.execute(
                "UPDATE images SET look=? WHERE id=?",
                params![look::pack(stored), id],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
    }
    have.extend(measured);
    Ok(have
        .into_iter()
        .map(|(id, stored)| {
            (
                id,
                Look {
                    scene: stored.scene,
                    subject: stored.subject,
                    vehicle: false,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;

    /// A thumbnail of grey road with a car-sized block of `car` colour.
    fn thumbnail(lib: &Lib, name: &str, car: [u8; 3]) -> String {
        let (w, h) = (420usize, 280usize);
        let mut data = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let inside = (150..270).contains(&x) && (110..170).contains(&y);
                data.extend(if inside { car } else { [90, 90, 92] });
            }
        }
        let path = lib.root.join(name);
        image::RgbImage::from_raw(w as u32, h as u32, data)
            .unwrap()
            .save(&path)
            .unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_missing_look_is_measured_once_and_kept_until_the_box_moves() {
        let lib = Lib::new("looks-backfill");
        let a = lib.frame("1.jpg", None);
        let thumb = thumbnail(&lib, "thumb-1.jpg", [200, 30, 30]);
        lib.sql(
            "UPDATE images SET thumb_path=? WHERE id=?",
            params![thumb, a],
        );
        let boxes = HashMap::from([(a, [0.357, 0.393, 0.643, 0.607])]);

        let db = lib.db();
        let first = load(&db, lib.job, &boxes).unwrap();
        let stored: String = db
            .query_row("SELECT look FROM images WHERE id=?", [a], |r| r.get(0))
            .unwrap();
        assert!(look::unpack(&stored).is_some(), "written back: {stored}");
        assert!(first[&a].subject.is_some());

        // Nothing to measure now: even a deleted thumbnail does not matter.
        std::fs::remove_file(&thumb).unwrap();
        assert_eq!(load(&db, lib.job, &boxes).unwrap()[&a], first[&a]);

        // A subject that moved is another look: with no thumbnail to read, the
        // frame simply has none, rather than keeping a stale one.
        let moved = HashMap::from([(a, [0.1, 0.1, 0.3, 0.3])]);
        assert!(!load(&db, lib.job, &moved).unwrap().contains_key(&a));
    }
}
