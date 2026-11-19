//! What the camera's autofocus area says about a frame's subject.
//!
//! One rule, shared by the scan (`cull_frame`), a re-score (`rescore`) and the
//! focus-point backfill (`rescore::measure_af_frames`), so the three cannot
//! disagree. The AF area is the camera's own word on where the subject is, and
//! has three possible relations to the detector's boxes:
//!
//! - its centre is inside a detection: a second opinion on that subject's
//!   sharpness, which can only lower the score (`rescore::second_opinion`), and
//!   only when the area is big enough to measure a subject with
//!   (`MIN_AF_OPINION_SHARE`);
//! - its centre is outside every detection, or there is none: the AF area *is*
//!   the subject. It is measured with the edge-blur path a detected subject
//!   uses, and that score rescues the frame (`rescue`: the frame keeps the
//!   higher of it and what the detector and whole-frame paths produced) while
//!   every detection becomes a bystander;
//! - the AF area is unusable: absent, a zone covering a large share of the
//!   frame, or featureless. Then nothing here applies and the frame is rated
//!   exactly as it was before the camera's word was consulted.
//!
//! A focus grid needs a fourth answer. On an EOS R7 the camera reports ~100
//! cells in focus on a shot of a car filling a third of the frame: the grid is
//! a depth-of-field map, not a point on the subject, so taking it as the subject
//! demoted real keepers by two stars. Its union is trusted only when the
//! detector found nothing but small objects (a background truck, a distant
//! spectator): then everything it found is background and the camera's focus
//! is the only word left on the subject. A grid with any real detection on it
//! is unusable, as it was before the grid was read at all.
use conrod_io::af::AreaKind;
use conrod_vision::imageops::Gray;
use conrod_vision::sharpness;

/// An AF area covering more than this share of the frame is a zone or a
/// whole-area mode (mode 22 reads as most of the frame), not a point on a
/// subject, so it is not authoritative. A spot or flexible-zone area on the R7
/// is 3-11% of the frame.
pub const MAX_AF_SHARE: f64 = 0.40;

/// A detection smaller than this share of the frame is background for the
/// grid gate: too small to be the frame's subject, where a subject is what the
/// photographer would deliver. 0202's misplaced truck reads at 3%; a car filling
/// the frame at 30-50%.
pub const SMALL_DETECTION: f64 = 0.05;

/// An AF centre this close outside a detection's box (as a share of the
/// frame's long edge) is still taken as being on that subject: detector boxes
/// are tight, and a point on a mirror or a wheel arch can sit just outside one.
/// Such a frame is left as it was, rather than called a bystander.
pub const NEAR_MARGIN: f64 = 0.02;

/// An AF area smaller than this share of the frame is not a second opinion on a
/// detection. The EOS 1500D's single cell is about 0.3% (190 x 190 px on its 24
/// MP sensor), and as a second opinion it read soft on sharp subjects whose
/// centre sits on a smooth panel: with it, the Ford Falcon readiness case kept
/// 16.7% of its keepers rated sharp, without it 38.9%. The same tiny area may
/// still *rescue* a frame (see [`rescue`]), because a rescue can only raise.
pub const MIN_AF_OPINION_SHARE: f64 = 0.01;

/// What the AF area means for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Call {
    /// No usable AF area (absent, too large, or featureless): rate as before.
    Unused,
    /// Its centre is on a detection (or near enough): the detection stands.
    OnSubject,
    /// It is the subject: its edge-blur rating, a floor the frame is rescued
    /// to (see [`rescue`]) and the score stored for the UI.
    Subject(f64),
}

impl Call {
    /// The AF rating when the AF area is the subject.
    pub fn subject(self) -> Option<f64> {
        match self {
            Call::Subject(score) => Some(score),
            _ => None,
        }
    }
}

/// The camera's AF area for one frame, with the kind of record it came from.
/// `area` is in frame pixels; the record kind decides whether the area may
/// speak for the frame at all (see [`usable`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusArea {
    pub area: [f64; 4],
    pub kind: AreaKind,
}

