//! Is the background around a subject streaked in one direction?
//!
//! A held pan is recognised by what the background looks like, not by how
//! blurred the subject is: the camera moved with the car, so everything but
//! the car is a wash of parallel streaks. Edges are crisp (the smear is
//! uniform, not a defocus), they are just all parallel, which is exactly what
//! a structure tensor over the background measures.
//!
//! # The measurement behind [`PAN_COHERENCE`]
//!
//! `crates/conrod-engine/src/pantest.rs` measured this feature on 48 hand
//! labelled pans and 90 hand labelled static frames (`target/pan/measure.csv`),
//! with the subject box from the detector or from the labels. The feature is
//! the mean, over 32x32 tiles of the background, of the per-tile coherence
//! `(l1 - l2) / (l1 + l2)` of the structure tensor of the Sobel gradients
//! (after one pixel of Gaussian, on a grey plane downscaled so the region's
//! long edge is at most 768 px). Tiles with a gradient energy below the
//! region's 30th percentile carry no direction worth counting and are skipped.
//!
//! - ROC AUC 0.988 for pan against static, and the same separation inside each
//!   shoot set (it is not one shoot's background doing the work).
//! - Pans: 10th percentile 0.826. Static: 90th percentile 0.787.
//! - So 0.80 sits in the gap: it keeps nearly every pan and lets through about
//!   a tenth of the static backgrounds, which the pan-compatible preset and
//!   the not-blurred subject gate then narrow further.
//!
//! Edge-blur sigma of the same ring does *not* separate the two (streaks are
//! crisp lines, not defocus), which is why the tile measure is used here.
//!
//! # The pan axis
//!
//! The same tiles answer *which way* the background is smeared: the weighted
//! circular mean of `2*theta` over the per-tile structure tensors, where
//! `theta` is a tile's dominant gradient orientation. The gradient runs across
//! a streak, so the streak direction -- the pan axis -- is that mean plus 90
//! degrees. Angles are degrees clockwise from image-right, folded to
//! `(-90, 90]`, so 0 is a horizontal pan and 90 one straight up or down.
//!
//! A single frame gives the *line* of a pan, not the arrow: motion blur is
//! symmetric, so panning left and panning right leave the same background.
//! [`Pan::axis_deg`] is therefore unsigned by nature, and `framing`'s
//! `Motion::AxisDeg` scores lead room symmetrically rather than inventing a
//! direction.
//!
//! Cost: about 23 ms per subject, one pass over the ring after an integer
//! area-average downscale.

use crate::imageops::Gray;

/// Ring coherence above which a background is a held pan's streaking.
/// Measured on 48 pans and 90 static frames: pan p10 0.826, static p90 0.787,
/// ROC AUC 0.988 (see `crates/conrod-engine/src/pantest.rs`).
pub const PAN_COHERENCE: f64 = 0.80;
/// Long edge (px) the structure is computed at: 32 px tiles need a plane of
/// about this size for a useful number of them, and the estimate is about the
/// arrangement of edges, not their native-pixel width.
const FEATURE_EDGE: usize = 768;
/// Tile edge in analysis pixels.
const TILE: usize = 32;
/// Analysis pixels around the subject box a tile must clear. The 1 px
/// pre-smooth and the Sobel reach into the box, and the box's own edges say
/// nothing about the background: without this gap a flat background would read
/// the box outline as a perfectly coherent set of edges.
const GAP: usize = 8;
/// Fewer usable tiles than this is not an answer.
const MIN_TILES: usize = 8;
/// Smallest region edge worth analysing.
const MIN_EDGE: usize = 48;
/// Tiles below this percentile of the region's gradient energy are noise.
const TRACE_PERCENTILE: f32 = 0.30;

/// One subject's panned background, measured from the ring around its box.
///
/// The two numbers come from the same tiles, so a caller pays for the analysis
/// once: the coherence the pan flag reads, and the axis lead room is measured
/// along.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pan {
    /// Mean per-tile coherence `(l1 - l2) / (l1 + l2)` of the structure
    /// tensor, 0..=1. Compare with [`PAN_COHERENCE`].
    pub coherence: f64,
    /// The direction the background is smeared along, in degrees clockwise
    /// from image-right, folded to (-90, 90]: 0 is a horizontal pan, 90 a
    /// vertical one. An axis, not an arrow (see the module notes).
    pub axis_deg: f64,
    /// How parallel the tiles' orientations are, 0..=1 (1 = every tile
    /// agrees). A low value means the mean axis is noise.
    pub consistency: f64,
}

