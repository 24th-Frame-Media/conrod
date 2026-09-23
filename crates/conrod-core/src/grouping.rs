//! Recognising the same vehicle across frames, and agreeing on what it is.
//!
//! Port of `conrod/grouping.py`. Two clusterers decide which crops are one
//! car -- [`cluster_by_look`] on an embedding, [`cluster`] on the older
//! dHash-and-colour signature -- and [`consensus`] settles what the group
//! agrees it is. `signature(image)` itself needs PIL and stays in Python;
//! everything here is the pure comparison and voting that follows it.
//! `_second_look` and `consolidate` touch the database and the vision model
//! and stay in Python too -- `consolidate`'s shape is what the row and member
//! types here are modelled on.

use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// A Python-truthy string: present and not empty.
fn given(value: Option<&str>) -> Option<&str> {
    value.filter(|s| !s.is_empty())
}

/// Python truthiness of a decoded JSON value.
fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_none_or(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

// --- the dHash/colour signature, as a string -------------------------------

/// `int(head, 16)`: an optional `0x`/`0X` prefix, surrounding whitespace
/// allowed. ponytail: no sign handling -- `signature()`'s own hex heads are
/// never negative, and a fixture is free to add one if that ever changes.
fn parse_hex(text: &str) -> Option<u64> {
    let t = text.trim();
    let t = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    u64::from_str_radix(t, 16).ok()
}

/// `_parse`: the hex shape hash and the colour histogram it was packed with.
/// `None` stands in for the Python function's exception -- a signature that
/// does not parse, which every caller here treats as "give up gracefully".
pub fn parse_signature(sig: &str) -> Option<(u64, Vec<f32>)> {
    let (head, tail) = match sig.find(':') {
        Some(i) => (&sig[..i], &sig[i + 1..]),
        None => (sig, ""),
    };
    let hash = parse_hex(head)?;
    let mut values = Vec::new();
    for part in tail.split(',') {
        values.push(part.parse::<f32>().ok()?);
    }
    Some((hash, values))
}

/// `_shape_distance`: bits that differ between two dHashes. 999 -- far beyond
/// any real `max_bits` -- stands in for "could not be compared".
pub fn shape_distance(a: &str, b: &str) -> i64 {
    match (parse_signature(a), parse_signature(b)) {
        (Some((ha, _)), Some((hb, _))) => (ha ^ hb).count_ones().into(),
        _ => 999,
    }
}

/// `_colour_matches`: how much of the two colour histograms overlaps.
///
/// ponytail: Python's own try/except here only wraps `_parse` -- a length
/// mismatch reaches `np.minimum(ca, cb)` uncaught and crashes. Every real
/// signature has the same 36-bin histogram, so this is not reachable in
/// practice; returning `false` instead of panicking is a deliberate
/// deviation, not an oversight, and is not fixture-tested for that reason.
pub fn colour_matches(a: &str, b: &str, min_colour: f64) -> bool {
    let (Some((_, ca)), Some((_, cb))) = (parse_signature(a), parse_signature(b)) else {
        return false;
    };
    if ca.len() != cb.len() {
        return false;
    }
    // Summed in f32, like the numpy array it came from, so this lands on the
    // same rounding as Python rather than merely close to it.
    let overlap: f32 = ca.iter().zip(&cb).map(|(x, y)| x.min(*y)).sum();
    f64::from(overlap) >= min_colour
}

/// Do these two crops plausibly show the same vehicle?
pub fn similar(a: &str, b: &str, max_bits: i64, min_colour: f64) -> bool {
    colour_matches(a, b, min_colour) && shape_distance(a, b) <= max_bits
}

// --- Group -------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Group {
    pub key: i64,
    pub members: Vec<i64>,
    pub signature: String,
    pub last_frame: i64,
    pub frames: HashSet<i64>,
    pub swatch: Option<String>,
    pub cls: Option<String>,
    pub make: Option<String>,
    pub plates: HashSet<String>,
    pub numbers: HashSet<String>,
    pub bursts: HashSet<i64>,
    /// Every member's embedding, so a candidate is compared against the whole
    /// group rather than against one representative.
    pub vectors: Vec<Vec<f64>>,
}

/// How alike two crops have to look before [`cluster_by_look`] calls them the
/// same car. See `conrod/grouping.py` for the measurement behind it.
pub const SAME_CAR: f64 = 0.90;

