//! Cars this photographer has already met.
//!
//! `load`, `remember`, `seed`, `count` and `forget` are SQLite access and
//! live elsewhere; what lives here is the pure logic around them -- `fill`
//! only ever fills a blank, `to_csv`/`from_csv` become plain text in/out over
//! a `Row`, and `agreed`/`majority` turn a car's frames into one settled
//! reading without touching a database.

use crate::analysis::{given, VehicleAnalysis};
use crate::text::{is_truthy, value_to_string, Tally};
use serde_json::{Map, Value};
use std::collections::HashMap;

/// The fields a row holds, beyond the plate -- deliberately the fields
/// `VehicleAnalysis` already has, so seeding and answering both need no
/// translation.
pub const FIELDS: [&str; 7] = [
    "make",
    "model",
    "colour",
    "body_type",
    "team",
    "sponsors",
    "race_number",
];

pub const COLUMNS: [&str; 8] = [
    "plate",
    "make",
    "model",
    "colour",
    "body_type",
    "team",
    "sponsors",
    "race_number",
];

/// The key a plate is stored under: case and punctuation are noise, and
/// nothing else is corrected, because correcting one character is how a
/// lookup finds the wrong car.
pub fn normalise(plate: Option<&str>) -> String {
    let Some(plate) = plate.filter(|p| !p.is_empty()) else {
        return String::new();
    };
    plate
        .to_uppercase()
        .chars()
        .filter(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        .collect()
}

/// One known car's row, as CSV and (in spirit) the `known_vehicles` table
/// hold it: plain text columns, `sponsors` included -- a comma-joined
/// string, not yet split into a list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Row {
    pub plate: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub body_type: Option<String>,
    pub team: Option<String>,
    pub sponsors: Option<String>,
    pub race_number: Option<String>,
}

/// One known car, shaped the way `fill()` reads it: `sponsors` a list,
/// because a list field is blank when it is empty, not when it is `None`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KnownVehicle {
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub body_type: Option<String>,
    pub team: Option<String>,
    pub sponsors: Vec<String>,
    pub race_number: Option<String>,
}

impl KnownVehicle {
    /// The per-row transform `load()` applies before handing a row to
    /// `fill()`: `sponsors` goes from a comma-joined column to a list.
    pub fn from_row(row: &Row) -> KnownVehicle {
        KnownVehicle {
            make: row.make.clone(),
            model: row.model.clone(),
            colour: row.colour.clone(),
            body_type: row.body_type.clone(),
            team: row.team.clone(),
            sponsors: split_text(row.sponsors.as_deref().unwrap_or("")),
            race_number: row.race_number.clone(),
        }
    }
}

/// Fill this vehicle's blanks from what the plate is known to be. Returns
/// whether anything was filled. Never overwrites: what was read on the day
/// always wins.
pub fn fill(analysis: &mut VehicleAnalysis, known: &HashMap<String, KnownVehicle>) -> bool {
    if known.is_empty() {
        return false;
    }
    let Some(entry) = known.get(&normalise(analysis.plate.as_deref())) else {
        return false;
    };

    let mut filled = false;
    filled |= fill_text(&entry.make, &mut analysis.make);
    filled |= fill_text(&entry.model, &mut analysis.model);
    filled |= fill_text(&entry.colour, &mut analysis.colour);
    filled |= fill_text(&entry.body_type, &mut analysis.body_type);
    filled |= fill_text(&entry.team, &mut analysis.team);
    if !entry.sponsors.is_empty() && analysis.sponsors.is_empty() {
        analysis.sponsors = entry.sponsors.clone();
        filled = true;
    }
    filled |= fill_text(&entry.race_number, &mut analysis.race_number);
    filled
}

/// One text field of `fill()`: fill `current` from `remembered` only when
/// `current` is blank, and say whether it changed anything.
fn fill_text(remembered: &Option<String>, current: &mut Option<String>) -> bool {
    match given(remembered) {
        Some(value) if given(current).is_none() => {
            *current = Some(value.to_string());
            true
        }
        _ => false,
    }
}

/// Character pairs a plate reader confuses -- the same list `grouping`
/// uses for its own near-plate test, not re-derived here.
const CONFUSABLE: [&str; 11] = [
    "047", "8B", "0OQD", "1IL", "5S", "2Z", "6G", "VY", "MN", "CG", "UV",
];

/// Whether two plates are one plate read two ways: the same length, one
/// character apart, and that character is one a reader actually confuses.
pub fn near_plate(a: &str, b: &str) -> bool {
    if a.chars().count() != b.chars().count() {
        return false;
    }
    let diffs: Vec<(char, char)> = a.chars().zip(b.chars()).filter(|(x, y)| x != y).collect();
    let [(x, y)] = diffs[..] else {
        return false;
    };
    CONFUSABLE
        .iter()
        .any(|pair| pair.contains(x) && pair.contains(y))
}

