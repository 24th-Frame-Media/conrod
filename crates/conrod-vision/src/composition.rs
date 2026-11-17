//! What a frame's composition measures, from its thumbnail and the subject box.
//!
//! Everything here is classical, deterministic and dependency-free: one pass of
//! a Sobel gradient magnitude at 128 px is the saliency proxy (edges times a
//! centre-distance prior), its left-right mirror is symmetry, edge density plus
//! colour variety outside the subject is clutter, and a Hough transform of the
//! 256 px edges finds leading lines. Total cost is a few milliseconds per
//! thumbnail (see the ignored `real_composition_timing` test), which is what
//! makes it usable on every frame of a pan rather than only on the keepers.
//!
//! The numbers are *features*, not a score: rule-of-thirds adherence is a weak
//! predictor of what people keep (Denzler & Redies 2014), so every field is
//! bounded and explainable and the policy that weighs them lives elsewhere.
//! Sources: `docs/research/composition-saliency.md` (4.1 thirds/phi distance,
//! 4.3 saliency balance, 4.8 centring and symmetry) and `docs/plans/
//! keeper-signals.md`. Constants marked "own choice" are not from that research.
//!
//! Conventions: fractions of the frame, origin top-left, y grows down, boxes
//! `[x0, y0, x1, y1]`. Distances are fractions of the frame diagonal.

use std::f64::consts::PI;

use crate::framing::{PHI_POINTS, THIRDS_POINTS};
use crate::imageops::{Gray, Rgb};

/// The frame is sampled at this width for saliency and symmetry.
const SMALL_EDGE: usize = 128;
/// The frame is sampled at this width for edges, clutter and the Hough lines.
const MID_EDGE: usize = 256;

/// Flat (gradient-free) share of the saliency weight, so a featureless frame
/// still has a centre-weighted mass instead of no mass at all. Own choice.
const SALIENCY_FLAT: f64 = 0.25;
/// Saliency mass inside the padded subject box counts this many times. Own
/// choice; it is what keeps a small off-centre subject from owning the centroid.
const SUBJECT_GATE: f64 = 2.0;
/// The subject box is padded by this share of its own width/height before it
/// gates saliency or is excluded from clutter. Own choice.
const PAD: f64 = 0.10;

/// A pixel is an edge when its Sobel magnitude reaches this share of the 99th
/// percentile of all magnitudes (one threshold, no hysteresis), and never when
/// it is below [`MIN_EDGE`], which would make quantisation noise an edge.
///
/// The 99th, not the 90th: half the 90th percentile marks at least 10% of any
/// frame by construction, so every real photo then reads as maximally busy
/// (measured: 0.20-0.28 edge density on 60 album thumbnails, off the top of the
/// clutter mapping). Half the 99th percentile measures 0.04-0.12 on the same
/// frames, which is the range the mapping below is spread over.
const EDGE_SHARE_OF_P99: f64 = 0.50;
/// The percentile of the gradient magnitude that sets the edge threshold.
const EDGE_PERCENTILE: f64 = 0.99;
/// Gradient magnitudes (0..255 scale) below this are never edges. Own choice.
const MIN_EDGE: f64 = 10.0;

/// Hue histogram bins for the colour-variety half of clutter.
const HUES: usize = 24;

/// Anchors: how far (as a fraction of the diagonal) a subject may sit from the
/// anchor before its placement score reaches zero.
///
/// The centre radius is generous because a centred subject is a valid
/// composition, not a near miss (research 4.8); the two grids are tighter so
/// that being on a grid point is a real hit.
pub const CENTRE_RADIUS: f64 = 0.30;
pub const GRID_RADIUS: f64 = 0.18;

/// Clutter mapping, linear in edge density outside the subject:
/// density 0.02 -> 0.20, density 0.12 -> 0.90, clamped to 0..1. A frame with
/// 2% of its background in edges reads as mildly busy, 12% as very busy. Own
/// choice, to tune on labels.
pub const CLUTTER_DENSITY_LOW: f64 = 0.02;
pub const CLUTTER_AT_LOW: f64 = 0.20;
pub const CLUTTER_DENSITY_HIGH: f64 = 0.12;
pub const CLUTTER_AT_HIGH: f64 = 0.90;
/// How much of clutter is colour variety (normalised hue entropy of the
/// background) rather than edges.
pub const CLUTTER_COLOUR_WEIGHT: f64 = 0.30;

