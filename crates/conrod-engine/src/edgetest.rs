//! Labelled experiment: does a native-pixel blur estimate separate good frames
//! from blurred ones better than the current tile-based sharpness score?
//!
//! Frames are the ones the photographer delivered (`exported: true` in
//! labels.json), taken round-robin across shoots: human-confirmed sharp. Each
//! is blurred synthetically in native pixels -- defocus, camera shake, and a
//! panned background -- and both measures are asked to rank them.
//!
//! `CONROD_EVAL_LABELS` (labels.json), `CONROD_EVAL_MAX` (120 frames).
//! Writes `target/eval/edgeblur/variants.csv`.
//! `rtk cargo test -p conrod-engine --release edge_blur_experiment -- --ignored --nocapture`
//!
//! Nothing is written beside the originals; the shoot folders are snapshotted
//! before and after, as `evaltest.rs` does.

use super::*;
use conrod_vision::edgeblur::{self, EdgeBlur};
use conrod_vision::imageops::Gray;
use std::time::Instant;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn gray_crop(g: &Gray, [x0, y0, x1, y1]: [usize; 4]) -> Gray {
    let x1 = x1.min(g.width);
    let y1 = y1.min(g.height);
    let x0 = x0.min(x1);
    let y0 = y0.min(y1);
    let mut data = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        data.extend_from_slice(&g.data[y * g.width + x0..y * g.width + x1]);
    }
    Gray::new(x1 - x0, y1 - y0, data)
}

/// `cut` from `lib.rs`, on grey: crop, then Lanczos only to keep the long edge
/// within the configured crop limits.
fn cut_gray(g: &Gray, b: [f64; 4], cfg: &Settings) -> Gray {
    let [x0, y0, x1, y1] = b.map(|v| v as usize);
    let crop = gray_crop(g, [x0, y0, x1, y1]);
    if crop.width == 0 || crop.height == 0 {
        return crop;
    }
    let longest = crop.width.max(crop.height) as f64;
    let (lo, hi) = (cfg.crop_min_edge as f64, cfg.crop_max_edge as f64);
    let scale = if longest < lo {
        lo / longest
    } else if longest > hi {
        hi / longest
    } else {
        return crop;
    };
    let w = ((crop.width as f64 * scale) as usize).max(1);
    let h = ((crop.height as f64 * scale) as usize).max(1);
    crop.resize(w, h, Filter::Lanczos)
}

fn gauss_kernel(sigma: f32) -> Vec<f32> {
    let r = (sigma * 3.0).ceil() as isize;
    let k: Vec<f32> = (-r..=r)
        .map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = k.iter().sum();
    k.iter().map(|v| v / sum).collect()
}

fn gauss_gray(g: &Gray, sigma: f32) -> Gray {
    let (w, h) = (g.width, g.height);
    let k = gauss_kernel(sigma);
    let r = (k.len() / 2) as isize;
    let src: Vec<f32> = g.data.iter().map(|&v| f32::from(v)).collect();
    let mut tmp = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (i, &wt) in k.iter().enumerate() {
                let sx = (x as isize + i as isize - r).clamp(0, w as isize - 1) as usize;
                acc += wt * src[y * w + sx];
            }
            tmp[y * w + x] = acc;
        }
    }
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let mut acc = 0.0;
            for (i, &wt) in k.iter().enumerate() {
                let sy = (y as isize + i as isize - r).clamp(0, h as isize - 1) as usize;
                acc += wt * tmp[sy * w + x];
            }
            out.push(acc.round().clamp(0.0, 255.0) as u8);
        }
    }
    Gray::new(w, h, out)
}

/// Horizontal box of `len` px, row by row. `keep` is copied through untouched:
/// the panned background leaves the subject alone.
fn shake_gray(g: &Gray, len: usize, keep: Option<[usize; 4]>) -> Gray {
    let (w, h) = (g.width, g.height);
    let len = len.max(1);
    let mut out = g.data.clone();
    for y in 0..h {
        let row = &g.data[y * w..(y + 1) * w];
        for x in 0..w {
            if keep.is_some_and(|[a, b, c, d]| x >= a && x < c && y >= b && y < d) {
                continue;
            }
            let mut acc = 0u32;
            for i in 0..len {
                let sx = (x as isize + i as isize - len as isize / 2).clamp(0, w as isize - 1);
                acc += u32::from(row[sx as usize]);
            }
            out[y * w + x] = ((acc + len as u32 / 2) / len as u32) as u8;
        }
    }
    Gray::new(w, h, out)
}

