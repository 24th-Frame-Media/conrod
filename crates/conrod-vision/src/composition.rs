//! What a frame's composition measures, from its thumbnail and the subject box.
//!
//! Everything here is classical, deterministic and dependency-free: one pass of
//! a Sobel gradient magnitude at 128 px is the saliency proxy (edges times a
//! centre-distance prior), its left-right mirror is symmetry, edge density plus
//! colour variety outside the subject is clutter, and a Hough transform of the
//! 256 px edges finds leading lines. Total cost is under ten milliseconds per
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
//! `[x0, y0, x1, y1]`. Placement distances are Euclidean in frame fractions (x
//! and y each a fraction of its own axis); line distances are fractions of the
//! frame diagonal.

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

/// Anchors: how far, in frame fractions, a subject may sit from an anchor
/// before its placement score reaches zero. The distance is Euclidean with x
/// and y each a fraction of its own axis (so 0.10 is 10% of the width and 10%
/// of the height), not diagonal-normalised: a diagonal radius would let half a
/// frame's subjects "hit" a grid at a 3:2 frame. The centre is a zone: both
/// axes must be within [`CENTRE_GATE`] before the centre anchor is considered
/// at all, and its score radius is [`CENTRE_RADIUS`]. Measured on a track
/// album most moving subjects sit off every anchor, which is the honest answer;
/// the score separates the frames that do land on one.
pub const CENTRE_GATE: f64 = 0.10;
pub const CENTRE_RADIUS: f64 = 0.14;
pub const GRID_RADIUS: f64 = 0.10;

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
pub const LINE_NEAR: f64 = 0.08;
/// A line must be supported over this fraction of the diagonal to count.
pub const LINE_MIN_LEN: f64 = 0.35;
/// A peak must reach this share of the strongest peak in the frame, so texture
/// that survives the length test still has to compete with the real line.
const LINE_SHARE_OF_MAX: f64 = 0.25;
/// Supporting edge pixels must form one run: a gap larger than this fraction of
/// the diagonal breaks it. A peak that only gathers scattered collinear texture
/// inside the 1.5-pixel corridor is not a line, however far apart its extremes.
const LINE_SUPPORT_GAP: f64 = 0.04;
/// The centroid's foot on the line must lie inside the line's own support span
/// extended by this fraction of the diagonal at each end: a strong edge across
/// the far side of the frame points at nothing, however well it extends.
const LINE_SPAN_SLACK: f64 = 0.10;
/// At most this many lines per [`THETA_BUCKET`]-wide orientation bucket, so a
/// fence's parallel rails count once or twice, not seven times.
const LINES_PER_BUCKET: usize = 2;
/// Orientation bucket width in theta bins, i.e. degrees.
const THETA_BUCKET: usize = 30;
/// At most this many leading lines are reported.
pub const MAX_LINES: u8 = 7;

/// Edge pixels voted into the Hough accumulator (sampled evenly when denser).
const HOUGH_SAMPLES: usize = 3000;
/// Hough resolution: theta bins over 0..180 degrees, one-pixel rho bins.
const THETA_BINS: usize = 180;
/// Peaks within this many theta bins and rho pixels are the same line.
const MERGE_THETA: i32 = 6;
const MERGE_RHO: isize = 6;
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
    let mass = saliency(&small_grad.mag, gate);
    let (mass_px, mass_py) = centroid(&mass, small.w, small.h);
    let mass_xy = (mass_px / small.w as f64, mass_py / small.h as f64);

    // Placement: the subject box centre when there is a box, else the mass.
    let point = subject.map_or(mass_xy, box_centre);
    let (placement, mode) = placement_at(point);

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

    let symmetry = symmetry(&small_grad.mag);
    let threshold = edge_threshold(&mid_grad.mag);
    let background = subject.map(|b| PxBox::of(b, mid.w, mid.h).padded(PAD));
    let clutter = clutter(image, &mid, &mid_grad.mag, threshold, background);

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

/// A gradient map: the magnitude plane plus the two Sobel responses that the
/// non-maximum suppression needs. One pass, shared by symmetry, clutter and the
/// Hough transform.
struct Sobel {
    mag: Plane,
    gx: Vec<f64>,
    gy: Vec<f64>,
}

/// Sobel magnitude, divided by four so a full black-to-white step reads 255,
/// clamped to the same 0..255 scale. Borders replicate.
fn sobel(p: &Plane) -> Sobel {
    let (gx, gy) = sobel_components(p);
    Sobel {
        mag: Plane {
            w: p.w,
            h: p.h,
            v: gx
                .iter()
                .zip(&gy)
                .map(|(x, y)| x.hypot(*y).min(255.0))
                .collect(),
        },
        gx,
        gy,
    }
}

