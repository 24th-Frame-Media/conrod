//! Exposure problems: highlights the sensor or the JPEG preview had already
//! blown, shadows that are crushed, and frames that are simply too dark to be
//! anything but a lens cap or a flash that never fired.
//!
//! This measures the camera's embedded JPEG preview, not the RAW. A preview has
//! its own tone curve and its own (8-bit) clipping point, so a blown highlight
//! here means "gone in the preview", which is what a cull decision needs, but
//! it is approximate: the RAW may still hold a stop above it, and a picture
//! processed with lifted shadows can crush in the preview while the RAW is
//! fine. Thresholds are therefore forgiving, and the verdict flags the frames
//! worth a second look rather than judging exposure.
//!
//! The whole measure runs on a grid of at most [`WORK_EDGE`] on the long edge.
//! Each grid cell reads one contiguous 2x2 block of pixels at the cell's
//! centre, so a 24-megapixel preview costs a quarter of a million blocks and
//! the measure stays in the low milliseconds. Cells are never averaged:
//! downscaling with a mean would pull a blown window's edge back under the
//! clipping level and make the clipping disappear as the frame gets bigger, so
//! the block's *maximum* channel decides whether that block is blown. Detail is
//! read from how much neighbouring cells differ, which costs nothing extra.

use crate::imageops::{thumbnail_size, Rgb};

/// Long edge of the sampling grid. Big enough that a blown window is a
/// recognisable blob, small enough that the measure costs ~1M pixel reads.
pub const WORK_EDGE: usize = 512;

/// A block is blown when any of its samples carries a channel at or above this
/// (of 255). 250 rather than 255 leaves room for the preview's own rounding and
/// JPEG ringing: a pixel the camera pushed to the top has neighbours that stop
/// one or two levels short.
pub const HIGHLIGHT_LEVEL: u8 = 250;

/// A block is crushed when its luma is at or below this (of 255): shadow
/// detail is gone, the noise floor shows through.
pub const SHADOW_LEVEL: u8 = 4;

/// A connected blown region smaller than this fraction of the frame is a
/// specular speck (a chrome tap, a bulb) and is not counted as a blown window.
pub const WINDOW_MIN_AREA: f64 = 0.005;

/// The top 30% of the frame is the band a sky lives in when the camera is held
/// level. Interiors rarely keep a bright, textureless band there.
pub const SKY_BAND: f64 = 0.30;

/// The band must be this bright (mean luma, 0..1) to be taken for sky.
pub const SKY_MIN_LUMA: f64 = 0.55;

/// Mean per-cell luma step to the neighbouring cell (0..1) under which the
/// band is "low texture". A clear sky changes by a level or two across the
/// frame; a wall with window frames and mullions changes a great deal.
pub const SKY_MAX_TEXTURE: f64 = 0.06;

/// The band must be bright across this fraction of the frame's width: a sky
/// runs edge to edge, a single blown window does not.
pub const SKY_MIN_COLUMNS: f64 = 0.85;

/// A band column counts as bright above this mean luma (0..1).
pub const SKY_COLUMN_LUMA: f64 = 0.40;

/// The clipping must be at least this share of the whole frame's clipping for
/// the band to explain it.
pub const SKY_SHARE: f64 = 0.50;

/// Below this mean luma (0..1) the frame is a lens cap or a flash misfire.
pub const DARK_MEAN: f64 = 0.04;

/// Clip inside the subject box above this fraction is a blown subject.
pub const SUBJECT_CLIP: f64 = 0.05;

/// Frame area covered by large blown regions above which the frame reads as
/// blown windows in an interior.
pub const WINDOW_CLIP: f64 = 0.08;

/// Whole-frame clip above this is suspicious on its own, unless a sky explains
/// it.
pub const HIGHLIGHT_CLIP: f64 = 0.15;

/// Crushed area above this fraction is a shadow problem, when the frame is also
/// underexposed overall...
pub const SHADOW_CLIP: f64 = 0.35;

