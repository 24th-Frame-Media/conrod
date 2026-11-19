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
    /// The camera's wall clock as seconds since the Unix epoch, or `None` when the file carries no
    /// time. EXIF carries no zone, so this is never shifted into the viewer's zone; it is
    /// formatted back as UTC.
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

/// When the shutter fired, as the camera's wall clock in seconds since the Unix epoch, with
/// sub-seconds where recorded (an EXIF stamp has no zone; a zone suffix is discarded rather than
/// converted).
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

/// Between two neighbouring boxed frames of a burst, the main subject's box may
/// grow or shrink by at most this area factor, either way, and stay one moment.
/// A car approaching or receding changes by ~1.2x a frame and never gets near
/// it; a different car or a turn to the crowd jumps by more.
pub const MOMENT_SCALE_RATIO: f64 = 3.0;
/// ...and its centre may move at most this fraction of the frame diagonal.
pub const MOMENT_SHIFT: f64 = 0.30;
/// Both limits are this much looser between two panning frames: the car is
/// *meant* to cross the frame.
pub const MOMENT_PAN_LEEWAY: f64 = 1.5;
/// A box smaller than this share of the frame is a speck (a marshal, a far car
/// the detector flickers on), not a subject: it counts as a missed frame.
pub const MOMENT_MIN_AREA: f64 = 0.005;
/// A pause this long (seconds) between two frames of a burst is a gap in the
/// shooting: the next frame starts a new moment if the picture also changed.
pub const MOMENT_PAUSE: f64 = 0.8;
/// After a pause, the whole frame's colour layout (see [`Look`]) is "not
/// visually similar" past this distance (0 identical .. 1 disjoint). It is the
/// 99.5th percentile of the distance between frames under 0.2 s apart, so a
/// scene that is still the same almost never reads as changed.
pub const MOMENT_SCENE_DISTANCE: f64 = 0.26;
/// After a pause, two vehicles are different cars past this subject-livery
/// distance: the 99th percentile for one car seen a second apart (blur, angle).
pub const MOMENT_PAUSE_SUBJECT_DISTANCE: f64 = 0.45;
/// Without a pause, neighbouring vehicles are a *clearly* different car only
/// past this livery distance. Measured on a 6,912-frame track album: one car
/// frame to frame reaches 0.42 at the 99.5th percentile, because a second car
/// entering the box and a pan both move the histogram, and a split costs the
/// photographer a stack. Two different liveries measure 0.9 or more.
pub const MOMENT_SUBJECT_DISTANCE: f64 = 0.50;

/// The sensitivity ladder's words, least to most splitting. [`sensitivity_word`]
/// is the one place they are recognised, so the settings screen's choice and a
/// scan sheet's choice take the same path.
pub const SENSITIVITY_WORDS: [&str; 4] = ["off", "gentle", "normal", "eager"];

/// The stored form of a sensitivity word: trimmed, lower-cased, and `normal`
/// for anything the ladder does not know.
pub fn sensitivity_word(word: &str) -> String {
    let word = word.trim().to_ascii_lowercase();
    if SENSITIVITY_WORDS.contains(&word.as_str()) {
        word
    } else {
        "normal".to_string()
    }
}

/// The pause threshold a cut honours: clamped to 0..=10 seconds, where 0
/// disables the pause cut. What a scan stores for `grouping_pause`, too.
pub fn clamped_pause(pause: f64) -> f64 {
    pause.clamp(0.0, 10.0)
}

/// The thresholds a time burst is split by, as the album chose them.
///
/// The default is today's behaviour: the constants above. An album moves along
/// one ladder rather than holding eight separate numbers:
///
/// * `off` -- a time burst is one stack, whatever happens inside it;
/// * `gentle` -- every cut threshold 1.5x looser, so a scale change or a box
///   jump that `normal` splits stays one moment;
/// * `normal` -- the constants above, the behaviour every older album had;
/// * `eager` -- every cut threshold 0.7x tighter (about 1/1.4), so a burst of
///   look-alikes stacks less;
///
/// `pause` and `pan_leeway` are not on the ladder: they are their own album
/// settings and come from the arguments, not the word.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MomentRules {
    /// Whether a time burst is split at all. `false` is the `off` rung: one
    /// stack per burst.
    pub split: bool,
    pub scale_ratio: f64,
    pub shift: f64,
    pub pan_leeway: f64,
    pub pause: f64,
    pub scene_distance: f64,
    pub pause_subject_distance: f64,
    pub subject_distance: f64,
}

impl Default for MomentRules {
    fn default() -> Self {
        MomentRules {
            split: true,
            scale_ratio: MOMENT_SCALE_RATIO,
            shift: MOMENT_SHIFT,
            pan_leeway: MOMENT_PAN_LEEWAY,
            pause: MOMENT_PAUSE,
            scene_distance: MOMENT_SCENE_DISTANCE,
            pause_subject_distance: MOMENT_PAUSE_SUBJECT_DISTANCE,
            subject_distance: MOMENT_SUBJECT_DISTANCE,
        }
    }
}

