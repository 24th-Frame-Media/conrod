//! How many pixels wide is the blur that softened an edge?
//!
//! Gradient-ratio defocus estimation (Zhuo & Sim, "Defocus map estimation from
//! a single blurred image", 2011). An edge blurred by `sigma`, blurred again by
//! `sigma0`, loses gradient magnitude in the ratio
//! `R = sqrt(sigma^2 + sigma0^2) / sigma`, so `sigma = sigma0 / sqrt(R^2 - 1)`.
//! The answer is in native pixels, per edge direction, and does not care how
//! much texture the edge sits in -- which is what the tile-based score is
//! sensitive to.
//!
//! Calibrated constants (native pixels; gradients in 0..255 luma units, after
//! the 0.5 factor of the central difference cancels in the ratio):
//! - `PRE_SMOOTH = 0.6`: pre-blur that tames noise and JPEG blocks. Its
//!   variance is subtracted in quadrature afterwards, clamped at `MIN_SIGMA`.
//! - `SIGMA0 = 1.0`: the re-blur whose loss of gradient measures the blur.
//!   With it, R saturates at 1.01 (a flat-topped 8 px box edge reads as 7 px
//!   of Gaussian, since no Gaussian re-blur can narrow it): motion blur longer
//!   than about 6 native px all lands near the top of the scale.
//! - `MIN_SIGMA = 0.3`: the floor of what a sharp edge can read as.
//! - Edge floor = `max(2.0, 3.16 * p20(|Laplacian|), 0.2 * p98(rgm))`. The
//!   Laplacian term is a white-noise estimate: for white noise `p20(|L|)`
//!   is `0.2533 * sigma_L`, and the gradient of the pre-smoothed plane has
//!   about `0.2 * sigma_L`, so this is four gradient sigmas -- which is above
//!   every noise gradient and below every real edge.
//! - The pooled value is the 20th percentile of the per-edge estimates: depth
//!   of field legitimately leaves most of a subject soft, and the question is
//!   whether *any* of it is sharp.
//!
//! Cost and scale:
//! - Only the asked-for box plus [`PAD`] pixels of kernel reach is examined,
//!   never the whole frame: [`measure`] is a box measure.
//! - Above [`ANALYSIS_MAX_PIXELS`] the box is area-averaged by an integer
//!   factor `k` before measuring, and every reported sigma is multiplied by
//!   `k`: blur is linear in resolution, so the answer keeps its native-pixel
//!   meaning. (The `MIN_SIGMA` floor is applied at analysis resolution, so a
//!   downscaled very sharp edge reads up to `k * MIN_SIGMA` rather than
//!   `MIN_SIGMA`; that stays well inside the sharp threshold.)
//! - [`at_reference`] re-expresses a native-pixel sigma at
//!   [`REFERENCE_EDGE`], so a small preview and a large one are judged alike.

use crate::imageops::Gray;

/// Blur of the sharpest edges, all directions pooled (px); NaN if unmeasured.
#[derive(Debug, Clone, Copy)]
pub struct EdgeBlur {
    pub sigma: f64,
    /// Lowest per-direction value.
    pub sigma_best: f64,
    /// Highest per-direction value.
    pub sigma_worst: f64,
    /// Index of that direction bin (edge-normal angle: 0=0deg, 1=45, 2=90, 3=135).
    pub worst_dir: u8,
    /// Per-bin value, NaN where the bin had too few edges.
    pub dirs: [f64; 4],
    /// Edge pixels used.
    pub edges: usize,
    /// 98th percentile gradient magnitude of the region (0..255 luma
    /// units): how strong the edges are, not just how wide. NaN when the
    /// region had no measurable gradients.
    pub contrast: f64,
}

