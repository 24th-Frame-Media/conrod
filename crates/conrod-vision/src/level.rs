//! Is the horizon level, and do the building's verticals converge?
//!
//! Two of the keepers a real-estate photographer is judged on: the verticals
//! are straight and parallel (a wall that should be rectangular is not a
//! trapezoid) and the horizon is level. Both are readable from the angle of
//! the long edges in the frame, which is what this module measures.
//!
//! # The measurement
//!
//! The frame is downscaled so its long edge is at most 768 px (an integer box
//! average first, then one bilinear step: geometry does not need Pillow's
//! exact thumbnail and a 32 MP preview is expensive through it), smoothed one
//! pixel and given Sobel gradients (`streaks`'s smoother and the same /4 Sobel
//! scale). Pixels in the top quarter of gradient magnitude are kept -- a weak
//! edge belongs to texture, not to a wall. Each kept pixel votes into one of
//! two weighted histograms, 0.5 deg bins over +/-20 deg:
//!
//! - near-vertical edges, at `wrap(atan2(gy, gx))`, i.e. how far the gradient
//!   is from horizontal;
//! - near-horizontal edges, at `wrap(atan2(gy, gx) - 90)`, i.e. how far the
//!   gradient is from vertical.
//!
//! The vote weight is the gradient energy (magnitude squared), so a
//! high-contrast window frame counts far more than a faint tile joint; the
//! gates and the support factor count edge pixels, not weight, so they mean
//! the same thing whatever the contrast. `wrap` folds an angle into
//! (-90, 90]; gradient direction is only defined up to half a turn, and the
//! fold keeps the sign of the lean in both families (a frame rotated clockwise
//! reads positive in both).
//!
//! `tilt_deg` is the weighted median of the vertical histogram, refined to the
//! weighted mean of the votes within 3 deg of it (the median locates the
//! cluster, the mean gives sub-bin precision). Vertical edges are preferred
//! because a building has more of them than clear horizon lines; the
//! horizontal histogram is the fallback when the frame has no verticals worth
//! counting -- a beach, a field, a sky.
//!
//! `verticals_converge_deg` splits the verticals by column into thirds and
//! takes the difference between the median angle on the right and on the left.
//! A camera pointed up at a building converges the two sides: the left walls
//! lean one way and the right walls the other, so the medians separate while
//! the global median stays near zero. Each side needs its own support before
//! the number is reported.
//!
//! `confidence` is the share of the chosen family's vote weight held by its
//! strongest 4 deg-wide window, scaled down when there are fewer than 4000
//! edge pixels to go on. It is the one number that says whether to believe the
//! angles: a wall of windows puts most of its edges in one window, foliage and
//! noise do not. The window is not centred on the median, so a facade whose
//! two sides lean opposite ways still reads as two coherent directions.
//!
//! # Thresholds
//!
//! Both are proposals from the property-photography research rather than
//! fitted to the shoots: straight verticals are a top keeper criterion, and a
//! degree of roll is visible on a door frame while two degrees of convergence
//! is a wall that visibly leans. `TILT_LIMIT_DEG` is 1.0 and
//! `KEYSTONE_LIMIT_DEG` is 2.0. The verdict prefers "keystone": it is the
//! defect that survives a straightening crop.
//!
//! Cost: about 10 ms for a 768 px plane (one Sobel pass, one atan2 per strong
//! pixel). `measure` takes the frame as it comes, so on a 32 MP preview the
//! box-average downscale dominates: about 20-35 ms end to end on the two
//! property shoots in `crates/conrod-io/tests/level_local.rs`.

use crate::imageops::Gray;
use crate::streaks;

/// Roll beyond this reads as tilted. A proposal from the research: a degree
/// shows on a door frame, and the horizon in a property shot is expected to
/// be level.
pub const TILT_LIMIT_DEG: f64 = 1.0;
/// Left-to-right vertical separation beyond this reads as keystone. Same
/// source: two degrees is a wall that visibly leans in a wide frame.
pub const KEYSTONE_LIMIT_DEG: f64 = 2.0;

