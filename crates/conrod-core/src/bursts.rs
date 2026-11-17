//! Which camera took it, and which burst it belongs to.
//!
//! Two shooters at one event interleave into one folder, so a burst is
//! consecutive frames *from one body* with no gap longer than the album's
//! `burst_gap` setting (see [`DEFAULT_BURST_GAP_SECONDS`]).

use crate::text::value_to_string;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap};

/// The gap the engine groups with for an album that carries no setting: tight
/// enough that a lull at the corner does not split a burst, loose enough to
/// keep the next car out of it. This is the one default; an album can override
/// it in its settings (`burst_gap`).
pub const DEFAULT_BURST_GAP_SECONDS: f64 = 1.5;

/// The tags that identify a body and a moment.
pub const TAGS: [&str; 9] = [
    "SerialNumber",
    "InternalSerialNumber",
    "Model",
    "Make",
    "LensModel",
    "LensID",
    "DateTimeOriginal",
    "SubSecTimeOriginal",
    "SubSecDateTimeOriginal",
];

/// One file's tags, as exiftool's JSON gives them.
pub type Tags = Map<String, Value>;

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub path: String,
    pub camera: String,
    /// Seconds since the epoch; `None` when the file carries no time.
    pub taken: Option<f64>,
    pub burst: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Burst {
    pub key: u32,
    pub camera: String,
    pub frames: Vec<String>,
    pub started: Option<f64>,
    pub ended: Option<f64>,
}

/// The first of `names` that holds something other than nothing, "" or "-".
fn first<'a>(tags: &'a Tags, names: &[&str]) -> Option<&'a Value> {
    names.iter().filter_map(|n| tags.get(*n)).find(|v| match v {
        Value::Null => false,
        Value::String(s) => !s.is_empty() && s != "-",
        _ => true,
    })
}

/// A stable name for the body that took this frame.
pub fn camera_of(tags: &Tags, fallback: &str) -> String {
    let model = first(tags, &["Model"]).map(value_to_string);
    if let Some(serial) = first(tags, &["SerialNumber", "InternalSerialNumber"]) {
        let serial = value_to_string(serial);
        return match model {
            Some(model) => format!("{model} {serial}").trim().to_string(),
            None => serial,
        };
    }
    if let Some(model) = model {
        // No serial: two identical bodies collapse here, but the lens often
        // differs between shooters and costs nothing to include.
        return match first(tags, &["LensModel", "LensID"]) {
            Some(lens) => format!("{model} + {}", value_to_string(lens))
                .trim()
                .to_string(),
            None => model,
        };
    }
    fallback.to_string()
}

/// When the shutter fired, in seconds, with sub-seconds where recorded.
pub fn taken_at(tags: &Tags) -> Option<f64> {
    let stamp = first(
        tags,
        &[
            "SubSecDateTimeOriginal",
            "DateTimeOriginal",
            "DateTimeDigitized",
            "DateTime",
        ],
    )?;
    let mut seconds = parse_stamp(&value_to_string(stamp))?;
    // Presence of the key, not its value: an empty combined tag still means
    // the camera chose not to split the sub-second out.
    if !tags.contains_key("SubSecDateTimeOriginal") {
        if let Some(sub) = first(
            tags,
            &["SubSecTimeOriginal", "SubSecTime", "SubSecTimeDigitized"],
        ) {
            if let Ok(fraction) = format!("0.{}", value_to_string(sub).trim()).parse::<f64>() {
                seconds += fraction;
            }
        }
    }
    Some(seconds)
}

