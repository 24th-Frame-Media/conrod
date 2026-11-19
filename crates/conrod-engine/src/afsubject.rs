//! What the camera's autofocus area says about a frame's subject.
//!
//! One rule, shared by the scan (`cull_frame`) and a re-score (`rescore`), so
//! the two cannot disagree. The AF area is the camera's own word on where the
//! subject is, and has three possible relations to the detector's boxes:
//!
//! - its centre is inside a detection: a second opinion on that subject's
//!   sharpness, which can only lower the score (`rescore::second_opinion`);
//! - its centre is outside every detection, or there is none: the AF area *is*
//!   the subject. It is measured with the edge-blur path a detected subject
//!   uses and the frame is rated on it, and every detection is a bystander;
//! - the AF area is unusable: absent, a zone covering a large share of the
//!   frame, or featureless. Then nothing here applies and the frame is rated
//!   exactly as it was before the camera's word was consulted.
use conrod_vision::imageops::Gray;
use conrod_vision::sharpness;

/// An AF area covering more than this share of the frame is a zone or a
/// whole-area mode (mode 22 reads as most of the frame), not a point on a
/// subject, so it is not authoritative. A spot or flexible-zone area on the R7
/// is 3-11% of the frame.
pub const MAX_AF_SHARE: f64 = 0.40;

/// An AF centre this close outside a detection's box (as a share of the
/// frame's long edge) is still taken as being on that subject: detector boxes
/// are tight, and a point on a mirror or a wheel arch can sit just outside one.
/// Such a frame is left as it was, rather than called a bystander.
pub const NEAR_MARGIN: f64 = 0.02;

/// What the AF area means for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Call {
    /// No usable AF area (absent, too large, or featureless): rate as before.
    Unused,
    /// Its centre is on a detection (or near enough): the detection stands.
    OnSubject,
    /// It is the subject: its edge-blur rating, which rates the frame.
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

/// Does `subject` contain the centre of `af`? Both are frame pixels.
pub fn holds_middle(subject: [f64; 4], af: [f64; 4]) -> bool {
    let (cx, cy) = ((af[0] + af[2]) / 2.0, (af[1] + af[3]) / 2.0);
    cx >= subject[0] && cx <= subject[2] && cy >= subject[1] && cy <= subject[3]
}

/// The AF box's share of the frame, 0..1.
pub fn share(af: [f64; 4], frame: (usize, usize)) -> f64 {
    let area = (af[2] - af[0]).max(0.0) * (af[3] - af[1]).max(0.0);
    area / (frame.0.max(1) as f64 * frame.1.max(1) as f64)
}