/// Long edge (px) the angles are measured at. The same working plane as the
/// streak measure: the arrangement of edges is what matters, not their native
/// width.
const FEATURE_EDGE: usize = 768;
/// Histogram bin width, degrees.
const BIN_DEG: f64 = 0.5;
/// How far from vertical (or horizontal) an edge may be and still count.
const FAMILY_DEG: f64 = 20.0;
const BINS: usize = 80;
/// Table smaller than this in either direction has nothing to say.
const MIN_EDGE: usize = 96;
/// Edges below this percentile of gradient magnitude are texture, not walls.
const STRONG_PERCENTILE: f32 = 0.75;
/// Sobel/4 magnitude below which an edge is never counted, even if the frame's
/// own percentile is lower (an almost flat frame).
const MIN_GRADIENT: f32 = 6.0;
/// Edge pixels a family needs before it is used at all.
const MIN_VOTES: usize = 800;
/// ...and before one side of the frame may speak for the keystone.
const MIN_SIDE_VOTES: usize = 120;
/// Edge pixels at which support stops scaling the confidence down.
const SUPPORT_VOTES: usize = 4_000;
/// Half-width of the window whose share of the weight is the confidence, and
/// of the window the refined centre averages over.
const CONCENTRATION_DEG: f64 = 2.0;
const REFINE_DEG: f64 = 3.0;
/// Below this confidence no verdict is offered.
const MIN_CONFIDENCE: f64 = 0.30;

/// What the frame's long edges say about how the camera was held.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    /// Signed roll of the camera, degrees: positive when the frame is rotated
    /// clockwise on screen (horizontals fall towards the right, verticals lean
    /// like `/`). `None` when no family had enough edges to measure.
    pub tilt_deg: Option<f64>,
    /// Angle between the left-side and right-side verticals, degrees: `None`
    /// when either side had too few vertical edges. Magnitude only; the sign
    /// would only say which side leans further.
    pub verticals_converge_deg: Option<f64>,
    /// Edges that voted: near-vertical plus near-horizontal pixels.
    pub lines: usize,
    /// 0..1; the share of the vote weight that the strongest 4-deg-wide
    /// window of the chosen family holds, scaled by how much weight there was.
    pub confidence: f64,
    /// `"level"`, `"tilted"`, `"keystone"` or `"unknown"`.
    pub verdict: &'static str,
}

impl Level {
    fn unknown() -> Level {
        Level {
            tilt_deg: None,
            verticals_converge_deg: None,
            lines: 0,
            confidence: 0.0,
            verdict: "unknown",
        }
    }
}

/// Measure the frame's tilt and convergence.
pub fn measure(gray: &Gray) -> Level {
    let plane = working_plane(gray);
    if plane.width < MIN_EDGE || plane.height < MIN_EDGE {
        return Level::unknown();
    }
    let (w, h) = (plane.width, plane.height);
    let plane_f32: Vec<f32> = plane.data.iter().map(|&v| f32::from(v)).collect();
    let smooth = streaks::smooth_1px(&plane_f32, w, h);
    let (mags, angles) = gradients(&smooth, w, h);

    let mut sorted: Vec<f32> = mags.clone();
    let floor = pick(&mut sorted, STRONG_PERCENTILE).max(MIN_GRADIENT);

    // Verticals by third for the keystone, and one horizontal histogram.
    let mut verticals = [Family::new(), Family::new(), Family::new()];
    let mut horizontals = Family::new();
    let mut lines = 0usize;
    let third = w / 3;
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let weight = mags[i];
            if weight < floor {
                continue;
            }
            let angle = angles[i];
            let weight = weight * weight;
            let from_vertical = wrap_deg(angle);
            if f64::from(from_vertical).abs() <= FAMILY_DEG {
                let side = if x < third {
                    0
                } else if x >= w - third {
                    2
                } else {
                    1
                };
                verticals[side].add(from_vertical, weight);
                lines += 1;
                continue;
            }
            let from_horizontal = wrap_deg(angle - 90.0);
            if f64::from(from_horizontal).abs() <= FAMILY_DEG {
                horizontals.add(from_horizontal, weight);
                lines += 1;
            }
        }
    }

    let all_verticals = merged(&verticals);
    let chosen = if all_verticals.votes.len() >= MIN_VOTES {
        Some(&all_verticals)
    } else if horizontals.votes.len() >= MIN_VOTES {
        Some(&horizontals)
    } else {
        None
    };
    let mut level = Level {
        lines,
        ..Level::unknown()
    };
    let Some(chosen) = chosen else {
        return level;
    };
    let Some((tilt, _)) = chosen.refined() else {
        return level;
    };
    let support = (chosen.votes.len() as f64 / SUPPORT_VOTES as f64).min(1.0);
    level.tilt_deg = Some(tilt);
    level.confidence = chosen.concentration() * support;

    if all_verticals.votes.len() >= MIN_VOTES {
        let (left, right) = (&verticals[0], &verticals[2]);
        if left.votes.len() >= MIN_SIDE_VOTES && right.votes.len() >= MIN_SIDE_VOTES {
            if let (Some((l, _)), Some((r, _))) = (left.refined(), right.refined()) {
                level.verticals_converge_deg = Some((r - l).abs());
            }
        }
    }

    if level.confidence < MIN_CONFIDENCE {
        return level;
    }
    level.verdict = match level.verticals_converge_deg {
        Some(c) if c > KEYSTONE_LIMIT_DEG => "keystone",
        _ if tilt.abs() > TILT_LIMIT_DEG => "tilted",
        _ => "level",
    };
    level
}