/// The Sobel kernel's x and y responses, divided by four. Borders replicate.
fn sobel_components(p: &Plane) -> (Vec<f64>, Vec<f64>) {
    let (w, h) = (p.w, p.h);
    let at = |x: isize, y: isize| -> f64 {
        let x = x.clamp(0, w as isize - 1) as usize;
        let y = y.clamp(0, h as isize - 1) as usize;
        p.at(x, y)
    };
    let mut gx = vec![0.0; w * h];
    let mut gy = vec![0.0; w * h];
    for y in 0..h as isize {
        for x in 0..w as isize {
            let i = y as usize * w + x as usize;
            gx[i] = ((at(x + 1, y - 1) + 2.0 * at(x + 1, y) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x - 1, y) + at(x - 1, y + 1)))
                / 4.0;
            gy[i] = ((at(x - 1, y + 1) + 2.0 * at(x, y + 1) + at(x + 1, y + 1))
                - (at(x - 1, y - 1) + 2.0 * at(x, y - 1) + at(x + 1, y - 1)))
                / 4.0;
        }
    }
    (gx, gy)
}

/// Edge pixels thinned by non-maximum suppression along the gradient direction,
/// so every ridge votes once. Without it each thick edge yields two or three
/// parallel rho peaks and the accumulator fills with texture: on real frames the
/// unthinned map marks ~10% of all pixels and the Hough kept six "lines" per
/// photo; thinned, a fence rail or a track edge is one ridge and a stray patch
/// has no long contiguous support.
fn edge_points(gradient: &Sobel, threshold: f64) -> Vec<(f64, f64)> {
    let w = gradient.mag.w;
    let h = gradient.mag.h;
    if w < 3 || h < 3 {
        return Vec::new();
    }
    let (gx, gy) = (&gradient.gx, &gradient.gy);
    let mag = &gradient.mag;
    let mut points = Vec::new();
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let m = mag.at(x, y);
            if m < threshold {
                continue;
            }
            let (ax, ay) = (gx[i].abs(), gy[i].abs());
            let (sx, sy) = if ax > 2.414 * ay {
                (1i32, 0i32)
            } else if ay > 2.414 * ax {
                (0, 1)
            } else if (gx[i] >= 0.0) == (gy[i] >= 0.0) {
                (1, 1)
            } else {
                (1, -1)
            };
            let ahead = mag.at((x as i32 + sx) as usize, (y as i32 + sy) as usize);
            let behind = mag.at((x as i32 - sx) as usize, (y as i32 - sy) as usize);
            if m >= ahead && m >= behind {
                points.push((x as f64 + 0.5, y as f64 + 0.5));
            }
        }
    }
    points
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
/// Distances are Euclidean in frame fractions (x and y each a fraction of its
/// own axis), not fractions of the diagonal. The centre anchor only exists
/// when the point is within [`CENTRE_GATE`] of the centre on **both** axes, so
/// a subject a sixth of a frame off centre cannot be called centred; the grids
/// are always available. The mode is the highest-scoring anchor; when every
/// score is zero (the frame fits no anchor) the mode is still the nearest
/// anchor, which is what names a frame "thirds" for a human to compare pans by,
/// while its score honestly says zero.
fn placement_at(point: (f64, f64)) -> (f64, &'static str) {
    let mut best = (0.0, "centre");
    let mut nearest = (f64::INFINITY, "centre");
    let mut consider = |px: f64, py: f64, radius: f64, mode: &'static str| {
        let distance = (point.0 - px).hypot(point.1 - py);
        if distance < nearest.0 {
            nearest = (distance, mode);
        }
        let score = (1.0 - distance / radius).clamp(0.0, 1.0);
        if score > best.0 {
            best = (score, mode);
        }
    };
    if (point.0 - 0.5).abs() <= CENTRE_GATE && (point.1 - 0.5).abs() <= CENTRE_GATE {
        consider(0.5, 0.5, CENTRE_RADIUS, "centre");
    }
    for &px in &THIRDS_POINTS {
        for &py in &THIRDS_POINTS {
            consider(px, py, GRID_RADIUS, "thirds");
        }
    }
    for &px in &PHI_POINTS {
        for &py in &PHI_POINTS {
            consider(px, py, GRID_RADIUS, "phi");
        }
    }
    if best.0 > 0.0 {
        best
    } else {
        (0.0, nearest.1)
    }
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

