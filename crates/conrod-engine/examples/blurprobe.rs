//! One-off diagnostic: what a frame with no detected subject is rated on, and
//! how much real detail it holds anywhere. Not part of the app.
//!
//! cargo run --release -p conrod-engine --example blurprobe -- <db> <job> <file-name-part>...
use conrod_core::settings::Settings;
use conrod_io::raw;
use conrod_vision::edgeblur;
use conrod_vision::imageops::Rgb;
use conrod_vision::sharpness;
use rusqlite::Connection;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db = Connection::open_with_flags(&args[0], rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    let job: i64 = args[1].parse().map_err(|e| format!("{e}"))?;
    let strictness = Settings::default().effective_blur();
    for needle in &args[2..] {
        let (path, rating): (String, f64) = db
            .query_row(
                "SELECT path, COALESCE(rating,0) FROM images WHERE job_id=?1 AND path LIKE ?2",
                rusqlite::params![job, format!("%{needle}%")],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        let frame = raw::read(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
        let image = Rgb::decode_jpeg(&frame.preview, 1)
            .map_err(|e| e.to_string())?
            .orient(frame.orientation);
        let grey = image.to_gray();
        let report = probe(&grey, &strictness);
        println!(
            "{needle} stored {rating:.3} {}x{} {report}",
            grey.width, grey.height
        );
    }
    Ok(())
}

fn probe(grey: &conrod_vision::imageops::Gray, strictness: &str) -> String {
    let whole = edgeblur::measure(grey, None);
    let bare = conrod_engine::bare_frame(grey, strictness);
    let focal = sharpness::focal_score(grey);
    let edge = sharpness::rating_from_blur(
        edgeblur::at_reference(whole.sigma_best, grey.width.max(grey.height)),
        sharpness::BlurStrictness::parse(strictness),
    );
    // A 6 x 4 grid of census readings: the best tile is "the sharpest place".
    let (gw, gh) = (6usize, 4usize);
    let mut best_contrast: f64 = 0.0;
    let mut best_snr: f64 = 0.0;
    let mut sharp_tiles = 0;
    for ty in 0..gh {
        for tx in 0..gw {
            let rect = [
                tx * grey.width / gw,
                ty * grey.height / gh,
                (tx + 1) * grey.width / gw,
                (ty + 1) * grey.height / gh,
            ];
            let e = edgeblur::measure(grey, Some(rect));
            if e.contrast.is_nan() {
                continue;
            }
            best_contrast = best_contrast.max(e.contrast);
            if e.noise > 0.0 {
                best_snr = best_snr.max(e.contrast / e.noise);
            }
            if e.contrast >= 12.0 {
                sharp_tiles += 1;
            }
        }
    }
    format!(
        "| whole contrast {:.1} noise {:.2} snr {:.2} edges {} best {:.2} | focal {:.3} edge {:.3} bare {:.3} whole {:.3} | tiles best contrast {:.1} best snr {:.2} n12 {}",
        whole.contrast,
        whole.noise,
        whole.contrast / whole.noise,
        whole.edges,
        whole.sigma_best,
        focal.unwrap_or(f64::NAN),
        edge.unwrap_or(f64::NAN),
        bare.score.unwrap_or(f64::NAN),
        bare.whole.unwrap_or(f64::NAN),
        best_contrast,
        best_snr,
        sharp_tiles
    )
}