/// ...which "underexposed overall" means: mean luma below this (0..1).
pub const SHADOW_MEAN: f64 = 0.15;
/// A cell is clipped white when every channel of its whole 2x2 block is at or
/// above this (of 255): the preview carries no tone, and no colour, there. The
/// encoder's own top is 255, so this sits two levels under it for rounding.
pub const WHITE_LEVEL: u8 = 253;
/// A cell is clipped black when every channel of its whole 2x2 block is at or
/// below this (of 255): under the sensor's noise floor, so nothing is left to
/// lift.
pub const BLACK_LEVEL: u8 = 6;

/// What the frame's exposure looks like. See [`measure`].
#[derive(Debug, Clone, PartialEq)]
pub struct Exposure {
    /// Mean luma of the frame, 0..1.
    pub mean: f64,
    /// Fraction of the frame with any channel at or above [`HIGHLIGHT_LEVEL`]
    /// in a 2x2 block.
    pub highlight_clip: f64,
    /// Fraction of the frame at or below [`SHADOW_LEVEL`] luma.
    pub shadow_clip: f64,
    /// Clipping inside the top [`SKY_BAND`] band (a fraction of the band, not
    /// of the frame), when that band is bright, smooth and edge-to-edge (a
    /// sky, so its clipping is not a fault).
    pub sky_clip: Option<f64>,
    /// Fraction of the frame inside connected blown regions larger than
    /// [`WINDOW_MIN_AREA`]: blown windows in an interior.
    pub window_clip: f64,
    /// Clipping inside the subject box, when one was given.
    pub subject_clip: Option<f64>,
    /// Share of the judged region (the subject box when one was given and
    /// fits the frame, else the whole frame) at clipped white
    /// ([`WHITE_LEVEL`]): detail the preview cannot give back.
    pub white_clip: f64,
    /// Share of the judged region at clipped black ([`BLACK_LEVEL`]).
    pub black_clip: f64,
    /// Whether the two shares above were read inside a subject box.
    pub clip_on_subject: bool,
    /// Mean below [`DARK_MEAN`].
    pub dark_frame: bool,
    /// `"ok"`, `"highlights"`, `"shadows"` or `"dark"`.
    pub verdict: &'static str,
}

impl Exposure {
    /// Nothing measurable: an empty or undecodable frame.
    fn empty() -> Exposure {
        Exposure {
            mean: 0.0,
            highlight_clip: 0.0,
            shadow_clip: 0.0,
            sky_clip: None,
            window_clip: 0.0,
            subject_clip: None,
            white_clip: 0.0,
            black_clip: 1.0,
            clip_on_subject: false,
            dark_frame: true,
            verdict: "dark",
        }
    }
}

/// Measure a frame's exposure. `subject` is an optional box in normalised
/// frame coordinates, `[x0, y0, x1, y1]` with 1.0 the far edge; a box that
/// falls outside the frame, or is inverted, yields no `subject_clip` rather
/// than a panic.
///
/// The verdict is the first of: `"dark"` ([`DARK_MEAN`]), `"highlights"`
/// (subject clip > [`SUBJECT_CLIP`], large blown regions > [`WINDOW_CLIP`]
/// with no sky to explain them, or whole-frame clip > [`HIGHLIGHT_CLIP`] with
/// no sky), `"shadows"` (crushed area > [`SHADOW_CLIP`] while the mean is under
/// [`SHADOW_MEAN`]), else `"ok"`.
pub fn measure(rgb: &Rgb, subject: Option<[f64; 4]>) -> Exposure {
    let (gw, gh) = grid(rgb.width, rgb.height);
    if gw == 0 || gh == 0 {
        return Exposure::empty();
    }
    let cells = Cells::sample(rgb, gw, gh);
    let total = (gw * gh) as f64;

    let mean = cells.luma.iter().map(|&l| f64::from(l)).sum::<f64>() / total;
    let highlight_clip = cells.blown.iter().filter(|&&b| b).count() as f64 / total;
    let shadow_clip = cells
        .luma
        .iter()
        .filter(|&&l| l <= f32::from(SHADOW_LEVEL) / 255.0)
        .count() as f64
        / total;
    let dark_frame = mean < DARK_MEAN;

    let sky_clip = sky(&cells, highlight_clip);
    let window_clip = window(&cells);
    let subject_clip = subject.and_then(|box_| subject_clip(&cells, box_));

    let verdict = if dark_frame {
        "dark"
    } else if subject_clip.is_some_and(|s| s > SUBJECT_CLIP)
        || (window_clip > WINDOW_CLIP && sky_clip.is_none())
        || (highlight_clip > HIGHLIGHT_CLIP && sky_clip.is_none())
    {
        "highlights"
    } else if shadow_clip > SHADOW_CLIP && mean < SHADOW_MEAN {
        "shadows"
    } else {
        "ok"
    };

    let (white_clip, black_clip, clip_on_subject) = region_clips(&cells, subject);

    Exposure {
        mean,
        highlight_clip,
        shadow_clip,
        sky_clip,
        window_clip,
        subject_clip,
        white_clip,
        black_clip,
        clip_on_subject,
        dark_frame,
        verdict,
    }
}

