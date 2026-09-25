//! Is the subject sharp?
//!
//! Port of `conrod/sharpness.py`; the reasoning behind every constant lives in
//! that file's comments and is not repeated here. In short: measure inside the
//! vehicle, in tiles, each tile's focus energy divided by its own contrast, and
//! take a high percentile of the tiles -- so spinning wheels and a smeared pan
//! background cannot condemn a sharp car, and a missed focus has no sharp tile
//! anywhere. The rest of the crop is scored the same way as background, which
//! is what tells a held pan from a missed frame.

use crate::imageops::Gray;
use conrod_core::ridge::{self, SharpModel};

pub const TILE_GRID: usize = 6;
pub const BACKGROUND_MIN_TILES: usize = 2;
pub const TILE_PERCENTILE: f64 = 80.0;
pub const MIN_TILE_CONTRAST: f64 = 1.5;
pub const WORKING_EDGE: usize = 1024;
pub const PAN_MARGIN: f64 = 0.15;
pub const PAN_BACKGROUND_CEILING: f64 = 0.55;
pub const BANDS: usize = 3;
pub const END_MARGIN: f64 = 0.12;
pub const UNCERTAIN_MARGIN: f64 = 0.06;
pub const MIN_SUBJECT_TILES: usize = 4;
/// Raw focus ratios mapped onto 0..1 on a log scale between these.
pub const RAW_BLURRED: f64 = 0.10;
pub const RAW_SHARP: f64 = 0.85;
/// Star floors, fitted to one photographer's hand ratings.
pub const STAR_BANDS: [(f64, u8); 5] = [(0.958, 5), (0.825, 4), (0.728, 3), (0.606, 2), (0.0, 1)];
pub const SHARP_AT: f64 = 0.825;
pub const BLURRED_BELOW: f64 = 0.606;

pub const FEATURE_NAMES: [&str; 18] = [
    "score",
    "tiles_p50",
    "tiles_p95",
    "tiles_p20",
    "log_focus",
    "log_focus_over_half",
    "log_focus_over_quarter",
    "usable_tiles",
    "background",
    "has_background",
    "band_0",
    "band_1",
    "band_2",
    "band_spread",
    "log_contrast",
    "log_subject_px",
    "subject_fraction",
    "log_noise",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Sharpness {
    pub score: f64,
    pub verdict: &'static str,
    /// What is behind the subject; -1 where it was not measured.
    pub background: f64,
    /// Subject clearly sharper than a smeared background: a held pan.
    pub panning: bool,
    /// "left" | "right" | "top" | "bottom" | "middle" | "even".
    pub sharp_end: &'static str,
    pub bands: Vec<f64>,
    pub uncertain: bool,
    /// Whether the measure ran at all, as opposed to what it came back with.
    pub measured: bool,
    pub features: Vec<f64>,
    /// The hand-built score where a learned model replaced `score`, else -1.
    pub heuristic: f64,
    pub learned: bool,
}

impl Default for Sharpness {
    fn default() -> Self {
        Sharpness {
            score: 0.0,
            verdict: "unknown",
            background: -1.0,
            panning: false,
            sharp_end: "even",
            bands: Vec::new(),
            uncertain: false,
            measured: false,
            features: Vec::new(),
            heuristic: -1.0,
            learned: false,
        }
    }
}

impl Sharpness {
    /// One end of the car is sharp even though the score overall is not.
    pub fn partly_sharp(&self) -> bool {
        match (max(&self.bands), min(&self.bands)) {
            (Some(hi), Some(lo)) => hi - lo >= END_MARGIN,
            _ => false,
        }
    }
}

fn max(v: &[f64]) -> Option<f64> {
    v.iter().copied().reduce(f64::max)
}

fn min(v: &[f64]) -> Option<f64> {
    v.iter().copied().reduce(f64::min)
}

/// A float image, row-major, and a view of a rectangle within it.
#[derive(Clone)]
struct Plane {
    w: usize,
    h: usize,
    px: Vec<f32>,
}

impl Plane {
    fn from_gray(g: &Gray) -> Plane {
        Plane {
            w: g.width,
            h: g.height,
            px: g.data.iter().map(|&v| f32::from(v)).collect(),
        }
    }

    fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Plane {
        let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for y in y0..y1 {
            px.extend_from_slice(&self.px[y * self.w + x0..y * self.w + x1]);
        }
        Plane {
            w: x1 - x0,
            h: y1 - y0,
            px,
        }
    }

    fn at(&self, x: usize, y: usize) -> f32 {
        self.px[y * self.w + x]
    }

    /// Population standard deviation of a rectangle.
    fn std(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
        let n = ((x1 - x0) * (y1 - y0)) as f64;
        let mut sum = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                sum += f64::from(self.at(x, y));
            }
        }
        let mean = sum / n;
        let mut sq = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                let d = f64::from(self.at(x, y)) - mean;
                sq += d * d;
            }
        }
        (sq / n).sqrt()
    }

    fn mean(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> f64 {
        let mut sum = 0.0;
        for y in y0..y1 {
            for x in x0..x1 {
                sum += f64::from(self.at(x, y));
            }
        }
        sum / ((x1 - x0) * (y1 - y0)) as f64
    }

    /// Squared gradient energy (Tenengrad): central differences, the border
    /// rows and columns left at zero.
    fn focus_map(&self) -> Plane {
        let mut out = vec![0f32; self.px.len()];
        for y in 0..self.h {
            for x in 0..self.w {
                let gx = if x >= 1 && x + 1 < self.w {
                    self.at(x + 1, y) - self.at(x - 1, y)
                } else {
                    0.0
                };
                let gy = if y >= 1 && y + 1 < self.h {
                    self.at(x, y + 1) - self.at(x, y - 1)
                } else {
                    0.0
                };
                out[y * self.w + x] = gx * gx + gy * gy;
            }
        }
        Plane {
            w: self.w,
            h: self.h,
            px: out,
        }
    }

    /// Mean of each `f` x `f` block, the remainder rows and columns dropped.
    fn shrink(&self, f: usize) -> Plane {
        let (h, w) = (self.h / f, self.w / f);
        let mut px = Vec::with_capacity(w * h);
        for by in 0..h {
            for bx in 0..w {
                px.push(self.mean(bx * f, by * f, bx * f + f, by * f + f) as f32);
            }
        }
        Plane { w, h, px }
    }
}

