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
/// Background streak coherence that marks a frame as a pan when there is no
/// subject box to compare against (same bar the engine's pan flag uses).
pub use crate::streaks::PAN_COHERENCE;
/// A tile at or above this normalised score counts as sharp when measuring how
/// much of a panning frame is sharp.
pub const PAN_SHARE_SHARP: f64 = 0.75;
/// A pan's rating is capped by how much of the frame is sharp: the cap says how
/// the picture reads, so a small car in a big streaked frame is a three, and it
/// never goes lower than the three-star floor because the cap must not cull a
/// pan or leave a burst without a keeper.
/// cap = three-star floor + share * slope, clamped to 1: a car filling a third
/// of the frame is still a three, about 35% reaches four, and only a frame that
/// is mostly sharp reaches five.
pub const PAN_SHARE_FLOOR: f64 = STAR_BANDS[2].0;
pub const PAN_SHARE_SLOPE: f64 = 0.28;
pub const PAN_BACKGROUND_CEILING: f64 = 0.55;
pub const BANDS: usize = 3;
pub const END_MARGIN: f64 = 0.12;
pub const UNCERTAIN_MARGIN: f64 = 0.06;
pub const MIN_SUBJECT_TILES: usize = 4;
/// Tiles per side for the in-focus area score: finer than [`TILE_GRID`], so a
/// detail a tenth of the frame wide still fills whole tiles.
pub const FOCAL_GRID: usize = 12;
/// How many of the sharpest tiles make a frame "focused somewhere": about 4%
/// of a 144-tile frame, so one sharp speck or noisy tile cannot pass.
pub const FOCAL_MIN_TILES: usize = 6;
/// Raw focus ratios mapped onto 0..1 on a log scale between these.
pub const RAW_BLURRED: f64 = 0.10;
pub const RAW_SHARP: f64 = 0.85;
/// Star floors, fitted to one photographer's hand ratings.
pub const STAR_BANDS: [(f64, u8); 5] = [(0.958, 5), (0.825, 4), (0.728, 3), (0.606, 2), (0.0, 1)];
/// A burst with nothing this good has no keeper at all: the engine says "all
/// soft" rather than proposing the least-bad frame of a bad set. Three stars --
/// "fair" -- is the bar. A frame the photographer starred by hand always counts,
/// whatever it measures.
pub const KEEPER_MIN_RATING: f64 = STAR_BANDS[2].0;
pub use conrod_core::settings::{BLURRED_BELOW, SHARP_AT};
/// Edge-blur (`sigma_worst`, normalised to
/// [`conrod_vision::edgeblur::REFERENCE_EDGE`]) at or below which a subject is
/// plainly sharp. Deliberately low: the ramp from here to [`BLUR_AT_SHARP`]
/// is what lets a sharp frame rank above a merely sharp one, and the
/// photographer's delivered frames read 0.875 px median, so most sit on it.
pub const BLUR_SHARP: f64 = 0.4;

/// How hard the album culls on blur, from the per-album `blur_strictness`
/// setting. Unknown text is `Moderate`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlurStrictness {
    Strict,
    Moderate,
    Lenient,
    /// Each star floor sits on its own blur, so the stars spread instead of
    /// piling into four: fitted on a 6,006-frame track album where sigma <= 0.70
    /// px is the sharpest 17%, <= 1.0 px the next 32%, <= 1.9 px the next 40%;
    /// three stars ends at 1.9 px.
    Critical,
}

/// The (sigma, score) points of one strictness in ascending sigma. Strict,
/// Moderate and Lenient keep the old three knots -- `SHARP_AT` then
/// `BLURRED_BELOW` then zero -- and Critical puts one knot on each star floor.
const STRICT_SCALE: [(f64, f64); 4] = [
    (BLUR_SHARP, 1.0),
    (1.5, SHARP_AT),
    (2.2, BLURRED_BELOW),
    (3.5, 0.0),
];
const MODERATE_SCALE: [(f64, f64); 4] = [
    (BLUR_SHARP, 1.0),
    (2.0, SHARP_AT),
    (3.0, BLURRED_BELOW),
    (4.5, 0.0),
];
const LENIENT_SCALE: [(f64, f64); 4] = [
    (BLUR_SHARP, 1.0),
    (2.5, SHARP_AT),
    (3.5, BLURRED_BELOW),
    (5.0, 0.0),
];
const CRITICAL_SCALE: [(f64, f64); 6] = [
    (BLUR_SHARP, 1.0),
    (0.70, STAR_BANDS[0].0),
    (1.00, SHARP_AT),
    (1.90, STAR_BANDS[2].0),
    (2.60, BLURRED_BELOW),
    (4.0, 0.0),
];