/// Does `subject` contain the centre of `af`? Both are frame pixels.
pub fn holds_middle(subject: [f64; 4], af: [f64; 4]) -> bool {
    let (cx, cy) = ((af[0] + af[2]) / 2.0, (af[1] + af[3]) / 2.0);
    cx >= subject[0] && cx <= subject[2] && cy >= subject[1] && cy <= subject[3]
}

/// The box's share of the frame, 0..1 (also used for a detection's box).
pub fn share(af: [f64; 4], frame: (usize, usize)) -> f64 {
    let area = (af[2] - af[0]).max(0.0) * (af[3] - af[1]).max(0.0);
    area / (frame.0.max(1) as f64 * frame.1.max(1) as f64)
}

/// May the record speak for this frame at all? A point always may, up to
/// [`MAX_AF_SHARE`]. A grid may only when the detector found at least one and
/// nothing but small objects: then its union stands in for the subject the
/// detector missed, and every detection is background. `boxes` are the
/// vehicle and person detections.
pub fn usable(af: FocusArea, frame: (usize, usize), boxes: &[[f64; 4]]) -> bool {
    if share(af.area, frame) > MAX_AF_SHARE {
        return false;
    }
    match af.kind {
        AreaKind::Point => true,
        AreaKind::Grid => {
            !boxes.is_empty() && boxes.iter().all(|b| share(*b, frame) < SMALL_DETECTION)
        }
    }
}

/// The AF area as a second opinion on a detection: `Some` only when the record
/// may speak for the frame ([`usable`]) and the area is at least
/// [`MIN_AF_OPINION_SHARE`] of it, so a body's single small cell cannot pull a
/// sharp subject down. The same area may still be the subject and rescue the
/// frame, which is why this is separate from [`usable`].
pub fn opinion(af: FocusArea, frame: (usize, usize), boxes: &[[f64; 4]]) -> Option<[f64; 4]> {
    (share(af.area, frame) >= MIN_AF_OPINION_SHARE && usable(af, frame, boxes)).then_some(af.area)
}

/// The frame rating, with the camera's focus area as a rescue only.
///
/// The app promises that a sharp frame is not culled, so the camera's word is
/// evidence of the intended subject, never a veto: where the AF area is the
/// subject its score is a floor under what the detector and the whole-frame
/// path already produced. An AF patch on a smooth panel (a wheel arch, a
/// mirror, a small cell on a 9-point body) can then no longer drag a sharp
/// frame down, while a frame the detector only found a background object in is
/// lifted onto the subject the camera actually focused on.
pub fn rescue(detected: f64, af: Option<f64>) -> f64 {
    af.map_or(detected, |score| detected.max(score))
}

/// The geometry half of the rule, which needs no pixels: is the AF area, by its
/// kind, size and where its centre falls among the detection `boxes`, a
/// candidate to be the subject? `Some(true)` when it is, `Some(false)` when a
/// detection holds it, `None` when it cannot decide (unusable, or a zone).
pub fn geometry(af: FocusArea, frame: (usize, usize), boxes: &[[f64; 4]]) -> Option<bool> {
    if !usable(af, frame, boxes) {
        return None;
    }
    let area = af.area;
    if boxes.iter().any(|b| holds_middle(*b, area)) {
        return Some(false);
    }
    let margin = NEAR_MARGIN * frame.0.max(frame.1) as f64;
    let near = boxes.iter().any(|b| {
        holds_middle(
            [b[0] - margin, b[1] - margin, b[2] + margin, b[3] + margin],
            area,
        )
    });
    Some(!near)
}

/// One AF box's edge-blur rating on native luma: the same measure as a
/// detected subject's whole box. `None` for a box too small to measure or
/// without edges (a completely blurred or featureless patch has a sigma at the
/// floor, which is not evidence of focus).
pub fn rate_box(grey: &Gray, af: [f64; 4], strictness: &str) -> Option<f64> {
    let (fw, fh) = (grey.width, grey.height);
    let px = |v: f64, edge: usize| (v.max(0.0) as usize).min(edge);
    let region = [px(af[0], fw), px(af[1], fh), px(af[2], fw), px(af[3], fh)];
    if region[2] <= region[0] + 8 || region[3] <= region[1] + 8 {
        return None;
    }
    let blur = conrod_vision::edgeblur::measure(grey, Some(region));
    if blur.flat() {
        return None;
    }
    sharpness::rating_from_blur(
        blur.sigma_worst_at_reference(fw.max(fh)),
        sharpness::BlurStrictness::parse(strictness),
    )
}