#[allow(clippy::too_many_arguments)]
fn join(
    group: &mut Group,
    det_id: i64,
    frame_index: i64,
    cls: Option<&str>,
    make: Option<&str>,
    plate: Option<&str>,
    number: Option<&str>,
    assignment: &mut HashMap<i64, i64>,
    burst: Option<i64>,
) {
    group.members.push(det_id);
    group.last_frame = group.last_frame.max(frame_index);
    group.frames.insert(frame_index);
    if given(group.cls.as_deref()).is_none() {
        group.cls = cls.map(str::to_string);
    }
    // `group.make = group.make or make`: only replaces an absent make, and
    // may replace it with another absent one.
    if given(group.make.as_deref()).is_none() {
        group.make = make.map(str::to_string);
    }
    if let Some(p) = given(plate) {
        group.plates.insert(p.to_string());
    }
    if let Some(n) = given(number) {
        group.numbers.insert(n.to_string());
    }
    if let Some(b) = burst {
        group.bursts.insert(b);
    }
    assignment.insert(det_id, group.key);
}

// --- cluster_by_look ---------------------------------------------------------

/// One row of `cluster_by_look`'s input: a detection, its embedding, which
/// frame and burst it came from, vehicle class, and any plate read off it.
#[derive(Debug, Clone)]
pub struct LookRow {
    pub det_id: i64,
    pub vector: Option<Vec<f64>>,
    pub frame_index: i64,
    pub burst: Option<i64>,
    pub plate: Option<String>,
    pub number: Option<String>,
    pub cls: Option<String>,
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Assign each detection to a vehicle by embedding similarity. See
/// `conrod/grouping.py::cluster_by_look` for the two rules this follows.
pub fn cluster_by_look(rows: &[LookRow], same_car: f64) -> HashMap<i64, i64> {
    let mut groups: Vec<Group> = Vec::new();
    let mut assignment: HashMap<i64, i64> = HashMap::new();

    // An insertion-ordered map, like the Python dict `by_burst.setdefault`
    // builds: small enough here that a linear scan costs nothing.
    let mut by_burst: Vec<(Option<i64>, Vec<&LookRow>)> = Vec::new();
    for row in rows {
        if row.vector.is_none() {
            continue;
        }
        match by_burst.iter_mut().find(|(b, _)| *b == row.burst) {
            Some((_, entries)) => entries.push(row),
            None => by_burst.push((row.burst, vec![row])),
        }
    }

    for (burst, entries) in by_burst {
        let mut in_burst: Vec<usize> = Vec::new();
        for row in entries {
            let vector = row.vector.as_ref().unwrap();
            let plate = tidy_plate(row.plate.as_deref());
            let number = row
                .number
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let mut best: Option<usize> = None;
            let mut best_score = 0.0_f64;
            for &gi in &in_burst {
                let group = &groups[gi];
                // A car cannot be in the same photograph twice.
                if group.frames.contains(&row.frame_index) {
                    continue;
                }
                // Different vehicle classes (e.g. motorcycle vs car) cannot merge.
                if let (Some(c), Some(gc)) = (row.cls.as_deref(), group.cls.as_deref()) {
                    if c != gc {
                        continue;
                    }
                }
                // Two plates that are genuinely different settle it outright.
                if plate_verdict(plate.as_deref(), &group.plates) == Some(false) {
                    continue;
                }
                // If plates match directly, instant match within same burst
                if let Some(ref p) = plate {
                    if group.plates.contains(p) || nearly_seen(Some(p), &group.plates) {
                        best = Some(gi);
                        best_score = 1.0;
                        break;
                    }
                }
                let score = group
                    .vectors
                    .iter()
                    .map(|other| dot(vector, other))
                    .fold(f64::MIN, f64::max);
                if score > best_score {
                    best = Some(gi);
                    best_score = score;
                }
            }
            if let Some(gi) = best {
                if best_score >= same_car {
                    join(
                        &mut groups[gi],
                        row.det_id,
                        row.frame_index,
                        row.cls.as_deref(),
                        None,
                        plate.as_deref(),
                        number,
                        &mut assignment,
                        burst,
                    );
                    groups[gi].vectors.push(vector.clone());
                    continue;
                }
            }
            let key = groups.len() as i64 + 1;
            let mut group = Group {
                key,
                vectors: vec![vector.clone()],
                cls: row.cls.clone(),
                ..Default::default()
            };
            join(
                &mut group,
                row.det_id,
                row.frame_index,
                row.cls.as_deref(),
                None,
                plate.as_deref(),
                number,
                &mut assignment,
                burst,
            );
            groups.push(group);
            in_burst.push(groups.len() - 1);
        }
    }

    merge_on_plates(&mut groups, &mut assignment);
    assignment
}

/// `_merge_on_plates`: join groups that read the same plate, allowing for the
/// characters a reader confuses, as long as frames are disjoint.
fn merge_on_plates(groups: &mut [Group], assignment: &mut HashMap<i64, i64>) {
    let mut merged: HashMap<i64, i64> = HashMap::new();
    for i in 0..groups.len() {
        if groups[i].plates.is_empty() || merged.contains_key(&groups[i].key) {
            continue;
        }
        for j in (i + 1)..groups.len() {
            if groups[j].plates.is_empty() || merged.contains_key(&groups[j].key) {
                continue;
            }
            if !groups[i].frames.is_disjoint(&groups[j].frames) {
                continue; // same photo cannot hold the same car twice
            }
            if let (Some(ref ci), Some(ref cj)) = (&groups[i].cls, &groups[j].cls) {
                if ci != cj {
                    continue;
                }
            }
            let should_merge = groups[j]
                .plates
                .iter()
                .any(|p| groups[i].plates.contains(p) || nearly_seen(Some(p), &groups[i].plates));
            if !should_merge {
                continue;
            }
            merged.insert(groups[j].key, groups[i].key);
            // Grows `groups[i]` in place, so a later `other` in this same
            // pass sees the merge -- one wrong read must not survive simply
            // because it was not the first other group examined.
            let (head, tail) = groups.split_at_mut(j);
            let (gi, gj) = (&mut head[i], &tail[0]);
            gi.plates.extend(gj.plates.iter().cloned());
            gi.numbers.extend(gj.numbers.iter().cloned());
            gi.bursts.extend(gj.bursts.iter().cloned());
            gi.frames.extend(gj.frames.iter().cloned());
            gi.members.extend(gj.members.iter().cloned());
            if gi.cls.is_none() {
                gi.cls = gj.cls.clone();
            }
        }
    }
    if merged.is_empty() {
        return;
    }
    for key in assignment.values_mut() {
        let mut seen = HashSet::new();
        while let Some(&next) = merged.get(key) {
            if !seen.insert(*key) {
                break;
            }
            *key = next;
        }
    }
}

// --- cluster (dHash/colour signature) ---------------------------------------

/// One row of `cluster`'s input: a detection, its signature, and whatever
/// else was read off it by the time grouping runs.
#[derive(Debug, Clone, Default)]
pub struct SignatureRow {
    pub det_id: i64,
    pub signature: String,
    pub frame_index: i64,
    pub swatch: Option<String>,
    pub cls: Option<String>,
    pub make: Option<String>,
    pub plate: Option<String>,
    pub burst: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ClusterOptions {
    pub max_bits: i64,
    pub min_colour: f64,
    pub frame_window: i64,
    pub max_swatch: i64,
}

impl Default for ClusterOptions {
    fn default() -> Self {
        ClusterOptions {
            max_bits: 14,
            min_colour: 0.62,
            frame_window: 6,
            max_swatch: 52,
        }
    }
}

/// Assign each detection to a group by dHash, colour, class, make and frame
/// proximity, all subject to a read plate. See `conrod/grouping.py::cluster`
/// for what each signal is worth and why the gates are ordered as they are.
pub fn cluster(rows: &[SignatureRow], options: &ClusterOptions) -> HashMap<i64, i64> {
    let ClusterOptions {
        max_bits,
        min_colour,
        frame_window,
        max_swatch,
    } = *options;
    let mut groups: Vec<Group> = Vec::new();
    let mut assignment: HashMap<i64, i64> = HashMap::new();

    for row in rows {
        if row.signature.is_empty() {
            continue;
        }
        let plate = tidy_plate(row.plate.as_deref());
        let mut joined = false;

        for group in &mut groups {
            if group.frames.contains(&row.frame_index) {
                continue;
            }
            let same_burst = row.burst.is_some_and(|b| group.bursts.contains(&b));

            let verdict = plate_verdict(plate.as_deref(), &group.plates);
            if verdict == Some(false) && !same_burst {
                continue;
            }
            if verdict == Some(true) {
                join(
                    group,
                    row.det_id,
                    row.frame_index,
                    row.cls.as_deref(),
                    row.make.as_deref(),
                    plate.as_deref(),
                    None,
                    &mut assignment,
                    row.burst,
                );
                joined = true;
                break;
            }

            let near = nearly_seen(plate.as_deref(), &group.plates);

            if let (Some(c), Some(gc)) = (given(row.cls.as_deref()), given(group.cls.as_deref())) {
                if c != gc {
                    continue;
                }
            }
            if !near && !same_burst && !same_make(row.make.as_deref(), group.make.as_deref()) {
                continue;
            }
            if !colour_matches(&row.signature, &group.signature, min_colour) {
                continue;
            }
            if !swatch_matches(row.swatch.as_deref(), group.swatch.as_deref(), max_swatch) {
                continue;
            }

            let crosses_burst = row.burst.is_some()
                && !group.bursts.is_empty()
                && !row.burst.is_some_and(|b| group.bursts.contains(&b));
            let make_agrees = given(row.make.as_deref()).is_some()
                && given(group.make.as_deref()).is_some()
                && same_make(row.make.as_deref(), group.make.as_deref());
            let swatch_agrees = given(row.swatch.as_deref()).is_some()
                && given(group.swatch.as_deref()).is_some()
                && swatch_matches(row.swatch.as_deref(), group.swatch.as_deref(), max_swatch);
            let corroborated = make_agrees || swatch_agrees;

            let mut shape_agrees = shape_distance(&row.signature, &group.signature) <= max_bits;
            let mut nearby = (row.frame_index - group.last_frame).abs() <= frame_window;
            if crosses_burst && !corroborated {
                shape_agrees = false;
                nearby = false;
            }

            if shape_agrees || nearby || near || same_burst {
                join(
                    group,
                    row.det_id,
                    row.frame_index,
                    row.cls.as_deref(),
                    row.make.as_deref(),
                    plate.as_deref(),
                    None,
                    &mut assignment,
                    row.burst,
                );
                joined = true;
                break;
            }
        }

        if !joined {
            let key = groups.len() as i64 + 1;
            let mut group = Group {
                key,
                members: vec![row.det_id],
                signature: row.signature.clone(),
                last_frame: row.frame_index,
                swatch: row.swatch.clone(),
                cls: row.cls.clone(),
                make: row.make.clone(),
                ..Default::default()
            };
            group.frames.insert(row.frame_index);
            if let Some(p) = &plate {
                group.plates.insert(p.clone());
            }
            if let Some(b) = row.burst {
                group.bursts.insert(b);
            }
            assignment.insert(row.det_id, key);
            groups.push(group);
        }
    }
    assignment
}

// --- plates ------------------------------------------------------------------

/// A plate reduced to what can be compared: letters and digits only.
pub fn tidy_plate(value: Option<&str>) -> Option<String> {
    let value = given(value)?;
    let cleaned: String = value
        .to_uppercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    given(Some(&cleaned)).map(str::to_string)
}

/// Character pairs a plate reader confuses. Not a general edit distance:
/// these are the substitutions that actually happen on a photographed plate.
const CONFUSABLE: [&str; 11] = [
    "047", "8B", "0OQD", "1IL", "5S", "2Z", "6G", "VY", "MN", "CG", "UV",
];

/// One character apart, and that character is one a reader confuses.
pub fn near_plate(a: &str, b: &str) -> bool {
    let (ac, bc): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if ac.len() != bc.len() {
        return false;
    }
    let differences: Vec<(char, char)> = ac
        .iter()
        .zip(&bc)
        .filter(|(x, y)| x != y)
        .map(|(&x, &y)| (x, y))
        .collect();
    if differences.len() != 1 {
        return false;
    }
    let (x, y) = differences[0];
    CONFUSABLE
        .iter()
        .any(|pair| pair.contains(x) && pair.contains(y))
}

/// Whether this plate is one confusable character from one already seen.
pub fn nearly_seen(plate: Option<&str>, seen: &HashSet<String>) -> bool {
    let Some(plate) = given(plate) else {
        return false;
    };
    if seen.is_empty() {
        return false;
    }
    seen.iter().any(|other| near_plate(plate, other))
}

/// `Some(true)`: same vehicle. `Some(false)`: a different one. `None`: no
/// opinion -- a near miss is not proof of either, so it abstains and lets the
/// measured signals in [`cluster`] decide.
pub fn plate_verdict(plate: Option<&str>, seen: &HashSet<String>) -> Option<bool> {
    let plate = given(plate)?;
    if seen.is_empty() {
        return None;
    }
    if seen.contains(plate) {
        return Some(true);
    }
    if nearly_seen(Some(plate), seen) {
        return None;
    }
    Some(false)
}

/// Two crops may be one vehicle unless they named different makes. An absent
/// make on either side is not a mismatch.
pub fn same_make(a: Option<&str>, b: Option<&str>) -> bool {
    match (given(a), given(b)) {
        (Some(a), Some(b)) => a.trim().to_lowercase() == b.trim().to_lowercase(),
        _ => true,
    }
}

// --- paint -------------------------------------------------------------------

/// How far two hues may sit apart and still be called the same paint, of 1.0
/// around the wheel.
const HUE_TOLERANCE: f64 = 0.075;

/// `#rrggbb` -> its channels, or `None` for anything that is not one.
pub fn rgb(value: &str) -> Option<(u8, u8, u8)> {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    let mut out = [0u8; 3];
    for (i, slot) in out.iter_mut().enumerate() {
        let start = (1 + i * 2).min(len);
        let end = (3 + i * 2).min(len);
        let text: String = chars[start..end].iter().collect();
        *slot = u8::from_str_radix(&text, 16).ok()?;
    }
    Some((out[0], out[1], out[2]))
}

/// `colorsys.rgb_to_hsv`, transcribed rather than reinvented so its edge
/// cases (grey, black, white) match Python's exactly.
fn rgb_to_hsv(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let v = maxc;
    if minc == maxc {
        return (0.0, 0.0, v);
    }
    let rangec = maxc - minc;
    let s = rangec / maxc;
    let rc = (maxc - r) / rangec;
    let gc = (maxc - g) / rangec;
    let bc = (maxc - b) / rangec;
    let h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };
    let h = (h / 6.0).rem_euclid(1.0);
    (h, s, v)
}

/// Whether two sampled paint colours are close enough to be one car, compared
/// by hue rather than distance in RGB so exposure does not split one colour
/// into two. `max_swatch` is accepted for symmetry with its callers but,
/// like the Python it is ported from, unused: the comparison moved to hue
/// and never took the limit back out of the signature.
pub fn swatch_matches(a: Option<&str>, b: Option<&str>, _max_swatch: i64) -> bool {
    let (Some(a), Some(b)) = (given(a), given(b)) else {
        return true;
    };
    let (Some(pa), Some(pb)) = (rgb(a), rgb(b)) else {
        return true;
    };

    let (ha, sa, va) = rgb_to_hsv(
        f64::from(pa.0) / 255.0,
        f64::from(pa.1) / 255.0,
        f64::from(pa.2) / 255.0,
    );
    let (hb, sb, vb) = rgb_to_hsv(
        f64::from(pb.0) / 255.0,
        f64::from(pb.1) / 255.0,
        f64::from(pb.2) / 255.0,
    );

    // Neither has a usable hue: black, white, silver, grey. Compare lightness
    // instead, generously, because exposure moves this a lot.
    if sa < 0.18 && sb < 0.18 {
        return (va - vb).abs() <= 0.45;
    }
    // One is coloured and the other is not. That is a real difference.
    if (sa < 0.18) != (sb < 0.18) {
        return false;
    }
    let apart = (ha - hb).abs();
    apart.min(1.0 - apart) <= HUE_TOLERANCE // hue is a circle
}

// --- an insertion-ordered counter --------------------------------------------

/// `collections.Counter`, close enough for this module: increments never
/// move a key, and [`OrderedCounter::most_common`] breaks ties by insertion
/// order the way Python's stable sort does.
#[derive(Debug, Clone, Default)]
struct OrderedCounter<T: Eq + std::hash::Hash + Clone> {
    order: Vec<T>,
    counts: HashMap<T, usize>,
}

impl<T: Eq + std::hash::Hash + Clone> OrderedCounter<T> {
    fn add(&mut self, key: T, n: usize) {
        match self.counts.get_mut(&key) {
            Some(c) => *c += n,
            None => {
                self.counts.insert(key.clone(), n);
                self.order.push(key);
            }
        }
    }