/// The shares of clipped white and clipped black cells inside the subject box,
/// or over the whole frame when there is no box (or it misses the frame), and
/// whether a box was used.
fn region_clips(cells: &Cells, subject: Option<[f64; 4]>) -> (f64, f64, bool) {
    let clamp = |v: f64, n: usize| (v * n as f64).round().clamp(0.0, n as f64) as usize;
    let (x0, x1, y0, y1, on_subject) = match subject {
        Some(b)
            if clamp(b[0], cells.width) < clamp(b[2], cells.width)
                && clamp(b[1], cells.height) < clamp(b[3], cells.height) =>
        {
            (
                clamp(b[0], cells.width),
                clamp(b[2], cells.width),
                clamp(b[1], cells.height),
                clamp(b[3], cells.height),
                true,
            )
        }
        _ => (0, cells.width, 0, cells.height, false),
    };
    let total = ((x1 - x0) * (y1 - y0)).max(1) as f64;
    let (mut white, mut black) = (0usize, 0usize);
    for y in y0..y1 {
        for x in x0..x1 {
            let i = cells.at(x, y);
            white += usize::from(cells.white[i]);
            black += usize::from(cells.black[i]);
        }
    }
    (white as f64 / total, black as f64 / total, on_subject)
}

/// The sampling grid: the frame's aspect at [`WORK_EDGE`], or the frame itself
/// when it already fits. Zero when the frame has no pixels.
fn grid(width: usize, height: usize) -> (usize, usize) {
    if width == 0 || height == 0 {
        return (0, 0);
    }
    thumbnail_size(width, height, WORK_EDGE).unwrap_or((width, height))
}

/// Per-cell statistics, one entry per grid cell, row-major.
struct Cells {
    luma: Vec<f32>,
    /// How much the cell differs from its right and lower neighbours, 0..1:
    /// large over detail, near zero over a clear sky.
    texture: Vec<f32>,
    blown: Vec<bool>,
    /// Every channel of the block at or above [`WHITE_LEVEL`].
    white: Vec<bool>,
    /// Every channel of the block at or below [`BLACK_LEVEL`].
    black: Vec<bool>,
    width: usize,
    height: usize,
}