fn percentile(v: &[f64], p: f64) -> f64 {
    let mut v: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(f64::total_cmp);
    v[((v.len() - 1) as f64 * p / 100.0).round() as usize]
}

fn mean(v: &[f64]) -> f64 {
    let v: Vec<f64> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        f64::NAN
    } else {
        v.iter().sum::<f64>() / v.len() as f64
    }
}

/// Mann-Whitney AUC: P(a positive outranks a negative), ties half. NaN drops.
fn auc(pos: &[f64], neg: &[f64]) -> f64 {
    let pos: Vec<f64> = pos.iter().copied().filter(|v| v.is_finite()).collect();
    let neg: Vec<f64> = neg.iter().copied().filter(|v| v.is_finite()).collect();
    if pos.is_empty() || neg.is_empty() {
        return f64::NAN;
    }
    let mut all: Vec<(f64, bool)> = pos
        .iter()
        .map(|&v| (v, true))
        .chain(neg.iter().map(|&v| (v, false)))
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut i = 0;
    let mut rank_sum = 0.0;
    while i < all.len() {
        let mut j = i;
        while j + 1 < all.len() && all[j + 1].0 == all[i].0 {
            j += 1;
        }
        let shared = ((i + 1) as f64 + (j + 1) as f64) / 2.0;
        for item in &all[i..=j] {
            if item.1 {
                rank_sum += shared;
            }
        }
        i = j + 1;
    }
    let n = pos.len() as f64;
    (rank_sum - n * (n + 1.0) / 2.0) / (n * neg.len() as f64)
}

/// The threshold on `values` (higher = positive) with the best balanced
/// accuracy over `pos` and `neg`.
fn best_threshold(pos: &[f64], neg: &[f64]) -> (f64, f64) {
    let mut cand: Vec<f64> = pos
        .iter()
        .chain(neg)
        .copied()
        .filter(|v| v.is_finite())
        .collect();
    cand.sort_by(f64::total_cmp);
    cand.dedup();
    let mut best = (f64::NAN, -1.0);
    for &t in &cand {
        let tp = pos.iter().filter(|v| **v >= t).count() as f64;
        let fp = neg.iter().filter(|v| **v >= t).count() as f64;
        let tpr = tp / pos.len().max(1) as f64;
        let tnr = 1.0 - fp / neg.len().max(1) as f64;
        let bal = (tpr + tnr) / 2.0;
        if bal > best.1 {
            best = (t, bal);
        }
    }
    best
}

fn rate(flags: &[bool]) -> f64 {
    if flags.is_empty() {
        0.0
    } else {
        flags.iter().filter(|&&f| f).count() as f64 / flags.len() as f64
    }
}

struct Row {
    file: String,
    shoot: String,
    frame: usize,
    subject_box: bool,
    box_fraction: f64,
    variant: &'static str,
    score: f64,
    background: f64,
    panning: bool,
    inside: EdgeBlur,
    outside: Option<EdgeBlur>,
    ms: f64,
}

/// Names, sizes and mtimes of everything in `dir`, as `evaltest::snapshot`.
fn snapshot(dir: &Path) -> Vec<(String, u64, u128)> {
    let mut out: Vec<(String, u64, u128)> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| {
                    let meta = e.metadata().ok();
                    let mtime = meta
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos());
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        meta.map_or(0, |m| m.len()),
                        mtime,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn fig(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.3}")
    } else {
        "-".into()
    }
}

