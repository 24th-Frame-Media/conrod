//! Labels-free blur sensitivity of `sharpness::measure`: synthetic blur on the
//! preview pixels, then the same crop and measure as `cull_frame`.
//! `CONROD_BLURTEST_DIR=<dir with CRs and boxes.json> cargo test -p conrod-engine --release blur_sensitivity -- --ignored --nocapture`
//! boxes.json: [{"file": "X.CR3", "box": [x1, y1, x2, y2]}] in oriented preview pixels.

use super::*;
use conrod_vision::imageops::Gray;

fn gaussian(sigma: f64) -> Vec<f32> {
    let r = (sigma * 3.0).ceil() as i64;
    let k: Vec<f32> = (-r..=r)
        .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp() as f32)
        .collect();
    let sum: f32 = k.iter().sum();
    k.iter().map(|v| v / sum).collect()
}

/// Convolve along x (or y), edge-clamped.
fn convolve(img: &Rgb, k: &[f32], vertical: bool) -> Rgb {
    let (w, h) = (img.width as i64, img.height as i64);
    let r = (k.len() / 2) as i64;
    let mut out = vec![0u8; img.data.len()];
    for y in 0..h {
        for x in 0..w {
            for c in 0..3 {
                let mut acc = 0.0;
                for (i, wt) in k.iter().enumerate() {
                    let o = i as i64 - r;
                    let (sx, sy) = if vertical {
                        (x, (y + o).clamp(0, h - 1))
                    } else {
                        ((x + o).clamp(0, w - 1), y)
                    };
                    acc += wt * img.data[((sy * w + sx) * 3 + c) as usize] as f32;
                }
                out[((y * w + x) * 3 + c) as usize] = acc.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Rgb::new(img.width, img.height, out)
}

enum Blur {
    Gauss(f64),
    Motion(usize),
}

impl Blur {
    fn reach(&self) -> usize {
        match self {
            Blur::Gauss(s) => (s * 3.0).ceil() as usize + 1,
            Blur::Motion(l) => l + 1,
        }
    }
    fn apply(&self, img: &Rgb) -> Rgb {
        match *self {
            Blur::Gauss(s) if s <= 0.0 => img.clone(),
            Blur::Gauss(s) => convolve(&convolve(img, &gaussian(s), false), &gaussian(s), true),
            Blur::Motion(l) => convolve(img, &vec![1.0 / l as f32; l], false),
        }
    }
}

#[test]
#[ignore]
fn blur_sensitivity() {
    let dir = PathBuf::from(std::env::var("CONROD_BLURTEST_DIR").expect("CONROD_BLURTEST_DIR"));
    let list: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(dir.join("boxes.json")).unwrap()).unwrap();
    let cfg = Settings::default();
    let opts = DetectOptions {
        crop_padding: cfg.crop_padding,
        dominant_subject_fraction: cfg.dominant_subject_fraction,
        ..Default::default()
    };
    let levels: Vec<(String, Blur)> = [0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 5.0]
        .into_iter()
        .map(|s| (format!("gauss {s}"), Blur::Gauss(s)))
        .chain([3, 5, 9, 15, 25].map(|l| (format!("motion {l}"), Blur::Motion(l))))
        .collect();
    // [score, reblur] per level, per frame.
    let mut vals = vec![[Vec::new(), Vec::new()]; levels.len()];
    let (mut dims, mut shrinks) = (std::collections::BTreeSet::new(), Vec::new());
    let (mut t_measure, mut t_native) = (std::time::Duration::ZERO, std::time::Duration::ZERO);
    for item in &list {
        let frame = raw::read(&dir.join(item["file"].as_str().unwrap())).unwrap();
        let image = Rgb::decode_jpeg(&frame.preview, 1)
            .unwrap()
            .orient(frame.orientation);
        dims.insert((image.width, image.height));
        let b: Vec<f64> = (0..4).map(|i| item["box"][i].as_f64().unwrap()).collect();
        let bbox = [b[0], b[1], b[2], b[3]];
        let crop_box = detect::expand_box(bbox, image.width, image.height, &opts);
        // Blur a margin-padded window; it equals blurring the whole preview.
        let m = levels.iter().map(|l| l.1.reach()).max().unwrap();
        let x0 = (crop_box[0] as usize).saturating_sub(m);
        let y0 = (crop_box[1] as usize).saturating_sub(m);
        let win = image.crop(
            x0,
            y0,
            (crop_box[2] as usize + m).min(image.width),
            (crop_box[3] as usize + m).min(image.height),
        );
        let local = [
            crop_box[0] - x0 as f64,
            crop_box[1] - y0 as f64,
            crop_box[2] - x0 as f64,
            crop_box[3] - y0 as f64,
        ];
        let subject = [
            bbox[0] - x0 as f64,
            bbox[1] - y0 as f64,
            bbox[2] - x0 as f64,
            bbox[3] - y0 as f64,
        ];
        for (n, (_, blur)) in levels.iter().enumerate() {
            let blurred = blur.apply(&win);
            let crop = cut(&blurred, local, &cfg);
            let scale = crop.width as f64 / (local[2] - local[0]);
            let inner = [
                (subject[0] - local[0]) * scale,
                (subject[1] - local[1]) * scale,
                (subject[2] - local[0]) * scale,
                (subject[3] - local[1]) * scale,
            ];
            if n == 0 {
                shrinks.push(
                    ((crop_box[2] - crop_box[0]).max(crop_box[3] - crop_box[1])
                        / sharpness::WORKING_EDGE as f64)
                        .max(1.0),
                );
            }
            let t = std::time::Instant::now();
            let mut focus = sharpness::measure(&crop.to_gray(), Some(inner), None);
            t_measure += t.elapsed();
            let t = std::time::Instant::now();
            let [a, b, c, d] = subject.map(|v| v.max(0.0) as usize);
            let reblur = reblur_ratio(&blurred.crop(a, b, c, d).to_gray());
            t_native += t.elapsed();
            focus.score = focus.score.max(0.0);
            vals[n][0].push(focus.score);
            vals[n][1].push(reblur);
        }
    }
    let frames = list.len() * levels.len();
    println!("preview dims: {dims:?}");
    shrinks.sort_by(f64::total_cmp);
    let n = shrinks.len();
    println!(
        "crop box long edge / 1024: min {:.2} median {:.2} max {:.2} (n={n})",
        shrinks[0],
        shrinks[n / 2],
        shrinks[n - 1]
    );
    println!(
        "time per frame-level: measure {:?}, native features {:?}",
        t_measure / frames as u32,
        t_native / frames as u32
    );
    // Separability = (median at 0 - median at level) / IQR at 0, signed so a
    // blur that moves the measure the way blur should is positive (re-blur
    // ratio rises with blur, the others fall). "moved" = by at least half the
    // unblurred IQR.
    let names = ["score", "reblur"];
    for (k, name) in names.iter().enumerate() {
        let sign = if k == 1 { -1.0 } else { 1.0 };
        let base = &vals[0][k];
        let iqr = percentile_of(base, 75.0) - percentile_of(base, 25.0);
        println!(
            "
{name}: IQR at blur 0 = {iqr:.4}, nan frames = {}",
            base.iter().filter(|v| v.is_nan()).count()
        );
        println!(
            "{:<12} {:>9} {:>8} {:>9}",
            "level", "median", "moved", "separab."
        );
        for (n, (label, _)) in levels.iter().enumerate() {
            let v = &vals[n][k];
            let moved = v
                .iter()
                .zip(base)
                .filter(|(a, b)| sign * (**b - **a) >= iqr / 2.0)
                .count();
            println!(
                "{:<12} {:>9.4} {:>7}% {:>9.2}",
                label,
                percentile_of(v, 50.0),
                100 * moved / v.len(),
                sign * (percentile_of(base, 50.0) - percentile_of(v, 50.0)) / iqr
            );
        }
    }
}

/// `p`th percentile (nearest rank) of the finite values.
fn percentile_of(v: &[f64], p: f64) -> f64 {
    let mut v: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p / 100.0).round() as usize]
}