/// The ring around `bbox`: the box grown by one width horizontally and one
/// height vertically, clipped to the frame, minus the box. `None` when there
/// is nothing to measure (no box, too small a frame, or too few usable tiles).
///
/// When the box is large (at least about a third of the frame in either
/// dimension) its one-box expansion covers the frame, so the whole frame
/// outside the box is used instead: a "ring" that is clipped to the frame edge
/// is not a ring, but the background is still the background.
pub fn background_pan(gray: &Gray, bbox: [f64; 4]) -> Option<Pan> {
    let (iw, ih) = (gray.width, gray.height);
    if iw < MIN_EDGE || ih < MIN_EDGE {
        return None;
    }
    let b = to_px(bbox, iw, ih);
    let (bw, bh) = (b[2].saturating_sub(b[0]), b[3].saturating_sub(b[1]));
    if bw == 0 || bh == 0 {
        return None;
    }
    let ring = [
        b[0].saturating_sub(bw),
        b[1].saturating_sub(bh),
        (b[2] + bw).min(iw),
        (b[3] + bh).min(ih),
    ];
    let ring_covers_frame = ring[0] == 0 && ring[1] == 0 && ring[2] == iw && ring[3] == ih;
    let region = if bw.saturating_mul(3) >= iw || bh.saturating_mul(3) >= ih || ring_covers_frame {
        [0, 0, iw, ih]
    } else {
        ring
    };
    let (rw, rh) = (region[2] - region[0], region[3] - region[1]);
    if rw < MIN_EDGE || rh < MIN_EDGE {
        return None;
    }

    let k = rw.max(rh).div_ceil(FEATURE_EDGE).max(1);
    let (src, w, h) = area_avg(gray, region, k);
    if w < MIN_EDGE || h < MIN_EDGE {
        return None;
    }

    // The box in the analysis plane, with the gap the subject's own edges
    // reach across. Tiles that touch it are the subject's edges, not the
    // background's.
    let boxed = [
        (b[0] - region[0]) / k,
        (b[1] - region[1]) / k,
        (b[2] - region[0]).div_ceil(k),
        (b[3] - region[1]).div_ceil(k),
    ];
    let keep_out = [
        boxed[0].saturating_sub(GAP),
        boxed[1].saturating_sub(GAP),
        (boxed[2] + GAP).min(w),
        (boxed[3] + GAP).min(h),
    ];

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
            // Sobel / 4, so the magnitudes are on `edgeblur`'s central
            // difference scale and its edge floors would mean the same thing.
            gx[i] = ((tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl)) / 4.0;
            gy[i] = ((bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr)) / 4.0;
        }
    }
    drop(p);

    let mut traces: Vec<f32> = Vec::new();
    let mut tensors: Vec<(f32, f32, f32)> = Vec::new();
    let mut ty = 0;
    while ty + TILE <= h {
        let mut tx = 0;
        while tx + TILE <= w {
            let near_box = tx < keep_out[2]
                && tx + TILE > keep_out[0]
                && ty < keep_out[3]
                && ty + TILE > keep_out[1];
            if !near_box {
                let (mut jxx, mut jxy, mut jyy) = (0f32, 0f32, 0f32);
                for y in (ty + 1)..(ty + TILE - 1) {
                    for x in (tx + 1)..(tx + TILE - 1) {
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
            tx += TILE;
        }
        ty += TILE;
    }
    if tensors.is_empty() {
        return None;
    }

    // The region's own 30th percentile of gradient energy: a flat sky or a
    // smooth area has no direction to report, and counting it as incoherent
    // would dilute the streaks that matter.
    let mut sorted_traces = traces.clone();
    sorted_traces.sort_by(f32::total_cmp);
    let floor = pick(&sorted_traces, TRACE_PERCENTILE);

    let (mut sum, mut n, mut z, mut s) = (0f64, 0usize, 0f64, 0f64);
    for (&(jxx, jxy, jyy), &t) in tensors.iter().zip(traces.iter()) {
        if t <= 0.0 || t < floor {
            continue;
        }
        let mean = (jxx + jyy) / 2.0;
        let disc = (((jxx - jyy) / 2.0).powi(2) + jxy * jxy).sqrt();
        let (l1, l2) = (mean + disc, mean - disc);
        if l1 + l2 <= 0.0 {
            continue;
        }
        let c = ((l1 - l2) / (l1 + l2)) as f64;
        // The tile's gradient orientation; the mean of the doubled angle is
        // the circular mean of a line, which has no 180-degree ambiguity.
        let theta = 0.5 * ((2.0f32 * jxy).atan2(jxx - jyy) as f64);
        sum += c;
        z += c * (2.0 * theta).cos();
        s += c * (2.0 * theta).sin();
        n += 1;
    }
    if n < MIN_TILES || sum <= 0.0 {
        return None;
    }
    // The streak is across the gradient, so the pan axis is the mean gradient
    // orientation plus 90 degrees, folded back onto the half turn.
    let gradient_deg = 0.5 * s.atan2(z).to_degrees();
    Some(Pan {
        coherence: sum / n as f64,
        axis_deg: fold_deg(gradient_deg + 90.0),
        consistency: z.hypot(s) / sum,
    })
}

/// The pan measure's coherence alone, for the callers that only ask the
/// yes/no question (the pan flag, the tests, the measurement harness).
pub fn background_streaks(gray: &Gray, bbox: [f64; 4]) -> Option<f64> {
    background_pan(gray, bbox).map(|pan| pan.coherence)
}

/// Fold an angle in degrees into (-90, 90]: a line has no arrow, so 100 and
/// -80 are the same direction.
fn fold_deg(deg: f64) -> f64 {
    let d = deg.rem_euclid(180.0);
    if d > 90.0 {
        d - 180.0
    } else {
        d
    }
}

/// The box in pixels, clamped to the frame.
fn to_px(b: [f64; 4], w: usize, h: usize) -> [usize; 4] {
    [
        (b[0].max(0.0) as usize).min(w),
        (b[1].max(0.0) as usize).min(h),
        (b[2].max(0.0) as usize).min(w),
        (b[3].max(0.0) as usize).min(h),
    ]
}

/// `k` x `k` area average straight from the grey bytes; the remainder columns
/// and rows are dropped, as `edgeblur` does.
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
/// Shared with `level`, which measures edge angles on the same smoothed plane.
pub(crate) fn smooth_1px(src: &[f32], w: usize, h: usize) -> Vec<f32> {
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
        return f32::NAN;
    }
    let i = (((v.len() - 1) as f32) * p).round() as usize;
    v[i.min(v.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(x: usize, y: usize, seed: u64) -> u32 {
        let mut h = (x as u32)
            .wrapping_mul(0x9E37_79B9)
            .wrapping_add(seed as u32)
            ^ (y as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 13;
        h = h.wrapping_mul(0xC2B2_AE35);
        h ^ (h >> 16)
    }

    /// Isotropic value noise at several block scales: edges in every
    /// direction, like a static background or a real subject.
    fn texture(w: usize, h: usize, seed: u64) -> Vec<u8> {
        let mut data = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut v = 0.0;
                for (block, amp) in [(16usize, 90.0), (8, 60.0), (4, 40.0), (2, 25.0)] {
                    let n = hash(x / block, y / block, seed) % 256;
                    v += amp * (f64::from(n) / 255.0 - 0.5);
                }
                data[y * w + x] = (128.0 + v).clamp(0.0, 255.0) as u8;
            }
        }
        data
    }

    /// A panned background: everything smeared along x, so what is left is a
    /// wash of horizontal streaks -- value varies with y only.
    fn pan_background(w: usize, h: usize) -> Vec<u8> {
        let bands = texture(1, h, 21);
        let mut data = vec![0u8; w * h];
        for y in 0..h {
            data[y * w..(y + 1) * w].fill(bands[y]);
        }
        data
    }

    /// The same smear turned on its side: vertical streaks, a vertical pan.
    fn vertical_pan_background(w: usize, h: usize) -> Vec<u8> {
        let bands = texture(w, 1, 21);
        let mut data = vec![0u8; w * h];
        for y in 0..h {
            data[y * w..(y + 1) * w].copy_from_slice(&bands);
        }
        data
    }

    /// The frame with a sharp textured subject pasted into the box.
    fn frame(mut data: Vec<u8>, w: usize, h: usize, box_px: [usize; 4], seed: u64) -> Gray {
        let [x0, y0, x1, y1] = box_px;
        let subject = texture(x1 - x0, y1 - y0, seed);
        for y in y0..y1 {
            let row = &subject[(y - y0) * (x1 - x0)..(y - y0 + 1) * (x1 - x0)];
            data[y * w + x0..y * w + x1].copy_from_slice(row);
        }
        Gray::new(w, h, data)
    }

    const W: usize = 640;
    const H: usize = 480;
    const BOX: [f64; 4] = [220.0, 170.0, 420.0, 310.0];

    #[test]
    fn horizontal_streaks_read_as_a_pan() {
        let g = frame(pan_background(W, H), W, H, px(BOX), 7);
        let s = background_streaks(&g, BOX).expect("streaks measured");
        println!("panned background coherence {s:.3}");
        assert!(s >= PAN_COHERENCE, "panned background read {s:.3}");
    }

    #[test]
    fn isotropic_texture_does_not_read_as_a_pan() {
        let g = frame(texture(W, H, 3), W, H, px(BOX), 7);
        let s = background_streaks(&g, BOX).expect("texture measured");
        println!("isotropic background coherence {s:.3}");
        assert!(s < PAN_COHERENCE, "static background read {s:.3}");
    }

    #[test]
    fn the_pan_axis_is_the_streak_direction() {
        // A horizontal smear is a pan along x: 0 degrees.
        let g = frame(pan_background(W, H), W, H, px(BOX), 7);
        let pan = background_pan(&g, BOX).expect("pan measured");
        println!("horizontal pan {pan:?}");
        assert!(pan.coherence >= PAN_COHERENCE, "{pan:?}");
        assert!(
            pan.axis_deg.abs() < 10.0,
            "horizontal pan read as {:.1} deg",
            pan.axis_deg
        );
        assert!(pan.consistency > 0.9, "tiles disagree: {pan:?}");
        // Turned on its side the axis is vertical, and folding makes
        // up-or-down the same 90 degrees.
        let g = frame(vertical_pan_background(W, H), W, H, px(BOX), 7);
        let pan = background_pan(&g, BOX).expect("vertical pan measured");
        println!("vertical pan {pan:?}");
        assert!(
            (pan.axis_deg.abs() - 90.0).abs() < 10.0,
            "vertical pan read as {:.1} deg",
            pan.axis_deg
        );
        // A still background has no axis worth using: the flag's own number
        // stays below the threshold, and the tile orientations disagree.
        let g = frame(texture(W, H, 3), W, H, px(BOX), 7);
        let pan = background_pan(&g, BOX).expect("texture measured");
        assert!(pan.coherence < PAN_COHERENCE, "{pan:?}");
        assert!(pan.consistency < 0.5, "a still frame looked parallel: {pan:?}");
    }

    #[test]
    fn a_folded_line_has_no_arrow() {
        assert!((fold_deg(0.0)).abs() < 1e-9);
        assert!((fold_deg(180.0)).abs() < 1e-9);
        assert!((fold_deg(90.0) - 90.0).abs() < 1e-9);
        assert!((fold_deg(100.0) + 80.0).abs() < 1e-9);
        assert!((fold_deg(-100.0) - 80.0).abs() < 1e-9);
    }

    #[test]
    fn a_flat_background_has_no_answer() {
        let g = frame(vec![128; W * H], W, H, px(BOX), 7);
        assert_eq!(background_streaks(&g, BOX), None);
    }

    #[test]
    fn the_subject_does_not_set_the_answer() {
        let mut g = frame(texture(W, H, 3), W, H, px(BOX), 7);
        let a = background_streaks(&g, BOX).unwrap();
        // Repaint the box interior with something wildly different: the ring's
        // answer must not move.
        let [x0, y0, x1, y1] = px(BOX);
        for y in y0..y1 {
            g.data[y * W + x0..y * W + x1].fill(if y % 2 == 0 { 0 } else { 255 });
        }
        let b = background_streaks(&g, BOX).unwrap();
        assert_eq!(a.to_bits(), b.to_bits(), "{a:.3} vs {b:.3}");
    }

    fn px(b: [f64; 4]) -> [usize; 4] {
        [b[0] as usize, b[1] as usize, b[2] as usize, b[3] as usize]
    }
}