    fn most_common(&self) -> Vec<(T, usize)> {
        let mut out: Vec<(T, usize)> = self
            .order
            .iter()
            .map(|k| (k.clone(), self.counts[k]))
            .collect();
        out.sort_by_key(|(_, count)| std::cmp::Reverse(*count)); // stable: ties keep insertion order
        out
    }
}

// --- Consensus ---------------------------------------------------------------

/// Below this level of agreement the group has no answer, only a
/// disagreement.
const MIN_AGREEMENT: f64 = 0.5;
/// A make on its own is a weaker claim than a full name, so it needs a
/// clearer majority before it is worth writing down.
const MIN_MAKE_AGREEMENT: f64 = 0.6;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Consensus {
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub race_number: Option<String>,
    pub plate: Option<String>,
    /// Sampled paint, not the model's word for the colour.
    pub colour_hex: Option<String>,
    pub team: Option<String>,
    pub driver: Option<String>,
    pub country: Option<String>,
    pub sponsors: Vec<String>,
    pub livery_text: Vec<String>,
    /// How much of the group backed the winning name.
    pub agreement: f64,
    pub size: usize,
    pub disputed: Vec<String>,
    /// True when the name came from looking at the pictures again rather
    /// than from the per-frame readings. Not produced here -- that is
    /// `_second_look`'s job in Python -- but kept so a caller can set it.
    pub second_look: bool,
}

/// The channel-wise median of several `#rrggbb` strings.
///
/// Faithful to a quirk in the Python: parsing a malformed value can append to
/// one channel and then fail on the next, leaving that channel's list one
/// entry longer than its neighbours -- so the three medians are not
/// necessarily taken over the same set of readings. Reproduced rather than
/// fixed, because this ports what Python actually computed, not what it
/// meant to.
pub fn median_hex(values: &[String]) -> Option<String> {
    let mut channels: [Vec<u32>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for value in values {
        let chars: Vec<char> = value.chars().collect();
        let len = chars.len();
        for (i, channel) in channels.iter_mut().enumerate() {
            let start = (1 + i * 2).min(len);
            let end = (3 + i * 2).min(len);
            let text: String = chars[start..end].iter().collect();
            match u32::from_str_radix(&text, 16) {
                Ok(v) => channel.push(v),
                Err(_) => break,
            }
        }
    }
    if channels[0].is_empty() {
        return None;
    }
    let mut middle = [0u32; 3];
    for (i, channel) in channels.iter().enumerate() {
        let mut sorted = channel.clone();
        sorted.sort_unstable();
        middle[i] = sorted[sorted.len() / 2];
    }
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        middle[0], middle[1], middle[2]
    ))
}

