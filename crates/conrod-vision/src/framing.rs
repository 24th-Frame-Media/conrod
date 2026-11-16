//! Framing features from detection boxes (keeper signal K4).
//!
//! Everything here is pure geometry over boxes the detector already produced:
//! no pixels, no I/O, sub-millisecond. The numbers are *features* for the
//! taste head (K1/K6), not a ranker: composition on its own is a weak signal
//! (Denzler & Redies 2014; Hoh et al. 2023), so each rule is bounded and the
//! policy that combines them is per genre.
//!
//! Sources are `docs/research/composition-saliency.md` (sections 4.1-4.9) and
//! `docs/plans/keeper-signals.md` (K4). Constants say which side of that line
//! they come from: a research note, or "own choice, tune on labels".
//!
//! Conventions: pixels, origin top-left, `[x0, y0, x1, y1]`, y grows down.
//! A motion angle is degrees clockwise from image-right (+x), so 90 is down.

/// A box this close (as a fraction of the frame's width/height) to an edge is
/// "at" that edge. Source: research 4.6 uses `edge_gap < 0.02` as a common
/// reject for a car's nose/tail.
pub const EDGE_MARGIN: f64 = 0.02;

/// Below this share of the frame a subject is too small to carry the picture.
/// Source: research 4.7 ("below ~0.02 small subject").
pub const SMALL_SUBJECT_FRAC: f64 = 0.02;

/// Above this share the subject fills the frame. Source: research 4.7
/// ("above ~0.6 fill-the-frame (a valid CADB class)").
pub const TIGHT_SUBJECT_FRAC: f64 = 0.60;

/// A box covering this much of the frame is the frame: a crop or a dominant
/// subject, not an edge cut. Own choice, tune on labels (research 4.6 warns
/// that a wheel cut at the bottom is normal on a pan).
pub const WHOLE_FRAME_FRAC: f64 = 0.98;

/// Centred mode: centroid within this fraction of the frame diagonal.
/// Own choice, tune on labels; research 4.8 only says centring is valid.
pub const CENTRE_RADIUS: f64 = 0.05;

/// Rule-of-thirds power points, as a fraction of width/height.
pub const THIRDS_POINTS: [f64; 2] = [1.0 / 3.0, 2.0 / 3.0];

/// Golden-ratio power points. Source: research 4.1.
pub const PHI_POINTS: [f64; 2] = [0.382, 0.618];

/// Largest diagonal-normalised thirds distance: the frame corner to the
/// nearest third, `sqrt(2)/3 / sqrt(2)`. Exact, from the grid geometry.
pub const THIRDS_MAX_DIST: f64 = 1.0 / 3.0;

/// Largest diagonal-normalised phi distance: a corner to (0.382, 0.382).
/// Exact, from the grid geometry.
pub const PHI_MAX_DIST: f64 = 0.382;

/// Lead room the research formula targets, translated to this module's
/// signed scale: `L = 0.67` becomes `2 * 0.67 - 1 = 0.34`. Source: research
/// 4.4 (about twice as much room ahead as behind).
pub const LEAD_TARGET: f32 = 0.34;

/// Below this the subject is centred along the motion line: a head-on car or a
/// centred pan is a valid composition, and the lead term drops instead of
/// failing (research 4.4: no lead room for head-on cars, use centring; 4.7).
/// Own choice, tune on labels.
pub const LEAD_NEUTRAL: f32 = 0.10;

/// Horizontal imbalance beyond this is penalised, unless the subject is
/// centred. Source: research 4.3 (`|b_x| > 0.25`).
pub const BALANCE_THRESHOLD: f32 = 0.25;

/// A tilt below this is level. Source: research 4.2 (`|t| < 1 deg` good).
pub const TILT_GOOD_DEG: f32 = 1.0;

/// A tilt at or beyond this is fully penalised. Source: research 4.2
/// (`> 3 deg` penalise).
pub const TILT_BAD_DEG: f32 = 3.0;