/// One car's identity, out of every frame of it -- what `seed()` hands to
/// `remember()`. A plain struct rather than `VehicleAnalysis`: seeding reads
/// stored JSON and has no need for the rest of that type's behaviour.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reading {
    pub plate: String,
    pub aliases: Vec<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub body_type: Option<String>,
    pub team: Option<String>,
    pub sponsors: Option<Vec<String>>,
    pub race_number: Option<String>,
}

impl Reading {
    /// `_Reading.__init__`: shape a stored `attributes` dict into a reading,
    /// falling back to a detection's own competition number when the parsed
    /// attributes have none.
    pub fn from_parsed(plate: &str, parsed: &Map<String, Value>, number: Option<&str>) -> Reading {
        let text = |key: &str| match parsed.get(key) {
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
        let sponsors = match parsed.get("sponsors") {
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
            ),
            _ => None,
        };
        let mut reading = Reading {
            plate: plate.to_string(),
            aliases: Vec::new(),
            make: text("make"),
            model: text("model"),
            colour: text("colour"),
            body_type: text("body_type"),
            team: text("team"),
            sponsors,
            race_number: text("race_number"),
        };
        if given(&reading.race_number).is_none() {
            reading.race_number = number.map(str::to_string);
        }
        reading
    }
}

/// One detection belonging to a car being seeded: the plate as read on this
/// frame, and the parsed `attributes` JSON.
#[derive(Debug, Clone)]
pub struct Member {
    pub plate: Option<String>,
    pub attributes: Map<String, Value>,
}

/// One car's settled identity, out of every frame of it. Make and model
/// come from grouping's own vote (left blank where it could not settle);
/// everything else is a plain majority of the frames that offered a value.
pub fn agreed(members: &[Member]) -> Option<Reading> {
    let mut plates: Tally<String> = Tally::new();
    for member in members {
        let key = normalise(member.plate.as_deref());
        if !key.is_empty() {
            plates.add(key, 1);
        }
    }
    if plates.is_empty() {
        return None;
    }
    let best = plates.ranked()[0].0.clone();
    // Only readings plausibly the same plate misread -- grouping's own
    // near-plate test, not a general fuzzy match.
    let aliases: Vec<String> = plates
        .keys()
        .iter()
        .filter(|p| **p != best && near_plate(p, &best))
        .cloned()
        .collect();

    let first = &members[0].attributes;
    // `first.get("group_make") or _majority(...)`: falls back to a majority
    // vote only when grouping's own verdict is missing or empty.
    let group_make = match first.get("group_make") {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    };
    // `reading.model = first.get("group_model")`: a bare assignment, so an
    // empty string (unlike group_make) is kept as-is rather than falling
    // back to anything.
    let group_model = match first.get("group_model") {
        Some(Value::String(s)) => Some(s.clone()),
        _ => None,
    };

    Some(Reading {
        plate: best,
        aliases,
        make: group_make.or_else(|| majority(members, "make")),
        model: group_model,
        colour: majority(members, "colour"),
        body_type: majority(members, "body_type"),
        team: majority(members, "team"),
        sponsors: majority_sponsors(members),
        race_number: majority(members, "race_number"),
    })
}

fn field_votes(members: &[Member], field: &str) -> Tally<String> {
    let mut votes = Tally::new();
    let count_value = |value: &Value, votes: &mut Tally<String>| {
        if is_truthy(value) {
            votes.add(value_to_string(value).trim().to_string(), 1);
        }
    };
    for member in members {
        match member.attributes.get(field) {
            Some(Value::Array(items)) => {
                for item in items {
                    count_value(item, &mut votes);
                }
            }
            Some(value) => count_value(value, &mut votes),
            None => {}
        }
    }
    votes
}

/// The value most of this car's frames gave for one (non-list) field.
pub fn majority(members: &[Member], field: &str) -> Option<String> {
    let votes = field_votes(members, field);
    if votes.is_empty() {
        return None;
    }
    Some(votes.ranked()[0].0.clone())
}

/// Like [`majority`], but returns the whole ranked list for this one field
/// rather than a single winner, which is why it gets its own return type.
pub fn majority_sponsors(members: &[Member]) -> Option<Vec<String>> {
    let votes = field_votes(members, "sponsors");
    if votes.is_empty() {
        return None;
    }
    Some(votes.ranked().into_iter().map(|(s, _)| s).collect())
}