impl MomentRules {
    /// The rules an album's sensitivity word asks for. `pause` is clamped to
    /// 0..=10 seconds (0 disables the pause cut); `keep_pans` off makes a pan
    /// box compare like any other. An unknown word is `normal`.
    pub fn from_sensitivity(word: &str, pause: f64, keep_pans: bool) -> MomentRules {
        let default = MomentRules::default();
        // The ladder is one multiplier on every cut threshold; `off` has none.
        let loosen = match sensitivity_word(word).as_str() {
            "off" => None,
            "gentle" => Some(1.5),
            "eager" => Some(0.7),
            _ => Some(1.0),
        };
        let pause = clamped_pause(pause);
        let pan_leeway = if keep_pans { MOMENT_PAN_LEEWAY } else { 1.0 };
        let Some(loosen) = loosen else {
            return MomentRules {
                split: false,
                pause,
                pan_leeway,
                ..default
            };
        };
        MomentRules {
            split: true,
            scale_ratio: default.scale_ratio * loosen,
            shift: default.shift * loosen,
            pan_leeway,
            pause,
            scene_distance: default.scene_distance * loosen,
            pause_subject_distance: default.pause_subject_distance * loosen,
            subject_distance: default.subject_distance * loosen,
        }
    }
}

/// Bins of [`Look::scene`].
pub const LOOK_SCENE_BINS: usize = 28;
/// Bins of [`Look::subject`].
pub const LOOK_SUBJECT_BINS: usize = 15;

/// What a frame looks like: two small colour histograms, each quantised to
/// 0..=255 and summing to about 255 (`conrod_vision::look` measures them from
/// the cached thumbnail).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    /// The whole frame's colour layout.
    pub scene: [u8; LOOK_SCENE_BINS],
    /// The main subject's livery; `None` when the frame has no subject box.
    pub subject: Option<[u8; LOOK_SUBJECT_BINS]>,
    /// The main subject is a vehicle: liveries only tell cars apart.
    pub vehicle: bool,
}

/// How different two histograms are: 0 identical, 1 disjoint (one minus their
/// intersection).
pub fn look_distance(a: &[u8], b: &[u8]) -> f64 {
    let shared: u32 = a.iter().zip(b).map(|(&x, &y)| u32::from(x.min(y))).sum();
    (1.0 - f64::from(shared) / 255.0).clamp(0.0, 1.0)
}

/// One frame as the moment splitter sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MomentFrame {
    pub burst: i64,
    /// An album-local id for the camera, so two bodies' bursts are never joined
    /// ([`merge_bursts`]) and never share a time burst to begin with.
    pub camera: u32,
    /// The frame is displayed taller than wide. Two frames of different
    /// orientation are never one subject: the photographer turned the camera.
    pub portrait: bool,
    /// The camera's wall clock as seconds since the Unix epoch, or `None` when the file carries no
    /// time. EXIF carries no zone, so this is never shifted into the viewer's zone; it is
    /// formatted back as UTC.
    pub taken: Option<f64>,
    /// What the frame looks like, when its thumbnail could be measured.
    pub look: Option<Look>,
    /// The main subject's box as fractions of the frame: (x1, y1, x2, y2).
    pub subject: Option<[f64; 4]>,
    /// The frame was taken as a pan. A pan is one continuous action: the car is
    /// *meant* to cross the frame, so between two pan frames its box may travel
    /// further before it counts as a cut.
    pub panning: bool,
}

fn box_area([x1, y1, x2, y2]: [f64; 4]) -> f64 {
    (x2 - x1).abs() * (y2 - y1).abs()
}

/// Are two boxes of neighbouring frames plausibly the same subject? `leeway`
/// widens both limits (1.0 normally, [`MOMENT_PAN_LEEWAY`] between pans), and
/// `rules` holds the album's own scale ratio and shift.
fn continues(previous: [f64; 4], next: [f64; 4], leeway: f64, rules: &MomentRules) -> bool {
    let centre = |[x1, y1, x2, y2]: [f64; 4]| ((x1 + x2) / 2.0, (y1 + y2) / 2.0);
    let (fa, fb) = (box_area(previous), box_area(next));
    let scale_ok = fa.max(fb) <= rules.scale_ratio * leeway * fa.min(fb);
    let ((ax, ay), (bx, by)) = (centre(previous), centre(next));
    let shift = (ax - bx).hypot(ay - by) / std::f64::consts::SQRT_2;
    scale_ok && shift <= rules.shift * leeway
}

/// The moment currently open in one burst, as [`split_moments`] tracks it.
struct Open {
    key: i64,
    /// The last boxed frame's subject: what the next boxed frame is compared to.
    last: Option<[f64; 4]>,
    /// That frame's look (its livery is compared the same way).
    last_look: Option<Look>,
    /// That frame was a pan.
    pan: bool,
    /// The previous frame of the burst, boxed or not: when it was taken and
    /// how its scene looked, for the pause test.
    taken: Option<f64>,
    scene: Option<[u8; LOOK_SCENE_BINS]>,
}

/// Two vehicles' liveries, when both frames have one.
fn livery_distance(a: Option<Look>, b: Option<Look>) -> Option<f64> {
    let (a, b) = (a?, b?);
    if !(a.vehicle && b.vehicle) {
        return None;
    }
    Some(look_distance(&a.subject?, &b.subject?))
}

