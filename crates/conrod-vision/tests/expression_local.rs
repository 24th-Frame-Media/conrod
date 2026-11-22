//! What faces are doing, measured on real portrait and event frames.
//!
//! `cargo test -p conrod-vision --release --test expression_local -- --ignored --nocapture`
//!
//! Needs the YuNet, OCEC and MediaPipe models in `~/.conrod/models`, and the
//! photographer's labels at `target/groundtruth/labels.json` (override with
//! `CONROD_LABELS`). Frames are read for their embedded preview only; nothing
//! is written outside `E:\code\photo\conrod\target\expression\`.
//!
//! Writes `check.json` (every figure, every face) and `contact-sheet.png` (face
//! crops, eye-open bar cyan, smile bar magenta, red frame for a closed eye) so
//! the numbers can be looked at. Decoded previews are cached in `cache/`, so a
//! second run is a few seconds instead of twenty minutes of RAW reads.
//!
//! `CONROD_EXPR_WANT` and `CONROD_EXPR_ATTEMPTS` shrink the run; the sheet row
//! order matches the JSON's `rows`.

use conrod_core::models;
use conrod_vision::expression::{ExpressionModel, BLENDSHAPES, LANDMARKS};
use conrod_vision::faces::{Face, FaceDetector, EYES_CLOSED_BELOW};
use conrod_vision::imageops::Rgb;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

const OUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/expression");
const LABELS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/groundtruth/labels.json"
);
/// Shoots whose subjects are people, not cars.
const PEOPLE: [&str; 10] = [
    "person a", "person b", "person c", "person d", "person e", "person f", "person g", "person h",
    "person i", "event a",
];
/// Frames wanted, and the most to decode chasing them.
const WANT: usize = 40;
const ATTEMPTS: usize = 260;

fn env(name: &str) -> Option<usize> {
    std::env::var(name).ok().and_then(|v| v.parse().ok())
}

fn models_dir() -> PathBuf {
    PathBuf::from(
        std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap(),
    )
    .join(".conrod/models")
}

