//! Measurement only: is the blur *around* a subject the thing that tells a
//! pan (sharp subject, streaked background) from a static frame?
//!
//! For every labelled frame it decodes the preview the way `cull_frame` does,
//! takes the subject box (given, or the detector's largest vehicle box), and
//! measures three regions with `edgeblur`, all expressed at `REFERENCE_EDGE`:
//!
//! - **inside**: the box itself (`measure`),
//! - **ring**: the box expanded by one width each side and one height above and
//!   below, cropped out and measured with `measure_outside` against the box
//!   relative to that crop -- so only the ring around the subject counts,
//! - **out**: everything in the frame outside the box (`measure_outside`).
//!
//! It also records the current production `panning` flag and subject score for
//! the same box (`sharpness::measure` on the production crop, as
//! `evaltest::measure_box` does).
//!
//! Inputs are `target/pan/{frames,labels,frames_slow,labels_slow}.json`; the
//! write is `target/pan/measure.csv`. Nothing is written beside the originals.
//!
//! `rtk cargo test -p conrod-engine --release pan_measure -- --ignored --nocapture`

use super::*;
use conrod_vision::edgeblur::{self, EdgeBlur};
use conrod_vision::imageops::Gray;
use std::time::Instant;

const NAN: f64 = f64::NAN;
/// Long edge (px) the orientation structure is computed at: 32 px tiles need a
/// plane this size to give a useful number of them, and the estimate is about
/// the arrangement of edges, not their native-pixel width.
const FEATURE_EDGE: usize = 768;

fn unmeasured() -> EdgeBlur {
    EdgeBlur {
        sigma: NAN,
        sigma_best: NAN,
        sigma_worst: NAN,
        worst_dir: 0,
        dirs: [NAN; 4],
        edges: 0,
        contrast: NAN,
    }
}