/// The whole rule. `af` is the AF area in the preview's pixels, `boxes` the
/// detections' person and vehicle boxes (faces and eyes sit inside those).
pub fn decide(grey: &Gray, af: Option<FocusArea>, boxes: &[[f64; 4]], strictness: &str) -> Call {
    let Some(af) = af else {
        return Call::Unused;
    };
    match geometry(af, (grey.width, grey.height), boxes) {
        None => Call::Unused,
        Some(false) => Call::OnSubject,
        Some(true) => rate_box(grey, af.area, strictness).map_or(Call::Unused, Call::Subject),
    }
}

/// The scan's half of the rule, once the detector's subjects are measured:
/// decide, and where the AF area is the subject mark every detection a
/// bystander and drop the cull reason it would have carried. A bystander does
/// not drive the frame's rating, the keeper or the cull.
pub(crate) fn apply(
    subjects: &mut [crate::Subject],
    grey: &Gray,
    af: Option<FocusArea>,
    strictness: &str,
) -> Call {
    let boxes: Vec<[f64; 4]> = subjects
        .iter()
        .filter(|s| matches!(s.region_type, "vehicle" | "person"))
        .map(|s| s.bbox)
        .collect();
    let call = decide(grey, af, &boxes, strictness);
    if call.subject().is_some() {
        for s in subjects {
            s.bystander = true;
            s.cull_reason.clear();
        }
    }
    call
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: (usize, usize) = (6000, 4000);

    fn af(cx: f64, cy: f64, w: f64, h: f64) -> [f64; 4] {
        [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0]
    }

    fn point(at: [f64; 4]) -> FocusArea {
        FocusArea {
            area: at,
            kind: AreaKind::Point,
        }
    }

    fn grid(at: [f64; 4]) -> FocusArea {
        FocusArea {
            area: at,
            kind: AreaKind::Grid,
        }
    }

    /// A box `w` by `h` fractions of the frame, its top-left at (x, y).
    fn boxed(x: f64, y: f64, w: f64, h: f64) -> [f64; 4] {
        let (fw, fh) = (FRAME.0 as f64, FRAME.1 as f64);
        [x * fw, y * fh, (x + w) * fw, (y + h) * fh]
    }

    #[test]
    fn the_focus_area_never_lowers_a_frames_rating() {
        // A sharp frame whose AF patch read soft keeps its own number.
        assert_eq!(rescue(0.95, Some(0.60)), 0.95);
        // A frame the detector only found background in is lifted.
        assert_eq!(rescue(0.62, Some(0.92)), 0.92);
        // No usable area: unchanged.
        assert_eq!(rescue(0.70, None), 0.70);
        // Equal scores are the same frame either way.
        assert_eq!(rescue(0.80, Some(0.80)), 0.80);
    }

    #[test]
    fn a_tiny_focus_point_is_not_a_second_opinion_but_can_still_rescue() {
        // The 1500D's cell: ~0.25% of the frame, off the detected car.
        let tiny = point(af(1000.0, 700.0, 300.0, 200.0));
        assert!(share(tiny.area, FRAME) < MIN_AF_OPINION_SHARE);
        let car = boxed(0.4, 0.4, 0.3, 0.3);
        // Too small to check a detection's score...
        assert_eq!(opinion(tiny, FRAME, &[car]), None);
        // ...but still the camera's word on where the subject is.
        assert!(usable(tiny, FRAME, &[car]));
        assert_eq!(geometry(tiny, FRAME, &[car]), Some(true));
        // The scan's shape: the tiny patch rates 0.40, the car 0.93, and the
        // frame keeps 0.93 because the camera may only rescue.
        let detected = 0.93;
        assert_eq!(rescue(detected, Some(0.40)), detected);

        // An area at the threshold is a second opinion.
        let side = (MIN_AF_OPINION_SHARE * 6_000.0 * 4_000.0).sqrt() + 1.0;
        let big = point(af(1000.0, 700.0, side, side));
        assert_eq!(opinion(big, FRAME, &[car]), Some(big.area));
    }

    #[test]
    fn a_refused_grid_is_not_a_second_opinion_either() {
        // A big union that the grid gate refuses must not sneak in as a second
        // opinion on the detection that refused it.
        let union = boxed(0.1, 0.1, 0.6, 0.5);
        let car = boxed(0.3, 0.3, 0.3, 0.3);
        assert_eq!(opinion(grid(union), FRAME, &[car]), None);
        assert_eq!(opinion(grid(union), FRAME, &[]), None);
    }

    #[test]
    fn a_centre_inside_a_detection_is_on_the_subject() {
        let car = [1000.0, 1000.0, 3000.0, 2000.0];
        assert_eq!(
            geometry(point(af(2000.0, 1500.0, 300.0, 200.0)), FRAME, &[car]),
            Some(false)
        );
    }

    #[test]
    fn a_centre_outside_every_detection_makes_the_area_the_subject() {
        let truck = [5200.0, 1500.0, 6000.0, 2500.0];
        assert_eq!(
            geometry(point(af(2000.0, 1500.0, 300.0, 200.0)), FRAME, &[truck]),
            Some(true)
        );
        // No detection at all: nothing else could be the subject.
        assert_eq!(
            geometry(point(af(2000.0, 1500.0, 300.0, 200.0)), FRAME, &[]),
            Some(true)
        );
    }

    #[test]
    fn a_centre_just_outside_a_box_is_left_alone() {
        let car = [1000.0, 1000.0, 3000.0, 2000.0];
        // 60 px outside, with a margin of 2% of 6000 = 120 px.
        assert_eq!(
            geometry(point(af(3060.0, 1500.0, 300.0, 200.0)), FRAME, &[car]),
            Some(false)
        );
        assert_eq!(
            geometry(point(af(3300.0, 1500.0, 300.0, 200.0)), FRAME, &[car]),
            Some(true)
        );
    }

    #[test]
    fn a_zone_covering_much_of_the_frame_decides_nothing() {
        // 45% of the frame: a whole-area mode, not a point on a subject.
        let zone = af(3000.0, 2000.0, 6000.0 * 0.67, 4000.0 * 0.67);
        assert!(share(zone, FRAME) > MAX_AF_SHARE);
        assert_eq!(geometry(point(zone), FRAME, &[]), None);
        assert_eq!(
            geometry(point(zone), FRAME, &[[100.0, 100.0, 400.0, 400.0]]),
            None
        );
        // The R7's 2631x1285 spot area is a point on a subject.
        let spot = af(3000.0, 2000.0, 2631.0, 1285.0);
        assert!(share(spot, FRAME) < MAX_AF_SHARE);
    }

    #[test]
    fn a_grid_whose_only_detections_are_small_is_the_subject() {
        // 0202: the camera's in-focus cells span a third of the frame over the
        // car; the detector found a 3% background truck.
        let union = boxed(0.2, 0.13, 0.68, 0.49);
        assert!(share(union, FRAME) < MAX_AF_SHARE);
        let truck = boxed(0.7, 0.1, 0.17, 0.17);
        assert!(share(truck, FRAME) < SMALL_DETECTION);
        assert_eq!(geometry(grid(union), FRAME, &[truck]), Some(true));
    }

    #[test]
    fn a_grid_with_a_real_detection_is_unusable() {
        let union = boxed(0.2, 0.13, 0.68, 0.49);
        // A car at 12% of the frame: the detector has said what the subject is,
        // and the grid would only demote it.
        let car = boxed(0.3, 0.3, 0.4, 0.3);
        assert_eq!(geometry(grid(union), FRAME, &[car]), None);
        assert_eq!(geometry(grid(union), FRAME, &[]), None);
        // A point is unchanged: it is the camera's own spot.
        assert_eq!(
            geometry(point(af(500.0, 500.0, 300.0, 200.0)), FRAME, &[car]),
            Some(true)
        );
    }

    #[test]
    fn a_grid_centre_on_a_small_detection_is_only_a_second_opinion() {
        // The union's centre sits on a small car; the area is never the subject.
        let union = boxed(0.2, 0.3, 0.4, 0.2);
        let car = boxed(0.35, 0.35, 0.08, 0.06);
        assert!(share(car, FRAME) < SMALL_DETECTION);
        assert!(holds_middle(car, union));
        assert_eq!(geometry(grid(union), FRAME, &[car]), Some(false));
    }

    #[test]
    fn the_cap_applies_to_a_grid_union() {
        // A grid whose in-focus cells span half the frame is a zone, small
        // detections or not.
        let wide = boxed(0.05, 0.05, 0.9, 0.7);
        assert!(share(wide, FRAME) > MAX_AF_SHARE);
        let truck = boxed(0.7, 0.1, 0.17, 0.17);
        assert_eq!(geometry(grid(wide), FRAME, &[truck]), None);
    }

    /// A flat grey frame: no edges anywhere.
    fn flat() -> Gray {
        Gray::new(2400, 1600, vec![128; 2400 * 1600])
    }

    #[test]
    fn a_featureless_area_is_not_a_subject() {
        let grey = flat();
        let area = [800.0, 600.0, 1400.0, 1000.0];
        assert_eq!(rate_box(&grey, area, "balanced"), None);
        assert_eq!(
            decide(&grey, Some(point(area)), &[], "balanced"),
            Call::Unused
        );
    }

    #[test]
    fn no_area_leaves_the_frame_as_it_was() {
        assert_eq!(
            decide(&flat(), None, &[[0.0, 0.0, 100.0, 100.0]], "balanced"),
            Call::Unused
        );
    }

    #[test]
    fn a_tiny_area_cannot_be_measured() {
        let grey = flat();
        assert_eq!(rate_box(&grey, [10.0, 10.0, 14.0, 14.0], "balanced"), None);
    }

    /// Deterministic value noise at several block scales: edges in every
    /// direction, as a real subject has.
    fn textured() -> Gray {
        let (w, h) = (2400usize, 1600usize);
        let hash = |x: usize, y: usize| {
            let mut v = (x as u32).wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
            v ^= v >> 13;
            v = v.wrapping_mul(0xC2B2_AE35);
            v ^ (v >> 16)
        };
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut level = 0.0f64;
                for (block, amp) in [(16usize, 90.0), (8, 60.0), (4, 40.0), (2, 25.0)] {
                    let n = hash(x / block, y / block) % 256;
                    level += amp * (f64::from(n) / 255.0 - 0.5);
                }
                data.push((128.0 + level).clamp(0.0, 255.0) as u8);
            }
        }
        Gray::new(w, h, data)
    }

    fn subject(region: &'static str, bbox: [f64; 4], cull: &str) -> crate::Subject {
        crate::Subject {
            class: "car",
            conf: 0.9,
            bbox,
            sharpness: 0.3,
            panning: false,
            sharp_end: "subject",
            rating: 0.3,
            stars: 1,
            cull_reason: cull.to_string(),
            features: Vec::new(),
            heuristic: 0.3,
            region_type: region,
            quality: None,
            eyes_open: [None, None],
            cut_sides: 0,
            subject_frac: 0.0,
            lead_room: None,
            framing_score: None,
            bystander: false,
        }
    }

    #[test]
    fn the_scan_marks_every_detection_a_bystander_when_the_area_is_the_subject() {
        let grey = textured();
        // A truck at the right edge, and a face inside it; the camera focused
        // on a sharp patch on the left.
        let mut subjects = vec![
            subject("vehicle", [1900.0, 700.0, 2400.0, 1100.0], "2 stars"),
            subject("face", [2000.0, 750.0, 2100.0, 850.0], ""),
        ];
        let af = [300.0, 600.0, 700.0, 900.0];
        let call = apply(&mut subjects, &grey, Some(point(af)), "moderate");
        let score = call.subject().expect("the textured area is the subject");
        assert_eq!(Some(score), rate_box(&grey, af, "moderate"));
        assert!(subjects.iter().all(|s| s.bystander));
        assert!(
            subjects.iter().all(|s| s.cull_reason.is_empty()),
            "a bystander carries no cull"
        );
    }

    #[test]
    fn the_scan_reads_a_grid_when_every_detection_is_small() {
        let grey = textured();
        // A 3% background object, and the camera's grid over a third of the
        // frame: the grid is the subject and the object is a bystander.
        let small = subject("vehicle", [2000.0, 1400.0, 2100.0, 1500.0], "1 star");
        let mut subjects = vec![small];
        let union = [300.0, 300.0, 1500.0, 1100.0];
        let call = apply(&mut subjects, &grey, Some(grid(union)), "moderate");
        assert_eq!(call.subject(), rate_box(&grey, union, "moderate"));
        assert!(subjects[0].bystander);
        assert!(subjects[0].cull_reason.is_empty());

        // The same grid with a car filling a fifth of the frame: nothing moves.
        let mut big = vec![subject(
            "vehicle",
            [600.0, 400.0, 1800.0, 1200.0],
            "2 stars",
        )];
        assert_eq!(
            apply(&mut big, &grey, Some(grid(union)), "moderate"),
            Call::Unused
        );
        assert!(!big[0].bystander);
        assert_eq!(big[0].cull_reason, "2 stars");
    }

    #[test]
    fn the_scan_leaves_subjects_alone_when_a_detection_holds_the_area() {
        let grey = textured();
        let mut subjects = vec![subject("vehicle", [200.0, 500.0, 900.0, 1000.0], "2 stars")];
        let call = apply(
            &mut subjects,
            &grey,
            Some(point([300.0, 600.0, 700.0, 900.0])),
            "moderate",
        );
        assert_eq!(call, Call::OnSubject);
        assert!(!subjects[0].bystander);
        assert_eq!(subjects[0].cull_reason, "2 stars");
    }

    #[test]
    fn the_scan_leaves_subjects_alone_without_a_usable_area() {
        let grey = textured();
        let car = [1900.0, 700.0, 2400.0, 1100.0];
        // No AF at all.
        let mut subjects = vec![subject("vehicle", car, "2 stars")];
        assert_eq!(apply(&mut subjects, &grey, None, "moderate"), Call::Unused);
        assert!(!subjects[0].bystander);
        // A zone over most of the frame.
        let zone = [100.0, 100.0, 1700.0, 1300.0];
        assert!(share(zone, (2400, 1600)) > MAX_AF_SHARE);
        assert_eq!(
            apply(&mut subjects, &grey, Some(point(zone)), "moderate"),
            Call::Unused
        );
        assert!(!subjects[0].bystander);
        // A featureless area.
        let mut flat_subjects = vec![subject("vehicle", car, "2 stars")];
        assert_eq!(
            apply(
                &mut flat_subjects,
                &flat(),
                Some(point([300.0, 600.0, 700.0, 900.0])),
                "moderate"
            ),
            Call::Unused
        );
        assert!(!flat_subjects[0].bystander);
        assert_eq!(flat_subjects[0].cull_reason, "2 stars");
    }

    #[test]
    fn a_frame_with_no_detection_is_rated_on_the_area() {
        let grey = textured();
        let call = apply(
            &mut [],
            &grey,
            Some(point([300.0, 600.0, 700.0, 900.0])),
            "moderate",
        );
        assert!(call.subject().is_some());
        // A grid with no detection at all names nothing: the frame is rated as
        // it always was.
        assert_eq!(
            apply(
                &mut [],
                &grey,
                Some(grid([300.0, 600.0, 700.0, 900.0])),
                "moderate"
            ),
            Call::Unused
        );
    }
}