const PRE_SMOOTH: f32 = 0.6;
const SIGMA0: f32 = 1.0;
const MIN_SIGMA: f32 = 0.3;
/// 98th percentile gradient magnitude below which a region has no edges worth
/// judging. A completely blurred or featureless patch would otherwise read as
/// perfectly sharp because the edge census floor is relative to the patch
/// itself (B1000_4150: contrast 1.7, sigma pinned to the 0.30 floor, rated 5
/// stars inside the AF box).
pub const MIN_CONTRAST: f64 = 3.0;
const R_MIN: f32 = 1.01;
const R_MAX: f32 = 5.0;
const MIN_EDGES: usize = 80;
const MAX_EDGES: usize = 200_000;
/// Share of the region's 98th-percentile gradient magnitude that still counts.
const FLOOR_SHARE: f32 = 0.2;
/// `p20(|Laplacian|)` of white noise, divided by its sigma.
const LAP_P20_OVER_SIGMA: f32 = 0.2533;
/// Noise floor in gradient sigmas, times the gradient/Laplacian sigma ratio.
const NOISE_MULT: f32 = 4.0;
const GRAD_OVER_LAP: f32 = 0.2;
/// Below this there is nothing worth calling an edge, in luma units.
const ABS_FLOOR: f32 = 2.0;
/// Pixels of context the gradients and the re-blur need on each side.
const PAD: usize = 4;
/// Analysis budget: a larger box is area-averaged by an integer factor first.
/// 2.5 MP is about 8 ms of Gaussian work per pass on one core, and keeps a
/// 33 MP box under the 120 ms the scan budget allows per subject.
pub const ANALYSIS_MAX_PIXELS: usize = 2_500_000;
/// Long edge (px) every reported blur is normalised to, so a 3000 px preview
/// and a 6000 px one are judged on the same scale.
pub const REFERENCE_EDGE: usize = 6000;
/// Smallest analysis plane edge the downscaler may produce.
const MIN_ANALYSIS_EDGE: usize = 32;
/// Edge directions: the normal along x, 45deg, along y, 135deg.
const NORMALS: [(isize, isize); 4] = [(1, 0), (1, 1), (0, 1), (-1, 1)];

impl EdgeBlur {
    fn unmeasured(edges: usize) -> EdgeBlur {
        EdgeBlur {
            sigma: f64::NAN,
            sigma_best: f64::NAN,
            sigma_worst: f64::NAN,
            worst_dir: 0,
            dirs: [f64::NAN; 4],
            edges,
            contrast: f64::NAN,
        }
    }

    /// `sigma_worst` in native pixels, expressed at [`REFERENCE_EDGE`].
    pub fn sigma_worst_at_reference(&self, long_edge: usize) -> f64 {
        at_reference(self.sigma_worst, long_edge)
    }

    /// The region has no edges strong enough to judge: its census found only
    /// the noise floor, so a small sigma here is the floor, not sharpness.
    pub fn flat(&self) -> bool {
        !self.contrast.is_nan() && self.contrast < MIN_CONTRAST
    }
}

/// `sigma` in pixels of a `long_edge`-px image, expressed at [`REFERENCE_EDGE`].
/// NaN passes through; a zero long edge cannot be normalised and is NaN too.
pub fn at_reference(sigma: f64, long_edge: usize) -> f64 {
    if sigma.is_nan() || long_edge == 0 {
        return f64::NAN;
    }
    sigma * REFERENCE_EDGE as f64 / long_edge as f64
}

#[derive(Clone, Copy)]
struct Rect {
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

impl Rect {
    fn contains(self, x: usize, y: usize) -> bool {
        x >= self.x0 && x < self.x1 && y >= self.y0 && y < self.y1
    }
}

fn gauss(sigma: f32) -> Vec<f32> {
    let r = (sigma * 3.0).ceil() as isize;
    let k: Vec<f32> = (-r..=r)
        .map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f32 = k.iter().sum();
    k.iter().map(|v| v / sum).collect()
}

/// Separable Gaussian into `dst`, using `tmp` (`w * h`) as scratch. Rows are
/// mirrored into a padded line so the inner loop needs no clamp; columns are a
/// pass per tap over whole rows, which stays contiguous.
fn blur_into(src: &[f32], dst: &mut [f32], tmp: &mut [f32], w: usize, h: usize, k: &[f32]) {
    let r = k.len() / 2;
    let mut pad = vec![0f32; w + 2 * r];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        pad[..r].fill(row[0]);
        pad[r..r + w].copy_from_slice(row);
        pad[r + w..].fill(row[w - 1]);
        let out = &mut tmp[y * w..(y + 1) * w];
        for (x, o) in out.iter_mut().enumerate() {
            let win = &pad[x..x + k.len()];
            let mut acc = 0f32;
            for (&v, &wt) in win.iter().zip(k) {
                acc += v * wt;
            }
            *o = acc;
        }
    }
    dst.fill(0.0);
    for (i, &wt) in k.iter().enumerate() {
        let off = i as isize - r as isize;
        for y in 0..h {
            let sy = (y as isize + off).clamp(0, h as isize - 1) as usize;
            let row = &tmp[sy * w..(sy + 1) * w];
            for (o, &v) in dst[y * w..(y + 1) * w].iter_mut().zip(row) {
                *o += wt * v;
            }
        }
    }
}