/// Levenshtein distance, abandoned once it passes `limit`. Returns
/// `limit + 1` for anything further apart.
pub fn edit_distance(a: &str, b: &str, limit: usize) -> usize {
    let (ac, bc): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if ac.len().abs_diff(bc.len()) > limit {
        return limit + 1;
    }
    let mut previous: Vec<usize> = (0..=bc.len()).collect();
    for (i, &x) in ac.iter().enumerate() {
        let i = i + 1;
        let mut current: Vec<usize> = Vec::with_capacity(bc.len() + 1);
        current.push(i);
        for (j, &y) in bc.iter().enumerate() {
            let j = j + 1;
            let cost = usize::from(x != y);
            current.push(
                (previous[j] + 1)
                    .min(current[j - 1] + 1)
                    .min(previous[j - 1] + cost),
            );
        }
        if *current.iter().min().unwrap() > limit {
            return limit + 1;
        }
        previous = current;
    }
    *previous.last().unwrap()
}

/// When one spelling is a misreading of another. Two conditions have to hold
/// together: close enough to be the same word, and rare enough against it to
/// be a mistake rather than a second decal.
const VARIANT_RULES: [(usize, usize, usize); 2] = [(1, 4, 3), (2, 5, 10)];

fn is_variant(text: &str, count: usize, kept: &str, kept_count: usize) -> bool {
    for (edits, min_length, ratio) in VARIANT_RULES {
        let short_enough = text.chars().count().min(kept.chars().count()) >= min_length;
        if short_enough && count * ratio <= kept_count && edit_distance(text, kept, edits) <= edits
        {
            return true;
        }
    }
    false
}