/// A leading line counts when its infinite extension passes within this
/// fraction of the diagonal of the subject (or mass) centroid.
pub const LINE_NEAR: f64 = 0.12;
/// A line must be supported over this fraction of the diagonal to count.
pub const LINE_MIN_LEN: f64 = 0.25;
/// At most this many leading lines are reported.
pub const MAX_LINES: u8 = 7;

/// Edge pixels voted into the Hough accumulator (sampled evenly when denser).
const HOUGH_SAMPLES: usize = 3000;
/// Hough resolution: theta bins over 0..180 degrees, one-pixel rho bins.
const THETA_BINS: usize = 180;
/// Peaks within this many theta bins and rho pixels are the same line.
const MERGE_THETA: i32 = 4;
const MERGE_RHO: isize = 4;
/// How many of the strongest Hough peaks are checked, at most.
const PEAKS: usize = 64;

/// What a frame's composition measures, all cheap and explainable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Composition {
    /// How well the subject sits on a compositional anchor, 0..1 (1 = exactly
    /// on the best of centre, the thirds grid or the golden-ratio grid).
    pub placement: f64,
    /// Which anchor won: "centre", "thirds" or "phi".
    pub mode: &'static str,
    /// Signed horizontal visual mass, -1 (heavy left) .. 1 (heavy right),
    /// measured on the saliency proxy below, not the subject box alone.
    pub balance_x: f64,
    /// Signed vertical visual mass, -1 (top heavy) .. 1 (bottom heavy).
    pub balance_y: f64,
    /// Mirror symmetry of the frame's structure, 0..1 (1 = symmetric).
    pub symmetry: f64,
    /// Background busyness, 0..1 (1 = busiest); higher means more edges and
    /// more colour variety outside the subject.
    pub clutter: f64,
    /// Leading lines whose extension passes near the subject, 0..=7.
    pub lines: u8,
    /// The subject box as a share of the frame, 0..1.
    pub subject_share: f64,
}

/// Measure one frame. `subject` is the main subject's box as fractions of the
/// frame (x1, y1, x2, y2), when the detector or the camera's AF area gives one.
pub fn measure(image: &Rgb, subject: Option<[f64; 4]>) -> Composition {
    let subject = subject.map(sanitise_box);
    if image.width == 0 || image.height == 0 {
        return Composition {
            placement: 0.0,
            mode: "centre",
            balance_x: 0.0,
            balance_y: 0.0,
            symmetry: 0.0,
            clutter: 0.0,
            lines: 0,
            subject_share: subject.map_or(0.0, area),
        };
    }

    let gray = image.to_gray();
    let small = plane(&gray.thumbnail(SMALL_EDGE));
    let mid = plane(&gray.thumbnail(MID_EDGE));
    let small_grad = sobel(&small);
    let mid_grad = sobel(&mid);

    // Saliency: edges x centre prior, gated on the subject box.
    let gate = subject.map(|b| PxBox::of(b, small.w, small.h).padded(PAD));
    let mass = saliency(&small_grad, gate);
    let (mass_px, mass_py) = centroid(&mass, small.w, small.h);
    let mass_xy = (mass_px / small.w as f64, mass_py / small.h as f64);

    // Placement: the subject box centre when there is a box, else the mass.
    let point = subject.map_or(mass_xy, box_centre);
    let (placement, mode) = placement_at(point, small.w, small.h);

    // Balance: half the mass, half the box, so a small off-centre car does not
    // read as violently unbalanced.
    let (bx, by) = match subject {
        Some(b) => {
            let c = box_centre(b);
            (0.5 * (c.0 + mass_xy.0), 0.5 * (c.1 + mass_xy.1))
        }
        None => mass_xy,
    };
    let balance_x = ((bx - 0.5) * 2.0).clamp(-1.0, 1.0);
    let balance_y = ((by - 0.5) * 2.0).clamp(-1.0, 1.0);

    let symmetry = symmetry(&small_grad);
    let threshold = edge_threshold(&mid_grad);
    let background = subject.map(|b| PxBox::of(b, mid.w, mid.h).padded(PAD));
    let clutter = clutter(image, &mid, &mid_grad, threshold, background);

    let mid_point = match subject {
        Some(b) => {
            let c = box_centre(b);
            (c.0 * mid.w as f64, c.1 * mid.h as f64)
        }
        None => (mass_xy.0 * mid.w as f64, mass_xy.1 * mid.h as f64),
    };
    let lines = hough_lines(&mid_grad, threshold, background, mid_point);

    Composition {
        placement,
        mode,
        balance_x,
        balance_y,
        symmetry,
        clutter,
        lines,
        subject_share: subject.map_or(0.0, area),
    }
}

