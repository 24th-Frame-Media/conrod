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
    let stored_det: Option<f64> = db
        .query_row(
            "SELECT d.sharpness FROM detections d JOIN images i ON i.id=d.image_id WHERE i.path=?1 ORDER BY (d.x2-d.x1)*(d.y2-d.y1) DESC LIMIT 1",
            [&path],
            |r| r.get::<_, Option<f64>>(0),
        )
        .ok()
        .flatten();
    // The camera's focus area, in pixels, when the album has read it.
    let af: Option<[f64; 4]> = db
        .query_row(
            "SELECT af_x1,af_y1,af_x2,af_y2 FROM images WHERE path=?1",
            [&path],
            |r| {
                Ok([
                    r.get::<_, Option<f64>>(0)?,
                    r.get::<_, Option<f64>>(1)?,
                    r.get::<_, Option<f64>>(2)?,
                    r.get::<_, Option<f64>>(3)?,
                ])
            },
        )
        .ok()
        .and_then(|v| {
            let b = [v[0]?, v[1]?, v[2]?, v[3]?];
            let (w, h) = (
                db.query_row("SELECT width FROM images WHERE path=?1", [&path], |r| {
                    r.get::<_, Option<f64>>(0)
                })
                .ok()
                .flatten()?,
                db.query_row("SELECT height FROM images WHERE path=?1", [&path], |r| {
                    r.get::<_, Option<f64>>(0)
                })
                .ok()
                .flatten()?,
            );
            Some([b[0] * w, b[1] * h, b[2] * w, b[3] * h])
        });
    let frame = raw::read(std::path::Path::new(&path)).map_err(|e| e.to_string())?;
    let image = Rgb::decode_jpeg(&frame.preview, 1)
        .map_err(|e| e.to_string())?
        .orient(frame.orientation);
    let grey = image.to_gray();
    let long = grey.width.max(grey.height);
    let m = sharpness::measure(&grey, bbox, None);
    // The tile score a re-score computes: on the padded, downscaled crop, not on
    // the preview, so it is a different number from `m.score`.
    let crop_measure = bbox.map(|b| {
        let opts = conrod_vision::detect::DetectOptions {
            crop_padding: settings.crop_padding,
            dominant_subject_fraction: settings.dominant_subject_fraction,
            ..Default::default()
        };
        let crop_box = conrod_vision::detect::expand_box(b, image.width, image.height, &opts);
        let crop = conrod_engine::cut(&image, crop_box, &settings);
        let scale = crop.width as f64 / (crop_box[2] - crop_box[0]);
        let inner = [
            (b[0] - crop_box[0]) * scale,
            (b[1] - crop_box[1]) * scale,
            (b[2] - crop_box[0]) * scale,
            (b[3] - crop_box[1]) * scale,
        ];
        sharpness::measure(&crop.to_gray(), Some(inner), None)
    });
    let n = |v: Option<f64>| v.unwrap_or(f64::NAN);
    let (crop_tile, crop_uncertain) =
        crop_measure.map_or((f64::NAN, false), |c| (c.score, c.uncertain));
    // What the edge-blur score is built from, exactly as `cull_frame` and a
    // re-score build it: the worse of the whole box and its centre 60%.
    let blur = bbox.map(|b| conrod_engine::subject_blur(&grey, b, long));
    let subject = blur.and_then(|b| {
        let tile = m.score;
        let worst = if tile < 0.80 { b.worst } else { b.whole };
        // As the engine: the tile score only decides whether the centre was used.
        sharpness::rating_from_blur(worst, sharpness::BlurStrictness::parse(&strictness))
    });
    // What the scan / re-score stores for a frame with no detected subject: the
    // same `bare_frame` the engine uses, so the numbers cannot drift apart.
    let bare = bbox
        .is_none()
        .then(|| conrod_engine::bare_frame(&grey, &strictness));
    // What the edge blur would be inside the camera's own box: the subject the
    // photographer focused on, without the wall behind a detection box's edges.
    // As the engine: the whole AF box (it is already the subject), and the
    // score can only be lowered by it, never raised.
    let af_blur = af.map(|b| conrod_engine::subject_blur(&grey, b, long));
    let af_rating = bbox.and(af_blur).and_then(|a| {
        sharpness::rating_from_blur(a.whole, sharpness::BlurStrictness::parse(&strictness))
    });
    // The score that would be stored now: a bare frame's rescued score, else the
    // subject's edge blur (or the AF box's, when worse), else the tile score it
    // falls back to.
    let now = match &bare {
        Some(bare) => bare.score,
        None => conrod_engine::rescore::second_opinion(subject, af_rating).or(Some(m.score)),
    };
    let blur_line = match &blur {
        Some(blur) => format!(
            " · sigma whole {:.3} · sigma centre {:.3} · tile {:.3} · crop tile {:.3} (uncertain {}) · uncertain {}{}",
            blur.whole,
            blur.centre,
            m.score,
            crop_tile,
            crop_uncertain,
            m.uncertain,
            af_blur.map_or(String::new(), |a| format!(
                " · sigma af {:.3} ({:.3})",
                a.whole,
                n(af_rating)
            ))
        ),
        None => format!(" · tile {:.3}", m.score),
    };
    let bare_line = match &bare {
        Some(bare) => format!(
            " · bare score {:.3} · bare whole {:.3}",
            n(bare.score),
            n(bare.whole)
        ),
        None => String::new(),
    };
    // Per-direction edge sigmas (analysis pixels): a direction with no measurable
    // edges is where a smeared subject's edges vanished.
    let dirs_line = bbox.map_or(String::new(), |b| {
        let px = |v: f64, e: usize| (v.max(0.0) as usize).min(e);
        let rect = |r: [f64; 4]| {
            [
                px(r[0], grey.width),
                px(r[1], grey.height),
                px(r[2], grey.width),
                px(r[3], grey.height),
            ]
        };
        let fmt = |e: conrod_vision::edgeblur::EdgeBlur| {
            format!(
                "[{:.2},{:.2},{:.2},{:.2}] n{}",
                e.dirs[0], e.dirs[1], e.dirs[2], e.dirs[3], e.edges
            )
        };
        let whole = fmt(conrod_vision::edgeblur::measure(&grey, Some(rect(b))));
        let c = [
            b[0] + 0.2 * (b[2] - b[0]),
            b[1] + 0.2 * (b[3] - b[1]),
            b[2] - 0.2 * (b[2] - b[0]),
            b[3] - 0.2 * (b[3] - b[1]),
        ];
        let centre = fmt(conrod_vision::edgeblur::measure(&grey, Some(rect(c))));
        format!(" · dirs whole {whole} · centre {centre}")
    });
    // How much of the frame is sharp, and what a pan's frame rating is capped at
    // by it, whatever this frame's own subject is.
    let share = sharpness::frame_sharp_share(&grey);
    let cap = share.map(sharpness::pan_share_cap);
    println!(
        "{needle}: stored rating {rating:.3} · stored subject {:.3} · now score {:.3}{blur_line} · background {:.3} · panning {} · sharp end {} · share {:.2} · box {bbox:?} · frame share {:.3} · cap {:.3}{bare_line}{dirs_line}",
        n(stored_det),
        n(now),
        m.background,
        m.panning,
        m.sharp_end,
        m.pan_share,
        n(share),
        n(cap),
    );
    Ok(())
}