/// The same estimate in native pixels, re-expressed at `REFERENCE_EDGE` so a
/// 1600 px preview and a 6960 px one are on one scale.
fn referenced(e: EdgeBlur, long: usize) -> EdgeBlur {
    EdgeBlur {
        sigma: edgeblur::at_reference(e.sigma, long),
        sigma_best: edgeblur::at_reference(e.sigma_best, long),
        sigma_worst: edgeblur::at_reference(e.sigma_worst, long),
        worst_dir: e.worst_dir,
        dirs: e.dirs.map(|v| edgeblur::at_reference(v, long)),
        edges: e.edges,
        contrast: e.contrast,
    }
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

/// The box expanded by one width horizontally and one height vertically,
/// clipped to the frame.
fn ring_box(b: [usize; 4], iw: usize, ih: usize) -> Option<[usize; 4]> {
    let [x0, y0, x1, y1] = b;
    let (bw, bh) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    if bw == 0 || bh == 0 {
        return None;
    }
    Some([
        x0.saturating_sub(bw),
        y0.saturating_sub(bh),
        (x1 + bw).min(iw),
        (y1 + bh).min(ih),
    ])
}

/// The ring's edges, measured from outside the box on the expanded region
/// alone. The bool says the expansion covered the whole frame, so what is left
/// is not a ring at all.
fn ring_measure(g: &Gray, b: [usize; 4], long: usize) -> (EdgeBlur, bool) {
    let Some(r) = ring_box(b, g.width, g.height) else {
        return (referenced(unmeasured(), long), true);
    };
    let [rx0, ry0, rx1, ry1] = r;
    if rx1 <= rx0 || ry1 <= ry0 {
        return (referenced(unmeasured(), long), true);
    }
    let clamped = rx0 == 0 && ry0 == 0 && rx1 >= g.width && ry1 >= g.height;
    let win = gray_crop(g, r);
    let [x0, y0, x1, y1] = b;
    let local = [x0 - rx0, y0 - ry0, x1 - rx0, y1 - ry0];
    (referenced(edgeblur::measure_outside(&win, local), long), clamped)
}

/// Production box in pixels, exactly as `cull_frame` clamps it.
fn to_px(b: [f64; 4], w: usize, h: usize) -> [usize; 4] {
    [
        (b[0].max(0.0) as usize).min(w),
        (b[1].max(0.0) as usize).min(h),
        (b[2].max(0.0) as usize).min(w),
        (b[3].max(0.0) as usize).min(h),
    ]
}

/// The detector's largest vehicle-class box at conf >= 0.4, run the way
/// `evaltest::raw_listing` runs it (all 80 classes at a 0.01 floor).
fn detected_box(image: &Rgb, detector: &Mutex<Detector>) -> Option<[f64; 4]> {
    let opts = DetectOptions {
        classes: (0..80).collect(),
        conf: 0.01,
        ..DetectOptions::default()
    };
    let found = match lock(detector).raw(image, &opts) {
        Ok(v) => v,
        Err(e) => {
            println!("detector: {e}");
            return None;
        }
    };
    let (iw, ih) = (image.width as f64, image.height as f64);
    found
        .iter()
        .filter(|(class, conf, _)| detect::VEHICLES.contains(class) && *conf >= 0.4)
        .filter_map(|(_, _, b)| {
            let b = [
                b[0].clamp(0.0, iw),
                b[1].clamp(0.0, ih),
                b[2].clamp(0.0, iw),
                b[3].clamp(0.0, ih),
            ];
            let area = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
            (area > 0.0).then_some((area, b))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, b)| b)
}

/// `cull_frame`'s own crop, scoring and panning decision for one box.
/// Returns `(old tile-based flag, new measured flag, subject score, streaks,
/// learned)`.
fn production(
    image: &Rgb,
    grey: &Gray,
    bbox: [f64; 4],
    settings: &Settings,
    preset: ShootPreset,
    models: &std::collections::HashMap<String, conrod_core::ridge::SharpModel>,
) -> (bool, bool, f64, f64, bool) {
    let opts = options(settings, preset.profile());
    let crop_box = detect::expand_box(bbox, image.width, image.height, &opts);
    if crop_box[2] <= crop_box[0] || crop_box[3] <= crop_box[1] {
        return (false, false, NAN, NAN, false);
    }
    let crop = cut(image, crop_box, settings);
    let scale = crop.width as f64 / (crop_box[2] - crop_box[0]);
    let inner = [
        (bbox[0] - crop_box[0]) * scale,
        (bbox[1] - crop_box[1]) * scale,
        (bbox[2] - crop_box[0]) * scale,
        (bbox[3] - crop_box[1]) * scale,
    ];
    let mut focus = sharpness::measure(&crop.to_gray(), Some(inner), models.get("vehicle"));
    // As `cull_frame` scores a subject: the edge-blur estimate inside the box
    // on native pixels replaces the tile score.
    if !focus.learned {
        let (fw, fh) = (image.width, image.height);
        let region = to_px(bbox, fw, fh);
        let blur = edgeblur::measure(grey, Some(region));
        if let Some(score) = sharpness::rating_from_blur(
            blur.sigma_worst_at_reference(fw.max(fh)),
            sharpness::BlurStrictness::parse(&settings.effective_blur()),
        ) {
            focus.score = score;
        }
    }
    // The old decision, kept for comparison.
    let old = focus.panning && preset.pan_compatible();
    // The new one, exactly as `cull_frame` takes it.
    let streak = conrod_vision::streaks::background_streaks(grey, bbox).unwrap_or(NAN);
    let new = preset.pan_compatible()
        && sharpness::verdict_for(focus.score, settings.sharp_at, settings.blurred_below) != "blurred"
        && streak >= conrod_vision::streaks::PAN_COHERENCE;
    (old, new, focus.score, streak, focus.learned)
}

/// Edge-orientation structure of one region: how much of its edge budget sits
/// in one direction (streaks of a panned background are crisp, but only along
/// the motion), and how coherent its per-tile structure tensors are.
#[derive(Clone, Copy)]
struct Feat {
    /// Edge pixels above the region's own floor, per orientation bin (the same
    /// 0/45/90/135 bins `edgeblur::dir_bin` uses).
    bins: [usize; 4],
    /// Largest bin's share of those edges: 0.25 = no preferred direction.
    dom: f64,
    /// Mean per-tile coherence `(l1 - l2) / (l1 + l2)`.
    coh: f64,
    /// `|mean exp(i 2 theta)|` over tiles, weighted by coherence: 1 = every
    /// tile has the same orientation.
    cons: f64,
    /// That mean orientation, in degrees, in (-90, 90].
    deg: f64,
    /// Tiles that passed the energy floor.
    tiles: usize,
}

fn no_feat() -> Feat {
    Feat {
        bins: [0; 4],
        dom: NAN,
        coh: NAN,
        cons: NAN,
        deg: NAN,
        tiles: 0,
    }
}

/// `k` x `k` area average straight from the grey bytes; the remainder columns
/// and rows are dropped, as in `edgeblur::average_window`.
fn area_avg(g: &Gray, [x0, y0, x1, y1]: [usize; 4], k: usize) -> (Vec<f32>, usize, usize) {
    let (dw, dh) = ((x1 - x0) / k, (y1 - y0) / k);
    let inv = 1.0 / (k * k) as f32;
    let mut out = vec![0f32; dw * dh];
    for by in 0..dh {
        for bx in 0..dw {
            let mut acc = 0u32;
            for yy in 0..k {
                let start = (y0 + by * k + yy) * g.width + x0 + bx * k;
                for &v in &g.data[start..start + k] {
                    acc += u32::from(v);
                }
            }
            out[by * dw + bx] = acc as f32 * inv;
        }
    }
    (out, dw, dh)
}

/// Separable `[0.25, 0.5, 0.25]`: one pixel of Gaussian before the Sobel.
fn smooth_1px(src: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut tmp = vec![0f32; w * h];
    for y in 0..h {
        for x in 0..w {
            let a = src[y * w + x.saturating_sub(1)];
            let c = src[y * w + (x + 1).min(w - 1)];
            tmp[y * w + x] = 0.25 * a + 0.5 * src[y * w + x] + 0.25 * c;
        }
    }
    let mut out = vec![0f32; w * h];
    for y in 0..h {
        let up = y.saturating_sub(1) * w;
        let down = (y + 1).min(h - 1) * w;
        for x in 0..w {
            out[y * w + x] = 0.25 * tmp[up + x] + 0.5 * tmp[y * w + x] + 0.25 * tmp[down + x];
        }
    }
    out
}

fn pick(v: &[f32], p: f32) -> f32 {
    if v.is_empty() {
        return NAN as f32;
    }
    let i = (((v.len() - 1) as f32) * p).round() as usize;
    v[i.min(v.len() - 1)]
}

/// Orientation structure of `region` (and, for a ring, of the part of it
/// outside `exclude`), on a plane whose long edge is at most `FEATURE_EDGE`.
fn features(g: &Gray, region: [usize; 4], exclude: Option<[usize; 4]>) -> Feat {
    let [rx0, ry0, rx1, ry1] = region;
    let (rw, rh) = (rx1.saturating_sub(rx0), ry1.saturating_sub(ry0));
    if rw < 48 || rh < 48 {
        return no_feat();
    }
    let long = rw.max(rh);
    let k = long.div_ceil(FEATURE_EDGE).max(1);
    let (src, w, h) = area_avg(g, region, k);
    if w < 48 || h < 48 {
        return no_feat();
    }
    // The box, in the downscaled plane's coordinates, so the ring's tiles that
    // sit inside it can be dropped.
    let cut = exclude.map(|[a, b, c, d]| {
        [
            a.saturating_sub(rx0) / k,
            b.saturating_sub(ry0) / k,
            c.saturating_sub(rx0).div_ceil(k),
            d.saturating_sub(ry0).div_ceil(k),
        ]
    });

    let p = smooth_1px(&src, w, h);
    drop(src);
    let mut gx = vec![0f32; w * h];
    let mut gy = vec![0f32; w * h];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            let tl = p[i - w - 1];
            let tc = p[i - w];
            let tr = p[i - w + 1];
            let ml = p[i - 1];
            let mr = p[i + 1];
            let bl = p[i + w - 1];
            let bc = p[i + w];
            let br = p[i + w + 1];
            // Sobel / 4, so the magnitudes are on edgeblur's central-difference
            // scale and its edge floor means the same thing here.
            gx[i] = ((tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl)) / 4.0;
            gy[i] = ((bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr)) / 4.0;
        }
    }

    // (a) Edge pixels per orientation bin, above the same kind of floor
    // edgeblur uses: an absolute 2 luma units or 20% of the region's p98.
    let mut mags: Vec<f32> = Vec::with_capacity(w * h);
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            mags.push((gx[i] * gx[i] + gy[i] * gy[i]).sqrt());
        }
    }
    let mut sorted = mags.clone();
    sorted.sort_by(f32::total_cmp);
    let floor = 2.0f32.max(0.2 * pick(&sorted, 0.98));
    let mut bins = [0usize; 4];
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let i = y * w + x;
            if (gx[i] * gx[i] + gy[i] * gy[i]).sqrt() < floor {
                continue;
            }
            if cut.is_some_and(|[a, b, c, d]| x >= a && x < c && y >= b && y < d) {
                continue;
            }
            bins[edgeblur::dir_bin(gx[i], gy[i]) as usize] += 1;
        }
    }
    let total: usize = bins.iter().sum();
    let dom = if total == 0 {
        NAN
    } else {
        *bins.iter().max().unwrap_or(&0) as f64 / total as f64
    };

    // (b) Per-tile structure tensor, tiles 32x32 of the downscaled plane.
    let mut traces: Vec<f32> = Vec::new();
    let mut tensors: Vec<(f32, f32, f32)> = Vec::new();
    let mut ty = 0;
    while ty + 32 <= h {
        let mut tx = 0;
        while tx + 32 <= w {
            // Tiles whose centre falls inside the excluded box are the subject's
            // own edges, not the ring's.
            let (cx, cy) = (tx + 16, ty + 16);
            let skip = cut.is_some_and(|[a, b, c, d]| cx >= a && cx < c && cy >= b && cy < d);
            if !skip {
                let (mut jxx, mut jxy, mut jyy) = (0f32, 0f32, 0f32);
                for y in (ty + 1)..(ty + 31) {
                    for x in (tx + 1)..(tx + 31) {
                        let i = y * w + x;
                        let (a, b) = (gx[i], gy[i]);
                        jxx += a * a;
                        jxy += a * b;
                        jyy += b * b;
                    }
                }
                tensors.push((jxx, jxy, jyy));
                traces.push(jxx + jyy);
            }
            tx += 32;
        }
        ty += 32;
    }
    if tensors.is_empty() {
        return Feat {
            bins,
            dom,
            ..no_feat()
        };
    }
    let mut sorted_traces = traces.clone();
    sorted_traces.sort_by(f32::total_cmp);
    let energy_floor = pick(&sorted_traces, 0.30);

    let (mut coh_sum, mut n, mut z, mut s) = (0f64, 0usize, 0f64, 0f64);
    for (&(jxx, jxy, jyy), &t) in tensors.iter().zip(traces.iter()) {
        if t < energy_floor || jxx + jyy <= 0.0 {
            continue;
        }
        let mean = (jxx + jyy) / 2.0;
        let disc = (((jxx - jyy) / 2.0).powi(2) + jxy * jxy).sqrt();
        let (l1, l2) = (mean + disc, mean - disc);
        if l1 + l2 <= 0.0 {
            continue;
        }
        let c = ((l1 - l2) / (l1 + l2)) as f64;
        let theta: f64 = 0.5 * ((2.0f32 * jxy).atan2(jxx - jyy) as f64);
        coh_sum += c;
        n += 1;
        z += c * (2.0 * theta).cos();
        s += c * (2.0 * theta).sin();
    }
    if n == 0 || coh_sum <= 0.0 {
        return Feat {
            bins,
            dom,
            tiles: n,
            ..no_feat()
        };
    }
    Feat {
        bins,
        dom,
        coh: coh_sum / n as f64,
        cons: z.hypot(s) / coh_sum,
        deg: 0.5 * s.atan2(z).to_degrees(),
        tiles: n,
    }
}

