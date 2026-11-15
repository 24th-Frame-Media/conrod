//! Keep-together sets: frames that are one picture, not several.
//!
//! A photographer who brackets exposure, stacks focus or runs a timelapse
//! leaves a trail of near-identical frames in the folder. Culled one by one,
//! the loser of a bracket looks like a bad frame and the rest of the stack
//! looks like duplicates; culled as a set, the pick is made between them and
//! the set survives or goes as a unit. So this module only answers *which
//! frames belong together*, never which one is best.
//!
//! The evidence is the capture settings the camera wrote down, not pixels:
//! exposure bias and shutter time for brackets, focus distance for stacks, and
//! the rhythm of the capture times for intervals. A setting the file does not
//! record cannot disqualify a witness -- an exported JPEG with no EXIF is
//! simply less evidence, not a different kind of frame.

use std::collections::BTreeMap;

/// The furthest apart, in seconds, two shots of one bracket can be. A bracket
/// is fired by hand or by the drive: the three to nine frames land within a
/// couple of seconds even when the tripod is being adjusted between rounds.
pub const BRACKET_GAP_SECONDS: f64 = 2.0;
/// Fewer is a lucky pair; more is not one bracket but several in a row (HDR
/// panoramas, which the repeat rule below splits into their rounds).
pub const BRACKET_MIN_FRAMES: usize = 3;
pub const BRACKET_MAX_FRAMES: usize = 9;
/// The smallest exposure change, in EV, that a bracket step must make. Auto
/// exposure drifts a third of a stop between frames of an ordinary burst; a
/// real bracket moves at least two thirds.
pub const BRACKET_STEP_EV: f64 = 0.6;
/// The whole bracket must span at least this much, in EV: two stops is the
/// usual +/-2 EV round and the smallest set worth keeping together.
pub const BRACKET_SPAN_EV: f64 = 1.5;
/// Two shots this close in exposure are the same shot again, so the sequence
/// has started a new round rather than continued one.
pub const BRACKET_REPEAT_EV: f64 = 0.2;

/// The gap a focus stack is shot with: the rail or the lens steps between
/// frames without the operator touching anything.
pub const STACK_GAP_SECONDS: f64 = 1.5;
pub const STACK_MIN_FRAMES: usize = 3;
/// A focus step has to actually move the plane; sensor noise in a repeated
/// reading must not read as a stack.
pub const STACK_STEP_RATIO: f64 = 0.01;

/// Below this many frames an interval is a burst with a slow shutter, not a
/// sequence anyone wants kept together.
pub const INTERVAL_MIN_FRAMES: usize = 10;
/// An interval of one frame a second is the fastest anyone runs a timelapse;
/// past two minutes it is a series of stills of the same view.
pub const INTERVAL_MIN_SECONDS: f64 = 1.0;
pub const INTERVAL_MAX_SECONDS: f64 = 120.0;
/// The standard deviation of the gaps, as a fraction of their mean: a
/// timelapse timer is quartz-accurate, so a shaky rhythm is not a sequence.
pub const INTERVAL_SPREAD: f64 = 0.10;

/// Focal length may wobble by this fraction (the lens reports whole
/// millimetres, and a zoom creeping between rounds is still one bracket).
pub const FOCAL_TOLERANCE: f64 = 0.01;
/// Aperture is either the same or it is not: two thirds of a stop is the
/// third-stop scale, so anything tighter than that is the same opening.
pub const APERTURE_TOLERANCE_EV: f64 = 0.34;
/// Shutter time across a focus stack or an interval, in EV, when it is only
/// the light that moved.
pub const SHUTTER_TOLERANCE_EV: f64 = 0.34;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetKind {
    /// Exposure bracket: one composition, several exposures.
    Bracket,
    /// Focus stack: one composition, the focus plane walked through it.
    FocusStack,
    /// Interval / timelapse sequence: one view, many identical frames.
    Interval,
}

/// One frame as the set finder sees it. Settings the file does not record are
/// `None` and are treated as "no evidence", never as a mismatch.
#[derive(Debug, Clone, PartialEq)]
pub struct SetFrame {
    /// Seconds since the epoch; `None` when the file carries no time.
    pub taken: Option<f64>,
    pub camera: String,
    /// Exposure compensation in EV. An auto-exposure bracket usually leaves
    /// this at 0 on every frame and moves the shutter instead.
    pub exposure_bias: Option<f64>,
    /// Shutter time in seconds.
    pub exposure_time: Option<f64>,
    pub f_number: Option<f64>,
    pub focal_length: Option<f64>,
    /// Focus distance in metres, when the file records it.
    pub focus_distance: Option<f64>,
    pub width: u32,
    pub height: u32,
}