/// `np.array_split(np.arange(n), k)` as (start, end) pairs, empty ones kept.
fn array_split(n: usize, k: usize) -> Vec<(usize, usize)> {
    let (q, r) = (n / k, n % k);
    let mut out = Vec::with_capacity(k);
    let mut start = 0;
    for i in 0..k {
        let len = q + usize::from(i < r);
        out.push((start, start + len));
        start += len;
    }
    out
}

/// `np.percentile(values, p)` with numpy's default linear interpolation.
fn percentile(values: &[f64], p: f64) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    let pos = (v.len() - 1) as f64 * p / 100.0;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(v.len() - 1);
    let t = pos - lo as f64;
    // numpy's _lerp, including its switch at t >= 0.5 for symmetry.
    let (a, b) = (v[lo], v[hi]);
    if t >= 0.5 {
        b - (b - a) * (1.0 - t)
    } else {
        a + (b - a) * t
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// Per-tile focus ratios of one region, skipping the featureless tiles.
fn tile_scores(region: &Plane, grid: usize) -> Vec<f64> {
    if region.w.min(region.h) < grid * 4 {
        return Vec::new();
    }
    let energy = region.focus_map();
    let mut scores = Vec::new();
    for &(r0, r1) in &array_split(region.h, grid) {
        for &(c0, c1) in &array_split(region.w, grid) {
            if r0 == r1 || c0 == c1 {
                continue;
            }
            let contrast = region.std(c0, r0, c1, r1);
            if contrast < MIN_TILE_CONTRAST {
                continue;
            }
            // Divided by contrast squared: gradient energy scales with the
            // square of the amplitude, and this measures focus, not paint.
            scores.push(energy.mean(c0, r0, c1, r1) / (contrast * contrast));
        }
    }
    scores
}

fn region_score(region: &Plane, grid: usize) -> f64 {
    let scores = tile_scores(region, grid);
    if scores.is_empty() {
        return -1.0;
    }
    normalise(percentile(&scores, TILE_PERCENTILE))
}

/// Map the raw focus ratio onto 0..1 across the range that occurs.
pub fn normalise(raw: f64) -> f64 {
    if raw <= 0.0 {
        return 0.0;
    }
    let (lo, hi) = (RAW_BLURRED.ln(), RAW_SHARP.ln());
    ((raw.ln() - lo) / (hi - lo)).clamp(0.0, 1.0)
}

/// Score how sharp the subject is, and say what is behind it. `bbox` is the
/// detector's box in `image` coordinates; without one the whole image is the
/// subject. `model` is the photographer's learned sharpness, if any.
pub fn measure(image: &Gray, bbox: Option<[f64; 4]>, model: Option<&SharpModel>) -> Sharpness {
    let longest = image.width.max(image.height);
    let scale = if longest > 0 {
        (WORKING_EDGE as f64 / longest as f64).min(1.0)
    } else {
        1.0
    };
    let grey = image.thumbnail(WORKING_EDGE);
    let data = Plane::from_gray(&grey);
    if data.w.min(data.h) < TILE_GRID * 4 {
        return Sharpness::default();
    }
    let (width, height) = (data.w as f64, data.h as f64);
    let inner = bbox.and_then(|b| {
        let (x1, y1) = ((b[0] * scale).max(0.0), (b[1] * scale).max(0.0));
        let (x2, y2) = ((b[2] * scale).min(width), (b[3] * scale).min(height));
        let edge = (TILE_GRID * 4) as f64;
        (x2 - x1 >= edge && y2 - y1 >= edge).then_some((
            x1 as usize,
            y1 as usize,
            x2 as usize,
            y2 as usize,
        ))
    });

    let Some((x1, y1, x2, y2)) = inner else {
        let score = region_score(&data, TILE_GRID);
        if score < 0.0 {
            return Sharpness::default();
        }
        return Sharpness {
            score,
            measured: true,
            ..Sharpness::default()
        };
    };

    let subject = data.crop(x1, y1, x2, y2);
    let subject_tiles = tile_scores(&subject, TILE_GRID);
    if subject_tiles.len() < MIN_SUBJECT_TILES {
        // Too little of the vehicle carries detail. The whole crop answers
        // instead, flagged, because the answer is about the picture.
        let score = region_score(&data, TILE_GRID);
        if score < 0.0 {
            return Sharpness::default();
        }
        return Sharpness {
            score,
            uncertain: true,
            measured: true,
            ..Sharpness::default()
        };
    }

    let score = normalise(percentile(&subject_tiles, TILE_PERCENTILE));
    let background = background_score(&data, (x1, y1, x2, y2));
    let panning =
        (0.0..=PAN_BACKGROUND_CEILING).contains(&background) && score - background >= PAN_MARGIN;
    let (bands, sharp_end) = bands_of(&subject);
    let features = features(&data, &subject, &subject_tiles, background, &bands);

    // Pan detection stays with the hand-built measure: whether the
    // background is smeared is a fact, not a matter of taste.
    let learned = model.and_then(|m| learned_score(m, &features));
    Sharpness {
        score: learned.unwrap_or(score),
        background,
        panning,
        sharp_end,
        bands,
        measured: true,
        features,
        heuristic: if learned.is_some() { score } else { -1.0 },
        learned: learned.is_some(),
        ..Sharpness::default()
    }
}

/// Focus of the crop outside the vehicle, in tiles the subject's size so the
/// two scores share a scale. Tiles touching the box are dropped.
fn background_score(data: &Plane, (x1, y1, x2, y2): (usize, usize, usize, usize)) -> f64 {
    let tile_px = (y2 - y1).max(x2 - x1) as f64 / TILE_GRID as f64;
    if tile_px <= 0.0 {
        return -1.0;
    }
    let splits = |n: usize| ((n as f64 / tile_px).round_ties_even() as usize).max(2);
    let energy = data.focus_map();
    let mut scores = Vec::new();
    for &(r0, r1) in &array_split(data.h, splits(data.h)) {
        for &(c0, c1) in &array_split(data.w, splits(data.w)) {
            if r0 == r1 || c0 == c1 {
                continue;
            }
            if !(c1 <= x1 || c0 >= x2 || r1 <= y1 || r0 >= y2) {
                continue;
            }
            let contrast = data.std(c0, r0, c1, r1);
            if contrast < MIN_TILE_CONTRAST {
                continue;
            }
            scores.push(energy.mean(c0, r0, c1, r1) / (contrast * contrast));
        }
    }
    if scores.len() < BACKGROUND_MIN_TILES {
        return -1.0;
    }
    normalise(percentile(&scores, TILE_PERCENTILE))
}

/// Sharpness along the vehicle's longer axis, and which end wins, in image
/// terms (nothing at cull time knows which way the car points).
fn bands_of(subject: &Plane) -> (Vec<f64>, &'static str) {
    let horizontal = subject.w >= subject.h;
    let length = if horizontal { subject.w } else { subject.h };
    if length < BANDS * TILE_GRID * 2 {
        return (Vec::new(), "even");
    }
    // np.linspace(0, length, 4).astype(int)
    let step = length as f64 / BANDS as f64;
    let edges: Vec<usize> = (0..=BANDS)
        .map(|i| {
            if i == BANDS {
                length
            } else {
                (i as f64 * step) as usize
            }
        })
        .collect();
    let mut scores = Vec::with_capacity(BANDS);
    for pair in edges.windows(2) {
        let piece = if horizontal {
            subject.crop(pair[0], 0, pair[1], subject.h)
        } else {
            subject.crop(0, pair[0], subject.w, pair[1])
        };
        scores.push(region_score(&piece, (TILE_GRID / 2).max(2)));
    }
    if scores.iter().any(|&s| s < 0.0) {
        return (Vec::new(), "even");
    }
    let bands: Vec<f64> = scores
        .iter()
        .map(|s| (s * 1000.0).round_ties_even() / 1000.0)
        .collect();
    let (hi, lo) = (max(&bands).unwrap(), min(&bands).unwrap());
    if hi - lo < END_MARGIN {
        return (bands, "even");
    }
    let (first, last) = (bands[0], bands[BANDS - 1]);
    if (first - last).abs() < END_MARGIN {
        let end = if bands[1] == hi { "middle" } else { "even" };
        return (bands, end);
    }
    let end = match (horizontal, first > last) {
        (true, true) => "left",
        (true, false) => "right",
        (false, true) => "top",
        (false, false) => "bottom",
    };
    (bands, end)
}

fn log_raw(tiles: &[f64]) -> f64 {
    percentile(tiles, TILE_PERCENTILE).max(1e-4).ln()
}

/// The subject as the 18 numbers the learned model reads; see FEATURE_NAMES.
/// Change the meaning or the length and `ridge::FEATURE_VERSION` moves too.
fn features(
    data: &Plane,
    subject: &Plane,
    tiles: &[f64],
    background: f64,
    bands: &[f64],
) -> Vec<f64> {
    let mut scales = vec![log_raw(tiles)];
    for factor in [2, 4] {
        let coarse = tile_scores(&subject.shrink(factor), TILE_GRID);
        scales.push(if coarse.is_empty() {
            *scales.last().unwrap()
        } else {
            log_raw(&coarse)
        });
    }
    let at = |p: f64| normalise(percentile(tiles, p));

    let mut laplacian = Vec::with_capacity(subject.px.len());
    for y in 1..subject.h.saturating_sub(1) {
        for x in 1..subject.w.saturating_sub(1) {
            let v = 4.0 * subject.at(x, y)
                - subject.at(x, y - 1)
                - subject.at(x, y + 1)
                - subject.at(x - 1, y)
                - subject.at(x + 1, y);
            laplacian.push(f64::from(v.abs()));
        }
    }
    let (h, w) = (subject.h as f64, subject.w as f64);
    let band = |i: usize| bands.get(i).copied().unwrap_or(0.0);
    vec![
        at(TILE_PERCENTILE),
        at(50.0),
        at(95.0),
        at(20.0),
        scales[0],
        scales[0] - scales[1],
        scales[0] - scales[2],
        tiles.len() as f64 / (TILE_GRID * TILE_GRID) as f64,
        background.max(0.0),
        if background >= 0.0 { 1.0 } else { 0.0 },
        band(0),
        band(1),
        band(2),
        match (max(bands), min(bands)) {
            (Some(hi), Some(lo)) => hi - lo,
            _ => 0.0,
        },
        (subject.std(0, 0, subject.w, subject.h) + 1.0).ln(),
        (h * w).sqrt().ln(),
        (h * w) / (data.h * data.w) as f64,
        (median(&mut laplacian) + 0.05).ln(),
    ]
}

/// Knots on the star-band floors, so a predicted 3.4 lands in the three-star
/// band and every threshold keeps its meaning. Below one star the line keeps
/// going, which is what keeps the worst frames orderable.
const STAR_KNOTS: [f64; 8] = [0.0, 1.0, 1.5, 2.5, 3.5, 4.5, 5.0, 6.0];
const STAR_VALUES: [f64; 8] = [0.0, 0.30, 0.606, 0.728, 0.825, 0.958, 0.99, 1.0];

/// The trained model's prediction, mapped from stars onto the same 0-1 focus
/// score the hand-built measure uses. Shared with `conrod-engine`'s rescore,
/// so an old scan's stored features and this mapping are the only inputs
/// needed to re-score it under a newly trained model.
pub fn learned_score(model: &SharpModel, features: &[f64]) -> Option<f64> {
    let stars = ridge::predict_sharp(model, features)?;
    Some(interp(stars, &STAR_KNOTS, &STAR_VALUES))
}

/// `np.interp`: linear between knots, clamped to the end values outside.
fn interp(x: f64, xs: &[f64], ys: &[f64]) -> f64 {
    if x <= xs[0] {
        return ys[0];
    }
    for i in 1..xs.len() {
        if x <= xs[i] {
            let t = (x - xs[i - 1]) / (xs[i] - xs[i - 1]);
            return ys[i - 1] + t * (ys[i] - ys[i - 1]);
        }
    }
    ys[ys.len() - 1]
}

// --- verdicts ----------------------------------------------------------------

pub fn verdict_for(score: f64, sharp_at: f64, blurred_below: f64) -> &'static str {
    if score >= sharp_at {
        "sharp"
    } else if score < blurred_below {
        "blurred"
    } else {
        "soft"
    }
}