/// Orientation difference folded into 0..90 degrees: orientation has no sign,
/// so 170 and 10 are 20 apart.
fn angle_gap(a: f64, b: f64) -> f64 {
    if !a.is_finite() || !b.is_finite() {
        return NAN;
    }
    let d = (a - b).rem_euclid(180.0);
    d.min(180.0 - d)
}

struct Row {
    id: String,
    set: &'static str,
    label: String,
    shoot: String,
    box_px: Option<[usize; 4]>,
    box_fraction: f64,
    inside: EdgeBlur,
    ring: EdgeBlur,
    out: EdgeBlur,
    panning_old: bool,
    panning_new: bool,
    streaks: f64,
    rating: f64,
    learned: bool,
    inside_feat: Feat,
    ring_feat: Feat,
    /// The ring's expanded region covered the whole frame: no ring to speak of.
    ring_clamped: bool,
    ms_in: f64,
    ms_ring: f64,
    ms_out: f64,
    ms_feat: f64,
    ms: f64,
}

fn fig(v: f64) -> String {
    if v.is_finite() {
        format!("{v:.4}")
    } else {
        "nan".into()
    }
}

fn row_text(r: &Row) -> String {
    let (b, frac) = match r.box_px {
        Some([x0, y0, x1, y1]) => (
            format!("{x0},{y0},{x1},{y1}"),
            format!("{:.5}", r.box_fraction),
        ),
        None => ("-1,-1,-1,-1".to_string(), "nan".to_string()),
    };
    format!(
        "\"{}\",{},\"{}\",\"{}\",{},{}",
        r.id,
        r.set,
        r.label,
        r.shoot.replace('"', "\"\""),
        b,
        frac
    ) + &format!(
        ",{},{},{},{},{},{},{},{},{}",
        fig(r.inside.sigma),
        fig(r.inside.sigma_best),
        fig(r.inside.sigma_worst),
        r.inside.worst_dir,
        fig(r.inside.dirs[0]),
        fig(r.inside.dirs[1]),
        fig(r.inside.dirs[2]),
        fig(r.inside.dirs[3]),
        r.inside.edges
    ) + &format!(
        ",{},{},{},{},{},{},{},{},{}",
        fig(r.ring.sigma),
        fig(r.ring.sigma_best),
        fig(r.ring.sigma_worst),
        r.ring.worst_dir,
        fig(r.ring.dirs[0]),
        fig(r.ring.dirs[1]),
        fig(r.ring.dirs[2]),
        fig(r.ring.dirs[3]),
        r.ring.edges
    ) + &format!(
        ",{},{},{},{},{},{},{},{},{}",
        fig(r.out.sigma),
        fig(r.out.sigma_best),
        fig(r.out.sigma_worst),
        r.out.worst_dir,
        fig(r.out.dirs[0]),
        fig(r.out.dirs[1]),
        fig(r.out.dirs[2]),
        fig(r.out.dirs[3]),
        r.out.edges
    ) + &format!(
        ",{},{},{},{},{},{},{},{}",
        u8::from(r.panning_old),
        u8::from(r.panning_new),
        fig(r.streaks),
        fig(r.rating),
        u8::from(r.learned),
        fig(r.ms_in),
        fig(r.ms_ring),
        fig(r.ms_out)
    ) + &format!(
        ",{},{},{},{},{},{},{},{},{}",
        r.inside_feat.bins[0],
        r.inside_feat.bins[1],
        r.inside_feat.bins[2],
        r.inside_feat.bins[3],
        fig(r.inside_feat.dom),
        fig(r.inside_feat.coh),
        fig(r.inside_feat.cons),
        fig(r.inside_feat.deg),
        r.inside_feat.tiles
    ) + &format!(
        ",{},{},{},{},{},{},{},{},{},{}",
        u8::from(r.ring_clamped),
        r.ring_feat.bins[0],
        r.ring_feat.bins[1],
        r.ring_feat.bins[2],
        r.ring_feat.bins[3],
        fig(r.ring_feat.dom),
        fig(r.ring_feat.coh),
        fig(r.ring_feat.cons),
        fig(r.ring_feat.deg),
        r.ring_feat.tiles
    ) + &format!(
        ",{},{},{}\n",
        fig(angle_gap(r.ring_feat.deg, r.inside_feat.deg)),
        fig(r.ms_feat),
        fig(r.ms)
    )
}

