//! One-off diagnostic: does the detector find cars on the frames it skipped at
//! the album's own confidence, and does the camera's focus area agree with the
//! boxes it finds? Not part of the app.
//!
//! cargo run --release -p conrod-engine --example detectprobe -- \
//!     <db> <job> <detect_conf> <min_box_fraction> [limit]
//!
//! `limit` is the total sample (default 60), split 2:1 into frames the scan
//! stored no detection for and frames it stored a vehicle on, each spread
//! evenly by `taken_at`. The detector settings come from the album's
//! `settings_json`; `detect_conf` and `min_box_fraction` replace the album's
//! own, and `min_box_fraction` still goes through the preset's factor exactly
//! as `conrod_engine`'s `options()` does, so the control run reproduces the
//! scan. The AF area is read from the RAW with exiftool in batches.

use conrod_core::models;
use conrod_core::profile::ShootPreset;
use conrod_core::settings::Settings;
use conrod_io::raw;
use conrod_vision::detect::{self, DetectOptions, Detector};
use conrod_vision::imageops::Rgb;
use rusqlite::Connection;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::Instant;

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db_path = args.first().ok_or("db path")?;
    let job: i64 = args
        .get(1)
        .ok_or("job")?
        .parse()
        .map_err(|e| format!("{e}"))?;
    let conf: f64 = args
        .get(2)
        .ok_or("detect_conf")?
        .parse()
        .map_err(|e| format!("{e}"))?;
    let min_box: f64 = args
        .get(3)
        .ok_or("min_box_fraction")?
        .parse()
        .map_err(|e| format!("{e}"))?;
    let limit: usize = match args.get(4) {
        Some(v) => v.parse().map_err(|e| format!("{e}"))?,
        None => 60,
    };
    let missed_wanted = (limit * 2 / 3).max(1);
    let known_wanted = (limit - missed_wanted).max(1);

    let started = Instant::now();
    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;

    // The album's settings the way a re-score builds them.
    let mut settings = Settings::default();
    let mut stored_json = String::new();
    if let Ok(stored) = db.query_row(
        "SELECT COALESCE(settings_json,'') FROM jobs WHERE id=?1",
        [job],
        |r| r.get::<_, String>(0),
    ) {
        stored_json = stored.clone();
        if let Ok(map) = serde_json::from_str::<Map<String, Value>>(&stored) {
            settings = settings.apply(&map);
        }
    }
    let preset = ShootPreset::parse(&settings.scan_profile);

    // The same DetectOptions the scan builds, with the two arguments on top.
    let classes = conrod_engine::detect_options(&settings).classes;
    let options = DetectOptions {
        conf: conf as f32,
        classes: classes.clone(),
        min_box_fraction: preset.min_box_fraction(min_box),
        max_per_frame: preset.max_subjects(settings.max_vehicles_per_frame.max(1) as usize),
        crop_padding: settings.crop_padding,
        dominant_subject_fraction: settings.dominant_subject_fraction,
        ..DetectOptions::default()
    };

    let missed = pick(&db, job, missed_wanted, true)?;
    let known = pick(&db, job, known_wanted, false)?;
    let all: Vec<&Frame> = missed.iter().chain(known.iter()).collect();
    let paths: Vec<String> = all.iter().map(|f| f.path.clone()).collect();

    // The AF area for every sampled frame, in a handful of exiftool calls.
    let (af, af_errors) = read_af(&paths);

    let model = conrod_engine::detector_model();
    let mut detector = Detector::load(&model, detect::Device::Auto)?;
    println!(
        "detectprobe: job {job} conf {:.3} min_box_fraction {:.4} (effective {:.4}, preset {}) · {} + {} frames · detector {} [{}] · {}",
        conf,
        min_box,
        options.min_box_fraction,
        preset.name(),
        missed.len(),
        known.len(),
        model.display(),
        detector.device,
        if limit == 60 { "default sample" } else { "reduced sample" },
    );

    let mut runner = Runner {
        detector: &mut detector,
        options: &options,
        af: &af,
        af_errors: &af_errors,
        failures: Vec::new(),
        t0: started,
    };

    let missed_rows: Vec<Row> = missed.iter().map(|f| runner.run(f)).collect();
    let known_rows: Vec<Row> = known.iter().map(|f| runner.run(f)).collect();
    let failures = runner.failures.clone();
    let elapsed = started.elapsed().as_secs_f64();

    // 1. recall on the frames the scan skipped.
    let found_missed: Vec<&Row> = missed_rows.iter().filter(|r| r.boxes > 0).collect();
    // 2. AF agreement, either test.
    let af_centre_in_box: Vec<&&Row> = found_missed.iter().filter(|r| r.af_centre_in_box).collect();
    let box_centre_in_af: Vec<&&Row> = found_missed.iter().filter(|r| r.box_centre_in_af).collect();
    let agreeing: Vec<&&Row> = found_missed
        .iter()
        .filter(|r| r.af_centre_in_box || r.box_centre_in_af)
        .collect();
    // 3. best-confidence distribution over all 40, misses counted as 0.
    let all_best: Vec<f32> = missed_rows.iter().map(|r| r.best_conf).collect();
    let found_best: Vec<f32> = found_missed.iter().map(|r| r.best_conf).collect();

    // 4. the known frames.
    let known_detected: usize = known_rows.iter().map(|r| r.boxes).sum();
    let known_stored: usize = known_rows.iter().map(|r| r.stored_boxes).sum();
    let known_found: Vec<&Row> = known_rows.iter().filter(|r| r.boxes > 0).collect();
    let known_best: Vec<f32> = known_rows.iter().map(|r| r.best_conf).collect();
    let known_best_found: Vec<f32> = known_found.iter().map(|r| r.best_conf).collect();
    let stored_best: Vec<f32> = known_rows.iter().filter_map(|r| r.stored_conf).collect();

    println!("\n== missed frames (scan stored no detection) ==");
    for r in &missed_rows {
        println!("{}", r.line());
    }
    println!("\n== known frames (scan stored a vehicle) ==");
    for r in &known_rows {
        println!("{}", r.line());
    }

    let bucket = |rows: &[Row]| -> Value {
        json!(rows
            .iter()
            .map(|r| json!({
                "frame": file_name(&r.path),
                "boxes": r.boxes,
                "stored_boxes": r.stored_boxes,
                "best_conf": round(f64::from(r.best_conf), 3),
                "stored_best_conf": r.stored_conf.map(|c| round(f64::from(c), 3)),
                "af_mode": r.af_mode,
                "af_read": r.af_read,
                "box_centre_in_af": r.box_centre_in_af,
                "af_centre_in_box": r.af_centre_in_box,
            }))
            .collect::<Vec<_>>())
    };

    let report = json!({
        "command": {
            "conf": conf,
            "min_box_fraction": min_box,
            "min_box_fraction_effective": round(options.min_box_fraction as f64, 4),
            "preset": preset.name(),
            "classes": classes,
            "max_per_frame": options.max_per_frame,
            "limit": limit,
            "sample_missed": missed.len(),
            "sample_known": known.len(),
        },
        "album": {
            "detect_model": settings.detect_model,
            "detect_conf": settings.detect_conf,
            "min_box_fraction": settings.min_box_fraction,
            "scan_profile": settings.scan_profile,
        },
        "detector": {
            "model": model.display().to_string(),
            "device": detector.device,
        },
        "missed": {
            "frames": missed_rows.len(),
            "frames_with_vehicle_box": found_missed.len(),
            "frames_af_centre_inside_box": af_centre_in_box.len(),
            "frames_box_centre_inside_af": box_centre_in_af.len(),
            "frames_af_agreeing": agreeing.len(),
            "best_conf_on_agreeing": agreeing.iter().map(|r| round(f64::from(r.best_conf), 3)).collect::<Vec<_>>(),
            "best_conf_all40_p10_p50_p90": [
                round(pct(&all_best, 10.0), 3),
                round(pct(&all_best, 50.0), 3),
                round(pct(&all_best, 90.0), 3)
            ],
            "best_conf_found_p10_p50_p90": if found_best.is_empty() { json!([null, null, null]) } else { json!([
                round(pct(&found_best, 10.0), 3),
                round(pct(&found_best, 50.0), 3),
                round(pct(&found_best, 90.0), 3)
            ]) },
            "box_counts": missed_rows.iter().map(|r| r.boxes).collect::<Vec<_>>(),
            "rows": bucket(&missed_rows),
        },
        "known": {
            "frames": known_rows.len(),
            "frames_with_vehicle_box": known_found.len(),
            "boxes_detected": known_detected,
            "boxes_stored": known_stored,
            "extra_boxes": known_detected as i64 - known_stored as i64,
            "boxes_per_frame_detected": round(known_detected as f64 / known_rows.len().max(1) as f64, 2),
            "boxes_per_frame_stored": round(known_stored as f64 / known_rows.len().max(1) as f64, 2),
            "best_conf_all_p10_p50_p90": [
                round(pct(&known_best, 10.0), 3),
                round(pct(&known_best, 50.0), 3),
                round(pct(&known_best, 90.0), 3)
            ],
            "best_conf_found_p10_p50_p90": if known_best_found.is_empty() { json!([null, null, null]) } else { json!([
                round(pct(&known_best_found, 10.0), 3),
                round(pct(&known_best_found, 50.0), 3),
                round(pct(&known_best_found, 90.0), 3)
            ]) },
            "stored_best_conf_p10_p50_p90": if stored_best.is_empty() { json!([null, null, null]) } else { json!([
                round(pct(&stored_best, 10.0), 3),
                round(pct(&stored_best, 50.0), 3),
                round(pct(&stored_best, 90.0), 3)
            ]) },
            "box_counts": known_rows.iter().map(|r| r.boxes).collect::<Vec<_>>(),
            "rows": bucket(&known_rows),
        },
        "failures": failures,
        "elapsed_s": round(elapsed, 1),
        "stored_settings_seen": !stored_json.is_empty(),
    });

    println!("\n---- JSON ----");
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