/// The plane the angles are measured on: at most [`FEATURE_EDGE`] on its long
/// edge, aspect kept.
///
/// Pillow's thumbnail is the exact downscale the sharpness fixtures need, but
/// a 32 MP preview costs tens of milliseconds through it. Geometry does not
/// need that exactness, so an integer box average brings the frame down first
/// and one bilinear step lands it on the working edge.
fn working_plane(gray: &Gray) -> Gray {
    let k = gray.width.max(gray.height) / FEATURE_EDGE;
    if k <= 1 {
        return gray.thumbnail(FEATURE_EDGE);
    }
    let coarse = box_average(gray, k);
    match crate::imageops::thumbnail_size(coarse.width, coarse.height, FEATURE_EDGE) {
        Some((w, h)) => {
            coarse.resize_bilinear_box(w, h, [0.0, 0.0, coarse.width as f64, coarse.height as f64])
        }
        None => coarse,
    }
}

/// The mean of each `k` by `k` block, rounded like `Gray::reduce` but summed a
/// row at a time: the per-block slicing of `reduce` costs more than the
/// arithmetic on a 32 MP preview, and this is the hot path of the measure.
fn box_average(gray: &Gray, k: usize) -> Gray {
    let (w, h) = (gray.width, gray.height);
    let (dw, dh) = (w.div_ceil(k), h.div_ceil(k));
    let mut out = Vec::with_capacity(dw * dh);
    let mut row_sum = vec![0u32; w];
    for by in 0..dh {
        let (y0, y1) = (by * k, ((by + 1) * k).min(h));
        row_sum.fill(0);
        for y in y0..y1 {
            for (sum, &v) in row_sum.iter_mut().zip(&gray.data[y * w..(y + 1) * w]) {
                *sum += u32::from(v);
            }
        }
        for bx in 0..dw {
            let (x0, x1) = (bx * k, ((bx + 1) * k).min(w));
            let sum: u32 = row_sum[x0..x1].iter().sum();
            let count = ((y1 - y0) * (x1 - x0)) as u32;
            out.push(((sum + count / 2) / count) as u8);
        }
    }
    Gray::new(dw, dh, out)
}

/// One family of edges: a weighted angle histogram plus the votes themselves,
/// which the refinement and the confidence read.
struct Family {
    bins: [f64; BINS],
    total: f64,
    votes: Vec<(f32, f32)>,
}

impl Family {
    fn new() -> Family {
        Family {
            bins: [0.0; BINS],
            total: 0.0,
            votes: Vec::new(),
        }
    }

    fn add(&mut self, deg: f32, weight: f32) {
        let bin = (((f64::from(deg) + FAMILY_DEG) / BIN_DEG) as usize).min(BINS - 1);
        self.bins[bin] += f64::from(weight);
        self.total += f64::from(weight);
        self.votes.push((deg, weight));
    }

    /// Weighted median, at bin resolution: the angle where half the vote
    /// weight has passed.
    fn median(&self) -> Option<f64> {
        if self.total <= 0.0 {
            return None;
        }
        let half = self.total / 2.0;
        let mut acc = 0.0;
        for (bin, &weight) in self.bins.iter().enumerate() {
            if acc + weight >= half {
                let within = if weight > 0.0 {
                    (half - acc) / weight
                } else {
                    0.0
                };
                return Some(-FAMILY_DEG + (bin as f64 + within) * BIN_DEG);
            }
            acc += weight;
        }
        Some(FAMILY_DEG)
    }