/// "YYYY:MM:DD HH:MM:SS", optionally .sss, optionally a zone -- parsed by hand
/// because a flat clock battery writes "0000:00:00 00:00:00", which means only
/// "unknown".
pub fn parse_stamp(stamp: &str) -> Option<f64> {
    let mut text = stamp.trim().replace('T', " ");
    for cut in ['+', '-'] {
        let head = text.split(' ').next_back().unwrap_or("");
        if head.contains(cut) {
            let at = text.rfind(cut)?;
            text.truncate(at);
        }
    }
    let text = text.replace(['/', '-'], ":");
    let parts: Vec<&str> = text.split(' ').collect();
    if parts.len() < 2 {
        return None;
    }
    let date: Vec<&str> = parts[0].split(':').collect();
    let time: Vec<&str> = parts[1].split(':').collect();
    if date.len() != 3 || time.len() < 3 {
        return None;
    }
    let parse_int = |s: &str| s.trim().parse::<i64>().ok();
    let (year, month, day) = (
        parse_int(date[0])?,
        parse_int(date[1])?,
        parse_int(date[2])?,
    );
    let (hour, minute) = (parse_int(time[0])?, parse_int(time[1])?);
    let second: f64 = time[2].trim().parse().ok()?;
    if year < 1970 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if year > 9999 {
        return None; // out of range for any calendar worth trusting
    }
    // The first of the month, plus the day, unvalidated: a nonsense day like
    // "31 February" rolls forward into March rather than being rejected.
    let days = i128::from(days_from_civil(year, month, 1) + day - 1);
    let base = ((days * 24 + i128::from(hour)) * 60 + i128::from(minute)) * 60;
    Some(base as f64 + second)
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Turn exiftool rows (each with a "SourceFile") into frames with bursts.
pub fn describe(rows: &[Tags], fallback: &str, gap: f64) -> Vec<Frame> {
    let mut frames: Vec<Frame> = rows
        .iter()
        .map(|row| Frame {
            path: row
                .get("SourceFile")
                .filter(|v| !v.is_null())
                .map(value_to_string)
                .unwrap_or_default(),
            camera: camera_of(row, fallback),
            taken: taken_at(row),
            burst: 0,
        })
        .collect();
    assign_bursts(&mut frames, gap);
    frames
}

/// Number the bursts, per camera in name order, each in time order. A frame
/// with no time gets a burst of its own: no clock is not evidence of anything.
pub fn assign_bursts(frames: &mut [Frame], gap: f64) {
    let mut by_camera: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, frame) in frames.iter().enumerate() {
        by_camera.entry(frame.camera.clone()).or_default().push(i);
    }
    let mut key = 0;
    for indices in by_camera.values() {
        let mut timed: Vec<(usize, f64)> = indices
            .iter()
            .filter_map(|&i| frames[i].taken.map(|taken| (i, taken)))
            .collect();
        timed.sort_by(|&(a, ta), &(b, tb)| {
            ta.total_cmp(&tb)
                .then_with(|| frames[a].path.cmp(&frames[b].path))
        });
        let mut previous: Option<f64> = None;
        for (i, taken) in timed {
            if previous.is_none_or(|p| taken - p > gap) {
                key += 1;
            }
            frames[i].burst = key;
            previous = Some(taken);
        }
        let untimed: Vec<usize> = indices
            .iter()
            .copied()
            .filter(|&i| frames[i].taken.is_none())
            .collect();
        for i in untimed {
            key += 1;
            frames[i].burst = key;
        }
    }
}

/// The bursts themselves, in the order they were shot.
pub fn collect(frames: &[Frame]) -> Vec<Burst> {
    let mut bursts: BTreeMap<u32, Burst> = BTreeMap::new();
    for frame in frames {
        let burst = bursts.entry(frame.burst).or_insert_with(|| Burst {
            key: frame.burst,
            camera: frame.camera.clone(),
            frames: Vec::new(),
            started: None,
            ended: None,
        });
        burst.frames.push(frame.path.clone());
        if let Some(t) = frame.taken {
            burst.started = Some(burst.started.map_or(t, |s| s.min(t)));
            burst.ended = Some(burst.ended.map_or(t, |e| e.max(t)));
        }
    }
    let mut out: Vec<Burst> = bursts.into_values().collect();
    out.sort_by(|a, b| {
        a.started
            .unwrap_or(0.0)
            .total_cmp(&b.started.unwrap_or(0.0))
            .then(a.key.cmp(&b.key))
    });
    out
}