/// The largest complete JPEG in a file: walk each SOI's segments (so an EXIF
/// thumbnail inside APP1 is skipped, not mistaken for the end) to its EOI.
fn largest_jpeg(bytes: &[u8]) -> Option<&[u8]> {
    let mut best: Option<&[u8]> = None;
    let mut i = 0;
    while i + 3 < bytes.len() {
        if bytes[i] == 0xFF && bytes[i + 1] == 0xD8 {
            if let Some(end) = jpeg_end(bytes, i) {
                let seg = &bytes[i..end];
                if best.is_none_or(|b| seg.len() > b.len()) {
                    best = Some(seg);
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    best
}

/// One past the EOI of the JPEG starting at `start`, or `None` if the markers
/// do not make sense (a byte pattern that only looks like a SOI).
fn jpeg_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 2;
    loop {
        while i + 1 < bytes.len() && bytes[i] != 0xFF {
            i += 1;
        }
        if i + 1 >= bytes.len() {
            return None;
        }
        let marker = bytes[i + 1];
        match marker {
            0xD8 | 0x01 | 0xD0..=0xD7 => i += 2,
            0xD9 => return Some(i + 2),
            0xDA => {
                // Entropy-coded data: scan for the next real marker, skipping
                // byte stuffing and restart markers.
                let mut j = i + 2;
                while j + 1 < bytes.len() {
                    if bytes[j] == 0xFF {
                        let next = bytes[j + 1];
                        if next != 0x00 && !(0xD0..=0xD7).contains(&next) {
                            if next == 0xD9 {
                                return Some(j + 2);
                            }
                            break;
                        }
                    }
                    j += 1;
                }
                if j + 1 >= bytes.len() {
                    return None;
                }
                i = j;
            }
            _ => {
                if i + 3 >= bytes.len() {
                    return None;
                }
                let len = usize::from(bytes[i + 2]) << 8 | usize::from(bytes[i + 3]);
                if len < 2 {
                    return None;
                }
                i += 2 + len;
            }
        }
    }
}

/// The preview at most 1200 px on the long edge, which is all the detector and
/// a 256 px face crop need. Cached as a JPEG so a rerun skips the RAW.
fn preview(frame: &Path, cache: &Path) -> Option<Rgb> {
    let stem = frame.file_stem()?.to_string_lossy().into_owned();
    let cached = cache.join(format!("{stem}.jpg"));
    if let Ok(image) = Rgb::open(&cached) {
        return Some(image);
    }
    let bytes = std::fs::read(frame).ok()?;
    let jpeg = largest_jpeg(&bytes)?;
    let image = Rgb::decode_jpeg(jpeg, 1).ok()?;
    let small = if image.width.max(image.height) > 1200 {
        let s = 1200.0 / image.width.max(image.height) as f64;
        image.resize_cv2_linear(
            ((image.width as f64 * s) as usize).max(1),
            ((image.height as f64 * s) as usize).max(1),
        )
    } else {
        image
    };
    let _ = std::fs::create_dir_all(cache);
    let buffer =
        image::RgbImage::from_raw(small.width as u32, small.height as u32, small.data.clone());
    if let Some(buffer) = buffer {
        let _ = buffer.save(&cached);
    }
    Some(small)
}

fn area(face: &Face) -> f64 {
    (face.bbox[2] - face.bbox[0]) * (face.bbox[3] - face.bbox[1])
}

#[test]
#[ignore = "needs the local models and the photographer's frames"]
fn expressions_on_real_portraits() {
    let want = env("CONROD_EXPR_WANT").unwrap_or(WANT);
    let attempts_max = env("CONROD_EXPR_ATTEMPTS").unwrap_or(ATTEMPTS);
    let labels = std::env::var_os("CONROD_LABELS")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(LABELS));
    if !labels.is_file() {
        eprintln!("no {}; skipping", labels.display());
        return;
    }
    let dir = models_dir();
    for name in [models::FACES, models::EYES] {
        assert!(
            dir.join(name).is_file(),
            "missing {}",
            dir.join(name).display()
        );
    }
    std::fs::create_dir_all(OUT).unwrap();
    let cache = Path::new(OUT).join("cache");

    let text = std::fs::read_to_string(&labels).unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    let frames = value["frames"].as_array().unwrap();

    let mut detector = FaceDetector::load(&dir.join(models::FACES)).unwrap();
    detector.load_eyes(&dir.join(models::EYES)).unwrap();
    let mut ep = ExpressionModel::load(&dir.join(LANDMARKS)).unwrap();
    ep.load_blendshapes(&dir.join(BLENDSHAPES)).unwrap();
    let mut cpu = ExpressionModel::load_cpu(&dir.join(LANDMARKS)).unwrap();
    cpu.load_blendshapes_cpu(&dir.join(BLENDSHAPES)).unwrap();
    let mut geo_ep = ExpressionModel::load(&dir.join(LANDMARKS)).unwrap();
    let mut geo_cpu = ExpressionModel::load_cpu(&dir.join(LANDMARKS)).unwrap();
    eprintln!("blendshapes loaded: {}", ep.has_blendshapes());

    // At most five faces a shoot, so a shoot with 500 portraits does not crowd
    // out the others.
    let mut per_client: std::collections::HashMap<&str, usize> = Default::default();
    let mut rows: Vec<Value> = Vec::new();
    let mut tiles: Vec<(Rgb, [f64; 4], f32, f32)> = Vec::new();
    let (mut attempts, mut timing) = (0usize, Timing::default());

    for frame in frames {
        if rows.len() >= want || attempts >= attempts_max {
            break;
        }
        let client = frame["client"].as_str().unwrap_or("").to_lowercase();
        let Some(key) = PEOPLE.iter().find(|p| client.contains(*p)) else {
            continue;
        };
        let shot = per_client.entry(key).or_default();
        if *shot >= 5 {
            continue;
        }
        let path = PathBuf::from(frame["path"].as_str().unwrap_or(""));
        if !path.is_file() {
            continue;
        }
        attempts += 1;
        let Some(preview) = preview(&path, &cache) else {
            continue;
        };
        let mut found = None;
        for orientation in [1u16, 6, 8, 3] {
            let image = if orientation == 1 {
                preview.clone()
            } else {
                preview.clone().orient(orientation)
            };
            let Ok(faces) = detector.detect(&image) else {
                continue;
            };
            if !faces.is_empty() {
                found = Some((image, faces));
                break;
            }
        }
        let Some((image, mut faces)) = found else {
            continue;
        };
        faces.sort_by(|a, b| area(b).total_cmp(&area(a)));
        let face = &faces[0];
        let eyes = detector.eyes_open(&image, face);

        let t = Instant::now();
        let Ok(expression) = ep.read(&image, face) else {
            continue;
        };
        let ep_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let Ok(cpu_expression) = cpu.read(&image, face) else {
            continue;
        };
        let cpu_ms = t.elapsed().as_secs_f64() * 1e3;
        // The same face without the blend model: the geometry fallback, and
        // the landmarks-only timing.
        let t = Instant::now();
        let Ok(geo) = geo_ep.read(&image, face) else {
            continue;
        };
        let geo_ep_ms = t.elapsed().as_secs_f64() * 1e3;
        let t = Instant::now();
        let Ok(geo_cpu) = geo_cpu.read(&image, face) else {
            continue;
        };
        let geo_cpu_ms = t.elapsed().as_secs_f64() * 1e3;

        timing.ep.push((ep_ms, geo_ep_ms));
        timing.cpu.push((cpu_ms, geo_cpu_ms));

        let ocec: Vec<Option<f32>> = eyes.to_vec();
        let ocec_mean = ocec.iter().flatten().copied().sum::<f32>()
            / ocec.iter().flatten().count().max(1) as f32;
        eprintln!(
            "#{:<3} {:<34} eyes {:.2}/{:.2} ocec {:.2} smile {:.2} jaw {:.2} brow {:.2} yaw {:>4.0} pitch {:>4.0} roll {:>4.0} conf {:.2}",
            rows.len(),
            frame["stem"].as_str().unwrap_or(""),
            expression.eye_open_l,
            expression.eye_open_r,
            ocec_mean,
            expression.smile,
            expression.jaw_open,
            expression.brow_up,
            expression.yaw,
            expression.pitch,
            expression.roll,
            expression.confidence,
        );
        rows.push(json!({
            "frame": path.display().to_string(),
            "stem": frame["stem"],
            "client": frame["client"],
            "faces": faces.len(),
            "bbox": face.bbox,
            "detector": face.score,
            "expression": {
                "eye_open_l": expression.eye_open_l,
                "eye_open_r": expression.eye_open_r,
                "eye_open_min": expression.eye_open_min(),
                "smile": expression.smile,
                "jaw_open": expression.jaw_open,
                "brow_up": expression.brow_up,
                "yaw": expression.yaw,
                "pitch": expression.pitch,
                "roll": expression.roll,
                "confidence": expression.confidence,
            },
            "cpu_expression_differs_by": (expression.eye_open_l - cpu_expression.eye_open_l)
                .abs()
                .max((expression.smile - cpu_expression.smile).abs()),
            "geometry": {
                "eye_open_l": geo.eye_open_l,
                "eye_open_r": geo.eye_open_r,
                "smile": geo.smile,
                "jaw_open": geo.jaw_open,
                "smile_cpu": geo_cpu.smile,
            },
            "ocec": ocec,
            "ocec_mean": ocec_mean,
            "ms": {
                "ep_face": ep_ms,
                "ep_landmarks": geo_ep_ms,
                "cpu_face": cpu_ms,
                "cpu_landmarks": geo_cpu_ms,
            },
        }));
        *shot += 1;
        tiles.push((
            image,
            face.bbox,
            expression.eye_open_min(),
            expression.smile,
        ));
    }

    assert!(
        rows.len() >= 20,
        "only {} faces from {attempts} frames",
        rows.len()
    );

    // The blendshape model's own face-presence score gates the comparison: a
    // wheel or a dog is a YuNet false positive with no landmarks.
    let real: Vec<&Value> = rows
        .iter()
        .filter(|r| r["expression"]["confidence"].as_f64().unwrap() >= 0.5)
        .collect();
    let binary = |row: &Value, open: f64| {
        let ocec = row["ocec_mean"].as_f64().unwrap();
        (open < 0.5) == (ocec < f64::from(EYES_CLOSED_BELOW))
    };
    let agree = real
        .iter()
        .filter(|r| binary(r, r["expression"]["eye_open_min"].as_f64().unwrap()))
        .count();
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.0)
    };
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
    let ep_face: Vec<f64> = timing.ep.iter().map(|t| t.0).collect();
    let ep_lm: Vec<f64> = timing.ep.iter().map(|t| t.1).collect();
    let cpu_face: Vec<f64> = timing.cpu.iter().map(|t| t.0).collect();
    let cpu_lm: Vec<f64> = timing.cpu.iter().map(|t| t.1).collect();
    let summary = json!({
        "models": {
            "landmarks": {"file": LANDMARKS, "source": "huggingface.co/t9749/mediapipe-face-landmarker-onnx", "licence": "Apache-2.0 (Google MediaPipe)"},
            "blendshapes": {"file": BLENDSHAPES, "source": "huggingface.co/t9749/mediapipe-face-landmarker-onnx", "licence": "Apache-2.0 (Google MediaPipe)"},
            "detector": models::FACES,
            "closed_eyes": models::EYES,
        },
        "frames": rows.len(),
        "real_faces": real.len(),
        "attempted": attempts,
        "agreement_with_ocec": agree as f64 / real.len().max(1) as f64,
        "ms_per_face": {
            "ep_total_median": median(ep_face),
            "ep_landmarks_median": median(ep_lm),
            "cpu_total_median": median(cpu_face),
            "cpu_landmarks_median": median(cpu_lm),
            "ep_total_mean": mean(&timing.ep.iter().map(|t| t.0).collect::<Vec<_>>()),
            "cpu_total_mean": mean(&timing.cpu.iter().map(|t| t.0).collect::<Vec<_>>()),
        },
        "rows": rows,
    });
    // Steady-state per-face cost, warm and without RAW reads in the way: the
    // first tile's face, read twenty times.
    let mut bench = json!({});
    if let Some((image, bbox, _, _)) = tiles.first() {
        let face = Face {
            score: 1.0,
            bbox: *bbox,
            eyes: [(bbox[0], bbox[1]), (bbox[2], bbox[1])],
            landmarks: [
                (bbox[0], bbox[1]),
                (bbox[2], bbox[1]),
                ((bbox[0] + bbox[2]) / 2.0, (bbox[1] + bbox[3]) / 2.0),
                (bbox[0], bbox[3]),
                (bbox[2], bbox[3]),
            ],
        };
        let time = |model: &mut ExpressionModel| -> f64 {
            for _ in 0..3 {
                let _ = model.read(image, &face);
            }
            let mut samples: Vec<f64> = (0..20)
                .map(|_| {
                    let t = Instant::now();
                    let _ = model.read(image, &face);
                    t.elapsed().as_secs_f64() * 1e3
                })
                .collect();
            samples.sort_by(f64::total_cmp);
            samples[samples.len() / 2]
        };
        let ep_ms = time(&mut ep);
        let cpu_ms = time(&mut cpu);
        bench = json!({"ep_ms": ep_ms, "cpu_ms": cpu_ms});
        eprintln!("steady state: EP {ep_ms:.2} ms/face, CPU {cpu_ms:.2} ms/face");
    }
    let mut summary = summary;
    summary["bench"] = bench;
    std::fs::write(
        Path::new(OUT).join("check.json"),
        serde_json::to_string_pretty(&summary).unwrap(),
    )
    .unwrap();
    if let Err(e) = write_sheet(
        &tiles
            .iter()
            .zip(&rows)
            .filter(|(_, row)| row["expression"]["confidence"].as_f64().unwrap() >= 0.5)
            .map(|(tile, _)| tile.clone())
            .collect::<Vec<_>>(),
        Path::new(OUT).join("contact-sheet.png"),
    ) {
        eprintln!("contact sheet failed: {e}");
    }
    eprintln!(
        "{} faces ({} with landmarks) from {attempts} frames; OCEC agrees {:.0}% ({agree} of {}); EP {:.2} ms/face (landmarks {:.2}), CPU {:.2} ms/face (landmarks {:.2})",
        summary["frames"],
        real.len(),
        summary["agreement_with_ocec"].as_f64().unwrap() * 100.0,
        real.len(),
        summary["ms_per_face"]["ep_total_median"].as_f64().unwrap(),
        summary["ms_per_face"]["ep_landmarks_median"].as_f64().unwrap(),
        summary["ms_per_face"]["cpu_total_median"].as_f64().unwrap(),
        summary["ms_per_face"]["cpu_landmarks_median"].as_f64().unwrap(),
    );
    assert!(summary["frames"].as_u64().unwrap() >= 20);
}