impl Cells {
    /// Read the 2x2 block at each cell's centre. One block per cell keeps the
    /// reads on a couple of cache lines each: at 512 cells across a 24-megapixel
    /// preview, walking four scattered pixels per cell instead doubles the
    /// measure's time for no better statistics.
    fn sample(rgb: &Rgb, gw: usize, gh: usize) -> Cells {
        let (w, h) = (rgb.width, rgb.height);
        let (cw, ch) = (w as f64 / gw as f64, h as f64 / gh as f64);
        let mut lumas = Vec::with_capacity(gw * gh);
        let mut blown = Vec::with_capacity(gw * gh);
        let mut white = Vec::with_capacity(gw * gh);
        let mut black = Vec::with_capacity(gw * gh);
        for gy in 0..gh {
            let y0 = (((gy as f64 + 0.5) * ch) as usize).min(h.saturating_sub(2));
            let y1 = (y0 + 1).min(h - 1);
            for gx in 0..gw {
                let x0 = (((gx as f64 + 0.5) * cw) as usize).min(w.saturating_sub(2));
                let x1 = (x0 + 1).min(w - 1);
                let (mut sum, mut peak, mut floor) = (0u32, 0u8, 255u8);
                for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
                    let at = (y * w + x) * 3;
                    let (r, g, b) = (rgb.data[at], rgb.data[at + 1], rgb.data[at + 2]);
                    peak = peak.max(r).max(g).max(b);
                    floor = floor.min(r).min(g).min(b);
                    sum += u32::from(luma(r, g, b));
                }
                lumas.push(sum as f32 / (4.0 * 255.0));
                blown.push(peak >= HIGHLIGHT_LEVEL);
                white.push(floor >= WHITE_LEVEL);
                black.push(peak <= BLACK_LEVEL);
            }
        }
        let mut cells = Cells {
            luma: lumas,
            texture: Vec::new(),
            blown,
            white,
            black,
            width: gw,
            height: gh,
        };
        cells.texture = cells.gradient();
        cells
    }

    /// The mean absolute luma step to the right and downward neighbour, over
    /// the neighbours a cell has. Smooth sky is near zero; detail is not.
    fn gradient(&self) -> Vec<f32> {
        (0..self.height)
            .flat_map(|y| {
                (0..self.width).map(move |x| {
                    let i = y * self.width + x;
                    let (mut sum, mut n) = (0f32, 0);
                    if x + 1 < self.width {
                        sum += (self.luma[i + 1] - self.luma[i]).abs();
                        n += 1;
                    }
                    if y + 1 < self.height {
                        sum += (self.luma[i + self.width] - self.luma[i]).abs();
                        n += 1;
                    }
                    if n == 0 {
                        0.0
                    } else {
                        sum / n as f32
                    }
                })
            })
            .collect()
    }

    fn at(&self, x: usize, y: usize) -> usize {
        y * self.width + x
    }

    /// Mean luma of one column inside the sky band's rows.
    fn column_band_luma(&self, x: usize, rows: usize) -> f64 {
        (0..rows)
            .map(|y| f64::from(self.luma[self.at(x, y)]))
            .sum::<f64>()
            / rows as f64
    }
}

/// ITU-R 601 luma in Pillow's 16-bit fixed point, as `Gray::from_rgb` does it,
/// so the mean agrees with the sharpness path's greyscale.
fn luma(r: u8, g: u8, b: u8) -> u8 {
    let (r, g, b) = (u32::from(r), u32::from(g), u32::from(b));
    ((r * 19595 + g * 38470 + b * 7471 + 0x8000) >> 16) as u8
}

/// The band's clip fraction when the top band looks like sky. A blown sky is
/// the one highlight clip that is not a fault, so its presence also excuses the
/// whole-frame and window tests in the verdict.
fn sky(cells: &Cells, highlight_clip: f64) -> Option<f64> {
    let rows = ((cells.height as f64 * SKY_BAND).round() as usize).clamp(1, cells.height);
    let band = rows * cells.width;
    let band_luma = (0..rows)
        .flat_map(|y| (0..cells.width).map(move |x| cells.at(x, y)))
        .map(|i| f64::from(cells.luma[i]))
        .sum::<f64>()
        / band as f64;
    let band_texture = (0..rows)
        .flat_map(|y| (0..cells.width).map(move |x| cells.at(x, y)))
        .map(|i| f64::from(cells.texture[i]))
        .sum::<f64>()
        / band as f64;
    let band_clip = (0..rows)
        .flat_map(|y| (0..cells.width).map(move |x| cells.at(x, y)))
        .filter(|&i| cells.blown[i])
        .count() as f64
        / band as f64;
    let bright_columns = (0..cells.width)
        .filter(|&x| cells.column_band_luma(x, rows) > SKY_COLUMN_LUMA)
        .count() as f64
        / cells.width as f64;

    let is_sky = band_luma > SKY_MIN_LUMA
        && band_texture < SKY_MAX_TEXTURE
        && bright_columns > SKY_MIN_COLUMNS
        && (highlight_clip == 0.0 || band_clip >= SKY_SHARE * highlight_clip);
    is_sky.then_some(band_clip)
}