impl BlurStrictness {
    pub fn parse(value: &str) -> Self {
        match value {
            "strict" => BlurStrictness::Strict,
            "lenient" => BlurStrictness::Lenient,
            "critical" => BlurStrictness::Critical,
            _ => BlurStrictness::Moderate,
        }
    }

    /// The whole blur-to-score curve in px, ascending in sigma. Moderate sits
    /// where delivered keepers shot through heat haze (up to ~2.7 px) stay above
    /// `BLURRED_BELOW`, so none of them is culled, while heavy blur is still
    /// flagged as soft.
    fn scale(self) -> &'static [(f64, f64)] {
        match self {
            BlurStrictness::Strict => &STRICT_SCALE,
            BlurStrictness::Moderate => &MODERATE_SCALE,
            BlurStrictness::Lenient => &LENIENT_SCALE,
            BlurStrictness::Critical => &CRITICAL_SCALE,
        }
    }
}

pub const FEATURE_NAMES: [&str; 19] = [
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
    "fft_hf_ratio",
];
/// Above this fraction of the FFT's radius (after fftshift) counts as "high
/// frequency" for [`fft_hf_ratio`].
const FFT_HF_RADIUS: f64 = 0.5;
/// The FFT is run on the subject shrunk to at most this edge; a blur signature
/// does not need full resolution, and this keeps the cost bounded.
const FFT_MAX_EDGE: usize = 128;

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
    /// How much of the frame is sharp when it is a pan, 0..1; -1 otherwise.
    pub pan_share: f64,
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
            pan_share: -1.0,
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

