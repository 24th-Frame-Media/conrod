//! Race entry list: car number to team, drivers, car and sponsors.
//!
//! Optional per album. Loaded from a CSV with a header row, or a JSON list
//! (bare, or under `"entries"`). Lookup ignores leading zeros, so "007"
//! and "7" name the same car.

use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entry {
    pub number: String,
    pub team: Option<String>,
    pub drivers: Vec<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub sponsors: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EntryList {
    entries: Vec<Entry>,
}

pub fn load(path: &Path) -> Result<EntryList, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text)
}

/// JSON when the text opens with a bracket, CSV otherwise.
pub fn parse(text: &str) -> Result<EntryList, String> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    if text.starts_with('{') || text.starts_with('[') {
        parse_json(text)
    } else {
        parse_csv(text)
    }
}

pub fn parse_csv(text: &str) -> Result<EntryList, String> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(text.as_bytes());
    let headers = reader.headers().map_err(|e| e.to_string())?.clone();
    let column = |name: &str| headers.iter().position(|h| h.eq_ignore_ascii_case(name));
    let number_col = column("number").ok_or("CSV needs a 'number' column")?;
    let (team_col, drivers_col, make_col, model_col, sponsors_col) = (
        column("team"),
        column("drivers"),
        column("make"),
        column("model"),
        column("sponsors"),
    );
    let mut entries = Vec::new();
    for (row, record) in reader.records().enumerate() {
        let record = record.map_err(|e| e.to_string())?;
        let number = cell(&record, Some(number_col));
        if number.is_empty() {
            return Err(format!("row {}: no car number", row + 2));
        }
        entries.push(Entry {
            number: number.to_string(),
            team: non_empty(cell(&record, team_col)),
            drivers: split_drivers(cell(&record, drivers_col)),
            make: non_empty(cell(&record, make_col)),
            model: non_empty(cell(&record, model_col)),
            sponsors: split_list(cell(&record, sponsors_col)),
        });
    }
    Ok(EntryList { entries })
}

pub fn parse_json(text: &str) -> Result<EntryList, String> {
    let value: Value = serde_json::from_str(text).map_err(|e| format!("bad JSON: {e}"))?;
    let items = match &value {
        Value::Array(items) => items,
        Value::Object(obj) => obj
            .get("entries")
            .and_then(Value::as_array)
            .ok_or("JSON object needs an \"entries\" list")?,
        _ => return Err("JSON must be a list of entries or an object with \"entries\"".into()),
    };
    let entries = items
        .iter()
        .enumerate()
        .map(|(i, item)| json_entry(item).map_err(|e| format!("entry {}: {e}", i + 1)))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EntryList { entries })
}

fn json_entry(item: &Value) -> Result<Entry, String> {
    let obj = item.as_object().ok_or("not an object")?;
    let number = match obj.get("number") {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    };
    if number.is_empty() {
        return Err("no car number".into());
    }
    let car = obj.get("car").and_then(Value::as_object);
    let make = str_of(car.and_then(|c| c.get("make")).or_else(|| obj.get("make")));
    let model = str_of(
        car.and_then(|c| c.get("model"))
            .or_else(|| obj.get("model")),
    );
    let sponsors = match obj.get("sponsors") {
        Some(Value::Array(_)) => strings(obj.get("sponsors")),
        _ => str_of(obj.get("primary_sponsor"))
            .into_iter()
            .chain(strings(obj.get("livery_sponsors")))
            .collect(),
    };
    Ok(Entry {
        number,
        team: str_of(obj.get("team")),
        drivers: strings(obj.get("drivers")),
        make,
        model,
        sponsors,
    })
}

impl EntryList {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The entry for a car number; leading zeros do not matter.
    pub fn get(&self, number: &str) -> Option<&Entry> {
        let key = number_key(number);
        if key.is_empty() {
            return None;
        }
        self.entries.iter().find(|e| number_key(&e.number) == key)
    }