/// A row-major plane of `f64` samples and its size.
struct Plane {
    w: usize,
    h: usize,
    v: Vec<f64>,
}

impl Plane {
    fn at(&self, x: usize, y: usize) -> f64 {
        self.v[y * self.w + x]
    }
}

/// Luma as `f64` samples, 0..255.
fn plane(gray: &Gray) -> Plane {
    Plane {
        w: gray.width,
        h: gray.height,
        v: gray.data.iter().map(|&v| f64::from(v)).collect(),
    }
}

/// Sobel magnitude, divided by four so a full black-to-white step reads 255,
/// clamped to the same 0..255 scale. Borders replicate.
fn sobel(p: &Plane) -> Plane {
    let (w, h) = (p.w, p.h);
    let at = |x: isize, y: isize| -> f64 {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        p.at(x, y)
    };
    let mut v = vec![0.0; w * h];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let gx = (at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1));
            let gy = (at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1));
            v[y as usize * w + x as usize] = (gx.hypot(gy) / 4.0).min(255.0);
        }
    }
    Plane { w, h, v }
}

/// A box in pixel coordinates of one of the sampled grids.
#[derive(Debug, Clone, Copy)]
struct PxBox {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl PxBox {
    fn of(b: [f64; 4], w: usize, h: usize) -> PxBox {
        PxBox {
            x0: b[0] * w as f64,
            y0: b[1] * h as f64,
            x1: b[2] * w as f64,
            y1: b[3] * h as f64,
        }
    }

    fn padded(self, frac: f64) -> PxBox {
        let (px, py) = ((self.x1 - self.x0) * frac, (self.y1 - self.y0) * frac);
        PxBox {
            x0: self.x0 - px,
            y0: self.y0 - py,
            x1: self.x1 + px,
            y1: self.y1 + py,
        }
    }

    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

/// Keep a box inside the frame, swap reversed edges, drop non-finite corners.
fn sanitise_box(b: [f64; 4]) -> [f64; 4] {
    let clean = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    let (mut x0, mut x1) = (clean(b[0]), clean(b[2]));
    let (mut y0, mut y1) = (clean(b[1]), clean(b[3]));
    if x1 < x0 {
        std::mem::swap(&mut x0, &mut x1);
    }
    if y1 < y0 {
        std::mem::swap(&mut y0, &mut y1);
    }
    [x0, y0, x1, y1]
}

fn box_centre(b: [f64; 4]) -> (f64, f64) {
    ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)
}

fn area(b: [f64; 4]) -> f64 {
    (b[2] - b[0]) * (b[3] - b[1])
}

/// Saliency mass: `(0.25 + 0.75 * gradient) * centre prior`, doubled inside the
/// gated box, normalised to sum to one.
fn saliency(grad: &Plane, gate: Option<PxBox>) -> Vec<f64> {
    let (w, h) = (grad.w, grad.h);
    let half = (w as f64).hypot(h as f64) / 2.0;
    let mut mass = vec![0.0; w * h];
    let mut sum = 0.0;
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let prior = (1.0 - (px - w as f64 / 2.0).hypot(py - h as f64 / 2.0) / half).max(0.0);
            let edge = (grad.at(x, y) / 255.0).clamp(0.0, 1.0);
            let mut weight = (SALIENCY_FLAT + (1.0 - SALIENCY_FLAT) * edge) * prior;
            if gate.is_some_and(|b| b.contains(px, py)) {
                weight *= SUBJECT_GATE;
            }
            mass[y * w + x] = weight;
            sum += weight;
        }
    }
    if sum > 0.0 {
        for m in &mut mass {
            *m /= sum;
        }
    }
    mass
}