/// Which set each frame belongs to, in input order: `None` for a frame that
/// stands on its own, otherwise the set's key (the index of its first frame,
/// stable for one call) and its kind. A frame belongs to at most one set.
///
/// Input must be in capture order; frames are grouped only with their
/// neighbours from the same camera, so two shooters in one folder neither
/// join nor interrupt each other's sets. Brackets are found first, then focus
/// stacks, then intervals, which is what makes a bracketed frame part of the
/// bracket rather than of the timelapse it sits in.
pub fn find_sets(frames: &[SetFrame]) -> Vec<Option<(i64, SetKind)>> {
    let mut out: Vec<Option<(i64, SetKind)>> = vec![None; frames.len()];
    // Per camera, in input order: the frames still free to join a set. Two
    // cameras interleave in one folder, and each one's own sequence is what
    // "consecutive" means.
    let mut by_camera: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, frame) in frames.iter().enumerate() {
        by_camera.entry(frame.camera.as_str()).or_default().push(i);
    }
    for order in by_camera.values() {
        let mut free: Vec<usize> = order.clone();
        for kind in [SetKind::Bracket, SetKind::FocusStack, SetKind::Interval] {
            let found = match kind {
                SetKind::Bracket => brackets(frames, &free),
                SetKind::FocusStack => stacks(frames, &free),
                SetKind::Interval => intervals(frames, &free),
            };
            for (first, members) in found {
                for &i in &members {
                    out[i] = Some((first as i64, kind));
                }
                free.retain(|i| !members.contains(i));
            }
        }
    }
    out
}

/// The gap between two frames in seconds, or `None` when either has no clock
/// or the clock goes backwards (files are ordered by time, so it cannot).
fn gap(frames: &[SetFrame], a: usize, b: usize) -> Option<f64> {
    let (ta, tb) = (frames[a].taken?, frames[b].taken?);
    let d = tb - ta;
    (d >= 0.0 && d.is_finite()).then_some(d)
}

fn close(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance * a.abs().max(b.abs())
}

/// Same focal length, within what a lens reports.
fn same_focal(a: &SetFrame, b: &SetFrame) -> bool {
    match (a.focal_length, b.focal_length) {
        (Some(x), Some(y)) => close(x, y, FOCAL_TOLERANCE),
        _ => true,
    }
}

/// Same opening, within a third of a stop.
fn same_aperture(a: &SetFrame, b: &SetFrame) -> bool {
    match (a.f_number, b.f_number) {
        (Some(x), Some(y)) if x > 0.0 && y > 0.0 => {
            (2.0 * (x / y).log2()).abs() <= APERTURE_TOLERANCE_EV
        }
        _ => true,
    }
}

/// Same shutter time, within a third of a stop, for the kinds that hold the
/// exposure still.
fn same_shutter(a: &SetFrame, b: &SetFrame) -> bool {
    match (a.exposure_time, b.exposure_time) {
        (Some(x), Some(y)) if x > 0.0 && y > 0.0 => (x / y).log2().abs() <= SHUTTER_TOLERANCE_EV,
        _ => true,
    }
}

/// How bright the frame's exposure is, in EV, on an arbitrary but consistent
/// scale: bias plus the stop count of the shutter time. Both move in a real
/// auto-exposure bracket (Canon keeps bias at 0 and shifts the shutter), so
/// reading them together is what sees the steps.
fn brightness(frame: &SetFrame) -> Option<f64> {
    let shutter = frame
        .exposure_time
        .filter(|t| *t > 0.0 && t.is_finite())
        .map(f64::log2);
    match (frame.exposure_bias, shutter) {
        (Some(bias), Some(shutter)) => Some(bias + shutter),
        (Some(bias), None) => Some(bias),
        (None, Some(shutter)) => Some(shutter),
        (None, None) => None,
    }
}