/// A moment's main subject may grow or shrink by at most this area factor, either way, from the moment's first frame.
pub const MOMENT_SCALE_RATIO: f64 = 1.6;
/// The subject's centre may move at most this fraction of the frame diagonal from the moment's first frame.
pub const MOMENT_SHIFT: f64 = 0.25;

/// One frame as the moment splitter sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MomentFrame {
    pub burst: i64,
    /// Seconds since the epoch; `None` when the file carries no time.
    pub taken: Option<f64>,
    /// The main subject's box as fractions of the frame: (x1, y1, x2, y2).
    pub subject: Option<[f64; 4]>,
    /// The frame was taken as a pan. A pan is one continuous action: the car is
    /// *meant* to cross the frame, so its box travelling does not start a new
    /// moment the way a cut does.
    pub panning: bool,
}

fn same_moment(anchor: Option<[f64; 4]>, subject: Option<[f64; 4]>) -> bool {
    match (anchor, subject) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            let area = |[x1, y1, x2, y2]: [f64; 4]| (x2 - x1).abs() * (y2 - y1).abs();
            let centre = |[x1, y1, x2, y2]: [f64; 4]| ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
            let (fa, fb) = (area(a), area(b));
            let scale_ok = fa.max(fb) <= MOMENT_SCALE_RATIO * fa.min(fb);
            let ((ax, ay), (bx, by)) = (centre(a), centre(b));
            let shift = (ax - bx).hypot(ay - by) / std::f64::consts::SQRT_2;
            scale_ok && shift <= MOMENT_SHIFT
        }
        _ => false,
    }
}

/// A new group key per frame, splitting each burst into moments where the
/// main subject's framing changes. Input is in time order. Keys are dense from
/// 1 and never shared across bursts.
pub fn split_moments(frames: &[MomentFrame]) -> Vec<i64> {
    /// The moment currently open in one burst.
    struct Open {
        /// The first frame's subject box: what a static frame is compared to.
        anchor: Option<[f64; 4]>,
        key: i64,
        /// A pan has been seen in this moment. Pans are one continuous action:
        /// the car crossing the frame, and the frames the detector missed in the
        /// middle of it, do not start a new moment.
        pan: bool,
    }
    let mut open: HashMap<i64, Open> = HashMap::new();
    let mut moments: i64 = 0;
    frames
        .iter()
        .map(|frame| match open.get_mut(&frame.burst) {
            // A pan frame always continues the moment it is in, however far the
            // car has travelled.
            Some(o) if frame.panning => {
                o.pan = true;
                o.key
            }
            // Inside a pan, a frame with no box is a detector miss, not a cut:
            // the car was still crossing. A frame *with* a box is still compared,
            // so a genuinely different scene can start a new moment.
            Some(o) if o.pan && frame.subject.is_none() => o.key,
            Some(o) if same_moment(o.anchor, frame.subject) => o.key,
            _ => {
                moments += 1;
                open.insert(
                    frame.burst,
                    Open {
                        anchor: frame.subject,
                        key: moments,
                        pan: frame.panning,
                    },
                );
                moments
            }
        })
        .collect()
}

/// pHash bits two frames may differ by and still be candidate duplicates.
pub const DUPE_BITS: u32 = 10;
/// Tighter bound for a pair with no embeddings to confirm it (scenery, crowds).
pub const DUPE_BITS_UNCONFIRMED: u32 = 4;

/// Re-shoots of one scene happen minutes apart, not hours: two timed frames
/// further apart than this are never compared, which also keeps look-alikes
/// from different sessions out of one stack.
pub const DUPE_WINDOW_SECONDS: f64 = 600.0;

/// One frame as the duplicate finder sees it.
pub struct DupeFrame {
    pub id: i64,
    pub phash: u64,
    pub burst: Option<i64>,
    /// Seconds since the epoch; `None` when the file carries no time.
    pub taken: Option<f64>,
    /// Unit-vector embeddings of the frame's subjects.
    pub looks: Vec<Vec<f32>>,
}