fn jpeg(img: &Rgb) -> Vec<u8> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode(
            &img.data,
            img.width as u32,
            img.height as u32,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    out
}

/// The 100% patch of `subject` (up to 400 px square) with the most contrast.
fn busiest_patch(subject: &Rgb) -> Rgb {
    let gray = subject.to_gray();
    let (pw, ph) = (400.min(gray.width), 400.min(gray.height));
    let (mut best, mut at) = (-1.0, (0, 0));
    for y in (0..=gray.height - ph).step_by(100) {
        for x in (0..=gray.width - pw).step_by(100) {
            let (mut sum, mut sq) = (0.0, 0.0);
            for row in y..y + ph {
                for &v in &gray.data[row * gray.width + x..row * gray.width + x + pw] {
                    sum += f64::from(v);
                    sq += f64::from(v) * f64::from(v);
                }
            }
            let n = (pw * ph) as f64;
            let var = sq / n - (sum / n) * (sum / n);
            if var > best {
                (best, at) = (var, (x, y));
            }
        }
    }
    subject.crop(at.0, at.1, at.0 + pw, at.1 + ph)
}

/// Gradient energy (squared central differences, x plus y) over the interior,
/// 4 px in from the edge.
fn gradient_energy(p: &[f32], w: usize, h: usize) -> f64 {
    let mut sum = 0.0;
    for y in 4..h - 4 {
        for x in 4..w - 4 {
            let dx = p[y * w + x + 1] - p[y * w + x - 1];
            let dy = p[(y + 1) * w + x] - p[(y - 1) * w + x];
            sum += f64::from(dx * dx + dy * dy);
        }
    }
    sum / ((w - 8) * (h - 8)) as f64
}