/// The mass centroid in pixels, or the frame centre when there is no mass.
fn centroid(mass: &[f64], w: usize, h: usize) -> (f64, f64) {
    let (mut sx, mut sy, mut sum) = (0.0, 0.0, 0.0);
    for y in 0..h {
        for x in 0..w {
            let m = mass[y * w + x];
            sx += m * (x as f64 + 0.5);
            sy += m * (y as f64 + 0.5);
            sum += m;
        }
    }
    if sum > 0.0 {
        (sx / sum, sy / sum)
    } else {
        (w as f64 / 2.0, h as f64 / 2.0)
    }
}

/// The best of the three anchor families and its score.
///
/// Distances are per axis in fractions of the frame, then divided by the
/// diagonal, so the aspect ratio is respected. The mode is the anchor with the
/// highest score, not the smallest distance: the centre's larger radius means
/// a subject a fifth of a frame off centre still reads as "centre", which is
/// what a photographer means by it.
fn placement_at(point: (f64, f64), w: usize, h: usize) -> (f64, &'static str) {
    let diag = (w as f64).hypot(h as f64);
    let distance = |px: f64, py: f64| {
        ((point.0 - px) * w as f64).hypot((point.1 - py) * h as f64) / diag
    };
    let mut best = (1.0 - distance(0.5, 0.5) / CENTRE_RADIUS, "centre");
    for &px in &THIRDS_POINTS {
        for &py in &THIRDS_POINTS {
            let score = 1.0 - distance(px, py) / GRID_RADIUS;
            if score > best.0 {
                best = (score, "thirds");
            }
        }
    }
    for &px in &PHI_POINTS {
        for &py in &PHI_POINTS {
            let score = 1.0 - distance(px, py) / GRID_RADIUS;
            if score > best.0 {
                best = (score, "phi");
            }
        }
    }
    (best.0.clamp(0.0, 1.0), best.1)
}

/// Pearson correlation of the gradient magnitude with its left-right mirror,
/// clamped to 0..1. Zero when either side has no variance (a flat frame).
fn symmetry(grad: &Plane) -> f64 {
    let (w, h) = (grad.w, grad.h);
    let n = (w * h) as f64;
    let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for y in 0..h {
        for x in 0..w {
            let a = grad.at(x, y);
            let b = grad.at(w - 1 - x, y);
            sa += a;
            sb += b;
            saa += a * a;
            sbb += b * b;
            sab += a * b;
        }
    }
    let cov = n * sab - sa * sb;
    let va = n * saa - sa * sa;
    let vb = n * sbb - sb * sb;
    let denominator = (va * vb).max(0.0).sqrt();
    if denominator <= 1e-9 {
        0.0
    } else {
        (cov / denominator).clamp(0.0, 1.0)
    }
}

/// The edge threshold: half the 99th percentile, but never below [`MIN_EDGE`].
fn edge_threshold(grad: &Plane) -> f64 {
    let p99 = percentile(&grad.v, EDGE_PERCENTILE);
    (EDGE_SHARE_OF_P99 * p99).max(MIN_EDGE)
}

/// The `q` quantile of `v`, 0..1, by selection (no full sort).
fn percentile(v: &[f64], q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut copy = v.to_vec();
    let k = ((q * (copy.len() - 1) as f64).round() as usize).min(copy.len() - 1);
    copy.select_nth_unstable_by(k, f64::total_cmp);
    copy[k]
}