/// Brackets: a run of frames on one camera, on one lens setting, whose
/// exposures step by at least [`BRACKET_STEP_EV`] each and span at least
/// [`BRACKET_SPAN_EV`]. A repeat exposure starts a new round, so an HDR
/// panorama's rounds are separate sets rather than one long one.
fn brackets(frames: &[SetFrame], free: &[usize]) -> Vec<(usize, Vec<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < free.len() {
        let head = &frames[free[i]];
        let Some(mut ev) = brightness(head) else {
            i += 1;
            continue;
        };
        let mut members = vec![free[i]];
        let mut evs = vec![ev];
        let mut j = i + 1;
        while j < free.len() && members.len() < BRACKET_MAX_FRAMES {
            let next = &frames[free[j]];
            let Some(next_ev) = brightness(next) else {
                break;
            };
            let Some(step) = gap(frames, free[j - 1], free[j]) else {
                break;
            };
            if step > BRACKET_GAP_SECONDS
                || !same_focal(head, next)
                || !same_aperture(head, next)
                || (next_ev - ev).abs() < BRACKET_STEP_EV
                || evs
                    .iter()
                    .any(|&seen| (seen - next_ev).abs() < BRACKET_REPEAT_EV)
            {
                break;
            }
            members.push(free[j]);
            evs.push(next_ev);
            ev = next_ev;
            j += 1;
        }
        let span = evs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - evs.iter().copied().fold(f64::INFINITY, f64::min);
        if members.len() >= BRACKET_MIN_FRAMES && span >= BRACKET_SPAN_EV {
            out.push((members[0], members));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Focus stacks: a run of frames on one camera, same lens and exposure,
/// within [`STACK_GAP_SECONDS`] of each other, whose focus distance moves the
/// same way every frame. A stack shot by hand takes longer and is missed --
/// deliberately, because two stills of a room half a minute apart are not
/// evidence of a stack.
fn stacks(frames: &[SetFrame], free: &[usize]) -> Vec<(usize, Vec<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < free.len() {
        let mut members = vec![free[i]];
        let mut direction = 0.0f64;
        let mut j = i + 1;
        while j < free.len() {
            let (prev, next) = (&frames[free[j - 1]], &frames[free[j]]);
            let Some(step) = gap(frames, free[j - 1], free[j]) else {
                break;
            };
            let (Some(a), Some(b)) = (prev.focus_distance, next.focus_distance) else {
                break;
            };
            if step > STACK_GAP_SECONDS
                || !same_focal(&frames[free[i]], next)
                || !same_aperture(&frames[free[i]], next)
                || !same_shutter(&frames[free[i]], next)
            {
                break;
            }
            // The focus plane has to move, and keep moving the same way: a
            // reading that repeats is the rail settling, not another slice.
            let delta = b - a;
            if delta.abs() <= STACK_STEP_RATIO * a.abs().max(b.abs()) {
                break;
            }
            let sign = delta.signum();
            if direction != 0.0 && sign != direction {
                break;
            }
            direction = sign;
            members.push(free[j]);
            j += 1;
        }
        if members.len() >= STACK_MIN_FRAMES {
            out.push((members[0], members));
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

/// Intervals: [`INTERVAL_MIN_FRAMES`] or more frames on one camera and lens,
/// whose gaps have a mean between [`INTERVAL_MIN_SECONDS`] and
/// [`INTERVAL_MAX_SECONDS`] and a standard deviation under
/// [`INTERVAL_SPREAD`] of that mean. Exposure may drift across a timelapse
/// (the light does), which is why only the lens setting is held fixed here.
fn intervals(frames: &[SetFrame], free: &[usize]) -> Vec<(usize, Vec<usize>)> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < free.len() {
        // The longest stretch from `start` that could be one sequence: same
        // lens, and every gap inside the interval range at all.
        let mut end = start + 1;
        while end < free.len() {
            let Some(step) = gap(frames, free[end - 1], free[end]) else {
                break;
            };
            if !(INTERVAL_MIN_SECONDS..=INTERVAL_MAX_SECONDS).contains(&step)
                || !same_focal(&frames[free[start]], &frames[free[end]])
                || !same_aperture(&frames[free[start]], &frames[free[end]])
            {
                break;
            }
            end += 1;
        }
        if end - start >= INTERVAL_MIN_FRAMES && steady(frames, free, start, end) {
            out.push((free[start], free[start..end].to_vec()));
            start = end;
        } else {
            // A single off-beat gap does not spoil the frames after it.
            start += 1;
        }
    }
    out
}

/// Whether the gaps between `free[start..end]` are as regular as a timer.
fn steady(frames: &[SetFrame], free: &[usize], start: usize, end: usize) -> bool {
    let gaps: Vec<f64> = (start + 1..end)
        .filter_map(|k| gap(frames, free[k - 1], free[k]))
        .collect();
    if gaps.len() != end - start - 1 || gaps.is_empty() {
        return false;
    }
    let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
    if !(INTERVAL_MIN_SECONDS..=INTERVAL_MAX_SECONDS).contains(&mean) {
        return false;
    }
    let variance = gaps.iter().map(|g| (g - mean) * (g - mean)).sum::<f64>() / gaps.len() as f64;
    variance.sqrt() < INTERVAL_SPREAD * mean
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame at `t` seconds with ordinary daylight settings.
    fn frame(t: f64) -> SetFrame {
        SetFrame {
            taken: Some(t),
            camera: "Canon EOS R7".into(),
            exposure_bias: Some(0.0),
            exposure_time: Some(1.0 / 250.0),
            f_number: Some(8.0),
            focal_length: Some(35.0),
            focus_distance: None,
            width: 6960,
            height: 4640,
        }
    }

    fn at(t: f64, bias: f64) -> SetFrame {
        SetFrame {
            exposure_bias: Some(bias),
            ..frame(t)
        }
    }

    fn shutter(t: f64, seconds: f64) -> SetFrame {
        SetFrame {
            exposure_time: Some(seconds),
            ..frame(t)
        }
    }

    fn kinds(sets: &[Option<(i64, SetKind)>]) -> Vec<Option<SetKind>> {
        sets.iter().map(|s| s.map(|(_, k)| k)).collect()
    }

    #[test]
    fn a_three_stop_bracket_on_the_bias_is_one_set() {
        let frames: Vec<SetFrame> = [-2.0, 0.0, 2.0]
            .iter()
            .enumerate()
            .map(|(i, &bias)| at(1_000.0 + i as f64 * 0.4, bias))
            .collect();
        let got = find_sets(&frames);
        assert_eq!(got, vec![Some((0, SetKind::Bracket)); 3]);
    }

    #[test]
    fn a_bracket_that_moves_only_the_shutter_is_one_set() {
        // Canon's AEB leaves exposure compensation at 0 and shifts the shutter.
        let frames = [
            shutter(1_000.0, 1.0 / 1000.0),
            shutter(1_000.3, 1.0 / 250.0),
            shutter(1_000.6, 1.0 / 60.0),
        ];
        assert_eq!(kinds(&find_sets(&frames)), vec![Some(SetKind::Bracket); 3]);
    }

    #[test]
    fn five_rounds_of_a_bracket_stay_five_sets() {
        // An HDR panorama: -2, 0, +2 repeated. The repeat exposure is what
        // stops a round, not the frame count.
        let mut frames = Vec::new();
        for round in 0..5 {
            for (i, bias) in [-2.0, 0.0, 2.0].iter().enumerate() {
                frames.push(at(round as f64 * 10.0 + i as f64 * 0.3, *bias));
            }
        }
        let got = find_sets(&frames);
        assert_eq!(kinds(&got), vec![Some(SetKind::Bracket); 15]);
        let keys: Vec<i64> = got.iter().filter_map(|s| s.map(|(k, _)| k)).collect();
        assert_eq!(keys, vec![0, 0, 0, 3, 3, 3, 6, 6, 6, 9, 9, 9, 12, 12, 12]);
    }

    #[test]
    fn a_motorsport_burst_at_constant_exposure_is_no_set() {
        // 12 frames of a car, 4 a second: exposure never moves and no gap
        // reaches the interval floor, so nothing here is kept together.
        let frames: Vec<SetFrame> = (0..12)
            .map(|i| SetFrame {
                exposure_time: Some(1.0 / 2000.0),
                focal_length: Some(400.0),
                f_number: Some(2.8),
                ..frame(2_000.0 + f64::from(i) * 0.25)
            })
            .collect();
        assert_eq!(find_sets(&frames), vec![None; 12]);
    }

    #[test]
    fn auto_exposure_drift_is_not_a_bracket() {
        // A third of a stop a frame is the metering settling, which is the
        // step size BRACKET_STEP_EV exists to reject.
        let frames: Vec<SetFrame> = (0..7)
            .map(|i| at(3_000.0 + f64::from(i), f64::from(i) * 0.3))
            .collect();
        assert_eq!(find_sets(&frames), vec![None; 7]);
    }

    #[test]
    fn a_bracket_that_changes_aperture_is_not_one() {
        let mut frames: Vec<SetFrame> = [-2.0, 0.0, 2.0]
            .iter()
            .enumerate()
            .map(|(i, &bias)| at(4_000.0 + i as f64 * 0.4, bias))
            .collect();
        frames[1].f_number = Some(2.8);
        assert_eq!(find_sets(&frames), vec![None; 3]);
    }

    #[test]
    fn a_focus_stack_is_one_set() {
        let frames: Vec<SetFrame> = (0..5)
            .map(|i| SetFrame {
                focus_distance: Some(0.8 + f64::from(i) * 0.1),
                ..frame(5_000.0 + f64::from(i) * 0.4)
            })
            .collect();
        assert_eq!(
            kinds(&find_sets(&frames)),
            vec![Some(SetKind::FocusStack); 5]
        );
    }

    #[test]
    fn a_focus_stack_that_turns_back_is_not_one() {
        let distances = [0.8, 0.9, 0.85, 0.95];
        let frames: Vec<SetFrame> = distances
            .iter()
            .enumerate()
            .map(|(i, &d)| SetFrame {
                focus_distance: Some(d),
                ..frame(6_000.0 + i as f64 * 0.4)
            })
            .collect();
        assert_eq!(find_sets(&frames), vec![None; 4]);
    }

    #[test]
    fn a_regular_timelapse_is_one_set() {
        let frames: Vec<SetFrame> = (0..20)
            .map(|i| frame(7_000.0 + f64::from(i) * 30.0))
            .collect();
        assert_eq!(
            kinds(&find_sets(&frames)),
            vec![Some(SetKind::Interval); 20]
        );
    }

    #[test]
    fn a_timer_that_jitters_a_little_is_still_one_set() {
        let jitter = [0.0, 0.2, -0.1, 0.1, -0.2, 0.05, -0.05];
        let frames: Vec<SetFrame> = (0..14)
            .map(|i| frame(8_000.0 + f64::from(i) * 10.0 + jitter[i as usize % 7]))
            .collect();
        assert_eq!(
            kinds(&find_sets(&frames)),
            vec![Some(SetKind::Interval); 14]
        );
    }

    #[test]
    fn a_ragged_rhythm_is_not_a_timelapse() {
        // Ten frames, but the gaps wander by half: stills of one room, not a
        // sequence.
        let gaps = [2.0, 6.0, 3.0, 9.0, 1.5, 12.0, 4.0, 7.0, 2.5, 5.0];
        let mut t = 9_000.0;
        let mut frames = vec![frame(t)];
        for g in gaps {
            t += g;
            frames.push(frame(t));
        }
        assert_eq!(find_sets(&frames), vec![None; 11]);
    }

    #[test]
    fn a_timelapse_of_short_gaps_is_a_burst_not_an_interval() {
        // Two a second for a minute: a fast drive, not a sequence anyone is
        // going to keep as a unit.
        let frames: Vec<SetFrame> = (0..30)
            .map(|i| frame(10_000.0 + f64::from(i) * 0.5))
            .collect();
        assert_eq!(find_sets(&frames), vec![None; 30]);
    }

    #[test]
    fn a_bracketed_timelapse_gives_each_round_to_the_bracket() {
        // 12 frames two seconds apart, exposures cycling -2, 0, +2. Every
        // round is a three-frame bracket, so the interval never gets a frame.
        let mut frames = Vec::new();
        for round in 0..4 {
            for (i, bias) in [-2.0, 0.0, 2.0].iter().enumerate() {
                frames.push(at(11_000.0 + round as f64 * 6.0 + i as f64 * 2.0, *bias));
            }
        }
        let got = find_sets(&frames);
        assert_eq!(kinds(&got), vec![Some(SetKind::Bracket); 12]);
        assert_eq!(got[0], Some((0, SetKind::Bracket)));
        assert_eq!(got[11], Some((9, SetKind::Bracket)));
    }

    #[test]
    fn two_cameras_do_not_join_each_others_sets() {
        let mut frames: Vec<SetFrame> = Vec::new();
        for i in 0..10 {
            frames.push(frame(12_000.0 + f64::from(i) * 30.0));
        }
        for i in 0..10 {
            frames.push(SetFrame {
                camera: "Nikon Z8".into(),
                ..frame(12_015.0 + f64::from(i) * 30.0)
            });
        }
        let got = find_sets(&frames);
        assert_eq!(kinds(&got), vec![Some(SetKind::Interval); 20]);
        assert_ne!(got[0], got[10], "one key per camera's sequence");
    }

    #[test]
    fn an_untimed_frame_stands_alone() {
        let frames = [
            at(13_000.0, -2.0),
            at(13_000.4, 0.0),
            at(13_000.8, 2.0),
            // No clock: no evidence it belongs to anything.
            SetFrame {
                taken: None,
                ..frame(0.0)
            },
        ];
        let got = find_sets(&frames);
        assert_eq!(
            got,
            vec![
                Some((0, SetKind::Bracket)),
                Some((0, SetKind::Bracket)),
                Some((0, SetKind::Bracket)),
                None
            ]
        );
    }
}