/// Isotropic re-blur ratio over the whole of `g`: gradient energy after a
/// sigma-1 Gaussian over the energy before. Near 1 there was nothing finer than
/// the blur to lose (soft); lower is sharper. NaN if `g` is under 16 px a side.
fn reblur_ratio(g: &Gray) -> f64 {
    let (w, h) = (g.width, g.height);
    if w < 16 || h < 16 {
        return f64::NAN;
    }
    let src: Vec<f32> = g.data.iter().map(|&v| f32::from(v)).collect();
    let k = gaussian(1.0);
    let r = (k.len() / 2) as isize;
    let pass = |src: &[f32], vertical: bool| -> Vec<f32> {
        let mut out = vec![0f32; src.len()];
        for y in 0..h {
            for x in 0..w {
                out[y * w + x] = k
                    .iter()
                    .enumerate()
                    .map(|(i, wt)| {
                        let o = i as isize - r;
                        let (sx, sy) = if vertical {
                            (x, (y as isize + o).clamp(0, h as isize - 1) as usize)
                        } else {
                            ((x as isize + o).clamp(0, w as isize - 1) as usize, y)
                        };
                        wt * src[sy * w + sx]
                    })
                    .sum();
            }
        }
        out
    };
    let blurred = pass(&pass(&src, false), true);
    gradient_energy(&blurred, w, h) / gradient_energy(&src, w, h).max(1e-9)
}

struct Rated {
    file: String,
    bbox: [f64; 4],
    cls: String,
    old: f64,
    tile: Vec<u8>,
    patch: Vec<u8>,
}

/// Each `{"file", "box", ..}` of `dir/boxes.json` with its frame decoded as
/// `cull_frame` does.
fn subjects_of(dir: &Path) -> Vec<(serde_json::Value, Rgb, [f64; 4])> {
    let list: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(dir.join("boxes.json")).unwrap()).unwrap();
    list.into_iter()
        .map(|item| {
            let frame = raw::read(&dir.join(item["file"].as_str().unwrap())).unwrap();
            let image = Rgb::decode_jpeg(&frame.preview, 1)
                .unwrap()
                .orient(frame.orientation);
            let b: Vec<f64> = (0..4).map(|i| item["box"][i].as_f64().unwrap()).collect();
            (item, image, [b[0], b[1], b[2], b[3]])
        })
        .collect()
}

/// Sweep the subject's long edge: `CONROD_BLURTEST_DIR/<CONROD_SWEEP_SET>`
/// (default sweep64; or review2) holds the CR3s and boxes.json (with "number").
/// Each subject box is cut from the full preview and Lanczos-resized so its long
/// edge is S (never upscaled), then measured three ways. Writes sweep.csv beside
/// that set's review sheet; `target/sharpness-review/sweep.py` reads it.
#[test]
#[ignore]
fn size_sweep() {
    let set = std::env::var("CONROD_SWEEP_SET").unwrap_or("sweep64".into());
    let dir = PathBuf::from(std::env::var("CONROD_BLURTEST_DIR").expect("CONROD_BLURTEST_DIR"))
        .join(&set);
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join(if set == "review2" {
        "../../target/sharpness-review-2"
    } else {
        "../../target/sharpness-review"
    });
    let mut csv = String::from("number,file,S,capped,measured,score,fft_hf,reblur\n");
    for (n, (item, image, bbox)) in subjects_of(&dir).into_iter().enumerate() {
        let number = item["number"].as_u64().unwrap_or(n as u64 + 1);
        let [a, b, c, d] = bbox.map(|v| v.max(0.0) as usize);
        let subject = image.crop(a, b, c, d);
        let long = subject.width.max(subject.height);
        for s in [192usize, 256, 384, 512, 768, 1024] {
            let capped = long <= s;
            let cut = if capped {
                subject.clone()
            } else {
                let f = s as f64 / long as f64;
                subject.resize(
                    ((subject.width as f64 * f).round() as usize).max(1),
                    ((subject.height as f64 * f).round() as usize).max(1),
                    Filter::Lanczos,
                )
            };
            let gray = cut.to_gray();
            let whole = [0.0, 0.0, cut.width as f64, cut.height as f64];
            let m = sharpness::measure(&gray, Some(whole), None);
            let fft = m.features.get(18).copied().unwrap_or(f64::NAN);
            csv += &format!(
                "{number},{},{s},{},{},{:.6},{:.6},{:.6}\n",
                item["file"].as_str().unwrap(),
                u8::from(capped),
                u8::from(m.measured),
                m.score,
                fft,
                reblur_ratio(&gray)
            );
        }
    }
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("sweep.csv"), csv).unwrap();
    println!("sweep.csv written to {}", out.display());
}