    /// The one entry whose driver names appear in `evidence`, if the match is
    /// unique. Team names alone never decide: one team runs two cars. Driver
    /// surnames of four letters or more and full names count; two entries
    /// sharing a surname tie and return nothing.
    pub fn from_evidence(&self, evidence: &[String]) -> Option<&Entry> {
        let all: Vec<String> = evidence.iter().flat_map(|line| words(line)).collect();
        let text = all.join(" ");
        let mut scored: Vec<(&Entry, usize)> = Vec::new();
        for entry in &self.entries {
            let mut score = 0;
            for driver in &entry.drivers {
                let parts = words(driver);
                let Some(surname) = parts.last() else {
                    continue;
                };
                let full = parts.join(" ");
                if parts.len() > 1 && contains_phrase(&text, &full) {
                    score += 3;
                } else if surname.len() >= 4 && contains_phrase(&text, surname) {
                    score += 2;
                }
            }
            if score > 0 {
                scored.push((entry, score));
            }
        }
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        let (best, top) = *scored.first()?;
        if scored.iter().skip(1).any(|(_, score)| *score == top) {
            return None;
        }
        Some(best)
    }

    /// Distinct "Make Model" strings, sorted.
    pub fn candidates(&self) -> Vec<String> {
        let set: BTreeSet<String> = self
            .entries
            .iter()
            .filter_map(|e| {
                let text = [e.make.as_deref(), e.model.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                (!text.is_empty()).then_some(text)
            })
            .collect();
        set.into_iter().collect()
    }

    /// Distinct sponsor names across every entry, sorted.
    pub fn sponsors(&self) -> Vec<String> {
        let set: BTreeSet<&str> = self
            .entries
            .iter()
            .flat_map(|e| e.sponsors.iter().map(String::as_str))
            .collect();
        set.into_iter().map(str::to_string).collect()
    }
}

/// Digits with leading zeros dropped. A number with no digits compares as typed.
fn number_key(number: &str) -> String {
    let digits: String = number.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return number.trim().to_uppercase();
    }
    let significant = digits.trim_start_matches('0');
    if significant.is_empty() {
        "0".into()
    } else {
        significant.into()
    }
}

/// A line's words, uppercased and stripped of punctuation.
fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_uppercase)
        .collect()
}

/// Whether `phrase` appears as a whole-word run in `text`.
fn contains_phrase(text: &str, phrase: &str) -> bool {
    !phrase.is_empty() && format!(" {text} ").contains(&format!(" {phrase} "))
}

fn cell(record: &csv::StringRecord, col: Option<usize>) -> &str {
    col.and_then(|c| record.get(c)).map_or("", str::trim)
}

fn non_empty(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_string())
}

fn str_of(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::trim).and_then(non_empty)
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|items| items.iter().filter_map(|i| str_of(Some(i))).collect())
        .unwrap_or_default()
}

/// Drivers are separated by ';', '/' or " & ".
fn split_drivers(s: &str) -> Vec<String> {
    split_list(&s.replace(" & ", ";").replace('/', ";"))
}

