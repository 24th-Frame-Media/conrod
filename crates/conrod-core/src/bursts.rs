//! Which camera took it, and which burst it belongs to.
//!
//! Two shooters at one event interleave into one folder, so a burst is
//! consecutive frames *from one body* with no gap longer than
//! [`BURST_GAP_SECONDS`]. The camera is its serial number where the file has
//! one, since that is the only thing separating two identical bodies.

use crate::text::value_to_string;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// Longer than this between two frames of one camera starts a new burst. The
/// gap being detected is the one between two cars, not between two frames.
pub const BURST_GAP_SECONDS: f64 = 4.0;

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

/// pHash bits two frames may differ by and still be candidate duplicates.
pub const DUPE_BITS: u32 = 10;
/// Tighter bound for a pair with no embeddings to confirm it (scenery, crowds).
pub const DUPE_BITS_UNCONFIRMED: u32 = 4;

/// One frame as the duplicate finder sees it.
pub struct DupeFrame {
    pub id: i64,
    pub phash: u64,
    pub burst: Option<i64>,
    /// Unit-vector embeddings of the frame's subjects.
    pub looks: Vec<Vec<f32>>,
}

/// Re-shoots of the same picture across the whole shoot: pHash within
/// [`DUPE_BITS`] as candidates, confirmed by any subject pair's cosine at or
/// above `same`, joined transitively. Pairs within one burst are skipped since
/// the burst already stacks them. Returns frame id -> group key (the group's
/// smallest id), only for frames in a group of two or more.
pub fn near_dupes(frames: &[DupeFrame], same: f32) -> BTreeMap<i64, i64> {
    let mut parent: Vec<usize> = (0..frames.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i];
        }
        i
    }
    // ponytail: O(n²) Hamming scan, fine to ~20k frames; bucket by hash prefix beyond.
    for i in 0..frames.len() {
        for j in i + 1..frames.len() {
            let (a, b) = (&frames[i], &frames[j]);
            if a.burst.is_some() && a.burst == b.burst {
                continue;
            }
            let bits = (a.phash ^ b.phash).count_ones();
            if bits > DUPE_BITS {
                continue;
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
                let (ra, rb) = (root(&mut parent, i), root(&mut parent, j));
                parent[ra.max(rb)] = ra.min(rb);
            }
        }
    }
    let mut members: BTreeMap<usize, Vec<i64>> = BTreeMap::new();
    for i in 0..frames.len() {
        let r = root(&mut parent, i);
        members.entry(r).or_default().push(frames[i].id);
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
            looks: if look.is_empty() { vec![] } else { vec![look.to_vec()] },
        }
    }

    #[test]
    fn reshoots_group_across_bursts_but_not_within_or_when_the_look_differs() {
        let car = [1.0, 0.0];
        let other = [0.0, 1.0];
        let h = 0xFFFF_0000;
        let frames = [
            f(1, 0, Some(1), &car),
            f(2, 0b111, Some(2), &car),  // 3 bits, same car, other burst: joins 1
            f(3, 0b1111, Some(3), &car), // joins 2, so 1 transitively
            f(5, 0b1, Some(5), &[]),     // no looks, 1 bit: joins by hash alone
            f(4, h, Some(4), &other),
            f(10, h, Some(10), &car),     // same hash as 4, different car: apart
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
}