/// A blind review sheet of `CONROD_BLURTEST_DIR/review2` (CR3s and boxes.json
/// with "cls", already in the order to show) at `target/sharpness-review-2`:
/// numbered tiles with no scores or stars on the page, and ratings.csv with
/// them for afterwards.
#[test]
#[ignore]
fn review_sheet() {
    let dir = PathBuf::from(std::env::var("CONROD_BLURTEST_DIR").expect("CONROD_BLURTEST_DIR"))
        .join("review2");
    let cfg = Settings::default();
    let opts = DetectOptions {
        crop_padding: cfg.crop_padding,
        dominant_subject_fraction: cfg.dominant_subject_fraction,
        ..Default::default()
    };
    let mut rated = Vec::new();
    for (item, image, bbox) in subjects_of(&dir) {
        let crop_box = detect::expand_box(bbox, image.width, image.height, &opts);
        let crop = cut(&image, crop_box, &cfg);
        let scale = crop.width as f64 / (crop_box[2] - crop_box[0]);
        let inner = [
            (bbox[0] - crop_box[0]) * scale,
            (bbox[1] - crop_box[1]) * scale,
            (bbox[2] - crop_box[0]) * scale,
            (bbox[3] - crop_box[1]) * scale,
        ];
        let old = sharpness::measure(&crop.to_gray(), Some(inner), None);
        let [a, b, c, d] = bbox.map(|v| v.max(0.0) as usize);
        let subject = image.crop(a, b, c, d);
        let h = (480 * subject.height / subject.width.max(1)).max(1);
        rated.push(Rated {
            file: item["file"].as_str().unwrap().to_string(),
            bbox,
            cls: item["cls"].as_str().unwrap_or_default().to_string(),
            old: old.score,
            tile: jpeg(&subject.resize(480, h, Filter::Lanczos)),
            patch: jpeg(&busiest_patch(&subject)),
        });
    }

    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sharpness-review-2");
    std::fs::create_dir_all(&out).unwrap();
    let mut html = String::from(
        "<!doctype html><meta charset=utf-8><title>Sharpness review 2</title><style>\
         body{font:14px sans-serif;margin:16px}.s{display:inline-block;vertical-align:top;margin:0 12px 16px 0;border:1px solid #888;padding:6px}\
         img{display:block;margin-bottom:4px}</style><h1>Sharpness review 2</h1>\
         <p>Each: whole subject (shrunk), then a 100% patch from its busiest part.</p>",
    );
    let mut csv = String::from("number,file,x1,y1,x2,y2,cls,old_score,old_stars\n");
    for (i, r) in rated.iter().enumerate() {
        let n = i + 1;
        std::fs::write(out.join(format!("{n}_subject.jpg")), &r.tile).unwrap();
        std::fs::write(out.join(format!("{n}_patch.jpg")), &r.patch).unwrap();
        html += &format!(
            "<div class=s><b>#{n}</b> {}<br><img src={n}_subject.jpg><img src={n}_patch.jpg></div>\n",
            r.file
        );
        csv += &format!(
            "{n},{},{:.0},{:.0},{:.0},{:.0},{},{:.4},{}\n",
            r.file,
            r.bbox[0],
            r.bbox[1],
            r.bbox[2],
            r.bbox[3],
            r.cls,
            r.old,
            sharpness::stars_for(r.old)
        );
    }
    // score.js (copied beside the sheet, not generated) adds the 1-5 buttons.
    std::fs::write(out.join("index.html"), html + "\n<script src=score.js></script>\n").unwrap();
    std::fs::write(out.join("ratings.csv"), csv).unwrap();
    println!("sheet: {} subjects at {}", rated.len(), out.display());
}