/// Test helper: the same separable blur, allocating its own scratch.
#[cfg(test)]
fn blur(src: &[f32], w: usize, h: usize, k: &[f32]) -> Vec<f32> {
    let mut tmp = vec![0f32; w * h];
    let mut out = vec![0f32; w * h];
    blur_into(src, &mut out, &mut tmp, w, h, k);
    out
}

/// Central differences; the 0.5 factor cancels in the ratio, so it is left out.
fn gradients(p: &[f32], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let mut gx = vec![0f32; w * h];
    let mut gy = vec![0f32; w * h];
    for y in 1..h.saturating_sub(1) {
        let prev = &p[(y - 1) * w..y * w];
        let cur = &p[y * w..(y + 1) * w];
        let next = &p[(y + 1) * w..(y + 2) * w];
        let gxr = &mut gx[y * w..(y + 1) * w];
        let gyr = &mut gy[y * w..(y + 1) * w];
        for x in 1..w.saturating_sub(1) {
            gxr[x] = cur[x + 1] - cur[x - 1];
            gyr[x] = next[x] - prev[x];
        }
    }
    (gx, gy)
}

/// The gradient normal quantised to 0/45/90/135 degrees, without `atan2`:
/// `tan(22.5deg) = 0.4142`, so the 45-degree sectors are half the 90s.
/// Public so a caller can count edges per bin with the same quantisation this
/// module's per-direction estimates use.
pub fn dir_bin(rx: f32, ry: f32) -> u8 {
    let (ax, ay) = (rx.abs(), ry.abs());
    if ax >= 2.414_213_6 * ay {
        0
    } else if ay >= 2.414_213_6 * ax {
        2
    } else if rx * ry > 0.0 {
        1
    } else {
        3
    }
}

fn pick(sorted: &[f32], p: f32) -> f32 {
    if sorted.is_empty() {
        return f32::NAN;
    }
    let i = ((sorted.len() - 1) as f32 * p).round() as usize;
    sorted[i.min(sorted.len() - 1)]
}

fn sorted(v: &mut [f32]) {
    v.sort_by(f32::total_cmp);
}

/// Integer factor that brings `w * h` under `max_pixels`: 1 when it already
/// fits. Never shrinks the plane below [`MIN_ANALYSIS_EDGE`].
fn downscale_factor(w: usize, h: usize, max_pixels: usize) -> usize {
    let px = w.saturating_mul(h);
    if px <= max_pixels || max_pixels == 0 {
        return 1;
    }
    let mut k = ((px as f64 / max_pixels as f64).sqrt().ceil() as usize).max(1);
    while k > 1 && (w / k < MIN_ANALYSIS_EDGE || h / k < MIN_ANALYSIS_EDGE) {
        k -= 1;
    }
    k
}

/// `k` x `k` area average of the window straight from the grey bytes (no
/// full-resolution f32 copy); the remainder columns and rows are dropped.
fn average_window(g: &Gray, x0: usize, y0: usize, x1: usize, y1: usize, k: usize) -> (Vec<f32>, usize, usize) {
    let (dw, dh) = ((x1 - x0) / k, (y1 - y0) / k);
    let inv = 1.0 / (k * k) as f32;
    let mut out = vec![0f32; dw * dh];
    for by in 0..dh {
        for bx in 0..dw {
            let mut acc = 0u32;
            for yy in 0..k {
                let start = (y0 + by * k + yy) * g.width + x0 + bx * k;
                let row = &g.data[start..start + k];
                for &v in row {
                    acc += u32::from(v);
                }
            }
            out[by * dw + bx] = acc as f32 * inv;
        }
    }
    (out, dw, dh)
}

/// Blur of the edges inside `region` (`x1,y1,x2,y2`); `None` is the whole image.
pub fn measure(image: &Gray, region: Option<[usize; 4]>) -> EdgeBlur {
    estimate(image, region, false, ANALYSIS_MAX_PIXELS)
}