/// A burst seen the next one starts within this many seconds of its last frame
/// is joined to it, when the two look like the same subject. Measured on album
/// 109 (2026-10-06 Race-Week): 20261006_B1000_0201 and 0202 are the same green
/// car from the same angle 3.5 s apart and were split into two stacks by the
/// time gap alone, while the already-stacked 0198/0199 are 0.8 s apart. Ten
/// seconds keeps a short pause in a walkaround together without reaching into
/// the next car.
pub const MERGE_GAP: f64 = 10.0;
/// ...and the two frames' whole-frame colour layouts are at most this far
/// apart (0 identical, 1 disjoint). 0201 and 0202 measure 0.098, the same as
/// 0198/0199; 0.12 keeps that pair and stays under the distance between two
/// different cars at a meet (0.2 and up; see the album sweep in the change
/// log).
pub const MERGE_SCENE: f64 = 0.12;
/// A merged burst is at most this long, first frame to last: a slow walkaround
/// of a car is one subject, but a lap of the paddock is not. Twenty seconds is
/// about three pauses of [`MERGE_GAP`] chained, and covers one approach.
pub const MERGE_SPAN: f64 = 30.0;
/// The box leeway a join uses: 1.5x the moment splitter's own limits, because
/// the two frames are in different bursts and a burst boundary is exactly where
/// a photographer shuffles their feet.
pub const MERGE_LEEWAY: f64 = 1.5;

/// How freely [`merge_bursts`] joins neighbouring time bursts. The album's
/// `grouping_merge` setting turns the pass on or off; these are its measured
/// defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MergeRules {
    pub gap: f64,
    pub scene: f64,
    pub span: f64,
    pub leeway: f64,
}

impl Default for MergeRules {
    fn default() -> Self {
        MergeRules {
            gap: MERGE_GAP,
            scene: MERGE_SCENE,
            span: MERGE_SPAN,
            leeway: MERGE_LEEWAY,
        }
    }
}

/// Join neighbouring time bursts that are the same subject a few seconds
/// apart, with the measured defaults ([`MergeRules::default()`]). Input is in
/// time order, one frame per position; the answer is the burst key per frame,
/// which is the earlier burst's key for every frame of a joined group.
///
/// The time cut is a fixed gap ([`assign_bursts`]): a frame 3.5 s after the
/// last one starts a new burst even when it is the same car from the same
/// angle, and the moment splitter only ever *splits* a burst. This pass looks
/// the other way, at the boundary between two neighbouring bursts of one
/// camera (A's last frame `a`, B's first `b`), and joins them when:
///
/// * `b` is no more than [`MERGE_GAP`] seconds after `a`;
/// * both are the same orientation (the photographer did not turn the camera);
/// * their scene layouts are within [`MERGE_SCENE`] of each other;
/// * when both have a main subject box, the boxes continue each other with
///   [`MERGE_LEEWAY`] (see [`continues`]);
/// * when both are vehicles, their liveries are within
///   [`MOMENT_SUBJECT_DISTANCE`].
///
/// A join may chain, but never past [`MERGE_SPAN`] from the group's first
/// frame. A frame with no time or no look is never joined across: today's
/// behaviour is kept where there is nothing to compare.
pub fn merge_bursts(frames: &[MomentFrame]) -> Vec<i64> {
    merge_bursts_with(frames, &MergeRules::default())
}

/// [`merge_bursts`], with the album's own limits.
pub fn merge_bursts_with(frames: &[MomentFrame], rules: &MergeRules) -> Vec<i64> {
    // Each burst's first and last frame, and each camera's bursts in the order
    // they first appear (the input is in time order).
    let mut span: std::collections::HashMap<(u32, i64), (usize, usize)> =
        std::collections::HashMap::new();
    let mut cameras: std::collections::BTreeMap<u32, Vec<i64>> = std::collections::BTreeMap::new();
    for (i, frame) in frames.iter().enumerate() {
        let entry = span.entry((frame.camera, frame.burst)).or_insert_with(|| {
            cameras.entry(frame.camera).or_default().push(frame.burst);
            (i, i)
        });
        entry.1 = i;
    }
    // The joined key of every burst; a group carries the key of its first.
    let mut joined: std::collections::HashMap<(u32, i64), i64> = std::collections::HashMap::new();
    for (camera, bursts) in cameras {
        // The open group: its key, first frame's time, and the last frame so far.
        let mut group: Option<(i64, f64, usize)> = None;
        for burst in bursts {
            let (first, last) = span[&(camera, burst)];
            let key = match group {
                Some((key, started, previous))
                    if joins(&frames[previous], &frames[first], rules)
                        && frames[first]
                            .taken
                            .is_some_and(|taken| taken - started <= rules.span) =>
                {
                    key
                }
                _ => {
                    group = Some((burst, frames[first].taken.unwrap_or_default(), first));
                    burst
                }
            };
            let (_, started, _) = group.expect("a group is open");
            group = Some((key, started, last));
            joined.insert((camera, burst), key);
        }
    }
    frames
        .iter()
        .map(|frame| joined[&(frame.camera, frame.burst)])
        .collect()
}