/// The same bands, named for a picture rather than for its focus.
pub fn rating_for(score: f64, sharp_at: f64, blurred_below: f64) -> &'static str {
    match verdict_for(score, sharp_at, blurred_below) {
        "sharp" => "good",
        "blurred" => "poor",
        _ => "fair",
    }
}

/// Subject rating on the one-to-five scale every catalogue shares.
pub fn stars_for(rating: f64) -> u8 {
    STAR_BANDS
        .iter()
        .find(|(floor, _)| rating >= *floor)
        .map_or(1, |b| b.1)
}

/// The colour a culled frame turns: red dropped, green kept.
pub fn label_for(rating_verdict: &str) -> &'static str {
    match rating_verdict {
        "good" => "Green",
        "poor" => "Red",
        _ => "Yellow",
    }
}

/// Measure and label against the configured thresholds.
pub fn rate(
    image: &Gray,
    bbox: Option<[f64; 4]>,
    model: Option<&SharpModel>,
    sharp_at: f64,
    blurred_below: f64,
) -> Sharpness {
    let mut result = measure(image, bbox, model);
    if result.measured {
        result.verdict = verdict_for(result.score, sharp_at, blurred_below);
    }
    result
}

/// Whether a decision to cull this frame was a close call worth a person's eye.
pub fn doubtful(result: &Sharpness, rating_verdict: &str, blurred_below: f64) -> bool {
    if rating_verdict != "poor" {
        return false;
    }
    result.uncertain
        || (result.score - blurred_below).abs() < UNCERTAIN_MARGIN
        || result.partly_sharp()
}

/// A held pan is never culled automatically, however low the number is.
pub fn cullable(result: &Sharpness, rating_verdict: &str) -> bool {
    rating_verdict == "poor" && !result.panning
}