    /// The share of the weight that the strongest [`CONCENTRATION_DEG`]-wide
    /// window holds: how dominant one direction is. A wall of windows puts most
    /// of its edges in one window; foliage and noise spread them out. Measured
    /// on the window rather than around the median so a facade whose two sides
    /// lean opposite ways still reads as two coherent directions.
    fn concentration(&self) -> f64 {
        if self.total <= 0.0 {
            return 0.0;
        }
        let half = (CONCENTRATION_DEG / BIN_DEG) as usize;
        let mut best = 0.0f64;
        for bin in 0..BINS {
            let lo = bin.saturating_sub(half);
            let hi = (bin + half + 1).min(BINS);
            let sum: f64 = self.bins[lo..hi].iter().sum();
            best = best.max(sum);
        }
        best / self.total
    }

    /// The centre, refined to the weighted mean of the votes within
    /// [`REFINE_DEG`] of the median.
    fn refined(&self) -> Option<(f64, f64)> {
        let median = self.median()?;
        let (mut sum, mut weight) = (0.0f64, 0.0f64);
        for &(deg, w) in &self.votes {
            let d = f64::from(deg) - median;
            if d.abs() <= REFINE_DEG {
                sum += f64::from(deg) * f64::from(w);
                weight += f64::from(w);
            }
        }
        if weight <= 0.0 {
            return None;
        }
        Some((sum / weight, weight / self.total))
    }
}

fn merged(families: &[Family]) -> Family {
    let mut out = Family::new();
    for family in families {
        for (bin, &weight) in family.bins.iter().enumerate() {
            out.bins[bin] += weight;
        }
        out.total += family.total;
        out.votes.extend_from_slice(&family.votes);
    }
    out
}

/// Sobel/4 gradients of a smoothed plane: magnitude and gradient angle
/// (radians, `atan2(gy, gx)`), zero on the border.
fn gradients(plane: &[f32], w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
    let mut mags = vec![0f32; w * h];
    let mut angles = vec![0f32; w * h];
    for y in 1..h - 1 {
        for x in 1..w - 1 {
            let i = y * w + x;
            let tl = plane[i - w - 1];
            let tc = plane[i - w];
            let tr = plane[i - w + 1];
            let ml = plane[i - 1];
            let mr = plane[i + 1];
            let bl = plane[i + w - 1];
            let bc = plane[i + w];
            let br = plane[i + w + 1];
            let gx = ((tr + 2.0 * mr + br) - (tl + 2.0 * ml + bl)) / 4.0;
            let gy = ((bl + 2.0 * bc + br) - (tl + 2.0 * tc + tr)) / 4.0;
            mags[i] = gx.hypot(gy);
            angles[i] = gy.atan2(gx).to_degrees();
        }
    }
    (mags, angles)
}

/// Fold an angle in degrees into (-90, 90]. Gradient direction has no sign:
/// an edge and its opposite-contrast twin are the same edge.
fn wrap_deg(mut deg: f32) -> f32 {
    deg %= 180.0;
    if deg > 90.0 {
        deg -= 180.0;
    } else if deg <= -90.0 {
        deg += 180.0;
    }
    deg
}