/// Subject score from the edge-blur measure's worst direction, in pixels at
/// [`conrod_vision::edgeblur::REFERENCE_EDGE`].
///
/// Monotone decreasing and piecewise linear between the measured points, so it
/// speaks the same 0..1 language as the tile score and the configured star and
/// verdict thresholds keep their meaning: at or below [`BLUR_SHARP`] it is 1.0,
/// then it descends through exactly [`SHARP_AT`] and [`BLURRED_BELOW`] at the
/// strictness's points (see [`BlurStrictness::scale`]), and zero from its last
/// point. The line keeps descending below one star, which is what keeps the
/// worst frames orderable. `None` when there is no measurement (NaN).
pub fn rating_from_blur(sigma_worst_ref: f64, strictness: BlurStrictness) -> Option<f64> {
    if sigma_worst_ref.is_nan() {
        return None;
    }
    let scale = strictness.scale();
    let (first_sigma, first_score) = scale[0];
    if sigma_worst_ref <= first_sigma {
        return Some(first_score);
    }
    for pair in scale.windows(2) {
        let ((lo_sigma, lo_score), (hi_sigma, hi_score)) = (pair[0], pair[1]);
        if sigma_worst_ref < hi_sigma {
            let t = (sigma_worst_ref - lo_sigma) / (hi_sigma - lo_sigma);
            return Some(lo_score + t * (hi_score - lo_score));
        }
    }
    Some(scale.last().map_or(0.0, |&(_, score)| score))
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
        // No box (a close, cropped car the detector missed). If the frame's own
        // surroundings streak the way a pan does, a sharp roof over a smeared
        // body must not pass as a sharp frame: judge the centre the way the
        // subject would be judged.
        let (w, h) = (data.w as f64, data.h as f64);
        let centre = [w * 0.25, h * 0.25, w * 0.75, h * 0.75];
        let pan = crate::streaks::background_streaks(&grey, centre)
            .is_some_and(|coherence| coherence >= PAN_COHERENCE);
        let cap = pan
            .then(|| {
                pan_consistency(&data.crop(
                    (centre[0] as usize).min(data.w - 1),
                    (centre[1] as usize).min(data.h - 1),
                    (centre[2] as usize).min(data.w),
                    (centre[3] as usize).min(data.h),
                ))
            })
            .flatten();
        let panning = pan && cap.is_some();
        let share = pan.then(|| sharp_share(&data)).flatten();
        let share_cap = share.map(pan_share_cap);
        return Sharpness {
            score: [cap, share_cap].into_iter().flatten().fold(score, f64::min),
            panning,
            pan_share: share.unwrap_or(-1.0),
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
    // The subject's own score picks the keeper of a burst; how much of a pan's
    // frame is sharp caps the frame's rating instead, in `pan_capped`.
    let final_score = learned.unwrap_or(score);
    Sharpness {
        score: final_score,
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

/// How much of a panning frame is sharp, 0..1: the share of measurable tiles at
/// or above [`PAN_SHARE_SHARP`]. `None` when too few tiles can be measured.
fn sharp_share(data: &Plane) -> Option<f64> {
    let tiles = tile_scores(data, TILE_GRID);
    if tiles.len() < TILE_GRID * TILE_GRID / 2 {
        return None;
    }
    let sharp = tiles
        .iter()
        .filter(|score| normalise(**score) >= PAN_SHARE_SHARP)
        .count();
    Some(sharp as f64 / tiles.len() as f64)
}

/// How much of a whole frame is sharp, 0..1, for capping a pan's frame rating:
/// the share of its measurable tiles at or above [`PAN_SHARE_SHARP`], measured
/// on the same working plane [`measure`] builds. `None` when too few tiles can
/// be measured.
pub fn frame_sharp_share(grey: &Gray) -> Option<f64> {
    let grey = grey.thumbnail(WORKING_EDGE);
    let data = Plane::from_gray(&grey);
    sharp_share(&data)
}

/// The rating a pan is allowed by its sharp share: most pans land at three
/// stars, about 35% of the frame sharp reaches four, and only a mostly sharp
/// frame reaches five.
pub fn pan_share_cap(share: f64) -> f64 {
    (PAN_SHARE_FLOOR + share * PAN_SHARE_SLOPE).clamp(PAN_SHARE_FLOOR, 1.0)
}

/// The weakest measurable quarter of a panning subject, in the same 0..1 score.
/// A quarter with too little contrast to measure (a blank door, the sky inside
/// the box) is left out rather than counted as blurred. `None` when nothing can
/// be measured, in which case the ordinary subject score stands.
fn pan_consistency(subject: &Plane) -> Option<f64> {
    let (w, h) = (subject.w, subject.h);
    if w.min(h) < TILE_GRID * 4 {
        return None;
    }
    let (mx, my) = (w / 2, h / 2);
    let quarters = [
        (0, 0, mx, my),
        (mx, 0, w, my),
        (0, my, mx, h),
        (mx, my, w, h),
    ];
    let mut scores: Vec<f64> = quarters
        .iter()
        .map(|&(x1, y1, x2, y2)| region_score(&subject.crop(x1, y1, x2, y2), TILE_GRID))
        .filter(|score| *score > 0.0)
        .collect();
    scores.sort_by(f64::total_cmp);
    scores.first().copied()
}

/// The score a frame with no detected subject is rated by.
///
/// A detail shot is judged on its in-focus area: the sharp bit against a
/// deliberately blurred background is the picture. A pan is the exception -- its
/// whole point is the cap on how much of the frame is sharp, and the car's own
/// crisp detail (which `focal` finds) must not lift it back over that cap.
pub fn subjectless_score(whole: &Sharpness, focal: Option<f64>) -> Option<f64> {
    let base = whole.measured.then_some(whole.score);
    if whole.panning {
        return base;
    }
    match (base, focal) {
        (Some(base), Some(focal)) => Some(base.max(focal)),
        (base, focal) => base.or(focal),
    }
}

/// The frame's in-focus area: how sharp its sharpest real region is.
///
/// A frame with no subject is often a deliberate detail -- a badge or a gear
/// shifter held sharp against a background thrown out of focus. Scoring the
/// whole frame with [`measure`] needs a fifth of it sharp and calls those
/// blurred; this asks only that [`FOCAL_MIN_TILES`] of a finer grid are.
/// `None` when too few tiles carry enough contrast to be measurable.
pub fn focal_score(image: &Gray) -> Option<f64> {
    let grey = image.thumbnail(WORKING_EDGE);
    let data = Plane::from_gray(&grey);
    let mut scores = tile_scores(&data, FOCAL_GRID);
    if scores.len() < FOCAL_MIN_TILES {
        return None;
    }
    // The FOCAL_MIN_TILES-th best tile: a low percentile of the best ones.
    scores.sort_by(f64::total_cmp);
    Some(normalise(scores[scores.len() - FOCAL_MIN_TILES]))
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

/// The subject as the numbers the learned model reads; see FEATURE_NAMES.
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
        fft_hf_ratio(subject),
    ]
}

/// Share of the crop's 2-D FFT energy that sits above [`FFT_HF_RADIUS`] of the
/// spectrum's radius: a blurred crop's energy huddles near the DC term, a
/// sharp one spreads into high frequencies. Complements the tile-based Tenengrad
/// score above with a global measure the tiling can miss (e.g. fine, even
/// texture loss across the whole subject).
fn fft_hf_ratio(region: &Plane) -> f64 {
    // Bound the cost: shrink big crops before transforming.
    let mut plane = region.clone();
    while plane.w > FFT_MAX_EDGE || plane.h > FFT_MAX_EDGE {
        if plane.w / 2 < 8 || plane.h / 2 < 8 {
            break;
        }
        plane = plane.shrink(2);
    }
    let (w, h) = (plane.w, plane.h);
    if w < 8 || h < 8 {
        return 0.0;
    }
    use rustfft::{num_complex::Complex, FftPlanner};
    let mut planner = FftPlanner::new();
    let mut rows: Vec<Complex<f32>> = plane.px.iter().map(|&v| Complex::new(v, 0.0)).collect();
    let row_fft = planner.plan_fft_forward(w);
    for row in rows.chunks_mut(w) {
        row_fft.process(row);
    }
    // Transpose so a column is contiguous, then FFT those.
    let mut cols = vec![Complex::new(0.0, 0.0); w * h];
    for y in 0..h {
        for x in 0..w {
            cols[x * h + y] = rows[y * w + x];
        }
    }
    let col_fft = planner.plan_fft_forward(h);
    for col in cols.chunks_mut(h) {
        col_fft.process(col);
    }
    let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
    let r_max = cx.min(cy) * FFT_HF_RADIUS;
    let (mut total, mut high) = (0.0f64, 0.0f64);
    for x in 0..w {
        for y in 0..h {
            let c = cols[x * h + y];
            let energy = f64::from(c.re * c.re + c.im * c.im);
            // Frequency distance from the origin, after an implicit fftshift.
            let fx = if x as f64 > cx {
                w as f64 - x as f64
            } else {
                x as f64
            };
            let fy = if y as f64 > cy {
                h as f64 - y as f64
            } else {
                y as f64
            };
            total += energy;
            if (fx * fx + fy * fy).sqrt() > r_max {
                high += energy;
            }
        }
    }
    if total <= 0.0 {
        0.0
    } else {
        (high / total).clamp(0.0, 1.0)
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic value noise at several block scales: the coarse scales
    /// keep their contrast through a blur, the fine ones are what it eats.
    fn texture(edge: usize) -> Gray {
        let mut data = vec![0u8; edge * edge];
        for y in 0..edge {
            for x in 0..edge {
                let mut v = 0.0;
                for (block, amp) in [(16usize, 90.0), (8, 60.0), (4, 40.0), (2, 25.0)] {
                    let n = hash(x / block, y / block) % 256;
                    v += amp * (f64::from(n) / 255.0 - 0.5);
                }
                data[y * edge + x] = (128.0 + v).clamp(0.0, 255.0) as u8;
            }
        }
        Gray::new(edge, edge, data)
    }

    fn hash(x: usize, y: usize) -> u32 {
        let mut h = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 13;
        h = h.wrapping_mul(0xC2B2_AE35);
        h ^ (h >> 16)
    }

    /// Separable box blur twice over, edges clamped: out of focus, cheaply.
    fn blur(g: &Gray, radius: usize) -> Gray {
        let (w, h) = (g.width, g.height);
        let mut cur: Vec<f64> = g.data.iter().map(|&v| f64::from(v)).collect();
        for _ in 0..2 {
            let mut rows = vec![0.0; cur.len()];
            for y in 0..h {
                for x in 0..w {
                    let (lo, hi) = (x.saturating_sub(radius), (x + radius).min(w - 1));
                    rows[y * w + x] =
                        cur[y * w + lo..=y * w + hi].iter().sum::<f64>() / (hi - lo + 1) as f64;
                }
            }
            let mut out = vec![0.0; cur.len()];
            for y in 0..h {
                for x in 0..w {
                    let (lo, hi) = (y.saturating_sub(radius), (y + radius).min(h - 1));
                    let sum: f64 = (lo..=hi).map(|yy| rows[yy * w + x]).sum();
                    out[y * w + x] = sum / (hi - lo + 1) as f64;
                }
            }
            cur = out;
        }
        let data = cur
            .iter()
            .map(|&v| v.round().clamp(0.0, 255.0) as u8)
            .collect();
        Gray::new(w, h, data)
    }

    /// A frame blurred all over but for a square `fraction` of its area, which
    /// keeps the sharp texture: the detail shot the whole-frame score misses.
    fn detail_shot(edge: usize, fraction: f64, radius: usize) -> Gray {
        let sharp = texture(edge);
        let mut data = blur(&sharp, radius).data;
        let side = (edge as f64 * fraction.sqrt()) as usize;
        let corner = (edge - side) / 2;
        for y in corner..corner + side {
            for x in corner..corner + side {
                data[y * edge + x] = sharp.data[y * edge + x];
            }
        }
        Gray::new(edge, edge, data)
    }

    #[test]
    fn a_sharp_detail_against_a_blurred_background_is_sharp_where_it_counts() {
        // The detail shot: a sixth of the frame sharp, the rest thrown out.
        let shot = detail_shot(768, 0.06, 3);
        let whole = measure(&shot, None, None).score;
        let focal = focal_score(&shot).expect("measurable tiles");
        assert!(
            whole < conrod_core::settings::BLURRED_BELOW,
            "whole frame {whole:.3} should read as blurred"
        );
        assert!(
            focal >= conrod_core::settings::SHARP_AT,
            "in-focus area {focal:.3} should read as sharp"
        );
    }

    #[test]
    fn the_same_frame_blurred_all_over_has_no_focal_sharpness() {
        let soft = blur(&texture(768), 3);
        let focal = focal_score(&soft).expect("measurable tiles");
        assert!(
            focal < conrod_core::settings::BLURRED_BELOW,
            "in-focus area {focal:.3} should read as blurred"
        );
    }

    #[test]
    fn a_speck_of_detail_does_not_carry_the_frame() {
        // One percent is fewer than FOCAL_MIN_TILES of the 12x12 grid.
        let shot = detail_shot(768, 0.01, 3);
        let focal = focal_score(&shot);
        assert!(
            focal.is_none_or(|f| f < conrod_core::settings::BLURRED_BELOW),
            "a speck should not lift the in-focus area, got {focal:?}"
        );
    }

    /// A pan: the subject box holds sharp texture while everything around it is
    /// smeared; `half` smears the subject's lower half, the way a mismatched pan
    /// leaves the roof crisp and the body moving.
    fn pan_frame(edge: usize, half: bool) -> (Gray, [f64; 4]) {
        let sharp = texture(edge);
        let soft = blur(&sharp, 5);
        let mut out = soft.clone();
        let (x1, y1, x2, y2) = (edge * 2 / 10, edge * 3 / 10, edge * 8 / 10, edge * 8 / 10);
        let half_line = (y1 + y2) / 2;
        for y in y1..y2 {
            for x in x1..x2 {
                let source = if half && y >= half_line {
                    &soft
                } else {
                    &sharp
                };
                out.data[y * edge + x] = source.data[y * edge + x];
            }
        }
        (out, [x1 as f64, y1 as f64, x2 as f64, y2 as f64])
    }

    #[test]
    fn a_pan_that_only_keeps_half_the_car_sharp_loses_to_its_weakest_quarter() {
        let (all_sharp, [_ax1, _ay1, ax2, ay2]) = pan_frame(960, false);
        let (half, _) = pan_frame(960, true);
        let plane = |g: &Gray| Plane::from_gray(g);
        let (a, b) = (plane(&all_sharp), plane(&half));
        let edge = 960;
        let (x1, y1) = (edge * 2 / 10, edge * 3 / 10);
        let crop = |p: &Plane| p.crop(x1, y1, ax2 as usize, ay2 as usize);
        let cap_all = pan_consistency(&crop(&a)).expect("measurable quarters");
        let cap_half = pan_consistency(&crop(&b)).expect("measurable quarters");
        assert!(
            cap_half <= cap_all - 0.15,
            "a half-smeared car loses to its weakest quarter: {cap_half} vs {cap_all}"
        );
        let measured = measure(&half, Some(pan_frame(960, true).1), None);
        assert!(measured.panning, "still a pan");
    }

    #[test]
    fn a_small_car_in_a_streaked_frame_is_three_stars() {
        let (frame, bbox) = pan_frame(960, false);
        let measured = measure(&frame, Some(bbox), None);
        assert!(measured.panning, "the smeared background makes it a pan");
        // What the app rates the frame by: its subject's own score, capped by
        // how much of the panning frame is sharp.
        let share = frame_sharp_share(&frame).expect("a measurable frame");
        let rating = measured.score.min(pan_share_cap(share));
        assert_eq!(
            stars_for(rating),
            3,
            "a car filling a third of a streaked frame is a three: subject {:.3}, frame share {share:.3}, capped {rating:.3}",
            measured.score
        );
    }

    #[test]
    fn a_pan_is_rated_by_its_cap_not_by_its_sharpest_detail() {
        // A frame with no box is flagged a pan only when the ring round its
        // centre streaks one way; whatever the flag, its sharpest detail must
        // not lift the capped score back up.
        let whole = Sharpness {
            score: 0.78,
            panning: true,
            measured: true,
            ..Sharpness::default()
        };
        assert_eq!(
            subjectless_score(&whole, Some(1.0)),
            Some(whole.score),
            "taking the sharpest detail would undo the pan cap"
        );
        // A detail shot with no pan is still judged on its in-focus area.
        let shot = detail_shot(768, 0.06, 3);
        let measured = measure(&shot, None, None);
        assert!(!measured.panning);
        let focal = focal_score(&shot).expect("measurable tiles");
        assert_eq!(
            subjectless_score(&measured, Some(focal)),
            Some(measured.score.max(focal))
        );
    }

    #[test]
    fn sharp_all_over_is_never_worse_than_the_whole_frame_score() {
        let sharp = texture(768);
        let focal = focal_score(&sharp).expect("measurable tiles");
        assert!(focal >= measure(&sharp, None, None).score);
        // And where neither score is saturated, the finer grid only gains.
        let soft = blur(&sharp, 1);
        let focal = focal_score(&soft).expect("measurable tiles");
        assert!(focal >= measure(&soft, None, None).score);
    }

    #[test]
    fn blur_mapping_hits_its_anchors_and_never_turns_back() {
        for strictness in [
            BlurStrictness::Strict,
            BlurStrictness::Moderate,
            BlurStrictness::Lenient,
            BlurStrictness::Critical,
        ] {
            let scale = strictness.scale();
            assert_eq!(rating_from_blur(f64::NAN, strictness), None);
            let score = |v: f64| rating_from_blur(v, strictness).expect("finite sigma");
            assert!((score(0.0) - 1.0).abs() < 1e-12);
            for &(sigma, value) in scale {
                assert!(
                    (score(sigma) - value).abs() < 1e-12,
                    "{strictness:?} at {sigma}: {} vs {value}",
                    score(sigma)
                );
            }
            let first = scale[0].0;
            let last = scale[scale.len() - 1].0;
            assert!(score(last).abs() < 1e-12);
            assert!(score(10.0).abs() < 1e-12);
            // The top is a ramp, not a plateau: a sharper frame still ranks higher.
            let at_sharp = scale
                .iter()
                .find(|(_, value)| *value == SHARP_AT)
                .map(|&(sigma, _)| sigma)
                .expect("a point at SHARP_AT");
            assert!(score(first + 0.1) < 1.0);
            assert!(score(first + 0.1) > score((first + at_sharp) / 2.0));
            assert!(score((first + at_sharp) / 2.0) > score(at_sharp));
            // Monotone decreasing and inside 0..1 everywhere.
            let mut x = 0.0;
            let mut prev = score(0.0);
            while x < last + 2.0 {
                x += 0.01;
                let v = score(x);
                assert!(
                    v <= prev + 1e-12,
                    "score rose at {x:.2}: {prev:.3} -> {v:.3}"
                );
                assert!((0.0..=1.0).contains(&v), "score {v} out of range at {x:.2}");
                prev = v;
            }
        }
    }

    #[test]
    fn critical_gives_each_star_its_own_blur() {
        let score =
            |sigma: f64| rating_from_blur(sigma, BlurStrictness::Critical).expect("finite sigma");
        let stars = |sigma: f64| stars_for(score(sigma));
        assert_eq!(stars(0.5), 5);
        assert_eq!(stars(0.69), 5);
        assert_eq!(stars(0.71), 4);
        assert_eq!(stars(0.99), 4);
        assert_eq!(stars(1.01), 3);
        assert_eq!(stars(1.89), 3);
        assert_eq!(stars(1.91), 2);
        assert_eq!(stars(2.59), 2);
        assert_eq!(stars(2.61), 1);
        assert_eq!(stars(3.9), 1);
        assert!(score(4.0).abs() < 1e-12);
        assert!(score(6.0).abs() < 1e-12);
        let verdict = |sigma: f64| verdict_for(score(sigma), SHARP_AT, BLURRED_BELOW);
        assert_eq!(verdict(0.99), "sharp");
        assert_ne!(verdict(1.01), "sharp");
    }

    #[test]
    fn the_pan_cap_never_goes_below_three_stars() {
        assert_eq!(pan_share_cap(0.0), STAR_BANDS[2].0);
        assert_eq!(stars_for(pan_share_cap(0.0)), 3);
        assert_eq!(stars_for(pan_share_cap(1.0 / 3.0)), 3);
        assert_eq!(stars_for(pan_share_cap(0.5)), 4);
        assert_eq!(pan_share_cap(1.0), 1.0);
    }

    #[test]
    fn frame_sharp_share_counts_the_sharp_part_of_the_frame() {
        // Detail in a quarter of the frame, smooth gradient elsewhere.
        let shot = detail_shot(768, 0.25, 3);
        let share = frame_sharp_share(&shot).expect("a measurable frame");
        assert!(
            share > 0.05 && share < 0.5,
            "one sharp quarter reads as {share}"
        );
        let all_sharp = frame_sharp_share(&texture(768)).expect("a measurable frame");
        assert!(all_sharp > 0.8, "all sharp reads as {all_sharp}");
    }

    #[test]
    fn blur_verdicts_follow_strictness() {
        let verdict = |sigma: f64, strictness: BlurStrictness| {
            verdict_for(
                rating_from_blur(sigma, strictness).expect("finite sigma"),
                SHARP_AT,
                BLURRED_BELOW,
            )
        };
        assert_eq!(verdict(BLUR_SHARP, BlurStrictness::Strict), "sharp");
        assert_eq!(verdict(1.5, BlurStrictness::Strict), "sharp");
        assert_eq!(verdict(1.6, BlurStrictness::Strict), "soft");
        assert_eq!(verdict(2.2, BlurStrictness::Strict), "soft");
        assert_eq!(verdict(2.21, BlurStrictness::Strict), "blurred");
        // Heat-haze keepers, up to ~2.7 px, are not culled at Moderate.
        assert_ne!(verdict(2.7, BlurStrictness::Moderate), "blurred");
        assert_eq!(verdict(3.0, BlurStrictness::Moderate), "soft");
        assert_eq!(verdict(3.01, BlurStrictness::Moderate), "blurred");
    }

    #[test]
    fn looser_strictness_never_scores_a_frame_lower() {
        for i in 0..=60 {
            let sigma = f64::from(i) / 10.0;
            let at = |s: BlurStrictness| rating_from_blur(sigma, s).expect("finite sigma");
            assert!(at(BlurStrictness::Strict) <= at(BlurStrictness::Moderate) + 1e-12);
            assert!(at(BlurStrictness::Moderate) <= at(BlurStrictness::Lenient) + 1e-12);
            // Critical crosses Strict (it starts tighter) but never Moderate.
            assert!(at(BlurStrictness::Critical) <= at(BlurStrictness::Moderate) + 1e-12);
        }
    }

    #[test]
    fn blur_strictness_parses_known_names_and_unknown_is_moderate() {
        assert_eq!(BlurStrictness::parse("strict"), BlurStrictness::Strict);
        assert_eq!(BlurStrictness::parse("moderate"), BlurStrictness::Moderate);
        assert_eq!(BlurStrictness::parse("lenient"), BlurStrictness::Lenient);
        assert_eq!(BlurStrictness::parse("critical"), BlurStrictness::Critical);
        assert_eq!(BlurStrictness::parse("loose"), BlurStrictness::Moderate);
        assert_eq!(BlurStrictness::parse(""), BlurStrictness::Moderate);
    }
}