/// One labelled input frame.
struct Item {
    id: String,
    set: &'static str,
    path: PathBuf,
    shoot: String,
    label: String,
    bbox: Option<[f64; 4]>,
}

fn label_json(dir: &Path, name: &str) -> serde_json::Value {
    let path = dir.join(name);
    serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn items(dir: &Path) -> Vec<Item> {
    let mut out = Vec::new();
    // Main set: a list of frames, labels keyed by id.
    let labels = label_json(dir, "labels.json");
    for f in label_json(dir, "frames.json").as_array().expect("frames.json").iter() {
        let Some(id) = f["id"].as_str() else { continue };
        let Some(label) = labels[id]["label"].as_str() else { continue };
        let Some(path) = f["path"].as_str() else { continue };
        out.push(Item {
            id: id.to_string(),
            set: "main",
            path: PathBuf::from(path),
            shoot: f["shoot"].as_str().unwrap_or_default().to_string(),
            label: label.to_string(),
            bbox: f["box"].as_array().and_then(|b| {
                let v: Vec<f64> = b.iter().filter_map(serde_json::Value::as_f64).collect();
                (v.len() == 4).then(|| [v[0], v[1], v[2], v[3]])
            }),
        });
    }
    // Slow set: a list of frames, and a list of labels, both keyed by id. No
    // box: the detector has to find the subject.
    let slow_labels = label_json(dir, "labels_slow.json");
    let by_id: std::collections::HashMap<&str, &str> = slow_labels
        .as_array()
        .expect("labels_slow.json")
        .iter()
        .filter_map(|v| Some((v["id"].as_str()?, v["label"].as_str()?)))
        .collect();
    for f in label_json(dir, "frames_slow.json").as_array().expect("frames_slow.json").iter() {
        let Some(id) = f["id"].as_str() else { continue };
        let Some(label) = by_id.get(id) else { continue };
        let Some(path) = f["path"].as_str() else { continue };
        out.push(Item {
            id: id.to_string(),
            set: "slow",
            path: PathBuf::from(path),
            shoot: f["shoot"].as_str().unwrap_or_default().to_string(),
            label: label.to_string(),
            bbox: None,
        });
    }
    out
}

#[test]
#[ignore]
fn pan_measure() {
    let dir = PathBuf::from(
        std::env::var("CONROD_PAN_DIR")
            .unwrap_or_else(|_| String::from(r"E:\code\photo\conrod\target\pan")),
    );
    let csv_path = dir.join("measure.csv");
    assert!(
        csv_path.to_string_lossy().replace('\\', "/").to_lowercase().contains("/target/pan/"),
        "refusing to write to {}",
        csv_path.display()
    );
    let all = items(&dir);
    assert!(!all.is_empty(), "no labelled frames under {}", dir.display());

    let settings = Settings::default();
    let preset = ShootPreset::parse(&settings.scan_profile);
    let models: std::collections::HashMap<String, conrod_core::ridge::SharpModel> =
        ["vehicle"]
            .into_iter()
            .filter_map(|region| load_region_model(region).map(|m| (region.to_string(), m)))
            .collect();
    let detector = Mutex::new(
        Detector::load(&detector_model(), Device::Auto).expect("load the detector"),
    );
    println!(
        "device {}; detector {}; learned vehicle model {}",
        lock(&detector).device,
        detector_model().display(),
        models.contains_key("vehicle")
    );

    let mut rows: Vec<Row> = Vec::new();
    let mut skipped_label = 0;
    let mut no_box = 0;
    let mut unreadable = 0;
    let started = Instant::now();
    for item in &all {
        if item.label == "unsure" || item.label == "no_vehicle" {
            skipped_label += 1;
            continue;
        }
        let frame = match raw::read(&item.path) {
            Ok(f) => f,
            Err(e) => {
                println!("skip {}: {e}", item.path.display());
                unreadable += 1;
                continue;
            }
        };
        let image = match Rgb::decode_jpeg(&frame.preview, 1) {
            Ok(i) => i.orient(frame.orientation),
            Err(e) => {
                println!("skip {}: {e}", item.path.display());
                unreadable += 1;
                continue;
            }
        };
        let (iw, ih) = (image.width, image.height);
        let long = iw.max(ih);
        let area = (iw.max(1) * ih.max(1)) as f64;

        let bbox = match item.bbox {
            Some(b) => Some(b),
            None => detected_box(&image, &detector),
        };
        let region = bbox.map(|b| to_px(b, iw, ih));
        let empty = match region {
            Some([x0, y0, x1, y1]) => x1 <= x0 + 4 || y1 <= y0 + 4,
            None => true,
        };
        let started_frame = Instant::now();
        if let (Some(bbox), Some(region), false) = (bbox, region, empty) {
            let grey = image.to_gray();
            let t = Instant::now();
            let inside = referenced(edgeblur::measure(&grey, Some(region)), long);
            let ms_in = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let (ring, ring_clamped) = ring_measure(&grey, region, long);
            let ms_ring = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let out = referenced(edgeblur::measure_outside(&grey, region), long);
            let ms_out = t.elapsed().as_secs_f64() * 1000.0;
            // (a)+(b)+(c): the edge-orientation structure of the box, and of the
            // ring when the ring is a ring and not the whole frame.
            let t = Instant::now();
            let inside_feat = features(&grey, region, None);
            let ring_feat = match ring_box(region, iw, ih) {
                Some(r) if !ring_clamped => features(&grey, r, Some(region)),
                _ => no_feat(),
            };
            let ms_feat = t.elapsed().as_secs_f64() * 1000.0;
            let (panning_old, panning_new, rating, streaks, learned) =
                production(&image, &grey, bbox, &settings, preset, &models);
            drop(grey);
            let box_area = ((region[2] - region[0]) * (region[3] - region[1])) as f64;
            rows.push(Row {
                id: item.id.clone(),
                set: item.set,
                label: item.label.clone(),
                shoot: item.shoot.clone(),
                box_px: Some(region),
                box_fraction: box_area / area,
                inside,
                ring,
                out,
                panning_old,
                panning_new,
                streaks,
                rating,
                learned,
                inside_feat,
                ring_feat,
                ring_clamped,
                ms_in,
                ms_ring,
                ms_out,
                ms_feat,
                ms: started_frame.elapsed().as_secs_f64() * 1000.0,
            });
        } else {
            no_box += 1;
            let (panning_old, panning_new, rating, streaks, learned) =
                (false, false, NAN, NAN, false);
            rows.push(Row {
                id: item.id.clone(),
                set: item.set,
                label: item.label.clone(),
                shoot: item.shoot.clone(),
                box_px: None,
                box_fraction: NAN,
                inside: referenced(unmeasured(), long),
                ring: referenced(unmeasured(), long),
                out: referenced(unmeasured(), long),
                panning_old,
                panning_new,
                streaks,
                rating,
                learned,
                inside_feat: no_feat(),
                ring_feat: no_feat(),
                ring_clamped: true,
                ms_in: NAN,
                ms_ring: NAN,
                ms_out: NAN,
                ms_feat: NAN,
                ms: started_frame.elapsed().as_secs_f64() * 1000.0,
            });
        }
        if rows.len().is_multiple_of(20) {
            println!("… {} rows, {:.0} s", rows.len(), started.elapsed().as_secs_f64());
        }
    }

    let mut csv = String::from(
        "id,set,label,shoot,box_x0,box_y0,box_x1,box_y1,box_fraction,\
         in_sigma,in_best,in_worst,in_worst_dir,in_d0,in_d1,in_d2,in_d3,in_edges,\
         ring_sigma,ring_best,ring_worst,ring_worst_dir,ring_d0,ring_d1,ring_d2,ring_d3,ring_edges,\
         out_sigma,out_best,out_worst,out_worst_dir,out_d0,out_d1,out_d2,out_d3,out_edges,\
         panning_old,panning_new,streaks,rating,learned,ms_in,ms_ring,ms_out,\
         in_bin0,in_bin1,in_bin2,in_bin3,in_dom,in_coh,in_cons,in_orient,in_tiles,\
         ring_clamped,ring_bin0,ring_bin1,ring_bin2,ring_bin3,ring_dom,ring_coh,ring_cons,ring_orient,ring_tiles,\
         orient_gap,ms_feat,ms_total\n",
    );
    for r in &rows {
        csv += &row_text(r);
    }
    std::fs::write(&csv_path, &csv).unwrap_or_else(|e| panic!("{}: {e}", csv_path.display()));

    let mut ms: Vec<f64> = rows.iter().map(|r| r.ms).filter(|v| v.is_finite()).collect();
    ms.sort_by(f64::total_cmp);
    println!(
        "\nlabelled frames: {}, measured {}, skipped by label {}, no box {}, unreadable {}; {:.1} s",
        all.len(),
        rows.iter().filter(|r| r.box_px.is_some()).count(),
        skipped_label,
        no_box,
        unreadable,
        started.elapsed().as_secs_f64()
    );
    if !ms.is_empty() {
        println!(
            "per frame: median {:.0} ms, mean {:.0} ms, max {:.0} ms (decode+detect+3 measures+production)",
            ms[ms.len() / 2],
            ms.iter().sum::<f64>() / ms.len() as f64,
            ms[ms.len() - 1]
        );
    }
    for set in ["main", "slow"] {
        let v: Vec<&Row> = rows
            .iter()
            .filter(|r| r.set == set && r.box_px.is_some())
            .collect();
        if v.is_empty() {
            continue;
        }
        let mean = |f: &dyn Fn(&Row) -> f64| -> f64 {
            let xs: Vec<f64> = v.iter().map(|r| f(r)).filter(|x| x.is_finite()).collect();
            if xs.is_empty() {
                0.0
            } else {
                xs.iter().sum::<f64>() / xs.len() as f64
            }
        };
        println!(
            "{set}: n {}, in_worst {:.3}, ring_worst {:.3}, ring ratio {:.2}, out_worst {:.3}, panning old {:.2} new {:.2}",
            v.len(),
            mean(&|r| r.inside.sigma_worst),
            mean(&|r| r.ring.sigma_worst),
            mean(&|r| r.ring.sigma_worst / r.ring.sigma_best),
            mean(&|r| r.out.sigma_worst),
            mean(&|r| f64::from(u8::from(r.panning_old))),
            mean(&|r| f64::from(u8::from(r.panning_new)))
        );
    }

    // The new production flag against the hand labels: the pans it must keep
    // and the static frames it must not claim.
    let pans: Vec<&Row> = rows.iter().filter(|r| r.label == "pan").collect();
    let statics: Vec<&Row> = rows.iter().filter(|r| r.label == "static").collect();
    let rate = |v: &[&Row], f: &dyn Fn(&Row) -> bool| -> f64 {
        if v.is_empty() {
            return NAN;
        }
        100.0 * v.iter().filter(|r| f(r)).count() as f64 / v.len() as f64
    };
    let old = |r: &Row| r.panning_old;
    let new = |r: &Row| r.panning_new;
    println!(
        "\npanning flag: pans n {} old {:.0}% new {:.0}%; static n {} old {:.1}% new {:.1}%",
        pans.len(),
        rate(&pans, &old),
        rate(&pans, &new),
        statics.len(),
        rate(&statics, &old),
        rate(&statics, &new)
    );
    for set in ["main", "slow"] {
        let p: Vec<&Row> = pans.iter().copied().filter(|r| r.set == set).collect();
        let s: Vec<&Row> = statics.iter().copied().filter(|r| r.set == set).collect();
        println!(
            "{set}: pans n {} old {:.0}% new {:.0}%; static n {} old {:.1}% new {:.1}%",
            p.len(),
            rate(&p, &old),
            rate(&p, &new),
            s.len(),
            rate(&s, &old),
            rate(&s, &new)
        );
    }
    // AUC of the new feature on its own, over the frames where it measured.
    let mut xs: Vec<(f64, bool)> = pans
        .iter()
        .chain(statics.iter())
        .filter(|r| r.streaks.is_finite())
        .map(|r| (r.streaks, r.label == "pan"))
        .collect();
    xs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut wins = 0f64;
    for (i, &(_, pan)) in xs.iter().enumerate() {
        if !pan {
            continue;
        }
        for &(_, other_pan) in &xs[..i] {
            if !other_pan {
                wins += 1.0;
            }
        }
    }
    let positives = xs.iter().filter(|x| x.1).count();
    let negatives = xs.len() - positives;
    if positives > 0 && negatives > 0 {
        println!(
            "streaks AUC {:.3} ({} pans, {} static, {} measured)",
            wins / (positives * negatives) as f64,
            positives,
            negatives,
            xs.len()
        );
    }
    println!("csv: {}", csv_path.display());
}