/// Merge misreadings of one decal into the spelling most frames agreed on.
/// Only the rare spelling moves, and only towards a much commoner one.
fn fold_variants(counts: &OrderedCounter<String>) -> OrderedCounter<String> {
    let mut folded: OrderedCounter<String> = OrderedCounter::default();
    for (text, count) in counts.most_common() {
        let kept = folded
            .order
            .iter()
            .find(|k| is_variant(&text, count, k, folded.counts[*k]))
            .cloned();
        match kept {
            Some(kept) => folded.add(kept, count),
            None => folded.add(text, count),
        }
    }
    folded
}

/// Every distinct string any frame in the group saw under `key`, commonest
/// first. `key` is typically `"sponsors"` or `"livery_text"`.
pub fn accumulate(members: &[Map<String, Value>], key: &str) -> Vec<String> {
    let mut counts: OrderedCounter<String> = OrderedCounter::default();
    let mut original: HashMap<String, String> = HashMap::new();
    for member in members {
        let seen: Vec<Value> = match member.get(key) {
            Some(Value::String(s)) => vec![Value::String(s.clone())],
            Some(Value::Array(a)) => a.clone(),
            _ => Vec::new(),
        };
        for value in seen {
            let text = crate::py::str_of(&value).trim().to_string();
            if text.is_empty() {
                continue;
            }
            let lower = text.to_lowercase();
            counts.add(lower.clone(), 1);
            original.entry(lower).or_insert(text);
        }
    }
    fold_variants(&counts)
        .most_common()
        .into_iter()
        .map(|(k, _)| original[&k].clone())
        .collect()
}