/// Background clutter: edge density outside the padded subject box mapped
/// linearly (0.02 -> 0.20, 0.12 -> 0.90) plus 0.30 x the saturation-weighted
/// hue entropy of that background, clamped to 0..1.
fn clutter(
    image: &Rgb,
    mid: &Plane,
    grad: &Plane,
    threshold: f64,
    background: Option<PxBox>,
) -> f64 {
    let (w, h) = (mid.w, mid.h);
    let mut hist = [0.0; HUES];
    let (mut edges, mut total) = (0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            if background.is_some_and(|b| b.contains(px, py)) {
                continue;
            }
            total += 1;
            if grad.at(x, y) >= threshold {
                edges += 1;
            }
            let ox = x * image.width / w;
            let oy = y * image.height / h;
            let at = (oy * image.width + ox) * 3;
            let (hue, saturation) = hue_saturation(
                image.data[at],
                image.data[at + 1],
                image.data[at + 2],
            );
            if saturation > 0.0 {
                hist[((hue * HUES as f64) as usize).min(HUES - 1)] += saturation;
            }
        }
    }
    let density = if total > 0 {
        edges as f64 / total as f64
    } else {
        0.0
    };
    let from_edges = ((density - CLUTTER_DENSITY_LOW)
        / (CLUTTER_DENSITY_HIGH - CLUTTER_DENSITY_LOW)
        * (CLUTTER_AT_HIGH - CLUTTER_AT_LOW)
        + CLUTTER_AT_LOW)
        .clamp(0.0, 1.0);
    let from_colour = CLUTTER_COLOUR_WEIGHT * hue_entropy(&hist);
    (from_edges + from_colour).clamp(0.0, 1.0)
}

/// Hue as a 0..1 sector and saturation as 0..1, the same convention as
/// `look::hsv` but without the value component.
fn hue_saturation(r: u8, g: u8, b: u8) -> (f64, f64) {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    if delta <= 0.0 || max <= 0.0 {
        return (0.0, 0.0);
    }
    let sector = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (sector / 6.0, delta / max)
}

/// Shannon entropy of a saturation-weighted hue histogram, normalised by the
/// maximum possible entropy for [`HUES`] bins. Zero for a single hue.
fn hue_entropy(hist: &[f64; HUES]) -> f64 {
    let total: f64 = hist.iter().sum();
    if total <= 0.0 {
        return 0.0;
    }
    let mut entropy = 0.0;
    for &count in hist {
        if count > 0.0 {
            let p = count / total;
            entropy -= p * p.ln();
        }
    }
    (entropy / (HUES as f64).ln()).clamp(0.0, 1.0)
}