/// Blur of the edges outside `region` only.
pub fn measure_outside(image: &Gray, region: [usize; 4]) -> EdgeBlur {
    estimate(image, Some(region), true, ANALYSIS_MAX_PIXELS)
}

/// Blur of the edges inside `region`, analysed under a `max_pixels` budget.
fn estimate(
    image: &Gray,
    region: Option<[usize; 4]>,
    outside: bool,
    max_pixels: usize,
) -> EdgeBlur {
    let (iw, ih) = (image.width, image.height);
    if iw < 16 || ih < 16 {
        return EdgeBlur::unmeasured(0);
    }
    let Some(mut region) = clamp_region(region, iw, ih) else {
        return EdgeBlur::unmeasured(0);
    };
    // Measuring "outside" the whole frame has nothing to measure.
    if region.x1 <= region.x0 || region.y1 <= region.y0 {
        return EdgeBlur::unmeasured(0);
    }
    // The window carries the region plus enough reach for the blur and for the
    // area average's footprint. Outside, the window is the whole image.
    let (px0, py0, px1, py1) = if outside {
        (0, 0, iw, ih)
    } else {
        let guess = downscale_factor(region.x1 - region.x0, region.y1 - region.y0, max_pixels);
        let margin = PAD * guess.max(1) + 2 * guess;
        (
            region.x0.saturating_sub(margin),
            region.y0.saturating_sub(margin),
            (region.x1 + margin).min(iw),
            (region.y1 + margin).min(ih),
        )
    };
    let (mut w, mut h) = (px1 - px0, py1 - py0);
    if w < 8 || h < 8 {
        return EdgeBlur::unmeasured(0);
    }
    region = Rect {
        x0: region.x0 - px0,
        y0: region.y0 - py0,
        x1: region.x1 - px0,
        y1: region.y1 - py0,
    };

    // One pass over the window's bytes into f32 -- or straight into the
    // area-averaged plane, which never needs the full-resolution copy.
    let k = downscale_factor(w, h, max_pixels);
    let src: Vec<f32> = if k > 1 {
        average_window(image, px0, py0, px1, py1, k).0
    } else {
        let mut v = Vec::with_capacity(w * h);
        for y in 0..h {
            let row = &image.data[(y + py0) * iw + px0..(y + py0) * iw + px1];
            v.extend(row.iter().map(|&b| f32::from(b)));
        }
        v
    };

    if k > 1 {
        w /= k;
        h /= k;
        if w < 8 || h < 8 {
            return EdgeBlur::unmeasured(0);
        }
        // Blur is linear in resolution: the analysis plane is `k` times finer,
        // so the region shrinks the same way.
        region = Rect {
            x0: region.x0 / k,
            y0: region.y0 / k,
            x1: region.x1.div_ceil(k).min(w),
            y1: region.y1.div_ceil(k).min(h),
        };
        if region.x1 <= region.x0 || region.y1 <= region.y0 {
            return EdgeBlur::unmeasured(0);
        }
    }
    let kf = k as f64;
    // A k x k area average is itself a blur of variance (k^2 - 1) / 12 per
    // axis (analysis px^2). It is not part of the subject's blur, so it comes
    // back out of every edge estimate in quadrature.
    let extra_var = ((k * k - 1) as f32) / 12.0;

    // Outside, the pixels within the blur's reach of the region still carry
    // the region's gradient, so they are not answers about what is outside it.
    let excluded = Rect {
        x0: region.x0.saturating_sub(PAD),
        y0: region.y0.saturating_sub(PAD),
        x1: (region.x1 + PAD).min(w),
        y1: (region.y1 + PAD).min(h),
    };
    let wanted = |x: usize, y: usize| {
        if outside {
            !excluded.contains(x, y)
        } else {
            region.contains(x, y)
        }
    };

    // The Laplacian's low percentile is the noise level; it wants the raw
    // plane, which is not needed after this point.
    let mut laps = Vec::new();
    for y in (2..h.saturating_sub(2)).step_by(4) {
        for x in (2..w.saturating_sub(2)).step_by(4) {
            if !wanted(x, y) {
                continue;
            }
            let i = y * w + x;
            laps.push((src[i - w] + src[i + w] + src[i - 1] + src[i + 1] - 4.0 * src[i]).abs());
        }
    }

    let mut pre = vec![0f32; w * h];
    let mut tmp = vec![0f32; w * h];
    blur_into(&src, &mut pre, &mut tmp, w, h, &gauss(PRE_SMOOTH));
    drop(src);
    let (gx, gy) = gradients(&pre, w, h);
    let mut reblurred = vec![0f32; w * h];
    blur_into(&pre, &mut reblurred, &mut tmp, w, h, &gauss(SIGMA0));
    drop(pre);
    drop(tmp);
    let (rx, ry) = gradients(&reblurred, w, h);
    drop(reblurred);
    // The re-blurred magnitude and direction at every pixel, once: the
    // non-maximum suppression reads each of them three times.
    let mut rm = vec![0f32; w * h];
    let mut bins = vec![0u8; w * h];
    for i in 0..w * h {
        rm[i] = (rx[i] * rx[i] + ry[i] * ry[i]).sqrt();
        bins[i] = dir_bin(rx[i], ry[i]);
    }
    drop(rx);
    drop(ry);

    // One stride-4 sample of the region answers the gradient percentile,
    // without sorting a whole multi-megapixel plane.
    let mut mags = Vec::new();
    for y in (2..h.saturating_sub(2)).step_by(4) {
        for x in (2..w.saturating_sub(2)).step_by(4) {
            if wanted(x, y) {
                mags.push(rm[y * w + x]);
            }
        }
    }
    sorted(&mut mags);
    sorted(&mut laps);
    let p98 = pick(&mags, 0.98);
    let noise = NOISE_MULT * GRAD_OVER_LAP * pick(&laps, 0.20) / LAP_P20_OVER_SIGMA;
    let floor = if p98.is_nan() {
        return EdgeBlur::unmeasured(0);
    } else {
        ABS_FLOOR.max(noise).max(FLOOR_SHARE * p98)
    };

    // Non-maximum suppression along the (quantised) gradient normal, then the
    // ratio of the gradient before and after the extra blur.
    let mut edges: Vec<(f32, u8)> = Vec::new();
    for y in 2..h.saturating_sub(2) {
        for x in 2..w.saturating_sub(2) {
            let i = y * w + x;
            let m = rm[i];
            if m < floor || !wanted(x, y) {
                continue;
            }
            let bin = bins[i] as usize;
            let (dx, dy) = NORMALS[bin];
            let (ax, ay) = ((x as isize + dx) as usize, (y as isize + dy) as usize);
            let (bx, by) = ((x as isize - dx) as usize, (y as isize - dy) as usize);
            if m <= rm[ay * w + ax] || m <= rm[by * w + bx] {
                continue;
            }
            let before = (gx[i] * gx[i] + gy[i] * gy[i]).sqrt();
            let r = (before / m).clamp(R_MIN, R_MAX);
            let sp = SIGMA0 / (r * r - 1.0).sqrt();
            let st = (sp * sp - PRE_SMOOTH * PRE_SMOOTH).max(MIN_SIGMA * MIN_SIGMA).sqrt();
            // Below the analysis resolution's own floor the answer is "sharper
            // than this plane can say", never zero.
            let st = (st * st - extra_var).max(MIN_SIGMA * MIN_SIGMA).sqrt();
            edges.push((st, bin as u8));
        }
    }

    if edges.len() > MAX_EDGES {
        let stride = edges.len().div_ceil(MAX_EDGES);
        edges = edges.into_iter().step_by(stride).collect();
    }
    let n = edges.len();
    if n < MIN_EDGES {
        return EdgeBlur::unmeasured(n);
    }

    let mut all: Vec<f32> = edges.iter().map(|&(s, _)| s).collect();
    sorted(&mut all);
    let sigma = f64::from(pick(&all, 0.2)) * kf;

    // Per direction bin: the same 20th percentile, for bins with enough edges.
    let need = 40.max((n as f32 * 0.04).round() as usize);
    let mut dirs = [f64::NAN; 4];
    for (bin, slot) in dirs.iter_mut().enumerate() {
        let mut v: Vec<f32> = edges
            .iter()
            .filter(|&&(_, b)| b as usize == bin)
            .map(|&(s, _)| s)
            .collect();
        if v.len() >= need {
            sorted(&mut v);
            *slot = f64::from(pick(&v, 0.2)) * kf;
        }
    }
    let valid: Vec<(usize, f64)> = dirs
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| (!v.is_nan()).then_some((i, v)))
        .collect();
    let (worst_dir, sigma_worst) = valid
        .iter()
        .copied()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .unwrap_or((0, f64::NAN));
    let sigma_best = valid.iter().map(|&(_, v)| v).fold(f64::NAN, f64::min);

    EdgeBlur {
        sigma,
        sigma_best,
        sigma_worst,
        worst_dir: worst_dir as u8,
        dirs,
        edges: n,
        contrast: f64::from(p98),
    }
}