/// The edge threshold: half the 99th percentile magnitude, but never below
/// [`MIN_EDGE`]. The percentile comes from a 256-bin histogram of the
/// magnitudes (they are clamped to that range), so it costs one pass and no
/// sort or copy.
fn edge_threshold(grad: &Plane) -> f64 {
    let mut hist = [0usize; 256];
    for &v in &grad.v {
        hist[v.clamp(0.0, 255.0) as usize] += 1;
    }
    let target = ((EDGE_PERCENTILE * grad.v.len() as f64).ceil() as usize).max(1);
    let mut seen = 0;
    let mut percentile = 0.0;
    for (bin, &count) in hist.iter().enumerate() {
        seen += count;
        if seen >= target {
            percentile = bin as f64;
            break;
        }
    }
    (EDGE_SHARE_OF_P99 * percentile).max(MIN_EDGE)
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
/// orientation bookkeeping, but strict about what counts.
///
/// Every thinned edge pixel (non-maximum suppression along the gradient, then
/// evenly sampled down to [`HOUGH_SAMPLES`]) votes in a 180-theta by
/// one-pixel-rho accumulator. Peaks are taken in vote order above 10% of the
/// diagonal and [`LINE_SHARE_OF_MAX`] of the frame's strongest peak;
/// near-duplicates within six bins / six pixels are merged, and at most two
/// lines are kept per 30-degree orientation bucket. A peak is kept only when
/// all four hold:
/// * its infinite extension passes within [`LINE_NEAR`] of the centroid;
/// * its supporting edge pixels form one contiguous run longer than
///   [`LINE_MIN_LEN`] of the diagonal (gaps over [`LINE_SUPPORT_GAP`] break it),
///   so scattered texture in the corridor is not "a line";
/// * the centroid's foot on the infinite line lies within that span, extended
///   by [`LINE_SPAN_SLACK`] of the diagonal at each end (an edge across the far
///   side of the frame does not lead to the subject);
/// * some of its support lies outside the padded subject box (an edge traced
///   along the subject's own outline is not a leading line).
fn hough_lines(
    gradient: &Sobel,
    threshold: f64,
    subject: Option<PxBox>,
    point: (f64, f64),
) -> u8 {
    let (w, h) = (gradient.mag.w, gradient.mag.h);
    let diag = (w as f64).hypot(h as f64);
    let points = edge_points(gradient, threshold);
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
    let Some(&(strongest, _, _)) = peaks.first() else {
        return 0;
    };
    let competing = (LINE_SHARE_OF_MAX * f64::from(strongest)).round() as u32;

    let mut chosen: Vec<(usize, isize)> = Vec::new();
    let mut per_bucket = [0usize; THETA_BINS.div_ceil(THETA_BUCKET)];
    for &(votes, theta, rho) in peaks.iter().take(PEAKS) {
        if votes < competing {
            break;
        }
        if chosen
            .iter()
            .any(|&(ct, cr)| theta_gap(theta, ct) <= MERGE_THETA && (rho - cr).abs() <= MERGE_RHO)
        {
            continue;
        }
        let bucket = theta / THETA_BUCKET;
        if per_bucket[bucket] == LINES_PER_BUCKET {
            continue;
        }
        let (c, s) = (cos[theta], sin[theta]);
        let rho_px = rho as f64;
        if (point.0 * c + point.1 * s - rho_px).abs() > LINE_NEAR * diag {
            continue;
        }
        let foot = -point.0 * s + point.1 * c;
        let mut along: Vec<f64> = Vec::new();
        let mut outside = false;
        for &(x, y) in &points {
            if (x * c + y * s - rho_px).abs() > 1.5 {
                continue;
            }
            along.push(-x * s + y * c);
            if subject.is_none_or(|b| !b.contains(x, y)) {
                outside = true;
            }
        }
        if along.is_empty() {
            continue;
        }
        along.sort_by(f64::total_cmp);
        let gap = LINE_SUPPORT_GAP * diag;
        let (mut run, mut prev) = (along[0], along[0]);
        let (mut lo, mut hi) = (along[0], along[0]);
        for &a in &along[1..] {
            if a - prev > gap {
                run = a;
            }
            prev = a;
            if prev - run > hi - lo {
                lo = run;
                hi = prev;
            }
        }
        let slack = LINE_SPAN_SLACK * diag;
        if hi - lo <= LINE_MIN_LEN * diag || foot < lo - slack || foot > hi + slack || !outside {
            continue;
        }
        chosen.push((theta, rho));
        per_bucket[bucket] += 1;
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
    fn a_subject_outside_the_centre_zone_is_not_called_centred() {
        let (image, b) = subject_frame(0.5, 0.62);
        let c = measure(&image, Some(b));
        assert_ne!(c.mode, "centre", "{c:?}");
        assert!(c.placement < 0.5, "{c:?}");
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

    /// A stand-in for the detector's box, for the real-thumbnail check only: the
    /// 0.2 x 0.2 window with the most gradient energy, which is where a compact
    /// subject would be. The cached thumbnails carry no subject box, a fixed
    /// centre box would make placement a constant, and the centroid of all
    /// edges is pulled to the middle, so the densest window is the closest
    /// stand-in for a detector. It is a test proxy, not part of the measure.
    fn proxy_box(image: &Rgb) -> [f64; 4] {
        let small = plane(&image.to_gray().thumbnail(SMALL_EDGE));
        let gradient = sobel(&small);
        let (w, h) = (small.w, small.h);
        let mut sat = vec![0.0; (w + 1) * (h + 1)];
        for y in 0..h {
            for x in 0..w {
                sat[(y + 1) * (w + 1) + x + 1] = gradient.mag.at(x, y)
                    + sat[y * (w + 1) + x + 1]
                    + sat[(y + 1) * (w + 1) + x]
                    - sat[y * (w + 1) + x];
            }
        }
        let bw = ((0.2 * w as f64).round() as usize).clamp(2, w);
        let bh = ((0.2 * h as f64).round() as usize).clamp(2, h);
        let mut best = (f64::NEG_INFINITY, 0usize, 0usize);
        for y in 0..=h - bh {
            for x in 0..=w - bw {
                let sum = sat[(y + bh) * (w + 1) + x + bw] - sat[y * (w + 1) + x + bw]
                    - sat[(y + bh) * (w + 1) + x]
                    + sat[y * (w + 1) + x];
                if sum > best.0 {
                    best = (sum, x, y);
                }
            }
        }
        let cx = (best.1 as f64 + bw as f64 / 2.0) / w as f64;
        let cy = (best.2 as f64 + bh as f64 / 2.0) / h as f64;
        let (hx, hy) = (0.1, 0.1);
        [
            (cx - hx).clamp(0.0, 1.0),
            (cy - hy).clamp(0.0, 1.0),
            (cx + hx).clamp(0.0, 1.0),
            (cy + hy).clamp(0.0, 1.0),
        ]
    }

    /// `cargo test --release -p conrod-vision --lib composition -- --ignored
    /// --nocapture`: time the measure on real thumbnails and print where
    /// placement, mode and lines land, so a saturated label shows.
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
        let mut placements = Vec::with_capacity(paths.len());
        let (mut centre, mut thirds, mut phi) = (0usize, 0usize, 0usize);
        let mut lines = [0usize; MAX_LINES as usize + 1];
        let mut clutter = 0.0;
        let mut used = 0;
        for path in &paths {
            let Ok(image) = Rgb::open(path) else { continue };
            // The cached thumbnails carry no subject box, so the box comes from
            // `proxy_box`: a fixed centre box would make placement a constant.
            let subject = proxy_box(&image);
            let started = std::time::Instant::now();
            let c = measure(&image, Some(subject));
            ms.push(started.elapsed().as_secs_f64() * 1000.0);
            placements.push(c.placement);
            match c.mode {
                "centre" => centre += 1,
                "thirds" => thirds += 1,
                _ => phi += 1,
            }
            lines[c.lines as usize] += 1;
            clutter += c.clutter;
            used += 1;
        }
        if used == 0 {
            eprintln!("no readable thumbnails in {}", dir.display());
            return;
        }
        ms.sort_by(f64::total_cmp);
        placements.sort_by(f64::total_cmp);
        let p = |v: &[f64], q: f64| -> f64 {
            v[((q * (v.len() - 1) as f64).round() as usize).min(v.len() - 1)]
        };
        eprintln!(
            "{used} thumbnails: measure p50 {:.2} ms, p95 {:.2} ms, mean clutter {:.3}, one thread",
            p(&ms, 0.50),
            p(&ms, 0.95),
            clutter / used as f64
        );
        eprintln!(
            "placement p10 {:.2} p50 {:.2} p90 {:.2} max {:.2}",
            p(&placements, 0.10),
            p(&placements, 0.50),
            p(&placements, 0.90),
            placements[placements.len() - 1]
        );
        eprintln!("mode centre {centre} thirds {thirds} phi {phi}");
        eprintln!("lines 0..{} {lines:?}", MAX_LINES);
        let low = lines[0] + lines[1];
        let high: usize = lines[5..].iter().sum();
        eprintln!(
            "lines 0-1: {:.0}%, 5+: {:.0}%",
            100.0 * low as f64 / used as f64,
            100.0 * high as f64 / used as f64
        );
    }
}