#[derive(Default)]
struct Timing {
    ep: Vec<(f64, f64)>,
    cpu: Vec<(f64, f64)>,
}

/// One PNG, five tiles a row: the face, a cyan bar for eye openness and a
/// magenta one for the smile, and a green or red border for the eye call.
fn write_sheet(tiles: &[(Rgb, [f64; 4], f32, f32)], path: PathBuf) -> Result<(), String> {
    const TILE: usize = 320;
    const BAR: usize = 26;
    const COLS: usize = 5;
    let rows = tiles.len().div_ceil(COLS).max(1);
    let (w, h) = (COLS * TILE, rows * (TILE + BAR));
    let mut sheet = vec![24u8; w * h * 3];
    for (n, (image, bbox, eyes, smile)) in tiles.iter().enumerate() {
        let (ox, oy) = ((n % COLS) * TILE, (n / COLS) * (TILE + BAR));
        let side = (bbox[2] - bbox[0]).max(bbox[3] - bbox[1]) * 1.35;
        let (cx, cy) = ((bbox[0] + bbox[2]) / 2.0, (bbox[1] + bbox[3]) / 2.0);
        let x0 = (cx - side / 2.0).max(0.0) as usize;
        let y0 = (cy - side / 2.0).max(0.0) as usize;
        let x1 = ((cx + side / 2.0) as usize).min(image.width);
        let y1 = ((cy + side / 2.0) as usize).min(image.height);
        if x1 <= x0 + 4 || y1 <= y0 + 4 {
            continue;
        }
        let crop = image.crop(x0, y0, x1, y1).resize_cv2_linear(TILE, TILE);
        for y in 0..TILE {
            let i = ((oy + y) * w + ox) * 3;
            let j = (y * TILE) * 3;
            sheet[i..i + TILE * 3].copy_from_slice(&crop.data[j..j + TILE * 3]);
        }
        for y in TILE..(TILE + BAR) {
            for x in 0..TILE {
                let i = ((oy + y) * w + ox + x) * 3;
                sheet[i..i + 3].copy_from_slice(&[40, 40, 40]);
            }
        }
        let bar_y = oy + TILE;
        bar(
            &mut sheet,
            w,
            (ox + 4, bar_y + 4),
            TILE - 12,
            *eyes,
            [0, 200, 255],
        );
        bar(
            &mut sheet,
            w,
            (ox + 4, bar_y + 14),
            TILE - 12,
            *smile,
            [255, 90, 200],
        );
        let edge = if *eyes < 0.5 {
            [220, 40, 40]
        } else {
            [40, 190, 80]
        };
        rect(&mut sheet, w, (ox, oy), (TILE - 1, TILE - 1), edge, 3);
    }
    let out = image::RgbImage::from_raw(w as u32, h as u32, sheet).ok_or("sheet buffer")?;
    out.save(&path).map_err(|e| e.to_string())
}

fn bar(
    sheet: &mut [u8],
    w: usize,
    (x, y): (usize, usize),
    len: usize,
    value: f32,
    colour: [u8; 3],
) {
    let filled = (len as f32 * value.clamp(0.0, 1.0)) as usize;
    for i in 0..filled {
        let p = (y * w + x + i) * 3;
        sheet[p..p + 3].copy_from_slice(&colour);
    }
}

fn rect(
    sheet: &mut [u8],
    w: usize,
    (x, y): (usize, usize),
    (rw, rh): (usize, usize),
    colour: [u8; 3],
    t: usize,
) {
    for y2 in y..y + rh {
        for x2 in x..x + rw {
            if x2 < x + t || y2 < y + t || x2 + t >= x + rw || y2 + t >= y + rh {
                let p = (y2 * w + x2) * 3;
                if p + 3 <= sheet.len() {
                    sheet[p..p + 3].copy_from_slice(&colour);
                }
            }
        }
    }
}