/// `region` clipped to the image, `None` when it has no area.
fn clamp_region(region: Option<[usize; 4]>, iw: usize, ih: usize) -> Option<Rect> {
    match region {
        None => Some(Rect {
            x0: 0,
            y0: 0,
            x1: iw,
            y1: ih,
        }),
        Some([a, b, c, d]) => {
            let (x0, y0) = (a.min(iw), b.min(ih));
            let (x1, y1) = (c.min(iw).max(x0), d.min(ih).max(y0));
            (x1 > x0 && y1 > y0).then_some(Rect { x0, y0, x1, y1 })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic xorshift, so the tests never depend on a crate.
    struct Rng(u64);

    impl Rng {
        fn unit(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
    }

    /// 32 px blocks of two luma levels, with uniform noise on top.
    fn blocks(w: usize, h: usize, contrast: f32, noise: f32, seed: u64) -> Gray {
        let mut rng = Rng(seed | 1);
        let mut data = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut s = (x / 32).wrapping_mul(73856093) ^ (y / 32).wrapping_mul(19349663);
                s ^= s >> 13;
                s = s.wrapping_mul(2654435761);
                let base = if s & 1 == 0 {
                    128.0 - contrast / 2.0
                } else {
                    128.0 + contrast / 2.0
                };
                let v = base + (rng.unit() - 0.5) * 2.0 * noise;
                data[y * w + x] = v.clamp(0.0, 255.0) as u8;
            }
        }
        Gray::new(w, h, data)
    }

    fn blurred(img: &Gray, sigma: f32) -> Gray {
        let (w, h) = (img.width, img.height);
        let src: Vec<f32> = img.data.iter().map(|&v| f32::from(v)).collect();
        let k = gauss(sigma);
        let out = blur(&src, w, h, &k);
        Gray::new(w, h, out.iter().map(|v| v.round().clamp(0.0, 255.0) as u8).collect())
    }

    /// Horizontal box of `len` px: camera shake along x.
    fn shaken(img: &Gray, len: usize) -> Gray {
        let (w, h) = (img.width, img.height);
        let l = len as f32;
        let mut out = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut acc = 0.0;
                for i in 0..len {
                    let sx = (x as isize + i as isize - len as isize / 2).clamp(0, w as isize - 1);
                    acc += f32::from(img.data[y * w + sx as usize]);
                }
                out[y * w + x] = (acc / l).round() as u8;
            }
        }
        Gray::new(w, h, out)
    }

    #[test]
    fn gaussian_blur_is_measured_in_native_pixels() {
        let sharp = blocks(384, 384, 140.0, 4.0, 7);
        for &s in &[1.0f32, 2.0, 3.0, 5.0] {
            let m = measure(&blurred(&sharp, s), None);
            println!("gauss {s}: sigma {:.3}, dirs {:?}, edges {}", m.sigma, m.dirs, m.edges);
            assert!(m.edges >= 80, "sigma {s}: only {} edges", m.edges);
            assert!(
                (m.sigma - f64::from(s)).abs() <= 0.25 * f64::from(s),
                "sigma {s} read as {:.3} (edges {})",
                m.sigma,
                m.edges
            );
        }
    }

    #[test]
    fn directional_motion_blur_splits_the_bins() {
        let e = measure(&shaken(&blocks(384, 384, 140.0, 4.0, 11), 15), None);
        assert!(e.edges >= 80, "only {} edges", e.edges);
        assert!(
            e.sigma_worst >= 2.0 * e.sigma_best,
            "worst {:.3} vs best {:.3}",
            e.sigma_worst,
            e.sigma_best
        );
        assert_eq!(e.worst_dir, 0, "dirs {:?}", e.dirs);
    }

    #[test]
    fn noise_and_flat_images_are_rejected() {
        let mut rng = Rng(3);
        let noise: Vec<u8> = (0..256 * 256)
            .map(|_| (128.0 + (rng.unit() - 0.5) * 80.0) as u8)
            .collect();
        let e = measure(&Gray::new(256, 256, noise), None);
        assert!(e.edges < 80, "noise gave {} edges", e.edges);
        assert!(e.sigma.is_nan() && e.sigma_best.is_nan() && e.sigma_worst.is_nan());

        let flat = Gray::new(256, 256, vec![90; 256 * 256]);
        let e = measure(&flat, None);
        assert_eq!(e.edges, 0);
        assert!(e.sigma.is_nan() && e.dirs.iter().all(|v| v.is_nan()));
        assert_eq!(e.worst_dir, 0);
    }

    #[test]
    fn a_flat_low_contrast_patch_is_not_sharp() {
        // A blurred two-level pattern: the census can still find edges, but
        // their gradients are noise-sized. Its sigma must not be read as focus.
        let m = measure(&blurred(&blocks(384, 384, 2.0, 0.5, 13), 2.0), None);
        assert!(
            m.sigma.is_nan() || m.flat(),
            "contrast {:.2} sigma {:.3} should be flat or unmeasured",
            m.contrast,
            m.sigma
        );
    }

    #[test]
    fn contrast_does_not_move_the_estimate() {
        let full = blocks(384, 384, 140.0, 4.0, 5);
        let half = blocks(384, 384, 70.0, 4.0, 5);
        let a = measure(&blurred(&full, 3.0), None).sigma;
        let b = measure(&blurred(&half, 3.0), None).sigma;
        assert!(
            (a - b).abs() <= 0.15 * a.max(b),
            "contrast 140 read {a:.3}, contrast 70 read {b:.3}"
        );
    }

    #[test]
    fn outside_ignores_what_is_inside_the_region() {
        let sharp = blocks(256, 256, 140.0, 4.0, 13);
        let mut other = sharp.clone();
        for y in 64..192 {
            for x in 64..192 {
                other.data[y * 256 + x] = 200u8.wrapping_add((x as u8) ^ (y as u8));
            }
        }
        let region = [64, 64, 192, 192];
        let a = measure_outside(&sharp, region);
        let b = measure_outside(&other, region);
        assert_eq!(a.edges, b.edges);
        assert_eq!(a.sigma.to_bits(), b.sigma.to_bits());
        assert_eq!(a.sigma_worst.to_bits(), b.sigma_worst.to_bits());
        // And the inside measure does see the difference.
        assert_ne!(
            measure(&sharp, Some(region)).sigma.to_bits(),
            measure(&other, Some(region)).sigma.to_bits()
        );
    }

    /// Deterministic value noise at several block scales, as in
    /// `sharpness`'s tests: edges in every direction, like a real subject.
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

    #[test]
    fn a_downscaled_analysis_agrees_with_the_full_one() {
        // Gauss-2 over isotropic texture, measured whole and under a budget
        // that forces the 2x area-averaged path.
        let img = blurred(&texture(384), 2.0);
        let full = estimate(&img, None, false, usize::MAX);
        let half = estimate(&img, None, false, 40_000);
        assert!(full.sigma.is_finite() && half.sigma.is_finite());
        println!(
            "full sigma {:.3} worst {:.3} ({} edges, dirs {:?}); downscaled {:.3} worst {:.3} ({} edges, dirs {:?})",
            full.sigma, full.sigma_worst, full.edges, full.dirs, half.sigma, half.sigma_worst, half.edges, half.dirs
        );
        let d = (full.sigma - half.sigma).abs() / full.sigma;
        assert!(d <= 0.20, "sigma {:.3} vs {:.3}: {:.1}% apart", full.sigma, half.sigma, d * 100.0);
        let w = (full.sigma_worst - half.sigma_worst).abs() / full.sigma_worst;
        assert!(
            w <= 0.20,
            "sigma_worst {:.3} vs {:.3}: {:.1}% apart",
            full.sigma_worst,
            half.sigma_worst,
            w * 100.0
        );
    }

    #[test]
    fn reported_sigma_scales_to_the_reference_edge() {
        assert!((at_reference(1.0, REFERENCE_EDGE) - 1.0).abs() < 1e-12);
        assert!((at_reference(1.0, 3000) - 2.0).abs() < 1e-12);
        assert!(at_reference(f64::NAN, 6000).is_nan());
        assert!(at_reference(1.0, 0).is_nan());
    }
}