/// The majority answer among `values`, and how much of the present ones
/// agreed. Absent or blank entries are ignored rather than counted against
/// it.
pub fn vote(values: &[Option<String>]) -> (Option<String>, f64) {
    let present: Vec<String> = values
        .iter()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if present.is_empty() {
        return (None, 0.0);
    }
    let mut counts: OrderedCounter<String> = OrderedCounter::default();
    for p in &present {
        counts.add(p.to_lowercase(), 1);
    }
    let (top, hits) = counts.most_common().into_iter().next().unwrap();
    match present.iter().find(|v| v.to_lowercase() == top) {
        Some(value) => (Some(value.clone()), hits as f64 / present.len() as f64),
        None => (None, 0.0),
    }
}

/// The fields a frame reads for itself and a group can later overrule.
pub const OWN_FIELDS: [&str; 3] = ["make", "model", "colour"];

/// Keep what this frame's own reader said, before the group overwrites it.
/// Never records the absence of an answer, and never overwrites a reading
/// already stored -- see `conrod/grouping.py` for why both matter.
pub fn remember_own_reading(current: &mut Map<String, Value>) {
    for field in OWN_FIELDS {
        let Some(value) = current.get(field).cloned() else {
            continue;
        };
        if !truthy(Some(&value)) {
            continue;
        }
        let own_key = format!("own_{field}");
        if !truthy(current.get(&own_key)) {
            current.insert(own_key, value);
        }
    }
}