/// The frame's area inside connected blown regions of at least
/// [`WINDOW_MIN_AREA`], as a fraction. Four-connected: a window is one blob,
/// JPEG speckle around it is not joined by diagonals.
fn window(cells: &Cells) -> f64 {
    let total = cells.width * cells.height;
    let min_cells = (WINDOW_MIN_AREA * total as f64).ceil().max(1.0) as usize;
    let mut seen = vec![false; total];
    let mut stack = Vec::new();
    let mut covered = 0usize;
    for start in 0..total {
        if seen[start] || !cells.blown[start] {
            continue;
        }
        seen[start] = true;
        stack.push(start);
        let mut region = Vec::new();
        while let Some(i) = stack.pop() {
            region.push(i);
            let (x, y) = (i % cells.width, i / cells.width);
            let mut push = |j: usize| {
                if !seen[j] && cells.blown[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            };
            if x > 0 {
                push(i - 1);
            }
            if x + 1 < cells.width {
                push(i + 1);
            }
            if y > 0 {
                push(i - cells.width);
            }
            if y + 1 < cells.height {
                push(i + cells.width);
            }
        }
        if region.len() >= min_cells {
            covered += region.len();
        }
    }
    covered as f64 / total as f64
}

/// Clipping inside the normalised box, or `None` when the box misses the frame.
fn subject_clip(cells: &Cells, box_: [f64; 4]) -> Option<f64> {
    let clamp = |v: f64, n: usize| (v * n as f64).round().clamp(0.0, n as f64) as usize;
    let (x0, x1) = (clamp(box_[0], cells.width), clamp(box_[2], cells.width));
    let (y0, y1) = (clamp(box_[1], cells.height), clamp(box_[3], cells.height));
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let cells_in_box = (x1 - x0) * (y1 - y0);
    let blown = (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| cells.at(x, y)))
        .filter(|&i| cells.blown[i])
        .count();
    Some(blown as f64 / cells_in_box as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of one tone, at a size that gives the grid 2x2 cells.
    fn flat(width: usize, height: usize, tone: u8) -> Rgb {
        Rgb::new(width, height, vec![tone; width * height * 3])
    }

    /// Paint a rectangle, in normalised frame coordinates, onto a frame.
    fn paint(rgb: &mut Rgb, box_: [f64; 4], tone: u8) {
        let x0 = (box_[0] * rgb.width as f64) as usize;
        let x1 = (box_[2] * rgb.width as f64) as usize;
        let y0 = (box_[1] * rgb.height as f64) as usize;
        let y1 = (box_[3] * rgb.height as f64) as usize;
        for y in y0..y1.min(rgb.height) {
            for x in x0..x1.min(rgb.width) {
                let at = (y * rgb.width + x) * 3;
                rgb.data[at..at + 3].fill(tone);
            }
        }
    }

    #[test]
    fn a_mid_grey_frame_is_ok() {
        let e = measure(&flat(1200, 800, 128), None);
        assert!((e.mean - 0.502).abs() < 0.01, "mean {}", e.mean);
        assert_eq!(e.highlight_clip, 0.0);
        assert_eq!(e.shadow_clip, 0.0);
        assert_eq!(e.window_clip, 0.0);
        assert_eq!(e.sky_clip, None);
        assert!(!e.dark_frame);
        assert_eq!(e.verdict, "ok");
    }

    #[test]
    fn a_white_window_region_reads_as_highlights() {
        // A fifth of the frame, below the sky band, fully blown, in one blob.
        let mut rgb = flat(1200, 800, 110);
        paint(&mut rgb, [0.30, 0.35, 0.80, 0.75], 255);
        let e = measure(&rgb, None);
        assert!(
            (e.window_clip - 0.20).abs() < 0.03,
            "window {}",
            e.window_clip
        );
        assert!(
            (e.highlight_clip - 0.20).abs() < 0.03,
            "clip {}",
            e.highlight_clip
        );
        assert_eq!(e.sky_clip, None);
        assert_eq!(e.verdict, "highlights");
    }

    #[test]
    fn a_bright_smooth_sky_band_is_not_a_fault() {
        // Sky over the top third, blown; the rest of the frame is exposed.
        let mut rgb = flat(1200, 800, 120);
        paint(&mut rgb, [0.0, 0.0, 1.0, 0.30], 255);
        let e = measure(&rgb, None);
        // The sky is blown edge to edge, so the band's own clip is all of it.
        let sky = e.sky_clip.expect("the top band is a sky");
        assert!(sky > 0.95, "sky {sky}");
        assert!(e.window_clip > WINDOW_CLIP, "window {}", e.window_clip);
        assert_eq!(e.verdict, "ok");
    }

    #[test]
    fn a_black_frame_is_dark() {
        let e = measure(&flat(1200, 800, 0), None);
        assert!(e.dark_frame);
        assert_eq!(e.verdict, "dark");
    }

    #[test]
    fn a_blown_subject_box_reads_as_highlights() {
        // A blown patch too small to be a window and too small to lift the
        // whole-frame clip, but the box is all of it.
        let mut rgb = flat(1600, 1000, 100);
        paint(&mut rgb, [0.62, 0.55, 0.70, 0.62], 255);
        let e = measure(&rgb, Some([0.62, 0.55, 0.70, 0.62]));
        assert_eq!(e.subject_clip, Some(1.0));
        assert!(
            e.highlight_clip < HIGHLIGHT_CLIP,
            "clip {}",
            e.highlight_clip
        );
        assert!(e.window_clip < WINDOW_CLIP, "window {}", e.window_clip);
        assert_eq!(e.sky_clip, None);
        assert_eq!(e.verdict, "highlights");
    }

    #[test]
    fn crushed_shadows_read_as_shadows() {
        let mut rgb = flat(1200, 800, 0);
        paint(&mut rgb, [0.0, 0.5, 1.0, 1.0], 60);
        let e = measure(&rgb, None);
        assert!(e.mean < SHADOW_MEAN, "mean {}", e.mean);
        assert!(e.shadow_clip > SHADOW_CLIP, "shadows {}", e.shadow_clip);
        assert_eq!(e.verdict, "shadows");
    }

    #[test]
    fn a_box_outside_the_frame_or_inverted_has_no_clip() {
        let mut rgb = flat(1200, 800, 128);
        paint(&mut rgb, [0.0, 0.0, 1.0, 1.0], 255);
        assert_eq!(measure(&rgb, Some([1.2, 1.2, 1.5, 1.5])).subject_clip, None);
        assert_eq!(measure(&rgb, Some([0.6, 0.2, 0.4, 0.9])).subject_clip, None);
        assert_eq!(
            measure(&rgb, Some([0.0, 0.0, 1.0, 1.0])).subject_clip,
            Some(1.0)
        );
    }

    #[test]
    fn an_empty_frame_is_dark_not_a_panic() {
        let e = measure(&Rgb::new(0, 0, Vec::new()), None);
        assert_eq!(e.verdict, "dark");
    }

    #[test]
    fn a_measure_of_a_full_size_preview_is_fast() {
        // 24 megapixels, the shape a full-size embedded preview has.
        let rgb = flat(6000, 4000, 128);
        // The rest of this crate's tests run on the same cores, so the
        // fastest of a few runs is the honest read of the measure's cost.
        let mut spent = std::time::Duration::MAX;
        let mut verdict = "";
        for _ in 0..5 {
            let start = std::time::Instant::now();
            let e = measure(&rgb, None);
            spent = spent.min(start.elapsed());
            verdict = e.verdict;
        }
        assert_eq!(verdict, "ok");
        // CI runs unoptimised builds on shared runners, where this measure sits
        // right at 10 ms (one run read 10.2 ms); the guard is for a regression
        // of several times, not for scheduler noise.
        let budget = if cfg!(debug_assertions) { 40 } else { 10 };
        assert!(
            spent < std::time::Duration::from_millis(budget),
            "measure took {spent:?} (budget {budget} ms)"
        );
    }
}