fn file_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

fn round(v: f64, places: u32) -> f64 {
    let f = 10f64.powi(places as i32);
    (v * f).round() / f
}

/// Linear-interpolated percentile of `values`, 0 when empty.
fn pct(values: &[f32], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f32> = values.to_vec();
    v.sort_by(f32::total_cmp);
    if v.len() == 1 {
        return v[0] as f64;
    }
    let pos = (p / 100.0) * (v.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let t = pos - lo as f64;
    (v[lo] as f64) * (1.0 - t) + (v[hi] as f64) * t
}

struct Frame {
    path: String,
    stored_boxes: usize,
    stored_conf: Option<f32>,
}

struct Row {
    path: String,
    boxes: usize,
    best_conf: f32,
    stored_boxes: usize,
    stored_conf: Option<f32>,
    af_read: bool,
    af_mode: Option<i64>,
    box_centre_in_af: bool,
    af_centre_in_box: bool,
    seconds: f64,
}

impl Row {
    fn line(&self) -> String {
        format!(
            "{:<34} boxes {} stored {} best {:.3} stored_best {} af {} mode {} · box_in_af {} af_in_box {} · {:.1}s",
            file_name(&self.path),
            self.boxes,
            self.stored_boxes,
            self.best_conf,
            self.stored_conf.map_or("-".to_string(), |c| format!("{c:.3}")),
            if self.af_read { "ok" } else { "MISSING" },
            self.af_mode.map_or("-".to_string(), |m| m.to_string()),
            yes(self.box_centre_in_af),
            yes(self.af_centre_in_box),
            self.seconds,
        )
    }
}

fn yes(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

/// `k` row indices spread evenly across `n`.
fn spread(n: usize, k: usize) -> Vec<usize> {
    let k = k.min(n);
    if k == 0 {
        return Vec::new();
    }
    (0..k)
        .map(|i| (((i as f64 + 0.5) * n as f64 / k as f64) as usize).min(n - 1))
        .collect()
}

fn pick(db: &Connection, job: i64, wanted: usize, missed: bool) -> Result<Vec<Frame>, String> {
    let condition = if missed {
        "NOT EXISTS (SELECT 1 FROM detections d WHERE d.image_id=i.id)"
    } else {
        "EXISTS (SELECT 1 FROM detections d WHERE d.image_id=i.id AND d.region_type='vehicle')"
    };
    let sql = format!(
        "SELECT i.id, i.path FROM images i WHERE i.job_id=?1 AND COALESCE(i.copy_of,0)=0 AND {condition} ORDER BY i.taken_at, i.id"
    );
    let mut stmt = db.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([job], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut all: Vec<(i64, String)> = Vec::new();
    for r in rows {
        all.push(r.map_err(|e| e.to_string())?);
    }
    let mut out = Vec::new();
    for idx in spread(all.len(), wanted) {
        let (id, path) = &all[idx];
        let (count, conf): (i64, Option<f32>) = db
            .query_row(
                "SELECT COUNT(*), MAX(conf) FROM detections WHERE image_id=?1 AND region_type='vehicle'",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        out.push(Frame {
            path: path.clone(),
            stored_boxes: count as usize,
            stored_conf: conf,
        });
    }
    Ok(out)
}

struct Runner<'a> {
    detector: &'a mut Detector,
    options: &'a DetectOptions,
    af: &'a HashMap<String, Af>,
    af_errors: &'a HashMap<String, String>,
    failures: Vec<Value>,
    t0: Instant,
}

impl Runner<'_> {
    fn run(&mut self, frame: &Frame) -> Row {
        let t = Instant::now();
        let mut row = Row {
            path: frame.path.clone(),
            boxes: 0,
            best_conf: 0.0,
            stored_boxes: frame.stored_boxes,
            stored_conf: frame.stored_conf,
            af_read: false,
            af_mode: None,
            box_centre_in_af: false,
            af_centre_in_box: false,
            seconds: 0.0,
        };
        let key = path_key(&frame.path);
        let af = self.af.get(&key);
        if af.is_none() {
            let why = self
                .af_errors
                .get(&key)
                .cloned()
                .unwrap_or_else(|| "no AF tags".into());
            self.failures.push(json!({
                "frame": file_name(&frame.path),
                "stage": "af",
                "error": why,
                "taken_before": round(self.t0.elapsed().as_secs_f64(), 1),
            }));
        } else {
            row.af_read = true;
            row.af_mode = af.and_then(|a| a.mode);
        }

        // Read, decode and orient the preview exactly as `cull_frame` does.
        let frame_io = match raw::read(Path::new(&frame.path)) {
            Ok(f) => f,
            Err(e) => {
                self.failures.push(json!({
                    "frame": file_name(&frame.path),
                    "stage": "raw",
                    "error": e,
                }));
                row.seconds = t.elapsed().as_secs_f64();
                return row;
            }
        };
        let orientation = frame_io.orientation;
        let rgb = match Rgb::decode_jpeg(&frame_io.preview, 1).map(|i| i.orient(orientation)) {
            Ok(i) => i,
            Err(e) => {
                self.failures.push(json!({
                    "frame": file_name(&frame.path),
                    "stage": "decode",
                    "error": e,
                }));
                row.seconds = t.elapsed().as_secs_f64();
                return row;
            }
        };
        let (iw, ih) = (rgb.width, rgb.height);
        let _grey = rgb.to_gray();
        let input = detect::letterbox(&rgb);
        let found = match self.detector.detect_letterboxed(input, self.options) {
            Ok(f) => f,
            Err(e) => {
                self.failures.push(json!({
                    "frame": file_name(&frame.path),
                    "stage": "detect",
                    "error": e,
                }));
                row.seconds = t.elapsed().as_secs_f64();
                return row;
            }
        };
        let vehicles: Vec<&detect::Detection> = found
            .iter()
            .filter(|d| detect::VEHICLES.contains(&d.class_id))
            .collect();
        row.boxes = vehicles.len();
        row.best_conf = vehicles.iter().map(|d| d.conf).fold(0.0, f32::max);

        if let (true, Some(af)) = (row.af_read, af) {
            let (cx, cy, hw, hh) = af.rect(iw, ih, orientation);
            let (ax0, ay0, ax1, ay1) = (cx - hw, cy - hh, cx + hw, cy + hh);
            for d in &vehicles {
                let b = d.bbox;
                let (bx, by) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
                if bx >= ax0 && bx <= ax1 && by >= ay0 && by <= ay1 {
                    row.box_centre_in_af = true;
                }
                if cx >= b[0] && cx <= b[2] && cy >= b[1] && cy <= b[3] {
                    row.af_centre_in_box = true;
                }
            }
        }
        row.seconds = t.elapsed().as_secs_f64();
        row
    }
}

/// The AF area as the camera wrote it, in AF-canvas pixels.
struct Af {
    cx: f64,
    cy: f64,
    w: f64,
    h: f64,
    canvas_w: f64,
    canvas_h: f64,
    mode: Option<i64>,
}

impl Af {
    /// Centre and half-extents in the upright frame's pixels.
    fn rect(&self, img_w: usize, img_h: usize, orientation: u16) -> (f64, f64, f64, f64) {
        let nx = self.cx / self.canvas_w;
        let ny = self.cy / self.canvas_h;
        let hx = self.w / 2.0 / self.canvas_w;
        let hy = self.h / 2.0 / self.canvas_h;
        // Undo the same rotation `Rgb::orient` applies to the preview.
        let (nx, ny, hx, hy) = match orientation {
            3 => (1.0 - nx, 1.0 - ny, hx, hy),
            6 => (1.0 - ny, nx, hy, hx),
            8 => (ny, 1.0 - nx, hy, hx),
            _ => (nx, ny, hx, hy),
        };
        (
            nx * img_w as f64,
            ny * img_h as f64,
            hx * img_w as f64,
            hy * img_h as f64,
        )
    }
}

fn path_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn exiftool() -> String {
    if let Some(p) = models::exiftool() {
        return p.display().to_string();
    }
    if let Ok(p) = std::env::var("CONROD_EXIFTOOL") {
        return p;
    }
    "exiftool".into()
}

/// Read every frame's AF area, batched into a few exiftool runs.
fn read_af(paths: &[String]) -> (HashMap<String, Af>, HashMap<String, String>) {
    let mut af = HashMap::new();
    let mut errors = HashMap::new();
    let tool = exiftool();
    for chunk in paths.chunks(30) {
        let mut cmd = Command::new(&tool);
        cmd.args([
            "-n",
            "-json",
            "-G1",
            "-Canon:AFImageWidth",
            "-Canon:AFImageHeight",
            "-Canon:AFAreaWidths",
            "-Canon:AFAreaHeights",
            "-Canon:AFAreaXPositions",
            "-Canon:AFAreaYPositions",
            "-Canon:AFAreaMode",
        ]);
        cmd.args(chunk);
        let out = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                for p in chunk {
                    errors.insert(path_key(p), format!("exiftool failed to start: {e}"));
                }
                continue;
            }
        };
        if !out.status.success() && out.stdout.is_empty() {
            let e = String::from_utf8_lossy(&out.stderr).trim().to_string();
            for p in chunk {
                errors.insert(path_key(p), format!("exiftool exit {}: {e}", out.status));
            }
            continue;
        }
        let parsed: Vec<Value> = match serde_json::from_slice(&out.stdout) {
            Ok(v) => v,
            Err(e) => {
                for p in chunk {
                    errors.insert(path_key(p), format!("exiftool JSON: {e}"));
                }
                continue;
            }
        };
        for entry in parsed {
            let source = entry
                .get("SourceFile")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let key = path_key(&source);
            let number = |tag: &str| -> Option<f64> {
                let v = entry.get(tag)?;
                match v {
                    Value::Number(n) => n.as_f64(),
                    Value::String(s) => s.split_whitespace().next()?.parse().ok(),
                    _ => None,
                }
            };
            let mode = entry.get("Canon:AFAreaMode").and_then(|v| match v {
                Value::Number(n) => n.as_i64(),
                Value::String(s) => s.parse().ok(),
                _ => None,
            });
            let (Some(dx), Some(dy), Some(w), Some(h)) = (
                number("Canon:AFAreaXPositions"),
                number("Canon:AFAreaYPositions"),
                number("Canon:AFAreaWidths"),
                number("Canon:AFAreaHeights"),
            ) else {
                errors.insert(key, "no AFAreaXPositions/YPositions/Widths/Heights".into());
                continue;
            };
            if w <= 0.0 || h <= 0.0 {
                errors.insert(key, format!("AF area has no size ({w}x{h})"));
                continue;
            }
            let (Some(canvas_w), Some(canvas_h)) =
                (number("Canon:AFImageWidth"), number("Canon:AFImageHeight"))
            else {
                errors.insert(key, "no AFImageWidth/Height".into());
                continue;
            };
            af.insert(
                key,
                Af {
                    cx: canvas_w / 2.0 + dx,
                    cy: canvas_h / 2.0 - dy,
                    w,
                    h,
                    canvas_w,
                    canvas_h,
                    mode,
                },
            );
        }
    }
    (af, errors)
}