#[test]
#[ignore]
fn edge_blur_experiment() {
    let labels_path = std::env::var("CONROD_EVAL_LABELS").unwrap_or_else(|_| {
        String::from(r"E:\code\photo\conrod\target\groundtruth\labels.json")
    });
    let max = env_usize("CONROD_EVAL_MAX", 120);
    let labels: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&labels_path).expect("labels.json"))
            .expect("labels.json is JSON");
    let frames = labels["frames"].as_array().expect("frames");

    // Exported frames per shoot, in file order, then round-robin across shoots.
    type Shoot = (String, Vec<(String, PathBuf)>);
    let mut shoots: Vec<Shoot> = Vec::new();
    for f in frames {
        if f["exported"].as_bool() != Some(true) {
            continue;
        }
        let (Some(shoot), Some(path)) = (f["shoot"].as_str(), f["path"].as_str()) else {
            continue;
        };
        let name = Path::new(shoot)
            .file_name()
            .map_or_else(|| shoot.to_string(), |n| n.to_string_lossy().into_owned());
        let file = PathBuf::from(path);
        match shoots.iter_mut().find(|(s, _)| s == shoot) {
            Some((_, v)) => v.push((name, file)),
            None => shoots.push((shoot.to_string(), vec![(name, file)])),
        }
    }
    let mut order: Vec<(String, PathBuf)> = Vec::new();
    for round in 0..shoots.iter().map(|(_, v)| v.len()).max().unwrap_or(0) {
        for (_, v) in &shoots {
            if let Some(item) = v.get(round) {
                order.push(item.clone());
            }
        }
    }
    assert!(!order.is_empty(), "no exported frame in {labels_path}");

    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/eval/edgeblur");
    let target = out_dir.to_string_lossy().replace('\\', "/").to_lowercase();
    assert!(
        target.contains("/target/eval/"),
        "refusing to write to {}",
        out_dir.display()
    );

    let mut watched: Vec<PathBuf> = order
        .iter()
        .filter_map(|(_, p)| p.parent().map(Path::to_path_buf))
        .collect();
    watched.sort();
    watched.dedup();
    type Snap = (PathBuf, Vec<(String, u64, u128)>);
    let before: Vec<Snap> = watched.iter().map(|d| (d.clone(), snapshot(d))).collect();

    let detector = Mutex::new(
        Detector::load(&detector_model(), Device::Auto).expect("load the detector"),
    );
    println!("device {}", lock(&detector).device);
    let dopts = DetectOptions {
        classes: (0..80).collect(),
        conf: 0.01,
        ..Default::default()
    };

    let cfg = Settings::default();
    let crop_opts = DetectOptions {
        crop_padding: cfg.crop_padding,
        dominant_subject_fraction: cfg.dominant_subject_fraction,
        ..Default::default()
    };
    // Margin the synthesised blur needs to be whole: the largest reach (a 32 px
    // box, half of it either side) plus room for the background estimate.
    let reach = 34;

    let mut rows: Vec<Row> = Vec::new();
    let mut dims = std::collections::BTreeSet::new();
    let started = Instant::now();
    let mut read = 0;
    for (shoot, path) in &order {
        if read >= max {
            break;
        }
        let frame = match raw::read(path) {
            Ok(f) => f,
            Err(e) => {
                println!("skip {}: {e}", path.display());
                continue;
            }
        };
        let image = match Rgb::decode_jpeg(&frame.preview, 1) {
            Ok(i) => i.orient(frame.orientation),
            Err(e) => {
                println!("skip {}: {e}", path.display());
                continue;
            }
        };
        let frame_index = read;
        read += 1;
        dims.insert((image.width, image.height));
        let (iw, ih) = (image.width, image.height);
        let area = (iw * ih) as f64;

        // The highest-confidence detector box that is neither a speck nor the
        // whole frame.
        let found = match lock(&detector).raw(&image, &dopts) {
            Ok(v) => v,
            Err(e) => {
                println!("no detections for {}: {e}", path.display());
                Vec::new()
            }
        };
        let subject = found
            .iter()
            .filter(|(_, conf, _)| *conf >= 0.25)
            .filter_map(|(_, conf, b)| {
                let b = [
                    b[0].clamp(0.0, iw as f64),
                    b[1].clamp(0.0, ih as f64),
                    b[2].clamp(0.0, iw as f64),
                    b[3].clamp(0.0, ih as f64),
                ];
                let frac =
                    ((b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0)) / area;
                (0.005..=0.8).contains(&frac).then_some((*conf, b, frac))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0));

        let gray = image.to_gray();
        drop(image);
        let (bbox, box_fraction) = match subject {
            Some((_, b, frac)) => (b, frac),
            None => ([0.0, 0.0, iw as f64, ih as f64], 0.0),
        };
        let has_box = subject.is_some();
        let crop_box = if has_box {
            detect::expand_box(bbox, iw, ih, &crop_opts)
        } else {
            [0.0, 0.0, iw as f64, ih as f64]
        };

        // The window the blur is applied to: the production crop plus its
        // margin, plus a band of background for the outside measurement.
        let (cw, ch) = (crop_box[2] - crop_box[0], crop_box[3] - crop_box[1]);
        let extra = ((cw.max(ch) * 0.15) as usize).min(400);
        let m = reach + extra;
        let wx0 = (crop_box[0] as usize).saturating_sub(m);
        let wy0 = (crop_box[1] as usize).saturating_sub(m);
        let wx1 = ((crop_box[2] as usize) + m).min(iw);
        let wy1 = ((crop_box[3] as usize) + m).min(ih);
        let win = gray_crop(&gray, [wx0, wy0, wx1, wy1]);
        drop(gray);

        let local = [
            crop_box[0] - wx0 as f64,
            crop_box[1] - wy0 as f64,
            crop_box[2] - wx0 as f64,
            crop_box[3] - wy0 as f64,
        ];
        let subject_win = [
            (bbox[0] as usize).saturating_sub(wx0).min(win.width),
            (bbox[1] as usize).saturating_sub(wy0).min(win.height),
            (bbox[2] as usize).saturating_sub(wx0).min(win.width),
            (bbox[3] as usize).saturating_sub(wy0).min(win.height),
        ];

        let mut variants: Vec<(&'static str, u8, usize)> = vec![
            ("orig", 0, 0),
            ("gauss1", 1, 0),
            ("gauss2", 2, 0),
            ("gauss4", 3, 0),
            ("shake8", 4, 8),
            ("shake16", 4, 16),
            ("shake32", 4, 32),
        ];
        if has_box {
            variants.push(("pan32", 5, 32));
        }
        for (name, kind, param) in variants {
            let blurred = match kind {
                0 => win.clone(),
                1..=3 => gauss_gray(&win, [1.0, 2.0, 4.0][kind as usize - 1]),
                4 => shake_gray(&win, param, None),
                _ => shake_gray(&win, param, Some(subject_win)),
            };
            let crop = cut_gray(&blurred, local, &cfg);
            let scale = crop.width as f64 / (local[2] - local[0]).max(1.0);
            let inner = [
                (bbox[0] - local[0]) * scale,
                (bbox[1] - local[1]) * scale,
                (bbox[2] - local[0]) * scale,
                (bbox[3] - local[1]) * scale,
            ];
            let focus = sharpness::measure(&crop, Some(inner), None);

            let t = Instant::now();
            let inside = if has_box {
                edgeblur::measure(&blurred, Some(subject_win))
            } else {
                edgeblur::measure(&blurred, None)
            };
            let outside = has_box.then(|| edgeblur::measure_outside(&blurred, subject_win));
            let ms = t.elapsed().as_secs_f64() * 1000.0;

            rows.push(Row {
                file: path.to_string_lossy().into_owned(),
                shoot: shoot.clone(),
                frame: frame_index,
                subject_box: has_box,
                box_fraction,
                variant: name,
                score: focus.score.max(0.0),
                background: focus.background,
                panning: focus.panning,
                inside,
                outside,
                ms,
            });
        }
        if read % 10 == 0 {
            println!(
                "… {read} frames, {} rows, {:.0} s",
                rows.len(),
                started.elapsed().as_secs_f64()
            );
        }
    }

    println!("preview dims: {dims:?}");
    let frames_seen = read;
    let with_box = rows
        .iter()
        .filter(|r| r.subject_box)
        .map(|r| r.frame)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    println!(
        "frames: {frames_seen}, rows: {}, with a subject box: {with_box}",
        rows.len()
    );

    std::fs::create_dir_all(&out_dir).expect("create target/eval/edgeblur");
    let mut csv = String::from(
        "file,shoot,box,box_fraction,variant,score,background,panning,in_sigma,in_best,in_worst,in_worst_dir,in_edges,out_sigma,out_best,out_worst,out_worst_dir,out_edges,ms\n",
    );
    for r in &rows {
        let o = r.outside.unwrap_or(EdgeBlur {
            sigma: f64::NAN,
            sigma_best: f64::NAN,
            sigma_worst: f64::NAN,
            worst_dir: 0,
            dirs: [f64::NAN; 4],
            edges: 0,
            contrast: f64::NAN,
        });
        csv += &format!(
            "\"{}\",\"{}\",{},{:.5},{},{:.5},{:.4},{},{:.4},{:.4},{:.4},{},{},{:.4},{:.4},{:.4},{},{},{:.1}\n",
            r.file.replace('"', "\"\""),
            r.shoot.replace('"', "\"\""),
            u8::from(r.subject_box),
            r.box_fraction,
            r.variant,
            r.score,
            r.background,
            u8::from(r.panning),
            r.inside.sigma,
            r.inside.sigma_best,
            r.inside.sigma_worst,
            r.inside.worst_dir,
            r.inside.edges,
            o.sigma,
            o.sigma_best,
            o.sigma_worst,
            o.worst_dir,
            o.edges,
            r.ms
        );
    }
    std::fs::write(out_dir.join("variants.csv"), &csv).expect("write variants.csv");
    println!("csv: {}", out_dir.join("variants.csv").display());

    let variant_names = [
        "orig", "gauss1", "gauss2", "gauss4", "shake8", "shake16", "shake32", "pan32",
    ];
    let of = |names: &[&str]| -> Vec<&Row> {
        rows.iter()
            .filter(|r| names.contains(&r.variant))
            .collect()
    };
    let metric = |v: &[&Row], f: &dyn Fn(&Row) -> f64| -> Vec<f64> {
        v.iter().map(|r| f(r)).collect()
    };

    // 1. Per variant: median [p10, p90].
    println!("\n1. per variant: median [p10, p90]");
    println!(
        "{:<8} {:>4}  {:>24}  {:>24}  {:>24}  {:>24}",
        "variant", "n", "score", "inside sigma", "inside sigma_worst", "worst/best"
    );
    for name in variant_names {
        let v = of(&[name]);
        if v.is_empty() {
            continue;
        }
        let show = |vals: Vec<f64>| {
            format!(
                "{} [{}, {}]",
                fig(percentile(&vals, 50.0)),
                fig(percentile(&vals, 10.0)),
                fig(percentile(&vals, 90.0))
            )
        };
        println!(
            "{:<8} {:>4}  {:>24}  {:>24}  {:>24}  {:>24}",
            name,
            v.len(),
            show(metric(&v, &|r| r.score)),
            show(metric(&v, &|r| r.inside.sigma)),
            show(metric(&v, &|r| r.inside.sigma_worst)),
            show(metric(&v, &|r| r.inside.sigma_worst / r.inside.sigma_best)),
        );
    }

    // 2. AUC of the two families, and of the mild cases.
    let pos2: Vec<&Row> = of(&["orig", "pan32"]);
    let neg2: Vec<&Row> = of(&["gauss2", "gauss4", "shake16", "shake32"]);
    let mild_pos: Vec<&Row> = of(&["orig"]);
    let mild_neg: Vec<&Row> = of(&["gauss1", "shake8"]);
    fn within<'a>(v: &[&'a Row], want: bool) -> Vec<&'a Row> {
        v.iter().copied().filter(|r| r.subject_box == want).collect()
    }
    let scores = |v: &[&Row]| metric(v, &|r| r.score);
    let neg_sigma = |v: &[&Row]| metric(v, &|r| -r.inside.sigma);
    let neg_worst = |v: &[&Row]| metric(v, &|r| -r.inside.sigma_worst);
    println!("\n2. ROC AUC, positives {{orig, pan32}} vs negatives {{gauss2, gauss4, shake16, shake32}}");
    println!(
        "{:<26} {:>19} {:>19} {:>19}",
        "metric", "all", "box=1", "box=0"
    );
    for (label, f) in [
        ("current score", &scores as &dyn Fn(&[&Row]) -> Vec<f64>),
        ("-sigma", &neg_sigma),
        ("-sigma_worst", &neg_worst),
    ] {
        let mut cells = Vec::new();
        for subset in [None, Some(true), Some(false)] {
            let (p, n) = match subset {
                None => (pos2.clone(), neg2.clone()),
                Some(b) => (within(&pos2, b), within(&neg2, b)),
            };
            let a = auc(&f(&p), &f(&n));
            cells.push(format!(
                "{:.3} ({}/{})",
                a,
                p.len(),
                n.len()
            ));
        }
        println!("{label:<26} {:>19} {:>19} {:>19}", cells[0], cells[1], cells[2]);
    }
    println!(
        "mild, {{orig}} vs {{gauss1, shake8}}: score {:.3}, -sigma {:.3}, -sigma_worst {:.3}",
        auc(&scores(&mild_pos), &scores(&mild_neg)),
        auc(&neg_sigma(&mild_pos), &neg_sigma(&mild_neg)),
        auc(&neg_worst(&mild_pos), &neg_worst(&mild_neg))
    );

    // 3. Best balanced-accuracy threshold, and what it does per variant.
    println!("\n3. threshold with the best balanced accuracy on set 2");
    let negatives = ["gauss2", "gauss4", "shake16", "shake32"];
    let mut sigma_t = f64::NAN;
    for (label, f) in [
        ("current score", &scores as &dyn Fn(&[&Row]) -> Vec<f64>),
        ("-sigma_worst", &neg_worst),
    ] {
        let (t, bal) = best_threshold(&f(&pos2), &f(&neg2));
        // `t` is a floor on the metric as listed; -sigma_worst is sigma <= -t.
        let shown = if label == "-sigma_worst" { -t } else { t };
        println!("{label}: threshold {shown:.3}, balanced accuracy {bal:.3}");
        if label == "-sigma_worst" {
            sigma_t = shown;
        }
        let tpr = |v: &[&Row]| rate(&f(v).iter().map(|x| *x >= t).collect::<Vec<_>>());
        print!("    TPR orig {:.2}", tpr(&of(&["orig"])));
        print!(", pan32 {:.2}", tpr(&of(&["pan32"])));
        for n in negatives {
            print!(", FPR {n} {:.2}", tpr(&of(&[n])));
        }
        println!();
    }

    // 4. Panning: a smeared background behind a sharp subject.
    println!("\n4. pan detection (outside sigma_worst/sigma_best >= 2 and inside sigma_worst <= {sigma_t:.3})");
    let pan_rule = |r: &Row| -> bool {
        r.outside.is_some_and(|o| {
            o.sigma_worst / o.sigma_best >= 2.0
                && r.inside.sigma_worst.is_finite()
                && r.inside.sigma_worst <= sigma_t
        })
    };
    for name in ["pan32", "orig", "shake32"] {
        let v = of(&[name]);
        println!(
            "{name:<8} n={:<4} new rule {:.2}, current panning flag {:.2}",
            v.len(),
            rate(&v.iter().map(|r| pan_rule(r)).collect::<Vec<_>>()),
            rate(&v.iter().map(|r| r.panning).collect::<Vec<_>>())
        );
    }

    // 5. Cost, and the sharpest volunteers' worst direction.
    let per_frame: Vec<f64> = (0..frames_seen)
        .map(|i| rows.iter().filter(|r| r.frame == i).map(|r| r.ms).sum())
        .collect();
    println!(
        "\n5. new measure: mean {:.1} ms per frame (inside+outside over all variants), {:.1} ms per frame-variant",
        mean(&per_frame),
        mean(&metric(&rows.iter().collect::<Vec<_>>(), &|r| r.ms))
    );
    let mut worst: Vec<&Row> = of(&["orig"])
        .into_iter()
        .filter(|r| r.inside.sigma_worst.is_finite())
        .collect();
    worst.sort_by(|a, b| b.inside.sigma_worst.total_cmp(&a.inside.sigma_worst));
    println!("orig rows with the largest inside sigma_worst:");
    for r in worst.iter().take(10) {
        println!(
            "    {:<28} box {:.3}  sigma {:>6}  worst {:>6} dir {}  score {:.3}",
            Path::new(&r.file)
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            r.box_fraction,
            fig(r.inside.sigma),
            fig(r.inside.sigma_worst),
            r.inside.worst_dir,
            r.score
        );
    }

    for (dir, was) in &before {
        assert_eq!(
            *was,
            snapshot(dir),
            "the shoot folder {} changed during the run",
            dir.display()
        );
    }
    println!(
        "\nsources untouched: {} folder(s), {} file(s) unchanged",
        before.len(),
        before.iter().map(|(_, v)| v.len()).sum::<usize>()
    );
}