/// Motion direction for lead room. The caller decides where it comes from
/// (panning background flow, burst displacement, gaze); without it lead room
/// is absent rather than zero. Research 4.4.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// Direction in frame pixels, any length; ignored if ~zero.
    Vector([f64; 2]),
    /// Degrees clockwise from image-right (+x), y pointing down.
    AngleDeg(f64),
    /// Degrees of the line a subject moves along, with the direction along it
    /// unknown: a panned background (or any one frame) shows the line a subject
    /// travels on, not which way along it. Lead room is scored on whichever
    /// side has the room, so neither travel direction is guessed at.
    AxisDeg(f64),
}

impl Motion {
    /// The unit vector, or `None` for a degenerate direction.
    pub fn unit(self) -> Option<[f64; 2]> {
        let v = match self {
            Motion::Vector(v) => v,
            Motion::AngleDeg(deg) | Motion::AxisDeg(deg) => {
                let r = deg.to_radians();
                [r.cos(), r.sin()]
            }
        };
        let len = v[0].hypot(v[1]);
        if len.is_finite() && len > 1e-9 {
            Some([v[0] / len, v[1] / len])
        } else {
            None
        }
    }

    /// Does this motion know which way along its line the subject goes? Only an
    /// arrow does; an axis does not, so its lead room is symmetric.
    pub fn signed(self) -> bool {
        !matches!(self, Motion::AxisDeg(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeBand {
    /// Under `SMALL_SUBJECT_FRAC` of the frame: the subject does not carry it.
    TooSmall,
    /// A workable subject size.
    Good,
    /// Over `TIGHT_SUBJECT_FRAC`: filling the frame, a valid composition.
    Tight,
}

/// One frame's framing measurements. All fractions are of frame width/height
/// unless the field says diagonal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Framing {
    /// How many frame edges the box is at (0..=4). A box that covers the
    /// whole frame is not "cut".
    pub cut_sides: u8,
    pub cut_left: bool,
    pub cut_top: bool,
    pub cut_right: bool,
    pub cut_bottom: bool,
    /// Clipped subject area / frame area, 0..=1.
    pub subject_frac: f32,
    pub size_band: SizeBand,
    /// Room ahead minus room behind along the motion direction, over the
    /// frame's extent along that direction; -1..=1 when the direction is known
    /// (`Motion::Vector`/`AngleDeg`), 0..=1 when only the line is known
    /// (`Motion::AxisDeg`, which scores the side with the room). `None` without
    /// motion.
    pub lead_room: Option<f32>,
    /// Centroid distance to the nearest rule-of-thirds point, divided by the
    /// frame diagonal; 0..=1/3.
    pub thirds_dist: f32,
    /// Same for the golden-ratio points; 0..=0.382.
    pub phi_dist: f32,
    /// Centroid within `CENTRE_RADIUS` of the frame centre.
    pub centred: bool,
    /// Area-weighted horizontal mass of all boxes, -1 (left) ..= 1 (right).
    pub balance: f32,
    /// The largest share of the subject's box another detection covers,
    /// 0..=1. A burst miss is often the frame where something walked in front
    /// of the subject.
    pub occlusion: f32,
    /// Tilt passed in from the level detector; not measured here.
    pub tilt_deg: Option<f32>,
}

impl Default for Framing {
    fn default() -> Self {
        Framing {
            cut_sides: 0,
            cut_left: false,
            cut_top: false,
            cut_right: false,
            cut_bottom: false,
            subject_frac: 0.0,
            size_band: SizeBand::TooSmall,
            lead_room: None,
            // Worst-case distances, so a missing measurement does not read
            // as perfect composition.
            thirds_dist: THIRDS_MAX_DIST as f32,
            phi_dist: PHI_MAX_DIST as f32,
            centred: false,
            balance: 0.0,
            occlusion: 0.0,
            tilt_deg: None,
        }
    }
}

/// Which terms `framing_score` uses, and how much each counts. Weights are
/// relative (the score is their weighted mean), so only zero disables a term.
/// Starting values are own choices to tune on labels (K1 ablation), except
/// where noted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FramingPolicy {
    /// Lead room is a sport/motorsport/wildlife term, not a portrait one.
    /// Research 4.4 and `keeper-signals.md` wiring notes.
    pub lead_room: bool,
    pub lead_weight: f32,
    /// Cut-off sides: the clearest signal in the research (section 6).
    pub cut_weight: f32,
    /// Subject size band. Research section 6 ranks it second.
    pub size_weight: f32,
    /// Best of centre/thirds/phi, the research's 4.8/4.9 mode idea. The
    /// thirds rule is weak (Denzler & Redies), so this weight stays small.
    pub mode_weight: f32,
    /// Horizontal balance, skipped when the subject is centred (research 4.3).
    pub balance_weight: f32,
    /// Occlusion: another subject's box over this one. The burst-contact-sheet
    /// comparison put the delivered frame on the least crowded composition.
    pub occlusion_weight: f32,
    /// Level horizon/vertical, only when a tilt was measured.
    pub tilt_weight: f32,
}

impl Default for FramingPolicy {
    fn default() -> Self {
        FramingPolicy {
            lead_room: false,
            lead_weight: 0.0,
            cut_weight: 0.35,
            size_weight: 0.20,
            mode_weight: 0.15,
            balance_weight: 0.10,
            occlusion_weight: 0.10,
            tilt_weight: 0.10,
        }
    }
}

impl FramingPolicy {
    /// Motorsport/sport: lead room matters, thirds least.
    pub const fn sport() -> FramingPolicy {
        FramingPolicy {
            lead_room: true,
            lead_weight: 0.25,
            cut_weight: 0.30,
            size_weight: 0.20,
            mode_weight: 0.10,
            balance_weight: 0.05,
            occlusion_weight: 0.10,
            tilt_weight: 0.05,
        }
    }

    /// Portraits: no lead room (research 4.4: not for portraits), face size
    /// and centring/thirds modes weigh more.
    pub const fn portrait() -> FramingPolicy {
        FramingPolicy {
            lead_room: false,
            lead_weight: 0.0,
            cut_weight: 0.35,
            size_weight: 0.25,
            mode_weight: 0.20,
            balance_weight: 0.10,
            occlusion_weight: 0.10,
            tilt_weight: 0.05,
        }
    }
}

/// Measure one frame. `frame` is `[width, height]` in pixels; `subject` is
/// the chosen detection box; `others` are the remaining boxes (may be empty);
/// `motion` and `tilt_deg` are computed elsewhere and may be absent.
pub fn framing(
    frame: [f64; 2],
    subject: [f64; 4],
    others: &[[f64; 4]],
    motion: Option<Motion>,
    tilt_deg: Option<f32>,
) -> Framing {
    let (w, h) = (frame[0], frame[1]);
    if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
        return Framing {
            tilt_deg,
            ..Framing::default()
        };
    }

    let raw = ordered(subject);
    let b = clamp_box(raw, w, h);
    let area = box_area(b);
    let frame_area = w * h;
    let frac = (area / frame_area).clamp(0.0, 1.0);

    let (cx, cy) = centroid(b);
    let nx = cx / w;
    let ny = cy / h;

    // A box that is the frame is a crop, not an edge cut.
    let cuts = if frac >= WHOLE_FRAME_FRAC {
        [false; 4]
    } else {
        [
            raw[0] <= EDGE_MARGIN * w,
            raw[1] <= EDGE_MARGIN * h,
            raw[2] >= (1.0 - EDGE_MARGIN) * w,
            raw[3] >= (1.0 - EDGE_MARGIN) * h,
        ]
    };

    let size_band = if frac < SMALL_SUBJECT_FRAC {
        SizeBand::TooSmall
    } else if frac > TIGHT_SUBJECT_FRAC {
        SizeBand::Tight
    } else {
        SizeBand::Good
    };

    let centre_dist = ((nx - 0.5).powi(2) + (ny - 0.5).powi(2)).sqrt() / std::f64::consts::SQRT_2;

    Framing {
        cut_sides: cuts.iter().filter(|&&c| c).count() as u8,
        cut_left: cuts[0],
        cut_top: cuts[1],
        cut_right: cuts[2],
        cut_bottom: cuts[3],
        subject_frac: frac as f32,
        size_band,
        lead_room: motion
            .and_then(|m| m.unit().map(|d| (m, d)))
            .map(|(m, d)| {
                let l = lead_room(d, b, w, h);
                if m.signed() {
                    l
                } else {
                    l.abs()
                }
            })
            .map(|l| l as f32),
        thirds_dist: grid_dist(nx, ny, THIRDS_POINTS) as f32,
        phi_dist: grid_dist(nx, ny, PHI_POINTS) as f32,
        centred: centre_dist <= CENTRE_RADIUS,
        balance: balance(subject, others, w, h),
        occlusion: occlusion(b, others),
        tilt_deg,
    }
}

/// The largest share of `subject` another box covers, 0..=1. Cheap: no pixels,
/// just the boxes the detector already returned.
fn occlusion(subject: [f64; 4], others: &[[f64; 4]]) -> f32 {
    let area = box_area(subject);
    if area <= 0.0 {
        return 0.0;
    }
    let mut worst = 0.0f64;
    for other in others {
        let covered = intersection(subject, *other) / area;
        if covered > worst {
            worst = covered;
        }
    }
    worst.clamp(0.0, 1.0) as f32
}

/// Combined framing quality in 0..=1, as the weighted mean of the terms the
/// policy enables. A term that cannot be measured (no motion, no tilt, no
/// area) is dropped rather than failed; if nothing applies the score is 0.5.
pub fn framing_score(f: &Framing, policy: &FramingPolicy) -> f32 {
    let mut sum = 0.0f32;
    let mut weights = 0.0f32;
    let mut add = |score: f32, weight: f32| {
        if weight > 0.0 {
            sum += score.clamp(0.0, 1.0) * weight;
            weights += weight;
        }
    };

    add(1.0 - f.cut_sides as f32 / 4.0, policy.cut_weight);
    add(1.0 - size_penalty(f), policy.size_weight);
    add(mode_score(f), policy.mode_weight);
    // The least occluded composition is the one that was delivered.
    add(1.0 - f.occlusion, policy.occlusion_weight);

    // Research 4.3: a lopsided frame is fine when the subject is centred.
    if !f.centred {
        let off = (f.balance.abs() - BALANCE_THRESHOLD).max(0.0) / (1.0 - BALANCE_THRESHOLD);
        add(1.0 - off, policy.balance_weight);
    }

    if policy.lead_room {
        // A subject centred along the motion line is a head-on composition,
        // which the mode term already scores: the lead question does not apply.
        if let Some(lead) = f.lead_room.filter(|lead| lead.abs() >= LEAD_NEUTRAL) {
            let miss = (lead - LEAD_TARGET).abs() / (1.0 - LEAD_TARGET);
            add(1.0 - miss, policy.lead_weight);
        }
    }

    if let Some(tilt) = f.tilt_deg {
        let span = TILT_BAD_DEG - TILT_GOOD_DEG;
        let off = ((tilt.abs() - TILT_GOOD_DEG) / span).clamp(0.0, 1.0);
        add(1.0 - off, policy.tilt_weight);
    }

    if weights > 0.0 {
        (sum / weights).clamp(0.0, 1.0)
    } else {
        0.5
    }
}

/// 0 for a good size, up to 1 for a dot or an oversized box. A fill-the-frame
/// subject is a valid class (research 4.7), so its penalty is capped at 0.5.
/// Own choices, tune on labels.
fn size_penalty(f: &Framing) -> f32 {
    let frac = f.subject_frac as f64;
    match f.size_band {
        SizeBand::Good => 0.0,
        SizeBand::TooSmall => {
            ((SMALL_SUBJECT_FRAC - frac) / SMALL_SUBJECT_FRAC).clamp(0.0, 1.0) as f32
        }
        SizeBand::Tight => {
            (0.5 * ((frac - TIGHT_SUBJECT_FRAC) / (1.0 - TIGHT_SUBJECT_FRAC)).clamp(0.0, 1.0))
                as f32
        }
    }
}

/// Best of the three composition modes (research 4.8): centring, thirds, phi.
fn mode_score(f: &Framing) -> f32 {
    let thirds = 1.0 - (f.thirds_dist as f64 / THIRDS_MAX_DIST);
    let phi = 1.0 - (f.phi_dist as f64 / PHI_MAX_DIST);
    let best = thirds.max(phi).clamp(0.0, 1.0) as f32;
    if f.centred {
        best.max(1.0)
    } else {
        best
    }
}

/// Room ahead minus room behind, over the frame's extent along `d`. For a
/// horizontal direction this is `(W - x1 - x0) / W`; vertical motion uses the
/// height, diagonal motion the rectangle's projected extent.
fn lead_room(d: [f64; 2], b: [f64; 4], w: f64, h: f64) -> f64 {
    let project = |x: f64, y: f64| d[0] * x + d[1] * y;
    let frame_min = project(0.0, 0.0)
        .min(project(w, 0.0))
        .min(project(0.0, h))
        .min(project(w, h));
    let frame_max = project(0.0, 0.0)
        .max(project(w, 0.0))
        .max(project(0.0, h))
        .max(project(w, h));
    let box_min = project(b[0], b[1]).min(project(b[2], b[3]));
    let box_max = project(b[0], b[1]).max(project(b[2], b[3]));
    let extent = frame_max - frame_min;
    if extent <= 1e-9 {
        return 0.0;
    }
    let ahead = frame_max - box_max;
    let behind = box_min - frame_min;
    ((ahead - behind) / extent).clamp(-1.0, 1.0)
}

/// Distance from a normalised centroid to the nearest grid point, divided by
/// the diagonal. The research (4.1) normalises per axis instead; the diagonal
/// is the scale this module exposes, and `*_MAX_DIST` converts it back to a
/// 0..1 mode score.
fn grid_dist(nx: f64, ny: f64, points: [f64; 2]) -> f64 {
    let mut best = f64::INFINITY;
    for &gx in &points {
        for &gy in &points {
            best = best.min(((nx - gx).powi(2) + (ny - gy).powi(2)).sqrt());
        }
    }
    best / std::f64::consts::SQRT_2
}

/// Area-weighted horizontal mass of every box; 0 when there is no area.
fn balance(subject: [f64; 4], others: &[[f64; 4]], w: f64, h: f64) -> f32 {
    let mut mass = 0.0;
    let mut weight = 0.0;
    for raw in std::iter::once(subject).chain(others.iter().copied()) {
        let b = clamp_box(ordered(raw), w, h);
        let a = box_area(b);
        if a <= 0.0 {
            continue;
        }
        let (cx, _) = centroid(b);
        mass += a * (cx / w - 0.5) * 2.0;
        weight += a;
    }
    if weight <= 0.0 {
        0.0
    } else {
        (mass / weight).clamp(-1.0, 1.0) as f32
    }
}

fn ordered(b: [f64; 4]) -> [f64; 4] {
    [
        b[0].min(b[2]),
        b[1].min(b[3]),
        b[0].max(b[2]),
        b[1].max(b[3]),
    ]
}

fn clamp_box(b: [f64; 4], w: f64, h: f64) -> [f64; 4] {
    [
        b[0].clamp(0.0, w),
        b[1].clamp(0.0, h),
        b[2].clamp(0.0, w),
        b[3].clamp(0.0, h),
    ]
}

fn box_area(b: [f64; 4]) -> f64 {
    (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0)
}

/// The area two boxes share; 0 when they do not touch.
fn intersection(a: [f64; 4], b: [f64; 4]) -> f64 {
    let (w, h) = (
        (a[2].min(b[2]) - a[0].max(b[0])).max(0.0),
        (a[3].min(b[3]) - a[1].max(b[1])).max(0.0),
    );
    w * h
}

fn centroid(b: [f64; 4]) -> (f64, f64) {
    ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: [f64; 2] = [1000.0, 1000.0];

    fn score(s: f32) {
        assert!((0.0..=1.0).contains(&s), "score out of range: {s}");
    }

    #[test]
    fn one_edge_is_one_cut() {
        let f = framing(FRAME, [0.0, 400.0, 200.0, 600.0], &[], None, None);
        assert!(f.cut_left);
        assert!(!f.cut_top && !f.cut_right && !f.cut_bottom);
        assert_eq!(f.cut_sides, 1);
    }

    #[test]
    fn two_edges_are_two_cuts() {
        let f = framing(FRAME, [800.0, 0.0, 1000.0, 200.0], &[], None, None);
        assert!(f.cut_top && f.cut_right);
        assert!(!f.cut_left && !f.cut_bottom);
        assert_eq!(f.cut_sides, 2);
    }

    #[test]
    fn a_whole_frame_box_is_not_cut() {
        let f = framing(FRAME, [0.0, 0.0, 1000.0, 1000.0], &[], None, None);
        assert_eq!(f.cut_sides, 0);
        assert!(!f.cut_left && !f.cut_top && !f.cut_right && !f.cut_bottom);
        assert!((f.subject_frac - 1.0).abs() < 1e-6);
        assert_eq!(f.size_band, SizeBand::Tight);
    }

    #[test]
    fn zero_area_box_is_finite() {
        let f = framing(FRAME, [500.0, 500.0, 500.0, 500.0], &[], None, None);
        assert_eq!(f.subject_frac, 0.0);
        assert_eq!(f.size_band, SizeBand::TooSmall);
        assert!(f.centred);
        assert_eq!(f.balance, 0.0);
        assert!(f.cut_sides == 0);
        assert!((f.thirds_dist - (1.0 / 6.0) as f32).abs() < 1e-5);
        assert!((f.phi_dist - 0.118f32).abs() < 1e-3);
        score(framing_score(&f, &FramingPolicy::default()));
    }

    #[test]
    fn degenerate_frame_is_neutral() {
        let f = framing([0.0, 0.0], [0.0, 0.0, 10.0, 10.0], &[], None, Some(2.0));
        assert_eq!(f.lead_room, None);
        assert_eq!(f.cut_sides, 0);
        assert_eq!(f.tilt_deg, Some(2.0));
        let s = framing_score(&f, &FramingPolicy::default());
        score(s);
    }

    #[test]
    fn size_bands() {
        let small = framing(FRAME, [0.0, 0.0, 100.0, 100.0], &[], None, None);
        assert_eq!(small.size_band, SizeBand::TooSmall);
        let good = framing(FRAME, [0.0, 0.0, 300.0, 300.0], &[], None, None);
        assert_eq!(good.size_band, SizeBand::Good);
        let tight = framing(FRAME, [0.0, 0.0, 900.0, 900.0], &[], None, None);
        assert_eq!(tight.size_band, SizeBand::Tight);
        // Bands are inclusive at their lower edge.
        let edge = framing(FRAME, [0.0, 0.0, 200.0, 100.0], &[], None, None);
        assert!((edge.subject_frac - 0.02).abs() < 1e-6);
        assert_eq!(edge.size_band, SizeBand::Good);
    }

    #[test]
    fn motion_right_and_left_are_mirrored() {
        // Box centred horizontally would give 0; this one sits left of centre.
        let subject_box = [200.0, 400.0, 400.0, 600.0];
        let right = framing(FRAME, subject_box, &[], Some(Motion::AngleDeg(0.0)), None);
        let left = framing(FRAME, subject_box, &[], Some(Motion::AngleDeg(180.0)), None);
        assert!((right.lead_room.unwrap() - 0.4).abs() < 1e-5);
        assert!((left.lead_room.unwrap() + 0.4).abs() < 1e-5);
        // Same direction as a vector, not an angle.
        let vec = framing(
            FRAME,
            subject_box,
            &[],
            Some(Motion::Vector([3.0, 0.0])),
            None,
        );
        assert_eq!(vec.lead_room, right.lead_room);
        // Down the frame: height is the extent, not width.
        let down = framing(
            FRAME,
            [400.0, 0.0, 600.0, 200.0],
            &[],
            Some(Motion::AngleDeg(90.0)),
            None,
        );
        assert!((down.lead_room.unwrap() - 0.8).abs() < 1e-5);
        // No direction: unknown, not zero.
        assert_eq!(framing(FRAME, subject_box, &[], None, None).lead_room, None);
    }

    #[test]
    fn an_axis_gives_lead_room_without_guessing_the_direction() {
        // The car left of centre with the line running along x: the room ahead
        // (to the right) is bigger, so lead room is positive.
        let left = framing(FRAME, [200.0, 400.0, 400.0, 600.0], &[], Some(Motion::AxisDeg(0.0)), None);
        assert!((left.lead_room.unwrap() - 0.4).abs() < 1e-5);
        // The mirror image: the same car right of centre, the room ahead (left)
        // is bigger again, and the same positive number comes out. A frame
        // cannot say which way the car travelled, so neither result may be
        // penalised as "room behind".
        let right = framing(FRAME, [600.0, 400.0, 800.0, 600.0], &[], Some(Motion::AxisDeg(0.0)), None);
        assert!((right.lead_room.unwrap() - 0.4).abs() < 1e-5);
        // 0 and 180 degrees are the same line.
        let turned = framing(FRAME, [200.0, 400.0, 400.0, 600.0], &[], Some(Motion::AxisDeg(180.0)), None);
        assert_eq!(turned.lead_room, left.lead_room);
        // And a vertical pan measures the height, not the width.
        let up = framing(FRAME, [400.0, 0.0, 600.0, 200.0], &[], Some(Motion::AxisDeg(90.0)), None);
        assert!((up.lead_room.unwrap() - 0.8).abs() < 1e-5);
        // An arrow along the same line still mirrors.
        let along = framing(FRAME, [200.0, 400.0, 400.0, 600.0], &[], Some(Motion::AngleDeg(180.0)), None);
        assert!((along.lead_room.unwrap() + 0.4).abs() < 1e-5);
    }

    #[test]
    fn a_centred_subject_on_the_motion_line_drops_the_lead_term() {
        // A head-on car: centred along the pan line, no lead question to answer.
        let centred = framing(FRAME, [400.0, 400.0, 600.0, 600.0], &[], Some(Motion::AxisDeg(0.0)), None);
        assert!(centred.lead_room.unwrap().abs() < LEAD_NEUTRAL);
        let sport = FramingPolicy::sport();
        let no_lead = FramingPolicy {
            lead_weight: 0.0,
            ..sport
        };
        assert_eq!(
            framing_score(&centred, &sport),
            framing_score(&centred, &no_lead),
            "a centred subject is not penalised for being centred"
        );
        // But a subject clearly behind its room is scored, and scores lower.
        let behind = framing(FRAME, [600.0, 400.0, 800.0, 600.0], &[], Some(Motion::AngleDeg(0.0)), None);
        assert!(framing_score(&behind, &sport) < framing_score(&centred, &sport));
    }

    #[test]
    fn lead_room_stays_in_range() {
        let pinned = framing(
            FRAME,
            [0.0, 0.0, 100.0, 100.0],
            &[],
            Some(Motion::AngleDeg(0.0)),
            None,
        );
        assert!((pinned.lead_room.unwrap() - 0.9).abs() < 1e-5);
        let outside = framing(
            FRAME,
            [-100.0, 400.0, 0.0, 500.0],
            &[],
            Some(Motion::AngleDeg(0.0)),
            None,
        );
        assert!(outside.lead_room.unwrap().abs() <= 1.0);
    }

    #[test]
    fn thirds_and_phi_distance() {
        // Centroid exactly on a third point.
        let f = framing([300.0, 300.0], [90.0, 90.0, 110.0, 110.0], &[], None, None);
        assert!(f.thirds_dist < 1e-5);
        assert!((f.phi_dist - 0.049).abs() < 1e-2);
        // Dead centre is 1/6 of the diagonal from the nearest third point,
        // and closer to a phi point.
        let c = framing(FRAME, [450.0, 450.0, 550.0, 550.0], &[], None, None);
        assert!(c.centred);
        assert!((c.thirds_dist - (1.0 / 6.0) as f32).abs() < 1e-5);
        assert!((c.phi_dist - 0.118).abs() < 1e-3);
    }

    #[test]
    fn centring_radius() {
        let inside = framing(FRAME, [540.0, 480.0, 560.0, 520.0], &[], None, None);
        assert!(inside.centred);
        let outside = framing(FRAME, [580.0, 480.0, 600.0, 520.0], &[], None, None);
        assert!(!outside.centred);
    }

    #[test]
    fn balance_is_area_weighted() {
        let left = framing(FRAME, [100.0, 400.0, 300.0, 600.0], &[], None, None);
        assert!((left.balance + 0.6).abs() < 1e-5);
        // Equal masses either side cancel.
        let both = framing(
            FRAME,
            [100.0, 100.0, 300.0, 300.0],
            &[[700.0, 700.0, 900.0, 900.0]],
            None,
            None,
        );
        assert!(both.balance.abs() < 1e-5);
        // Zero-area boxes are ignored, not dominate.
        let tiny = framing(FRAME, [100.0, 100.0, 300.0, 300.0], &[[0.0; 4]], None, None);
        assert!((tiny.balance + 0.6).abs() < 1e-5);
    }

    #[test]
    fn tilt_is_passed_through_and_scored() {
        let t = framing(FRAME, [400.0, 400.0, 600.0, 600.0], &[], None, Some(2.5));
        assert_eq!(t.tilt_deg, Some(2.5));
        let level = framing(FRAME, [400.0, 400.0, 600.0, 600.0], &[], None, Some(0.0));
        let tilted = framing_score(&t, &FramingPolicy::default());
        let straight = framing_score(&level, &FramingPolicy::default());
        assert!(tilted < straight);
        score(tilted);
    }

    #[test]
    fn lead_room_only_when_policy_asks() {
        let subject_box = [200.0, 400.0, 400.0, 600.0];
        let right = framing(FRAME, subject_box, &[], Some(Motion::AngleDeg(0.0)), None);
        let left = framing(FRAME, subject_box, &[], Some(Motion::AngleDeg(180.0)), None);
        let portrait = FramingPolicy::portrait();
        assert_eq!(
            framing_score(&right, &portrait),
            framing_score(&left, &portrait)
        );
        let sport = FramingPolicy::sport();
        assert!(framing_score(&right, &sport) > framing_score(&left, &sport));
        // No direction: the term drops out instead of scoring zero.
        let unknown = framing(FRAME, subject_box, &[], None, None);
        assert_eq!(
            framing_score(&unknown, &sport),
            framing_score(
                &right,
                &FramingPolicy {
                    lead_room: false,
                    lead_weight: 0.0,
                    ..sport
                }
            )
        );
    }

    #[test]
    fn score_rewards_a_clean_frame() {
        let clean = framing(FRAME, [400.0, 400.0, 600.0, 600.0], &[], None, Some(0.0));
        assert!((framing_score(&clean, &FramingPolicy::default()) - 1.0).abs() < 1e-5);
        let cut = framing(FRAME, [0.0, 400.0, 200.0, 600.0], &[], None, Some(0.0));
        assert!(framing_score(&cut, &FramingPolicy::default()) < 1.0);
        let dot = framing(FRAME, [500.0, 500.0, 500.0, 500.0], &[], None, Some(0.0));
        assert!(framing_score(&dot, &FramingPolicy::default()) < 0.8);
        score(framing_score(&dot, &FramingPolicy::default()));
    }

    #[test]
    fn score_is_bounded_for_absurd_input() {
        let f = framing(
            [1.0, 1.0],
            [-50.0, -50.0, 500.0, 500.0],
            &[[-10.0, -10.0, 20.0, 20.0]],
            Some(Motion::Vector([0.0, 0.0])),
            Some(-400.0),
        );
        assert_eq!(f.subject_frac, 1.0);
        assert_eq!(f.cut_sides, 0);
        score(framing_score(&f, &FramingPolicy::default()));
        score(framing_score(&f, &FramingPolicy::sport()));
    }

    #[test]
    fn no_applicable_term_is_neutral() {
        let f = framing(FRAME, [400.0, 400.0, 600.0, 600.0], &[], None, None);
        let empty = FramingPolicy {
            lead_room: false,
            lead_weight: 0.0,
            cut_weight: 0.0,
            size_weight: 0.0,
            mode_weight: 0.0,
            balance_weight: 0.0,
            occlusion_weight: 0.0,
            tilt_weight: 0.0,
        };
        assert_eq!(framing_score(&f, &empty), 0.5);
    }
}