/// Are these two frames the same subject a few seconds apart? `a` is the last
/// frame of one burst, `b` the first of the next.
fn joins(a: &MomentFrame, b: &MomentFrame, rules: &MergeRules) -> bool {
    let (Some(ta), Some(tb)) = (a.taken, b.taken) else {
        return false;
    };
    let (Some(la), Some(lb)) = (a.look, b.look) else {
        return false;
    };
    if tb < ta || tb - ta > rules.gap {
        return false;
    }
    if a.portrait != b.portrait {
        return false;
    }
    if look_distance(&la.scene, &lb.scene) > rules.scene {
        return false;
    }
    let boxes = match (a.subject, b.subject) {
        (Some(previous), Some(next)) => {
            continues(previous, next, rules.leeway, &MomentRules::default())
        }
        _ => true,
    };
    boxes
        && livery_distance(Some(la), Some(lb))
            .is_none_or(|distance| distance <= MOMENT_SUBJECT_DISTANCE)
}

/// Does this frame belong to the moment `open` holds?
fn belongs(
    open: &Open,
    frame: &MomentFrame,
    subject: Option<[f64; 4]>,
    rules: &MomentRules,
) -> bool {
    // A pause AND a picture that is not visually similar: the whole frame's
    // colours moved on, or the vehicle's livery is not the previous one's. A
    // pause of zero is the album turning the pause cut off.
    if let (Some(before), Some(now)) = (open.taken, frame.taken) {
        if rules.pause > 0.0 && now - before > rules.pause {
            let scene = open.scene.zip(frame.look.map(|l| l.scene));
            if scene.is_some_and(|(a, b)| look_distance(&a, &b) > rules.scene_distance) {
                return false;
            }
            if subject.is_some()
                && livery_distance(open.last_look, frame.look)
                    .is_some_and(|d| d > rules.pause_subject_distance)
            {
                return false;
            }
        }
    }
    let (Some(next), Some(previous)) = (subject, open.last) else {
        return true;
    };
    let leeway = if open.pan && frame.panning {
        rules.pan_leeway
    } else {
        1.0
    };
    // The box jumped between neighbours, or the car is clearly not the same one.
    continues(previous, next, leeway, rules)
        && livery_distance(open.last_look, frame.look).is_none_or(|d| d <= rules.subject_distance)
}

/// A new group key per frame, splitting each time burst into moments only where
/// the picture cuts to something else, with today's thresholds
/// ([`MomentRules::default()`]). Input is in time order. Keys are dense from 1
/// and never shared across bursts.
///
/// A burst of shooting is one moment unless the frame in hand cuts from the
/// moment before it, in any of three ways:
///
/// * **the box jumps**: it is more than [`MOMENT_SCALE_RATIO`] times the size
///   or [`MOMENT_SHIFT`] of the diagonal away from the *previous* boxed frame
///   (see [`continues`]). Comparing with the previous frame, not the first,
///   means a car that approaches, recedes or crosses never splits by drifting;
/// * **a pause and a new picture**: more than [`MOMENT_PAUSE`] seconds since
///   the previous frame, and either the whole frame's colours have moved on
///   ([`MOMENT_SCENE_DISTANCE`]) or the vehicle's livery has
///   ([`MOMENT_PAUSE_SUBJECT_DISTANCE`]);
/// * **a clearly different car**: the livery of the boxed vehicle is further
///   than [`MOMENT_SUBJECT_DISTANCE`] from the previous boxed vehicle's.
///
/// A frame with no box (a detector miss) and a speck of a box
/// ([`MOMENT_MIN_AREA`]) never cut on the box. A frame with no [`Look`] is
/// never cut on looks.
pub fn split_moments(frames: &[MomentFrame]) -> Vec<i64> {
    split_moments_with(frames, &MomentRules::default())
}

