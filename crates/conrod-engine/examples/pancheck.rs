//! One-off diagnostic: measure a stored frame's sharpness again and say what a
//! pan is capped at. Not part of the app.
//!
//! cargo run --release -p conrod-engine --example pancheck -- <db> <job> <file-name-part>

use conrod_io::raw;
use conrod_vision::imageops::Rgb;
use conrod_vision::sharpness;
use rusqlite::Connection;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db_path = args.first().ok_or("db path")?;
    let job: i64 = args.get(1).ok_or("job")?.parse().map_err(|e| format!("{e}"))?;
    let needle = args.get(2).ok_or("file name part")?;
    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
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
    // What the scan / re-score stores for a frame with no detected subject: a pan
    // keeps its capped score, everything else may take its in-focus area.
    let stored = if bbox.is_none() {
        sharpness::subjectless_score(&m, sharpness::focal_score(&grey))
    } else {
        Some(m.score)
    };
    println!(
        "{needle}: stored rating {rating:.3} · now score {:.3} · stored {:.3} · background {:.3} · panning {} · sharp end {} · share {:.2} · box {bbox:?}",
        m.score,
        stored.unwrap_or(f64::NAN),
        m.background,
        m.panning,
        m.sharp_end,
        m.pan_share
    );
    Ok(())
}