fn pick(v: &mut [f32], p: f32) -> f32 {
    if v.is_empty() {
        return 0.0;
    }
    let i = (((v.len() - 1) as f32) * p) as usize;
    let (_, &mut v, _) = v.select_nth_unstable_by(i, f32::total_cmp);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 768;
    const H: usize = 512;

    /// A field of parallel bars `deg` away from vertical: the bars follow
    /// `x cos(deg) + y sin(deg) = constant`, so positive is the clockwise
    /// rotation and the measured tilt should come back at `+deg`.
    ///
    /// Each pixel is the exact area average of the hard pattern across its own
    /// row (a box of width `cos(deg)` in the phase), so the edges are
    /// antialiased the way an optically continuous edge sampled onto 8 bits
    /// is, with no supersampling quantisation to bury a one-degree slope.
    fn bars(w: usize, h: usize, deg: f64, period: f64) -> Gray {
        let (cx, cy) = (w as f64 / 2.0, h as f64 / 2.0);
        let (sin, cos) = deg.to_radians().sin_cos();
        let half = period / 2.0;
        let mut data = vec![0u8; w * h];
        for y in 0..h {
            for x in 0..w {
                let u = (x as f64 - cx) * cos + (y as f64 + 0.5 - cy) * sin;
                let band = (u / half).floor();
                let r = u - band * half;
                let dark_now = (band as i64).rem_euclid(2) == 0;
                let dark = if dark_now {
                    if r + cos <= half {
                        cos
                    } else {
                        half - r
                    }
                } else if r + cos <= half {
                    0.0
                } else {
                    r + cos - half
                };
                data[y * w + x] = (30.0 + 175.0 * (1.0 - dark / cos)) as u8;
            }
        }
        Gray::new(w, h, data)
    }

    /// An antialiased dark line: coverage falls off across the last pixel.
    fn line(data: &mut [u8], w: usize, h: usize, a: (f64, f64), b: (f64, f64), value: u8) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let length2 = dx * dx + dy * dy;
        for y in 0..h {
            for x in 0..w {
                let (px, py) = (x as f64 + 0.5 - a.0, y as f64 + 0.5 - a.1);
                let t = ((px * dx + py * dy) / length2).clamp(0.0, 1.0);
                let (qx, qy) = (px - t * dx, py - t * dy);
                let coverage = (1.25 - (qx * qx + qy * qy).sqrt()).clamp(0.0, 1.0);
                if coverage > 0.0 {
                    let old = f64::from(data[y * w + x]);
                    let new = old + (f64::from(value) - old) * coverage;
                    data[y * w + x] = new as u8;
                }
            }
        }
    }

    /// A facade whose verticals lean outwards from the top by about 7 deg each
    /// side -- a camera pointed up -- with level floor and ceiling lines.
    fn facade() -> Gray {
        let mut data = vec![190u8; W * H];
        line(&mut data, W, H, (150.0, 20.0), (90.0, H as f64 - 20.0), 40);
        line(&mut data, W, H, (600.0, 20.0), (660.0, H as f64 - 20.0), 40);
        for x in 0..W {
            data[(H / 2) * W + x] = 70;
            data[(H / 2 + 1) * W + x] = 70;
        }
        Gray::new(W, H, data)
    }

    fn noise(w: usize, h: usize) -> Gray {
        let mut state = 0x1234_5678u32;
        let data = (0..w * h)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state >> 24) as u8
            })
            .collect();
        Gray::new(w, h, data)
    }

    #[test]
    fn vertical_bars_read_as_level() {
        let level = measure(&bars(W, H, 0.0, 32.0));
        println!("{level:?}");
        let tilt = level.tilt_deg.expect("tilt measured");
        assert!(tilt.abs() < 0.3, "level frame read {tilt:.2}");
        assert_eq!(level.verdict, "level");
        assert!(level.lines > 1_000, "{} edges", level.lines);
    }

    #[test]
    fn the_tilt_signal_follows_the_rotation() {
        for deg in [1.2, 1.5, 3.0, -3.0, 6.0] {
            let level = measure(&bars(W, H, deg, 32.0));
            println!("{deg:+.1} -> {level:?}");
            let tilt = level.tilt_deg.expect("tilt measured");
            assert!(
                (tilt - deg).abs() < 0.5,
                "{deg:+.1} deg frame read {tilt:+.2}"
            );
            assert_eq!(level.verdict, "tilted");
        }
        let plus = measure(&bars(W, H, 2.0, 32.0)).tilt_deg.unwrap();
        let minus = measure(&bars(W, H, -2.0, 32.0)).tilt_deg.unwrap();
        assert!(plus > 0.0 && minus < 0.0, "signs: {plus:+.2} {minus:+.2}");
    }

    #[test]
    fn a_converging_facade_reads_as_keystone() {
        let level = measure(&facade());
        println!("{level:?}");
        let converge = level.verticals_converge_deg.expect("convergence measured");
        assert!(
            (11.0..=16.0).contains(&converge),
            "convergence read {converge:.2}"
        );
        assert_eq!(level.verdict, "keystone");
    }

    #[test]
    fn noise_has_no_verdict() {
        let level = measure(&noise(W, H));
        println!("{level:?}");
        assert_eq!(level.verdict, "unknown");
        assert!(level.confidence < MIN_CONFIDENCE, "{level:?}");
    }

    #[test]
    fn a_tiny_frame_is_unknown() {
        assert_eq!(
            measure(&Gray::new(64, 48, vec![0; 64 * 48])).verdict,
            "unknown"
        );
        assert_eq!(
            measure(&Gray::new(400, 300, vec![128; 400 * 300])).verdict,
            "unknown"
        );
    }
}
