//! One-off diagnostic: run the scan's own `cull_frame` over the album-109 frames
//! the photographer complained about, and print what the focus area decided.
//! Reads the database READ-ONLY and the RAW originals (never writes either).
//!
//! cargo run --release -p conrod-engine --example afcheck -- <db> <job> [from] [to]
//!
//! `from`/`to` are the trailing frame numbers (default 197..205); the frames are
//! matched by their file name `20261006_B1000_<n>.CR3`. Nothing is stored: the
//! run is exactly what a scan of these frames would produce, printed.

use conrod_core::settings::Settings;
use conrod_vision::detect::{self, Detector};
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db_path = args.first().ok_or("db path")?;
    let job: i64 = args
        .get(1)
        .ok_or("job")?
        .parse()
        .map_err(|e| format!("{e}"))?;
    let from: u32 = args.get(2).map_or(Ok(197), |v| v.parse()).map_err(err)?;
    let to: u32 = args.get(3).map_or(Ok(205), |v| v.parse()).map_err(err)?;

    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(err)?;
    // The album's settings the way a scan builds them.
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
    // The frames of the range, in order.
    let mut stmt = db
        .prepare(
            "SELECT path,taken_at,rating FROM images WHERE job_id=?1 \
             AND path LIKE '%20261006_B1000_0%' ORDER BY path",
        )
        .map_err(err)?;
    let found: Vec<(String, Option<f64>, Option<f64>)> = stmt
        .query_map([job], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .map_err(err)?
        .filter_map(|r| r.ok())
        .filter(|(path, _, _)| {
            std::path::Path::new(path)
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.rsplit('_').next())
                .and_then(|n| n.parse::<u32>().ok())
                .is_some_and(|n| n >= from && n <= to)
        })
        .collect();
    if found.is_empty() {
        return Err("no frames in that range".into());
    }

    let model = conrod_engine::detector_model();
    let detector = Mutex::new(Detector::load(&model, detect::Device::Auto).map_err(err)?);
    let models: HashMap<String, conrod_core::ridge::SharpModel> = HashMap::new();
    let profile = conrod_core::profile::ScanProfile::parse(&settings.scan_profile);
    println!(
        "afcheck: job {job} · {} frames · detector {} · strictness {}",
        found.len(),
        model.display(),
        settings.effective_blur()
    );
    for (path, taken, stored) in &found {
        let name = PathBuf::from(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(path)
            .to_string();
        // What the camera wrote: a single point, or a whole AF grid.
        let record = conrod_io::raw::af_header(std::path::Path::new(path))
            .ok()
            .and_then(|h| h.af);
        let result = conrod_engine::cull_frame(
            std::path::Path::new(path),
            &detector,
            &settings,
            profile,
            &models,
            None,
            None,
            None,
            |_| {},
        )
        .map_err(|e| format!("{name}: {e}"))?;
        println!(
            "\n{name} · taken {:.1} · stored rating {:.3} · NEW rating {:.3} · {:.3} whole · {} stars · af {:?} (mode {:?}, {}/{} points) · af_sharpness {:?}",
            taken.unwrap_or(f64::NAN),
            stored.unwrap_or(f64::NAN),
            result.rating,
            result.whole.unwrap_or(f64::NAN),
            result.stars,
            result.af.map(|a| [
                (a.x1 * 1e3).round() / 1e3,
                (a.y1 * 1e3).round() / 1e3,
                (a.x2 * 1e3).round() / 1e3,
                (a.y2 * 1e3).round() / 1e3
            ]),
            result.af.and_then(|a| a.mode),
            record.map_or(0, |r| r.valid),
            record.map_or(0, |r| r.points),
            result.af_sharpness.map(|s| (s * 1e3).round() / 1e3),
        );
        for s in &result.subjects {
            println!(
                "  {} {:.3} conf {:.2} · box [{:.0},{:.0},{:.0},{:.0}] · rating {:.3} · {} stars · bystander {} · cull {:?}",
                s.region_type,
                s.sharpness,
                s.conf,
                s.bbox[0],
                s.bbox[1],
                s.bbox[2],
                s.bbox[3],
                s.rating,
                s.stars,
                s.bystander,
                s.cull_reason,
            );
        }
    }
    Ok(())
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