/// Re-shoots of the same picture across the whole shoot: pHash within
/// [`DUPE_BITS`] as candidates, confirmed by any subject pair's cosine at or
/// above `same`, joined transitively. Only pairs shot within
/// [`DUPE_WINDOW_SECONDS`] of each other are compared; a frame with no time is
/// offered every frame. Pairs within one burst are skipped since the burst
/// already stacks them. Returns frame id -> group key (the group's smallest
/// id), only for frames in a group of two or more.
pub fn near_dupes(frames: &[DupeFrame], same: f32) -> BTreeMap<i64, i64> {
    let mut parent: Vec<usize> = (0..frames.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    fn consider(frames: &[DupeFrame], parent: &mut [usize], same: f32, i: usize, j: usize) {
        let (a, b) = (&frames[i], &frames[j]);
        if a.burst.is_some() && a.burst == b.burst {
            return;
        }
        let bits = (a.phash ^ b.phash).count_ones();
        if bits > DUPE_BITS {
            return;
        }
        let confirmed = if a.looks.is_empty() || b.looks.is_empty() {
            bits <= DUPE_BITS_UNCONFIRMED
        } else {
            a.looks.iter().any(|x| {
                b.looks
                    .iter()
                    .any(|y| x.iter().zip(y).map(|(p, q)| p * q).sum::<f32>() >= same)
            })
        };
        if confirmed {
            let (ra, rb) = (root(parent, i), root(parent, j));
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut timed: Vec<(usize, f64)> = frames
        .iter()
        .enumerate()
        .filter_map(|(i, frame)| Some((i, frame.taken?)))
        .collect();
    timed.sort_by(|&(a, ta), &(b, tb)| {
        ta.total_cmp(&tb)
            .then_with(|| frames[a].id.cmp(&frames[b].id))
    });
    // A sliding window over the time order: each frame only meets the next few
    // minutes of frames, not the whole shoot.
    for p in 0..timed.len() {
        let (i, start) = timed[p];
        for &(j, when) in &timed[p + 1..] {
            if when - start > DUPE_WINDOW_SECONDS {
                break;
            }
            consider(frames, &mut parent, same, i, j);
        }
    }
    // A frame with no time is rare, so it still meets every other frame.
    for i in (0..frames.len()).filter(|&i| frames[i].taken.is_none()) {
        for j in (0..frames.len()).filter(|&j| j != i && (frames[j].taken.is_some() || j > i)) {
            consider(frames, &mut parent, same, i, j);
        }
    }
    let mut members: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
    for (i, frame) in frames.iter().enumerate() {
        let r = root(&mut parent, i);
        members.entry(r).or_default().push(frame.id);
    }
    let mut out = BTreeMap::new();
    for ids in members.values().filter(|ids| ids.len() > 1) {
        let key = *ids.iter().min().unwrap();
        out.extend(ids.iter().map(|&id| (id, key)));
    }
    out
}

#[cfg(test)]
mod dupe_tests {
    use super::*;

    fn f(id: i64, phash: u64, burst: Option<i64>, look: &[f32]) -> DupeFrame {
        DupeFrame {
            id,
            phash,
            burst,
            taken: None,
            looks: if look.is_empty() {
                vec![]
            } else {
                vec![look.to_vec()]
            },
        }
    }

    fn at(taken: f64, id: i64, phash: u64, burst: Option<i64>, look: &[f32]) -> DupeFrame {
        DupeFrame {
            taken: Some(taken),
            ..f(id, phash, burst, look)
        }
    }

    #[test]
    fn reshoots_group_across_bursts_but_not_within_or_when_the_look_differs() {
        let car = [1.0, 0.0];
        let other = [0.0, 1.0];
        let h = 0xFFFF_0000;
        let frames = [
            f(1, 0, Some(1), &car),
            f(2, 0b111, Some(2), &car), // 3 bits, same car, other burst: joins 1
            f(3, 0b1111, Some(3), &car), // joins 2, so 1 transitively
            f(5, 0b1, Some(5), &[]),    // no looks, 1 bit: joins by hash alone
            f(4, h, Some(4), &other),
            f(10, h, Some(10), &car), // same hash as 4, different car: apart
            f(6, u64::MAX, Some(6), &car), // far hash: alone
            f(7, h | 0b11111, Some(7), &[]), // no looks, 5 bits: too far unconfirmed
            f(8, !0xFF, Some(9), &other),
            f(9, !0xFF, Some(9), &other), // same burst as 8: not joined
        ];
        let got = near_dupes(&frames, 0.9);
        for id in [1, 2, 3, 5] {
            assert_eq!(got.get(&id), Some(&1), "frame {id}");
        }
        assert_eq!(got.len(), 4, "{got:?}");
    }

    #[test]
    fn a_reshoot_minutes_apart_joins_but_one_from_another_session_does_not() {
        let car = [1.0, 0.0];
        let close = [
            at(1_000.0, 1, 0, Some(1), &car),
            at(1_030.0, 2, 0b111, Some(2), &car),
        ];
        assert_eq!(
            near_dupes(&close, 0.9),
            BTreeMap::from([(1, 1), (2, 1)]),
            "30 s apart in different bursts is a re-shoot"
        );
        let later = [
            at(1_000.0, 1, 0, Some(1), &car),
            at(1_000.0 + 2.0 * 3600.0, 2, 0b111, Some(2), &car),
        ];
        assert!(
            near_dupes(&later, 0.9).is_empty(),
            "two hours apart is a look-alike, not a re-shoot"
        );
    }

    #[test]
    fn an_untimed_frame_still_pairs_with_a_timed_look_alike() {
        let car = [1.0, 0.0];
        let frames = [at(1_000.0, 1, 0, Some(1), &car), f(2, 0b111, Some(2), &car)];
        assert_eq!(
            near_dupes(&frames, 0.9),
            BTreeMap::from([(1, 1), (2, 1)]),
            "no clock is not a reason to stop comparing"
        );
    }

    #[test]
    fn five_thousand_unrelated_frames_finish_quickly() {
        // The id sits in four 16-bit lanes, so any two hashes differ in at
        // least four bits. The look vector is that hash as +/-1s, whose cosine
        // (1 - bits/32) never reaches 0.9, so nothing is confirmed.
        let frames: Vec<DupeFrame> = (0..5_000)
            .map(|i| {
                let phash = i as u64 * 0x0001_0001_0001_0001;
                let looks = (0..64)
                    .map(|b| if phash >> b & 1 == 1 { 0.125 } else { -0.125 })
                    .collect();
                DupeFrame {
                    looks: vec![looks],
                    ..at(1_000.0 + i as f64, i, phash, Some(i), &[])
                }
            })
            .collect();
        let started = std::time::Instant::now();
        let got = near_dupes(&frames, 0.9);
        let took = started.elapsed();
        assert!(got.is_empty(), "{got:?}");
        assert!(took < std::time::Duration::from_secs(2), "took {took:?}");
    }
}

#[cfg(test)]
mod moment_tests {
    use super::*;

    /// A square box centred at (cx, cy) covering `area` of the frame.
    fn boxed(cx: f64, cy: f64, area: f64) -> Option<[f64; 4]> {
        let half = area.sqrt() / 2.0;
        Some([cx - half, cy - half, cx + half, cy + half])
    }

    fn mf(burst: i64, subject: Option<[f64; 4]>) -> MomentFrame {
        MomentFrame {
            burst,
            taken: None,
            subject,
            panning: false,
        }
    }

    fn pan_mf(burst: i64, subject: Option<[f64; 4]>) -> MomentFrame {
        MomentFrame {
            panning: true,
            ..mf(burst, subject)
        }
    }

    #[test]
    fn a_steady_burst_stays_one_moment() {
        let frames: Vec<_> = (0..10).map(|_| mf(1, boxed(0.5, 0.5, 0.1))).collect();
        assert_eq!(split_moments(&frames), vec![1; 10]);
    }

    #[test]
    fn a_car_growing_from_two_to_thirty_percent_becomes_a_few_moments() {
        let frames: Vec<_> = (0..20)
            .map(|i| mf(1, boxed(0.5, 0.5, 0.02 + 0.28 * f64::from(i) / 19.0)))
            .collect();
        let keys = split_moments(&frames);
        let moments = *keys.iter().max().unwrap();
        assert!((5..=7).contains(&moments), "{keys:?}");
        assert!(keys.windows(2).all(|w| w[0] <= w[1]), "{keys:?}");
        assert_eq!(keys[0], 1);
    }

    #[test]
    fn a_pan_survives_the_frames_the_detector_missed() {
        // The real sequence: eight panning frames, two the detector missed, a
        // panning frame again, then three more misses. One action, one moment.
        let mut frames: Vec<_> = (0..8)
            .map(|i| pan_mf(1, boxed(0.25 + 0.05 * f64::from(i), 0.5, 0.12)))
            .collect();
        frames.push(mf(1, None));
        frames.push(mf(1, None));
        frames.push(pan_mf(1, boxed(0.6, 0.5, 0.2)));
        frames.extend((0..3).map(|_| mf(1, None)));
        let keys = split_moments(&frames);
        assert_eq!(keys, vec![1; frames.len()], "{keys:?}");
    }

    #[test]
    fn a_frame_with_no_box_outside_a_pan_still_splits() {
        let frames = vec![
            mf(1, boxed(0.3, 0.5, 0.1)),
            mf(1, None),
            mf(1, boxed(0.3, 0.5, 0.1)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 2, 3]);
    }

    #[test]
    fn a_pan_across_the_frame_stays_one_moment() {
        // The car travels from the left third to the right third: far past the
        // moment shift, but a pan is one action, not a cut.
        let frames: Vec<_> = (0..12)
            .map(|i| pan_mf(1, boxed(0.2 + 0.05 * f64::from(i), 0.5, 0.1)))
            .collect();
        assert_eq!(split_moments(&frames), vec![1; 12]);
        // A pan followed by a static frame starts a new moment: the action ended.
        let mut frames = frames;
        frames.push(mf(1, boxed(0.9, 0.5, 0.1)));
        let keys = split_moments(&frames);
        assert_eq!(keys[11], 1);
        assert_eq!(keys[12], 2);
    }

    #[test]
    fn a_centre_jump_splits_the_burst() {
        let mut frames: Vec<_> = (0..5).map(|_| mf(1, boxed(0.3, 0.5, 0.1))).collect();
        frames.extend((0..5).map(|_| mf(1, boxed(0.8, 0.5, 0.1))));
        assert_eq!(split_moments(&frames), vec![1, 1, 1, 1, 1, 2, 2, 2, 2, 2]);
    }

    #[test]
    fn different_bursts_never_merge() {
        let frames = [
            mf(1, boxed(0.5, 0.5, 0.1)),
            mf(2, boxed(0.5, 0.5, 0.1)),
            mf(1, boxed(0.5, 0.5, 0.1)),
            mf(2, boxed(0.5, 0.5, 0.1)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 2, 1, 2]);
    }

    #[test]
    fn a_trailing_frame_with_a_different_framing_stands_alone() {
        let mut frames: Vec<_> = (0..4).map(|_| mf(1, boxed(0.5, 0.5, 0.1))).collect();
        frames.push(mf(1, boxed(0.5, 0.5, 0.5)));
        assert_eq!(split_moments(&frames), vec![1, 1, 1, 1, 2]);
    }

    #[test]
    fn subjects_that_appear_or_vanish_split_but_none_throughout_stays_one() {
        let frames = [
            mf(1, None),
            mf(1, None),
            mf(1, None),
            mf(1, boxed(0.5, 0.5, 0.1)),
            mf(1, boxed(0.5, 0.5, 0.1)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 1, 1, 2, 2]);
        let none: Vec<_> = (0..6).map(|_| mf(1, None)).collect();
        assert_eq!(split_moments(&none), vec![1; 6]);
    }
}
