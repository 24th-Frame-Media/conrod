//! One-off diagnostic: measure a stored frame's sharpness again and say what a
//! pan is capped at. Not part of the app.
//!
//! cargo run --release -p conrod-engine --example pancheck -- <db> <job> <file-name-part> [strictness]
//!
//! The strictness is the album's own, from `jobs.settings_json`, unless the
//! fourth argument names one.

use conrod_core::settings::Settings;
use conrod_io::raw;
use conrod_vision::imageops::Rgb;
use conrod_vision::sharpness;
use rusqlite::Connection;
use serde_json::{Map, Value};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db_path = args.first().ok_or("db path")?;
    let job: i64 = args.get(1).ok_or("job")?.parse().map_err(|e| format!("{e}"))?;
    let needle = args.get(2).ok_or("file name part")?;
    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    // The album's settings the way a re-score builds them. The global settings
    // file is not reachable from an example, but the blur strictness is the
    // album's own, and defaults plus the stored map answer it.
    let mut settings = Settings::default();
    if let Ok(stored) = db.query_row(
        "SELECT COALESCE(settings_json,'') FROM jobs WHERE id=?1",
        [job],
        |r| r.get::<_, String>(0),
    ) {
        if let Ok(map) = serde_json::from_str::<Map<String, Value>>(&stored) {
            settings = settings.apply(&map);
        }
    }
    let strictness = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| settings.effective_blur());
    let sql = "SELECT i.path,i.rating, i.width, i.height FROM images i WHERE i.job_id=?1 AND i.path LIKE ?2";
    let (path, rating): (String, f64) = db
        .query_row(sql, rusqlite::params![job, format!("%{needle}%")], |r| {
            Ok((r.get(0)?, r.get::<_, Option<f64>>(1)?.unwrap_or(0.0)))
        })
        .map_err(|e| e.to_string())?;
    let box_sql = "SELECT d.x1,d.y1,d.x2,d.y2 FROM detections d JOIN images i ON i.id=d.image_id WHERE i.path=?1 ORDER BY (d.x2-d.x1)*(d.y2-d.y1) DESC LIMIT 1";
    let bbox: Option<[f64; 4]> = db
        .query_row(box_sql, [&path], |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]))
        .ok();
    let frame = raw::read(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    let image = Rgb::decode_jpeg(&frame.preview, 1)
        .map_err(|e| e.to_string())?
        .orient(frame.orientation);
    let grey = image.to_gray();
    let m = sharpness::measure(&grey, bbox, None);
    let n = |v: Option<f64>| v.unwrap_or(f64::NAN);
    // What the scan / re-score stores for a frame with no detected subject: the
    // same `bare_frame` the engine uses, so the numbers cannot drift apart.
    let bare = bbox
        .is_none()
        .then(|| conrod_engine::bare_frame(&grey, &strictness));
    let stored = match &bare {
        Some(bare) => bare.score,
        None => Some(m.score),
    };
    let bare_line = match &bare {
        Some(bare) => format!(
            " · bare score {:.3} · bare whole {:.3}",
            n(bare.score),
            n(bare.whole)
        ),
        None => String::new(),
    };
    // How much of the frame is sharp, and what a pan's frame rating is capped at
    // by it, whatever this frame's own subject is.
    let share = sharpness::frame_sharp_share(&grey);
    let cap = share.map(sharpness::pan_share_cap);
    println!(
        "{needle}: stored rating {rating:.3} · now score {:.3} · stored {:.3} · background {:.3} · panning {} · sharp end {} · share {:.2} · box {bbox:?} · frame share {:.3} · cap {:.3}{bare_line}",
        m.score,
        n(stored),
        m.background,
        m.panning,
        m.sharp_end,
        m.pan_share,
        n(share),
        n(cap),
    );
    Ok(())
}