/// [`split_moments`], with the album's own thresholds. `rules.split` off makes
/// a time burst one stack: every frame of it gets the burst's key, numbered
/// densely in the order the bursts first appear, the same numbering space as
/// a split run.
pub fn split_moments_with(frames: &[MomentFrame], rules: &MomentRules) -> Vec<i64> {
    if !rules.split {
        let mut keys: HashMap<i64, i64> = HashMap::new();
        let mut next: i64 = 0;
        return frames
            .iter()
            .map(|frame| {
                *keys.entry(frame.burst).or_insert_with(|| {
                    next += 1;
                    next
                })
            })
            .collect();
    }
    let mut open: HashMap<i64, Open> = HashMap::new();
    let mut moments: i64 = 0;
    frames
        .iter()
        .map(|frame| {
            let subject = frame.subject.filter(|&b| box_area(b) >= MOMENT_MIN_AREA);
            let scene = frame.look.map(|l| l.scene);
            let joined = open.get_mut(&frame.burst).and_then(|o| {
                if !belongs(o, frame, subject, rules) {
                    return None;
                }
                if subject.is_some() {
                    o.last = subject;
                    o.last_look = frame.look;
                    o.pan = frame.panning;
                }
                o.taken = frame.taken;
                o.scene = scene;
                Some(o.key)
            });
            joined.unwrap_or_else(|| {
                moments += 1;
                open.insert(
                    frame.burst,
                    Open {
                        key: moments,
                        last: subject,
                        last_look: subject.and(frame.look),
                        pan: frame.panning,
                        taken: frame.taken,
                        scene,
                    },
                );
                moments
            })
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
    /// The camera's wall clock as seconds since the Unix epoch, or `None` when the file carries no
    /// time. EXIF carries no zone, so this is never shifted into the viewer's zone; it is
    /// formatted back as UTC.
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
            camera: 0,
            portrait: false,
            taken: None,
            look: None,
            subject,
            panning: false,
        }
    }

    /// A frame taken at `taken` seconds, with a look: the scene and the
    /// subject's livery each concentrated in one bin (a vehicle).
    fn shot(
        taken: f64,
        subject: Option<[f64; 4]>,
        scene: usize,
        livery: [u8; LOOK_SUBJECT_BINS],
    ) -> MomentFrame {
        let mut layout = [0u8; LOOK_SCENE_BINS];
        layout[scene] = 255;
        MomentFrame {
            taken: Some(taken),
            look: Some(Look {
                scene: layout,
                subject: Some(livery),
                vehicle: true,
            }),
            ..mf(1, subject)
        }
    }

    /// A livery that is all one bin.
    fn colour(bin: usize) -> [u8; LOOK_SUBJECT_BINS] {
        let mut l = [0u8; LOOK_SUBJECT_BINS];
        l[bin] = 255;
        l
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
    fn a_car_growing_from_two_to_thirty_percent_stays_one_moment() {
        let frames: Vec<_> = (0..20)
            .map(|i| mf(1, boxed(0.5, 0.5, 0.02 + 0.28 * f64::from(i) / 19.0)))
            .collect();
        assert_eq!(split_moments(&frames), vec![1; 20]);
    }

    #[test]
    fn an_approaching_car_growing_threefold_over_twelve_frames_stays_one_moment() {
        // 0.04 -> 0.12 of the frame while drifting a quarter of the way across:
        // far from the first frame, close to the one before it.
        let frames: Vec<_> = (0..12)
            .map(|i| {
                let t = f64::from(i) / 11.0;
                mf(1, boxed(0.3 + 0.25 * t, 0.5, 0.04 * 3.0_f64.powf(t)))
            })
            .collect();
        assert_eq!(split_moments(&frames), vec![1; 12]);
    }

    #[test]
    fn detector_misses_between_boxed_frames_of_one_car_stay_one_moment() {
        let mut frames = vec![mf(1, boxed(0.4, 0.5, 0.1)), mf(1, None), mf(1, None)];
        frames.push(mf(1, boxed(0.42, 0.5, 0.11)));
        frames.push(mf(1, None));
        frames.push(mf(1, boxed(0.44, 0.5, 0.12)));
        assert_eq!(split_moments(&frames), vec![1; 6]);
    }

    #[test]
    fn two_different_cars_back_to_back_split_at_the_cut() {
        // Car A leaves large at the right edge; car B enters small at the left.
        let mut frames: Vec<_> = (0..4)
            .map(|i| mf(1, boxed(0.6 + 0.08 * f64::from(i), 0.5, 0.2)))
            .collect();
        frames.extend((0..4).map(|i| mf(1, boxed(0.15 + 0.03 * f64::from(i), 0.5, 0.03))));
        assert_eq!(split_moments(&frames), vec![1, 1, 1, 1, 2, 2, 2, 2]);
    }

    #[test]
    fn a_cut_to_a_different_car_with_misses_between_still_splits() {
        let mut frames = vec![mf(1, boxed(0.8, 0.5, 0.2)), mf(1, None), mf(1, None)];
        frames.push(mf(1, boxed(0.2, 0.5, 0.03)));
        frames.push(mf(1, boxed(0.22, 0.5, 0.035)));
        assert_eq!(split_moments(&frames), vec![1, 1, 1, 2, 2]);
    }

    #[test]
    fn a_pan_followed_by_a_different_static_car_splits() {
        let mut frames: Vec<_> = (0..6)
            .map(|i| pan_mf(1, boxed(0.2 + 0.1 * f64::from(i), 0.5, 0.1)))
            .collect();
        frames.extend((0..3).map(|_| mf(1, boxed(0.2, 0.2, 0.01))));
        assert_eq!(split_moments(&frames), vec![1, 1, 1, 1, 1, 1, 2, 2, 2]);
    }

    #[test]
    fn look_distance_is_zero_for_the_same_histogram_and_one_for_disjoint_ones() {
        assert_eq!(look_distance(&colour(2), &colour(2)), 0.0);
        assert_eq!(look_distance(&colour(2), &colour(9)), 1.0);
    }

    #[test]
    fn two_cars_of_different_colour_in_the_same_spot_back_to_back_split() {
        // No jump in the box at all: only the livery changes.
        let frames: Vec<_> = (0..8)
            .map(|i| {
                let car = if i < 4 { 0 } else { 6 };
                shot(f64::from(i) * 0.13, boxed(0.5, 0.5, 0.1), 3, colour(car))
            })
            .collect();
        assert_eq!(split_moments(&frames), vec![1, 1, 1, 1, 2, 2, 2, 2]);
    }

    #[test]
    fn one_car_whose_livery_wobbles_a_little_stays_one_moment() {
        let mut wobble = colour(0);
        wobble[0] = 180;
        wobble[1] = 75;
        let frames = [
            shot(0.0, boxed(0.5, 0.5, 0.1), 3, colour(0)),
            shot(0.13, boxed(0.5, 0.5, 0.1), 3, wobble),
            shot(0.26, boxed(0.5, 0.5, 0.1), 3, colour(0)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 1, 1]);
    }

    #[test]
    fn a_pause_with_the_same_look_stays_one_moment() {
        let frames = [
            shot(0.0, boxed(0.5, 0.5, 0.1), 3, colour(0)),
            shot(0.13, boxed(0.5, 0.5, 0.1), 3, colour(0)),
            shot(1.5, boxed(0.52, 0.5, 0.11), 3, colour(0)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 1, 1]);
    }

    #[test]
    fn a_pause_and_a_different_scene_splits_even_with_no_box() {
        let frames = [
            shot(0.0, boxed(0.5, 0.5, 0.1), 3, colour(0)),
            shot(0.13, boxed(0.5, 0.5, 0.1), 3, colour(0)),
            MomentFrame {
                subject: None,
                ..shot(1.5, None, 20, colour(0))
            },
            MomentFrame {
                subject: None,
                ..shot(1.63, None, 20, colour(0))
            },
        ];
        assert_eq!(split_moments(&frames), vec![1, 1, 2, 2]);
    }

    #[test]
    fn a_pause_and_a_different_livery_splits_on_the_same_scene() {
        let frames = [
            shot(0.0, boxed(0.5, 0.5, 0.1), 3, colour(0)),
            shot(1.2, boxed(0.5, 0.5, 0.1), 3, colour(7)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 2]);
    }

    #[test]
    fn a_short_gap_with_a_different_scene_is_not_a_pause() {
        let frames = [
            shot(0.0, boxed(0.5, 0.5, 0.1), 3, colour(0)),
            shot(0.5, boxed(0.5, 0.5, 0.1), 20, colour(0)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 1]);
    }

    #[test]
    fn a_pause_with_no_look_to_compare_never_splits() {
        let frames = [
            MomentFrame {
                taken: Some(0.0),
                ..mf(1, boxed(0.5, 0.5, 0.1))
            },
            MomentFrame {
                taken: Some(2.0),
                ..mf(1, boxed(0.5, 0.5, 0.1))
            },
        ];
        assert_eq!(split_moments(&frames), vec![1, 1]);
    }

    /// A frame of `burst` at `taken`, with scene bin `scene` and a vehicle's
    /// livery in bin `livery`.
    fn burst_shot(
        burst: i64,
        taken: f64,
        scene: usize,
        livery: usize,
        subject: Option<[f64; 4]>,
    ) -> MomentFrame {
        MomentFrame {
            burst,
            ..shot(taken, subject, scene, colour(livery))
        }
    }

    #[test]
    fn a_look_alike_burst_a_few_seconds_later_is_joined() {
        // 0201/0202: the same car and angle 3.5 s after the last frame of the
        // burst before it (the time cut is 1.5 s).
        let frames = vec![
            burst_shot(1, 0.0, 3, 0, boxed(0.5, 0.5, 0.1)),
            burst_shot(1, 0.8, 3, 0, boxed(0.5, 0.52, 0.1)),
            burst_shot(2, 4.3, 3, 0, boxed(0.52, 0.5, 0.11)),
        ];
        assert_eq!(merge_bursts(&frames), vec![1, 1, 1]);
    }

    #[test]
    fn a_burst_well_after_the_last_frame_is_not_joined() {
        let frames = vec![
            burst_shot(1, 0.0, 3, 0, boxed(0.5, 0.5, 0.1)),
            burst_shot(2, 15.0, 3, 0, boxed(0.5, 0.5, 0.1)),
        ];
        assert_eq!(merge_bursts(&frames), vec![1, 2]);
    }

    #[test]
    fn a_turned_camera_is_not_joined() {
        let frames = vec![
            burst_shot(1, 0.0, 3, 0, boxed(0.5, 0.5, 0.1)),
            MomentFrame {
                portrait: true,
                ..burst_shot(2, 4.0, 3, 0, boxed(0.5, 0.5, 0.1))
            },
        ];
        assert_eq!(merge_bursts(&frames), vec![1, 2]);
    }

    #[test]
    fn a_different_livery_is_not_joined() {
        let frames = vec![
            burst_shot(1, 0.0, 3, 0, boxed(0.5, 0.5, 0.1)),
            burst_shot(2, 4.0, 3, 7, boxed(0.5, 0.5, 0.1)),
        ];
        assert_eq!(merge_bursts(&frames), vec![1, 2]);
    }

    #[test]
    fn a_different_scene_is_not_joined() {
        let frames = vec![
            burst_shot(1, 0.0, 3, 0, boxed(0.5, 0.5, 0.1)),
            burst_shot(2, 4.0, 20, 0, boxed(0.5, 0.5, 0.1)),
        ];
        assert_eq!(merge_bursts(&frames), vec![1, 2]);
    }

    #[test]
    fn a_chain_of_joins_stops_at_the_span_cap() {
        // Each burst starts 8 s after the last one, so every pair is within the
        // gap, but the group may not run past 30 s from its first frame.
        let frames: Vec<_> = (0..5i64)
            .map(|i| burst_shot(i + 1, i as f64 * 8.0, 3, 0, boxed(0.5, 0.5, 0.1)))
            .collect();
        assert_eq!(merge_bursts(&frames), vec![1, 1, 1, 1, 5]);
    }

    #[test]
    fn another_camera_is_never_joined() {
        let frames = vec![
            burst_shot(1, 0.0, 3, 0, boxed(0.5, 0.5, 0.1)),
            MomentFrame {
                camera: 1,
                ..burst_shot(2, 4.0, 3, 0, boxed(0.5, 0.5, 0.1))
            },
        ];
        assert_eq!(merge_bursts(&frames), vec![1, 2]);
    }

    #[test]
    fn frames_without_a_time_or_a_look_are_not_joined() {
        let untimed = vec![
            burst_shot(1, 0.0, 3, 0, boxed(0.5, 0.5, 0.1)),
            MomentFrame {
                taken: None,
                ..burst_shot(2, 4.0, 3, 0, boxed(0.5, 0.5, 0.1))
            },
        ];
        assert_eq!(merge_bursts(&untimed), vec![1, 2]);
        let unlooked = vec![
            burst_shot(1, 0.0, 3, 0, boxed(0.5, 0.5, 0.1)),
            MomentFrame {
                look: None,
                ..burst_shot(2, 4.0, 3, 0, boxed(0.5, 0.5, 0.1))
            },
        ];
        assert_eq!(merge_bursts(&unlooked), vec![1, 2]);
    }

    #[test]
    fn a_person_is_never_compared_by_livery() {
        let mut marshal = shot(0.13, boxed(0.5, 0.5, 0.1), 3, colour(8));
        if let Some(look) = marshal.look.as_mut() {
            look.vehicle = false;
        }
        let frames = [shot(0.0, boxed(0.5, 0.5, 0.1), 3, colour(0)), marshal];
        assert_eq!(split_moments(&frames), vec![1, 1]);
    }

    #[test]
    fn a_speck_of_a_box_is_a_miss_not_a_cut() {
        // A marshal the detector flickers on while the car is out of the box.
        let frames = [
            mf(1, boxed(0.5, 0.5, 0.1)),
            mf(1, boxed(0.1, 0.9, 0.001)),
            mf(1, boxed(0.52, 0.5, 0.11)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 1, 1]);
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
    fn a_frame_with_no_box_outside_a_pan_is_still_a_miss_not_a_cut() {
        let frames = vec![
            mf(1, boxed(0.3, 0.5, 0.1)),
            mf(1, None),
            mf(1, boxed(0.3, 0.5, 0.1)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 1, 1]);
    }

    #[test]
    fn a_pan_across_the_frame_stays_one_moment() {
        // The car travels from the left third to the right third: far past the
        // moment shift, but a pan is one action, not a cut.
        let frames: Vec<_> = (0..12)
            .map(|i| pan_mf(1, boxed(0.2 + 0.05 * f64::from(i), 0.5, 0.1)))
            .collect();
        assert_eq!(split_moments(&frames), vec![1; 12]);
        // A static frame right where the pan ended is the same car: one moment.
        let mut frames = frames;
        frames.push(mf(1, boxed(0.8, 0.5, 0.1)));
        assert_eq!(split_moments(&frames), vec![1; 13]);
    }

    #[test]
    fn a_pan_does_not_join_a_static_moment_framed_elsewhere() {
        // A pan beginning on another car must not swallow the static moment:
        // the box is both far smaller and far across the frame.
        let frames = [
            mf(1, boxed(0.2, 0.2, 0.05)),
            pan_mf(1, boxed(0.75, 0.7, 0.30)),
            pan_mf(1, boxed(0.8, 0.7, 0.30)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 2, 2]);
    }

    #[test]
    fn a_pan_joins_the_box_less_frames_before_it() {
        // The detector missed the car until the pan got going: the early frames
        // hold no framing to disagree with, so the pan continues that moment.
        let frames = [
            mf(1, None),
            mf(1, None),
            pan_mf(1, boxed(0.5, 0.5, 0.1)),
            pan_mf(1, boxed(0.8, 0.5, 0.1)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 1, 1, 1]);
    }

    #[test]
    fn a_pan_that_starts_on_the_same_framing_joins_the_static_moment() {
        let frames = [
            mf(1, boxed(0.3, 0.5, 0.1)),
            pan_mf(1, boxed(0.3, 0.5, 0.1)),
            pan_mf(1, boxed(0.9, 0.5, 0.1)),
        ];
        assert_eq!(split_moments(&frames), vec![1, 1, 1]);
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
    fn subjects_that_appear_or_vanish_never_split_and_none_throughout_stays_one() {
        let frames = [
            mf(1, None),
            mf(1, None),
            mf(1, None),
            mf(1, boxed(0.5, 0.5, 0.1)),
            mf(1, boxed(0.5, 0.5, 0.1)),
        ];
        assert_eq!(split_moments(&frames), vec![1; 5]);
        let none: Vec<_> = (0..6).map(|_| mf(1, None)).collect();
        assert_eq!(split_moments(&none), vec![1; 6]);
    }

    #[test]
    fn the_sensitivity_ladder_moves_every_threshold_the_right_way() {
        let normal = MomentRules::from_sensitivity("normal", MOMENT_PAUSE, true);
        assert_eq!(
            normal,
            MomentRules::default(),
            "normal is today's behaviour"
        );
        assert!(
            !MomentRules::from_sensitivity("off", MOMENT_PAUSE, true).split,
            "off never cuts a burst"
        );
        let gentle = MomentRules::from_sensitivity("gentle", MOMENT_PAUSE, true);
        let eager = MomentRules::from_sensitivity("eager", MOMENT_PAUSE, true);
        for (word, rules, factor) in [("gentle", gentle, 1.5), ("eager", eager, 0.7)] {
            assert_eq!(
                rules.scale_ratio,
                normal.scale_ratio * factor,
                "{word} scale"
            );
            assert_eq!(rules.shift, normal.shift * factor, "{word} shift");
            assert_eq!(
                rules.scene_distance,
                normal.scene_distance * factor,
                "{word} scene"
            );
            assert_eq!(
                rules.pause_subject_distance,
                normal.pause_subject_distance * factor,
                "{word} paused subject"
            );
            assert_eq!(
                rules.subject_distance,
                normal.subject_distance * factor,
                "{word} subject"
            );
            assert_eq!(rules.pause, MOMENT_PAUSE, "{word} takes the album's pause");
            assert_eq!(rules.pan_leeway, MOMENT_PAN_LEEWAY, "{word} keeps pans");
        }
        assert!(
            gentle.shift > normal.shift && eager.shift < normal.shift,
            "gentle loosens, eager tightens"
        );
        assert_eq!(
            MomentRules::from_sensitivity("normal", MOMENT_PAUSE, false).pan_leeway,
            1.0,
            "keep_pans off compares a pan like any other box"
        );
        assert_eq!(
            MomentRules::from_sensitivity("loud", MOMENT_PAUSE, true),
            normal,
            "an unknown word is normal"
        );
        assert_eq!(
            sensitivity_word("  EAGER "),
            "eager",
            "trimmed and lower-cased"
        );
        assert_eq!(sensitivity_word("gentle"), "gentle");
        assert_eq!(clamped_pause(-2.0), 0.0);
        assert_eq!(clamped_pause(0.0), 0.0, "zero disables, not the default");
        assert_eq!(clamped_pause(2.5), 2.5);
        assert_eq!(clamped_pause(11.0), 10.0);
        assert_eq!(
            MomentRules::from_sensitivity("normal", -1.0, true).pause,
            0.0,
            "a pause is clamped to 0..=10"
        );
        assert_eq!(
            MomentRules::from_sensitivity("normal", 99.0, true).pause,
            10.0
        );
    }

    #[test]
    fn splitting_off_makes_a_whole_time_burst_one_stack() {
        // Two very different cars back to back in one burst, then a second burst.
        let mut frames: Vec<_> = (0..4)
            .map(|i| mf(1, boxed(0.6 + 0.08 * f64::from(i), 0.5, 0.2)))
            .collect();
        frames.extend((0..4).map(|i| mf(1, boxed(0.15 + 0.03 * f64::from(i), 0.5, 0.03))));
        frames.push(mf(2, boxed(0.5, 0.5, 0.1)));
        assert_eq!(
            split_moments(&frames),
            vec![1, 1, 1, 1, 2, 2, 2, 2, 3],
            "normal cuts at the different car"
        );
        let off = MomentRules::from_sensitivity("off", MOMENT_PAUSE, true);
        assert_eq!(
            split_moments_with(&frames, &off),
            vec![1, 1, 1, 1, 1, 1, 1, 1, 2],
            "off is one key per burst, densely numbered"
        );
    }

    #[test]
    fn gentle_keeps_a_pair_together_that_normal_splits() {
        // Same size box, its centre 0.55 of the frame further right: a shift of
        // 0.39, past normal's 0.30 and inside gentle's 0.45.
        let frames = [mf(1, boxed(0.25, 0.5, 0.1)), mf(1, boxed(0.8, 0.5, 0.1))];
        assert_eq!(split_moments(&frames), vec![1, 2], "normal cuts");
        let gentle = MomentRules::from_sensitivity("gentle", MOMENT_PAUSE, true);
        assert_eq!(
            split_moments_with(&frames, &gentle),
            vec![1, 1],
            "gentle keeps it"
        );
    }

    #[test]
    fn eager_splits_a_pair_that_normal_keeps() {
        // A shift of 0.25: inside normal's 0.30, past eager's 0.21.
        let frames = [mf(1, boxed(0.3, 0.5, 0.1)), mf(1, boxed(0.65, 0.5, 0.1))];
        assert_eq!(split_moments(&frames), vec![1, 1], "normal keeps it");
        let eager = MomentRules::from_sensitivity("eager", MOMENT_PAUSE, true);
        assert_eq!(
            split_moments_with(&frames, &eager),
            vec![1, 2],
            "eager cuts"
        );
    }

    #[test]
    fn a_pause_of_zero_turns_the_pause_cut_off() {
        // Five seconds apart and a different scene: a pause, under the default.
        let frames = [
            shot(0.0, boxed(0.5, 0.5, 0.1), 3, colour(0)),
            shot(5.0, boxed(0.5, 0.5, 0.1), 20, colour(0)),
        ];
        assert_eq!(
            split_moments(&frames),
            vec![1, 2],
            "a pause with a new scene cuts"
        );
        let no_pause = MomentRules::from_sensitivity("normal", 0.0, true);
        assert_eq!(no_pause.pause, 0.0);
        assert_eq!(
            split_moments_with(&frames, &no_pause),
            vec![1, 1],
            "no pause cut: the same box and livery stay one moment"
        );
    }
}