/// Leading lines: a plain Hough transform, no probabilistic sampling, no
/// orientation bookkeeping.
///
/// Every edge pixel (evenly sampled down to [`HOUGH_SAMPLES`]) votes in a
/// 180-theta by one-pixel-rho accumulator. Peaks above 10% of the diagonal are
/// checked in vote order, near-duplicates within four bins / four pixels are
/// merged, and a peak is kept only when all three hold:
/// * its infinite extension passes within [`LINE_NEAR`] of the centroid;
/// * its supported edge pixels span more than [`LINE_MIN_LEN`] of the diagonal
///   (a short segment that merely points at the subject is not a leading line);
/// * some of its support lies outside the padded subject box (an edge traced
///   along the subject's own outline is not a leading line).
fn hough_lines(
    grad: &Plane,
    threshold: f64,
    subject: Option<PxBox>,
    point: (f64, f64),
) -> u8 {
    let (w, h) = (grad.w, grad.h);
    let diag = (w as f64).hypot(h as f64);
    let mut points: Vec<(f64, f64)> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if grad.at(x, y) >= threshold {
                points.push((x as f64 + 0.5, y as f64 + 0.5));
            }
        }
    }
    if points.is_empty() {
        return 0;
    }
    let stride = points.len().div_ceil(HOUGH_SAMPLES).max(1);
    let rho_max = diag.ceil() as isize;
    let rho_bins = (2 * rho_max + 1) as usize;
    let mut cos = [0.0; THETA_BINS];
    let mut sin = [0.0; THETA_BINS];
    for (t, (c, s)) in cos.iter_mut().zip(sin.iter_mut()).enumerate() {
        let angle = (t as f64 + 0.5) * PI / THETA_BINS as f64;
        *c = angle.cos();
        *s = angle.sin();
    }
    let mut accumulator = vec![0u32; THETA_BINS * rho_bins];
    for &(x, y) in points.iter().step_by(stride) {
        for t in 0..THETA_BINS {
            let rho = x * cos[t] + y * sin[t];
            let bin = rho.round() as isize + rho_max;
            if bin >= 0 && (bin as usize) < rho_bins {
                accumulator[t * rho_bins + bin as usize] += 1;
            }
        }
    }
    let min_votes = ((0.10 * diag).round() as u32).max(8);

    let mut peaks: Vec<(u32, usize, isize)> = Vec::new();
    for t in 0..THETA_BINS {
        for r in 0..rho_bins {
            let votes = accumulator[t * rho_bins + r];
            if votes < min_votes {
                continue;
            }
            let mut peak = true;
            for dt in [-1i32, 0, 1] {
                let tt = (t as i32 + dt).rem_euclid(THETA_BINS as i32) as usize;
                for dr in [-1i32, 0, 1] {
                    let rr = r as i32 + dr;
                    if (dt, dr) == (0, 0) || rr < 0 || rr >= rho_bins as i32 {
                        continue;
                    }
                    if accumulator[tt * rho_bins + rr as usize] > votes {
                        peak = false;
                    }
                }
            }
            if peak {
                peaks.push((votes, t, r as isize - rho_max));
            }
        }
    }
    peaks.sort_by(|a, b| b.0.cmp(&a.0));

    let mut chosen: Vec<(usize, isize)> = Vec::new();
    for &(_, theta, rho) in peaks.iter().take(PEAKS) {
        if chosen
            .iter()
            .any(|&(ct, cr)| theta_gap(theta, ct) <= MERGE_THETA && (rho - cr).abs() <= MERGE_RHO)
        {
            continue;
        }
        let (c, s) = (cos[theta], sin[theta]);
        let rho_px = rho as f64;
        if (point.0 * c + point.1 * s - rho_px).abs() > LINE_NEAR * diag {
            continue;
        }
        let (mut lo, mut hi, mut outside) = (f64::INFINITY, f64::NEG_INFINITY, false);
        for &(x, y) in &points {
            if (x * c + y * s - rho_px).abs() > 1.5 {
                continue;
            }
            let along = -x * s + y * c;
            lo = lo.min(along);
            hi = hi.max(along);
            if subject.is_none_or(|b| !b.contains(x, y)) {
                outside = true;
            }
        }
        if hi - lo <= LINE_MIN_LEN * diag || !outside {
            continue;
        }
        chosen.push((theta, rho));
        if chosen.len() == MAX_LINES as usize {
            break;
        }
    }
    chosen.len() as u8
}