/// Comma-separated text, split and trimmed.
fn split_text(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

/// `_split`: comma-separated text, or a list already split (returned as
/// its items' plain text, since Rust has no equivalent of handing back a
/// list of whatever type its items happened to be).
pub fn split_field(value: &Value) -> Vec<String> {
    if !is_truthy(value) {
        return Vec::new();
    }
    if let Value::Array(items) = value {
        return items.iter().map(value_to_string).collect();
    }
    split_text(&value_to_string(value))
}

/// One field, as it goes into a text column -- `_text`. A list is joined
/// with commas (used for `sponsors`, the one list field `remember()`
/// stores); anything else becomes its plain text, or `None` if that is
/// blank.
pub fn text_of(value: &Value) -> Option<String> {
    if value.is_null() {
        return None;
    }
    if let Value::Array(items) = value {
        let joined: Vec<String> = items
            .iter()
            .map(|v| value_to_string(v).trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let joined = joined.join(", ");
        return if joined.is_empty() {
            None
        } else {
            Some(joined)
        };
    }
    let text = value_to_string(value).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// The whole registry as CSV text, for editing elsewhere and loading back --
/// the pure part of `to_csv`, taking rows already read from the database.
pub fn to_csv(rows: &[Row]) -> String {
    let mut sorted: Vec<&Row> = rows.iter().collect();
    sorted.sort_by(|a, b| a.plate.cmp(&b.plate));

    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::Any(b'\n'))
        .from_writer(Vec::new());
    writer
        .write_record(COLUMNS)
        .expect("write to a Vec cannot fail");
    for row in sorted {
        writer
            .write_record([
                row.plate.as_str(),
                row.make.as_deref().unwrap_or(""),
                row.model.as_deref().unwrap_or(""),
                row.colour.as_deref().unwrap_or(""),
                row.body_type.as_deref().unwrap_or(""),
                row.team.as_deref().unwrap_or(""),
                row.sponsors.as_deref().unwrap_or(""),
                row.race_number.as_deref().unwrap_or(""),
            ])
            .expect("write to a Vec cannot fail");
    }
    String::from_utf8(writer.into_inner().expect("flush to a Vec cannot fail"))
        .expect("csv writer only emits UTF-8 given UTF-8 input")
}

/// Parse a registry CSV, normalising each row's plate and blanking any cell
/// that was empty. Returns the parsed rows and how many lines were skipped
/// for having no usable plate -- the pure parsing half of `from_csv`; the
/// database read/write that follows for each row is not here.
pub fn parse_csv(text: &str) -> Result<(Vec<Row>, usize), String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut records = reader.records();
    let header: Vec<String> = match records.next() {
        Some(Ok(record)) => record.iter().map(|h| h.trim().to_lowercase()).collect(),
        Some(Err(e)) => return Err(e.to_string()),
        None => return Err("that file has no 'plate' column".to_string()),
    };
    if !header.iter().any(|h| h == "plate") {
        return Err("that file has no 'plate' column".to_string());
    }

    let mut rows = Vec::new();
    let mut skipped = 0usize;
    for record in records {
        let record = record.map_err(|e| e.to_string())?;
        if record.len() == 1 && record.get(0) == Some("") {
            continue; // a blank line -- nothing to read
        }
        // A repeated column name: the LAST one wins, both position and
        // value, matching `dict(zip(fieldnames, row))` folded twice.
        let cell = |name: &str| -> Option<String> {
            let idx = header.iter().enumerate().rfind(|(_, h)| *h == name)?.0;
            record
                .get(idx)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let plate = normalise(cell("plate").as_deref());
        if plate.is_empty() {
            skipped += 1;
            continue;
        }
        rows.push(Row {
            plate,
            make: cell("make"),
            model: cell("model"),
            colour: cell("colour"),
            body_type: cell("body_type"),
            team: cell("team"),
            sponsors: cell("sponsors"),
            race_number: cell("race_number"),
        });
    }
    Ok((rows, skipped))
}

/// Merge a CSV-supplied row over what is already known: what is in the file
/// wins, the opposite rule to `remember()` -- a blank cell is left alone
/// rather than erasing what was there.
pub fn merge_csv_row(given: &Row, existing: Option<&Row>) -> Row {
    let Some(existing) = existing else {
        return given.clone();
    };
    Row {
        plate: given.plate.clone(),
        make: given.make.clone().or_else(|| existing.make.clone()),
        model: given.model.clone().or_else(|| existing.model.clone()),
        colour: given.colour.clone().or_else(|| existing.colour.clone()),
        body_type: given
            .body_type
            .clone()
            .or_else(|| existing.body_type.clone()),
        team: given.team.clone().or_else(|| existing.team.clone()),
        sponsors: given.sponsors.clone().or_else(|| existing.sponsors.clone()),
        race_number: given
            .race_number
            .clone()
            .or_else(|| existing.race_number.clone()),
    }
}