/// Sponsors, or any other ';'-separated list.
fn split_list(s: &str) -> Vec<String> {
    s.split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BATHURST_EXCERPT: &str = r#"{
  "event": "2026 Repco Bathurst 1000",
  "entries": [
    {"number": "1", "team": "Walkinshaw TWG Racing", "drivers": ["Chaz Mostert", "Fabian Coulthard"], "car": {"make": "Toyota", "model": "GR Supra"}, "primary_sponsor": "Mobil 1", "livery_sponsors": ["Optus"]},
    {"number": "2", "team": "Walkinshaw TWG Racing", "drivers": ["Ryan Wood", "Jaxon Evans"], "car": {"make": "Toyota", "model": "GR Supra"}, "primary_sponsor": "Mobil 1", "livery_sponsors": ["Truck Assist"]},
    {"number": "3", "team": "Blanchard Racing Team", "drivers": ["Aaron Cameron", "Zak Best"], "car": {"make": "Ford", "model": "Mustang S650"}, "primary_sponsor": "Liqui Moly", "livery_sponsors": ["Blahst"]}
  ]
}"#;

    #[test]
    fn driver_evidence_finds_one_entry_but_a_shared_team_does_not() {
        let list = parse(BATHURST_EXCERPT).unwrap();
        assert_eq!(
            list.from_evidence(&["Wood".into()])
                .map(|e| e.number.as_str()),
            Some("2")
        );
        assert_eq!(
            list.from_evidence(&["Mostert".into()])
                .map(|e| e.number.as_str()),
            Some("1")
        );
        assert_eq!(
            list.from_evidence(&["Ryan Wood".into(), "Evans".into()])
                .map(|e| e.number.as_str()),
            Some("2")
        );
        // Two Walkinshaw cars: the team alone does not pick one.
        assert!(list.from_evidence(&["Walkinshaw".into()]).is_none());
        assert!(list.from_evidence(&["a sponsor".into()]).is_none());
    }

    #[test]
    fn a_surname_shared_by_two_entries_is_not_a_match() {
        let json = r#"[
          {"number":"1","drivers":["Alex Jones"]},
          {"number":"2","drivers":["Sam Jones"]}
        ]"#;
        let list = parse(json).unwrap();
        assert!(list.from_evidence(&["Jones".into()]).is_none());
        assert_eq!(
            list.from_evidence(&["Alex Jones".into()])
                .map(|e| e.number.as_str()),
            Some("1")
        );
    }

    #[test]
    fn csv_handles_quotes_commas_and_any_column_order() {
        let csv = "Model,number,TEAM,drivers,make,sponsors\n\
                   \"Mustang, S650\",7,\"Team, Inc.\",Chaz Mostert & Fabian Coulthard,Ford,\"Mobil 1;Optus\"\n\
                   GR Supra,\"0042\",Solo,Ryan Wood / Jaxon Evans,Toyota,\n";
        let list = parse(csv).unwrap();
        let seven = list.get("7").unwrap();
        assert_eq!(seven.team.as_deref(), Some("Team, Inc."));
        assert_eq!(seven.drivers, ["Chaz Mostert", "Fabian Coulthard"]);
        assert_eq!(seven.make.as_deref(), Some("Ford"));
        assert_eq!(seven.model.as_deref(), Some("Mustang, S650"));
        assert_eq!(seven.sponsors, ["Mobil 1", "Optus"]);
        let forty_two = list.get("42").unwrap();
        assert_eq!(forty_two.drivers, ["Ryan Wood", "Jaxon Evans"]);
        assert!(forty_two.sponsors.is_empty());
    }

    #[test]
    fn csv_without_number_column_is_an_error() {
        assert!(parse("team,drivers\nA,B\n").is_err());
    }

    #[test]
    fn json_object_shape() {
        let list = parse(BATHURST_EXCERPT).unwrap();
        let one = list.get("1").unwrap();
        assert_eq!(one.team.as_deref(), Some("Walkinshaw TWG Racing"));
        assert_eq!(one.drivers, ["Chaz Mostert", "Fabian Coulthard"]);
        assert_eq!(one.make.as_deref(), Some("Toyota"));
        assert_eq!(one.model.as_deref(), Some("GR Supra"));
        assert_eq!(one.sponsors, ["Mobil 1", "Optus"]);
        assert_eq!(
            list.get("3").unwrap().model.as_deref(),
            Some("Mustang S650")
        );
    }

    #[test]
    fn json_bare_list_with_flat_make_and_sponsors_array() {
        let text = r#"[
            {"number": 9, "team": "Erebus", "drivers": ["A", "B"], "make": "Chevrolet", "model": "Camaro ZL1", "sponsors": ["Chiko", "Bendix"]},
            {"number": "10", "drivers": [], "primary_sponsor": "Bendix"}
        ]"#;
        let list = parse(text).unwrap();
        let nine = list.get("9").unwrap();
        assert_eq!(nine.make.as_deref(), Some("Chevrolet"));
        assert_eq!(nine.sponsors, ["Chiko", "Bendix"]);
        assert_eq!(list.get("10").unwrap().sponsors, ["Bendix"]);
        assert_eq!(list.get("10").unwrap().make, None);
    }

    #[test]
    fn json_entry_without_number_is_an_error() {
        assert!(parse(r#"{"entries": [{"team": "X"}]}"#).is_err());
    }

    #[test]
    fn lookup_ignores_leading_zeros() {
        let list = parse("number\n007\nA1\n").unwrap();
        assert!(list.get("7").is_some());
        assert!(list.get("0007").is_some());
        assert!(list.get("07").is_some());
        assert!(list.get("8").is_none());
        assert!(list.get("").is_none());
        assert!(list.get("a1").is_some());
    }

    #[test]
    fn candidates_and_sponsors_are_distinct_and_sorted() {
        let list = parse(BATHURST_EXCERPT).unwrap();
        assert_eq!(list.candidates(), ["Ford Mustang S650", "Toyota GR Supra"]);
        assert_eq!(
            list.sponsors(),
            ["Blahst", "Liqui Moly", "Mobil 1", "Optus", "Truck Assist"]
        );
    }

    #[test]
    fn empty_list() {
        let list = parse("number,team\n").unwrap();
        assert!(list.is_empty());
        assert!(list.candidates().is_empty());
    }
}