/// The geometry half of the rule, which needs no pixels: is the AF area, by its
/// size and where its centre falls among the detection `boxes`, a candidate to
/// be the subject? `Some(true)` when it is, `Some(false)` when a detection
/// holds it, `None` when it cannot decide (the box is a zone).
pub fn geometry(af: [f64; 4], frame: (usize, usize), boxes: &[[f64; 4]]) -> Option<bool> {
    if share(af, frame) > MAX_AF_SHARE {
        return None;
    }
    if boxes.iter().any(|b| holds_middle(*b, af)) {
        return Some(false);
    }
    let margin = NEAR_MARGIN * frame.0.max(frame.1) as f64;
    let near = boxes.iter().any(|b| {
        holds_middle(
            [b[0] - margin, b[1] - margin, b[2] + margin, b[3] + margin],
            af,
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

/// The whole rule. `af` is the AF box in the preview's pixels, `boxes` the
/// detections' person and vehicle boxes (faces and eyes sit inside those).
pub fn decide(grey: &Gray, af: Option<[f64; 4]>, boxes: &[[f64; 4]], strictness: &str) -> Call {
    let Some(af) = af else {
        return Call::Unused;
    };
    match geometry(af, (grey.width, grey.height), boxes) {
        None => Call::Unused,
        Some(false) => Call::OnSubject,
        Some(true) => rate_box(grey, af, strictness).map_or(Call::Unused, Call::Subject),
    }
}

/// The scan's half of the rule, once the detector's subjects are measured:
/// decide, and where the AF area is the subject mark every detection a
/// bystander and drop the cull reason it would have carried. A bystander does
/// not drive the frame's rating, the keeper or the cull.
pub(crate) fn apply(
    subjects: &mut [crate::Subject],
    grey: &Gray,
    af: Option<[f64; 4]>,
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

    #[test]
    fn a_centre_inside_a_detection_is_on_the_subject() {
        let car = [1000.0, 1000.0, 3000.0, 2000.0];
        assert_eq!(
            geometry(af(2000.0, 1500.0, 300.0, 200.0), FRAME, &[car]),
            Some(false)
        );
    }

    #[test]
    fn a_centre_outside_every_detection_makes_the_area_the_subject() {
        let truck = [5200.0, 1500.0, 6000.0, 2500.0];
        assert_eq!(
            geometry(af(2000.0, 1500.0, 300.0, 200.0), FRAME, &[truck]),
            Some(true)
        );
        // No detection at all: nothing else could be the subject.
        assert_eq!(
            geometry(af(2000.0, 1500.0, 300.0, 200.0), FRAME, &[]),
            Some(true)
        );
    }

    #[test]
    fn a_centre_just_outside_a_box_is_left_alone() {
        let car = [1000.0, 1000.0, 3000.0, 2000.0];
        // 60 px outside, with a margin of 2% of 6000 = 120 px.
        assert_eq!(
            geometry(af(3060.0, 1500.0, 300.0, 200.0), FRAME, &[car]),
            Some(false)
        );
        assert_eq!(
            geometry(af(3300.0, 1500.0, 300.0, 200.0), FRAME, &[car]),
            Some(true)
        );
    }

    #[test]
    fn a_zone_covering_much_of_the_frame_decides_nothing() {
        // 45% of the frame: a whole-area mode, not a point on a subject.
        let zone = af(3000.0, 2000.0, 6000.0 * 0.67, 4000.0 * 0.67);
        assert!(share(zone, FRAME) > MAX_AF_SHARE);
        assert_eq!(geometry(zone, FRAME, &[]), None);
        assert_eq!(geometry(zone, FRAME, &[[100.0, 100.0, 400.0, 400.0]]), None);
        // The R7's 2631x1285 spot area is a point on a subject.
        let spot = af(3000.0, 2000.0, 2631.0, 1285.0);
        assert!(share(spot, FRAME) < MAX_AF_SHARE);
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
        assert_eq!(decide(&grey, Some(area), &[], "balanced"), Call::Unused);
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
        let call = apply(&mut subjects, &grey, Some(af), "moderate");
        let score = call.subject().expect("the textured area is the subject");
        assert_eq!(Some(score), rate_box(&grey, af, "moderate"));
        assert!(subjects.iter().all(|s| s.bystander));
        assert!(
            subjects.iter().all(|s| s.cull_reason.is_empty()),
            "a bystander carries no cull"
        );
    }

    #[test]
    fn the_scan_leaves_subjects_alone_when_a_detection_holds_the_area() {
        let grey = textured();
        let mut subjects = vec![subject("vehicle", [200.0, 500.0, 900.0, 1000.0], "2 stars")];
        let call = apply(
            &mut subjects,
            &grey,
            Some([300.0, 600.0, 700.0, 900.0]),
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
            apply(&mut subjects, &grey, Some(zone), "moderate"),
            Call::Unused
        );
        assert!(!subjects[0].bystander);
        // A featureless area.
        let mut flat_subjects = vec![subject("vehicle", car, "2 stars")];
        assert_eq!(
            apply(
                &mut flat_subjects,
                &flat(),
                Some([300.0, 600.0, 700.0, 900.0]),
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
            Some([300.0, 600.0, 700.0, 900.0]),
            "moderate",
        );
        assert!(call.subject().is_some());
    }
}