/// The angular distance between two theta bins, 0..90 degrees.
fn theta_gap(a: usize, b: usize) -> i32 {
    let d = (a as i32 - b as i32).abs();
    d.min(THETA_BINS as i32 - d)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GREY: [u8; 3] = [90, 90, 92];
    const WHITE: [u8; 3] = [240, 240, 240];

    fn canvas(w: usize, h: usize, background: [u8; 3]) -> Vec<u8> {
        background.repeat(w * h)
    }

    /// Fill a fractional box with one colour on a canvas.
    fn paint(data: &mut [u8], w: usize, h: usize, b: [f64; 4], colour: [u8; 3]) {
        let x0 = (b[0] * w as f64).round().clamp(0.0, w as f64) as usize;
        let x1 = (b[2] * w as f64).round().clamp(0.0, w as f64) as usize;
        let y0 = (b[1] * h as f64).round().clamp(0.0, h as f64) as usize;
        let y1 = (b[3] * h as f64).round().clamp(0.0, h as f64) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let at = (y * w + x) * 3;
                data[at..at + 3].copy_from_slice(&colour);
            }
        }
    }

    /// A white square of side `2 * half` centred on a fraction of the frame.
    fn at(cx: f64, cy: f64, half: f64) -> [f64; 4] {
        [cx - half, cy - half, cx + half, cy + half]
    }

    fn subject_frame(cx: f64, cy: f64) -> (Rgb, [f64; 4]) {
        let (w, h) = (420, 280);
        let b = at(cx, cy, 0.10);
        let mut data = canvas(w, h, GREY);
        paint(&mut data, w, h, b, WHITE);
        (Rgb::new(w, h, data), b)
    }

    fn assert_finite(c: &Composition) {
        for v in [
            c.placement,
            c.balance_x,
            c.balance_y,
            c.symmetry,
            c.clutter,
            c.subject_share,
        ] {
            assert!(v.is_finite(), "{c:?}");
        }
        assert!((-1.0..=1.0).contains(&c.balance_x), "{c:?}");
        assert!((-1.0..=1.0).contains(&c.balance_y), "{c:?}");
        assert!((0.0..=1.0).contains(&c.symmetry), "{c:?}");
        assert!((0.0..=1.0).contains(&c.clutter), "{c:?}");
        assert!((0.0..=1.0).contains(&c.placement), "{c:?}");
        assert!(c.lines <= MAX_LINES, "{c:?}");
    }

    #[test]
    fn a_subject_on_a_thirds_intersection_reads_as_thirds() {
        let (image, b) = subject_frame(2.0 / 3.0, 1.0 / 3.0);
        let c = measure(&image, Some(b));
        assert_eq!(c.mode, "thirds");
        assert!(c.placement > 0.8, "{c:?}");
        assert_finite(&c);
    }

    #[test]
    fn a_centred_subject_reads_as_centre() {
        let (image, b) = subject_frame(0.5, 0.5);
        let c = measure(&image, Some(b));
        assert_eq!(c.mode, "centre");
        assert!(c.placement > 0.8, "{c:?}");
        assert_finite(&c);
    }

    #[test]
    fn a_subject_on_a_golden_ratio_point_reads_as_phi() {
        let (image, b) = subject_frame(0.382, 0.618);
        let c = measure(&image, Some(b));
        assert_eq!(c.mode, "phi");
        assert!(c.placement > 0.8, "{c:?}");
        assert_finite(&c);
    }

    #[test]
    fn a_flat_grey_frame_has_no_symmetry_clutter_or_nan() {
        let (w, h) = (200, 150);
        let image = Rgb::new(w, h, canvas(w, h, [120, 120, 120]));
        let c = measure(&image, None);
        assert_eq!(c.symmetry, 0.0, "{c:?}");
        assert!(c.clutter < 0.2, "{c:?}");
        assert_finite(&c);
        assert!(c.placement.is_finite());
    }

    #[test]
    fn a_noisy_background_is_busier_than_a_flat_sky() {
        let (w, h) = (420, 280);
        let b = at(0.5, 0.5, 0.10);
        let mut flat = canvas(w, h, [110, 140, 190]);
        paint(&mut flat, w, h, b, WHITE);

        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (seed >> 33) as u8
        };
        let mut noisy = canvas(w, h, [110, 140, 190]);
        for y in 0..h {
            for x in 0..w {
                let inside = (0.4..0.6).contains(&(x as f64 / w as f64))
                    && (0.4..0.6).contains(&(y as f64 / h as f64));
                if inside {
                    continue;
                }
                let at = (y * w + x) * 3;
                noisy[at] = next();
                noisy[at + 1] = next();
                noisy[at + 2] = next();
            }
        }
        paint(&mut noisy, w, h, b, WHITE);

        let calm = measure(&Rgb::new(w, h, flat), Some(b));
        let busy = measure(&Rgb::new(w, h, noisy), Some(b));
        assert!(
            busy.clutter >= calm.clutter + 0.2,
            "calm {calm:?} busy {busy:?}"
        );
        assert_finite(&calm);
        assert_finite(&busy);
    }

    #[test]
    fn a_mirrored_frame_is_symmetric_and_balanced() {
        let (w, h) = (420, 280);
        let mut data = canvas(w, h, GREY);
        paint(&mut data, w, h, [0.10, 0.35, 0.28, 0.65], WHITE);
        for y in 0..h {
            for x in 0..w / 2 {
                let from = (y * w + x) * 3;
                let to = (y * w + (w - 1 - x)) * 3;
                let px = [data[from], data[from + 1], data[from + 2]];
                data[to..to + 3].copy_from_slice(&px);
            }
        }
        let c = measure(&Rgb::new(w, h, data), None);
        assert!(c.symmetry > 0.8, "{c:?}");
        assert!(c.balance_x.abs() < 0.2, "{c:?}");
        assert_finite(&c);
    }

    #[test]
    fn a_bold_diagonal_through_the_subject_is_a_leading_line() {
        let (w, h) = (420, 280);
        let mut data = canvas(w, h, [35, 35, 35]);
        for x in 0..w {
            let y = (x as f64 * (h - 1) as f64 / (w - 1) as f64).round() as usize;
            for dy in 0..3 {
                if y + dy < h {
                    let at = ((y + dy) * w + x) * 3;
                    data[at..at + 3].copy_from_slice(&[245, 245, 245]);
                }
            }
        }
        let c = measure(&Rgb::new(w, h, data), Some([0.4, 0.4, 0.6, 0.6]));
        assert!(c.lines >= 1, "{c:?}");
        assert_finite(&c);
    }

    #[test]
    fn a_flat_frame_has_no_lines() {
        let (w, h) = (420, 280);
        let image = Rgb::new(w, h, canvas(w, h, GREY));
        let c = measure(&image, Some([0.4, 0.4, 0.6, 0.6]));
        assert_eq!(c.lines, 0, "{c:?}");
        assert_finite(&c);
    }

    #[test]
    fn subject_share_is_the_box_area() {
        let (image, _) = subject_frame(0.5, 0.5);
        let c = measure(&image, Some([0.4, 0.4, 0.6, 0.6]));
        assert!((c.subject_share - 0.04).abs() < 1e-9, "{c:?}");
        let none = measure(&image, None);
        assert_eq!(none.subject_share, 0.0);
        assert_finite(&none);
    }

    /// `cargo test --release -p conrod-vision --lib composition -- --ignored
    /// --nocapture`: time the measure on real thumbnails with a centre box and
    /// print the clutter mean, to compare with what the UI will label.
    #[test]
    #[ignore = "needs ~/.conrod/cache/native thumbnails"]
    fn real_composition_timing() {
        let dir = std::path::Path::new(r"C:\Users\kapsikkum\.conrod\cache\native");
        let Ok(read) = std::fs::read_dir(dir) else {
            eprintln!("no thumbnails in {}", dir.display());
            return;
        };
        let mut paths: Vec<_> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        paths.retain(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("thumb-"))
        });
        paths.truncate(200);
        let mut ms = Vec::with_capacity(paths.len());
        let mut clutter = 0.0;
        let mut used = 0;
        for path in &paths {
            let Ok(image) = Rgb::open(path) else { continue };
            let started = std::time::Instant::now();
            let c = measure(&image, Some([0.35, 0.4, 0.65, 0.7]));
            ms.push(started.elapsed().as_secs_f64() * 1000.0);
            clutter += c.clutter;
            used += 1;
        }
        if used == 0 {
            eprintln!("no readable thumbnails in {}", dir.display());
            return;
        }
        ms.sort_by(f64::total_cmp);
        let p = |q: f64| ms[((q * (ms.len() - 1) as f64).round() as usize).min(ms.len() - 1)];
        eprintln!(
            "{used} thumbnails: measure p50 {:.2} ms, p95 {:.2} ms, mean clutter {:.3}, one thread",
            p(0.50),
            p(0.95),
            clutter / used as f64
        );
    }
}