/// Vote on what each frame read, not on a previous round's group answer.
/// A blank `own_` value is not a reading, so it falls through to what the
/// frame currently says.
pub fn use_own_reading(parsed: &mut Map<String, Value>) {
    for field in OWN_FIELDS {
        let own_key = format!("own_{field}");
        if let Some(value) = parsed.get(&own_key).cloned() {
            if truthy(Some(&value)) {
                parsed.insert(field.to_string(), value);
            }
        }
    }
}

fn text_field(member: &Map<String, Value>, key: &str) -> Option<String> {
    match member.get(key) {
        Some(v) if truthy(Some(v)) => Some(crate::py::str_of(v).trim().to_string()),
        _ => None,
    }
}

/// The group's agreed identity. See `conrod/grouping.py::consensus` for why
/// make and model are voted as one unit while plates and race numbers are
/// read rather than guessed.
pub fn consensus(members: &[Map<String, Value>]) -> Consensus {
    let mut out = Consensus {
        size: members.len(),
        ..Default::default()
    };

    let pairs: Vec<(String, String)> = members
        .iter()
        .map(|m| {
            (
                text_field(m, "make").unwrap_or_default(),
                text_field(m, "model").unwrap_or_default(),
            )
        })
        .collect();
    let named: Vec<(String, String)> = pairs
        .into_iter()
        .filter(|(a, b)| !a.is_empty() || !b.is_empty())
        .collect();

    if !named.is_empty() {
        let mut counter: OrderedCounter<(String, String)> = OrderedCounter::default();
        for (a, b) in &named {
            counter.add((a.to_lowercase(), b.to_lowercase()), 1);
        }
        let ((top_make, top_model), hits) = counter.most_common().into_iter().next().unwrap();
        for (make, model) in &named {
            if make.to_lowercase() == top_make && model.to_lowercase() == top_model {
                out.make = (!make.is_empty()).then(|| make.clone());
                out.model = (!model.is_empty()).then(|| model.clone());
                break;
            }
        }
        out.agreement = hits as f64 / named.len() as f64;

        if out.agreement < MIN_AGREEMENT {
            // Eight frames of one Falcon came back as a Fiesta, an Astra, a
            // Commodore and a Mustang. The majority of that is still noise,
            // so report the disagreement instead of writing the plurality in.
            let disputed: std::collections::BTreeSet<String> = named
                .iter()
                .map(|(a, b)| format!("{a} {b}").trim().to_string())
                .collect();
            out.disputed = disputed.into_iter().collect();
            out.make = None;
            out.model = None;

            // The full name is noise, but the make on its own may not be.
            let makes: Vec<&String> = named
                .iter()
                .map(|(a, _)| a)
                .filter(|a| !a.is_empty())
                .collect();
            if !makes.is_empty() {
                let mut make_counter: OrderedCounter<String> = OrderedCounter::default();
                for m in &makes {
                    make_counter.add(m.to_lowercase(), 1);
                }
                let (top, hits) = make_counter.most_common().into_iter().next().unwrap();
                if hits as f64 / named.len() as f64 >= MIN_MAKE_AGREEMENT {
                    out.make = makes
                        .iter()
                        .find(|m| m.to_lowercase() == top)
                        .map(|s| (*s).clone());
                    out.agreement = hits as f64 / named.len() as f64;
                }
            }
        }
    }

    out.colour = vote(
        &members
            .iter()
            .map(|m| text_field(m, "colour"))
            .collect::<Vec<_>>(),
    )
    .0;

    // The swatch is measured per frame, so take the middle one rather than
    // voting: a single frame where the crop caught mostly windscreen or kerb
    // is then outvoted by the rest of the group instead of standing alone.
    let swatches: Vec<String> = members
        .iter()
        .filter_map(|m| text_field(m, "colour_hex"))
        .collect();
    if !swatches.is_empty() {
        out.colour_hex = median_hex(&swatches);
    }

    // Sponsor and livery text is accumulated, not voted: each frame only sees
    // the panels facing the camera, so a majority vote would throw away
    // whichever side of the car was photographed less.
    out.team = vote(
        &members
            .iter()
            .map(|m| text_field(m, "team"))
            .collect::<Vec<_>>(),
    )
    .0;
    out.driver = vote(
        &members
            .iter()
            .map(|m| text_field(m, "driver"))
            .collect::<Vec<_>>(),
    )
    .0;
    out.country = vote(
        &members
            .iter()
            .map(|m| text_field(m, "country"))
            .collect::<Vec<_>>(),
    )
    .0;
    out.sponsors = accumulate(members, "sponsors");
    out.livery_text = accumulate(members, "livery_text");

    for (field_name, conf_key) in [("plate", "plate_conf"), ("race_number", "number_conf")] {
        let mut best: Option<String> = None;
        let mut best_conf = 0.0_f64;
        for m in members {
            let Some(value) = m.get(field_name).filter(|v| truthy(Some(v))) else {
                continue;
            };
            let conf = match m.get(conf_key) {
                Some(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
                Some(Value::String(s)) => crate::py::float(s).unwrap_or(0.0),
                _ => 0.0,
            };
            if conf > best_conf {
                best = Some(crate::py::str_of(value));
                best_conf = conf;
            }
        }
        match field_name {
            "plate" => out.plate = best,
            "race_number" => out.race_number = best,
            _ => unreachable!(),
        }
    }
    out
}

/// Letters and digits only, lowercased -- "Harley-Davidson" == "harley
/// davidson".
pub fn plain(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Every make the frames' own readers actually named, folded for spelling.
pub fn proposed_makes(members: &[Map<String, Value>]) -> HashSet<String> {
    members
        .iter()
        .filter_map(|m| {
            let own = m.get("own_make").filter(|v| truthy(Some(v)));
            let make = own.or_else(|| m.get("make")).filter(|v| truthy(Some(v)))?;
            Some(plain(&crate::py::str_of(make)))
        })
        .collect()
}
