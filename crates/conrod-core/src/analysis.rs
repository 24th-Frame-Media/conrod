//! What was read off one vehicle.
//!
//! Stored as JSON in `detections.attributes`, which also carries keys this
//! struct does not own (grouping writes its own), so reading is tolerant:
//! unknown keys are ignored and a value of the wrong type falls back to the
//! default.
//!
//! Also carries `merge_number` and `corroborated`, the two pure decisions
//! made once the four readers (plate detection, OCR, the vision model) have
//! each had their turn.

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VehicleAnalysis {
    pub kind: String,
    pub is_bike: bool,
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub body_type: Option<String>,
    pub plate: Option<String>,
    pub plate_state: Option<String>,
    pub plate_conf: f64,
    /// The plate's box, in the coordinates of whichever crop it was found in (`crop` or `native`);
    /// `analyze::analyze`'s caller translates this to full-frame pixels before it is stored.
    pub plate_bbox: Option<[i64; 4]>,
    pub race_number: Option<String>,
    pub number_source: Option<String>,
    pub number_conf: f64,
    pub team: Option<String>,
    pub driver: Option<String>,
    pub country: Option<String>,
    pub team_corroborated: bool,
    pub sponsors: Vec<String>,
    pub text: Vec<String>,
    /// Visible action effects: `airborne`, `dust`, `tyre_smoke`, `spray`,
    /// `flames`, `sparks`, `contact`, `wheel_lift`.
    pub action: Vec<String>,
    pub is_competition: bool,
    pub vlm_conf: f64,
    /// What the model based make/model on: "badge", "script" or "design".
    pub model_evidence: Option<String>,
    /// Make/model came from the car's design alone: a suggestion, not a read.
    pub model_suggested: bool,
}

impl Default for VehicleAnalysis {
    fn default() -> Self {
        VehicleAnalysis {
            kind: "car".into(),
            is_bike: false,
            make: None,
            model: None,
            colour: None,
            body_type: None,
            plate: None,
            plate_state: None,
            plate_conf: 0.0,
            plate_bbox: None,
            race_number: None,
            number_source: None,
            number_conf: 0.0,
            team: None,
            driver: None,
            country: None,
            team_corroborated: false,
            sponsors: Vec::new(),
            text: Vec::new(),
            action: Vec::new(),
            is_competition: false,
            vlm_conf: 0.0,
            model_evidence: None,
            model_suggested: false,
        }
    }
}

/// A present, non-empty string.
pub(crate) fn given(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|s| !s.is_empty())
}

impl VehicleAnalysis {
    /// Read the stored JSON. Nothing here is allowed to fail: a blank, broken
    /// or partial record is the default analysis with whatever did parse.
    pub fn from_json(raw: Option<&str>) -> VehicleAnalysis {
        let data: Map<String, Value> = raw
            .filter(|r| !r.is_empty())
            .and_then(|r| serde_json::from_str(r).ok())
            .unwrap_or_default();
        Self::from_map(&data)
    }

    pub fn from_map(data: &Map<String, Value>) -> VehicleAnalysis {
        let mut a = VehicleAnalysis::default();
        let text = |key: &str, slot: &mut Option<String>| match data.get(key) {
            Some(Value::String(s)) => *slot = Some(s.clone()),
            Some(Value::Null) => *slot = None,
            _ => {}
        };
        text("make", &mut a.make);
        text("model", &mut a.model);
        text("colour", &mut a.colour);
        text("body_type", &mut a.body_type);
        text("plate", &mut a.plate);
        text("plate_state", &mut a.plate_state);
        text("race_number", &mut a.race_number);
        text("number_source", &mut a.number_source);
        text("team", &mut a.team);
        text("driver", &mut a.driver);
        text("country", &mut a.country);
        text("model_evidence", &mut a.model_evidence);
        if let Some(Value::String(kind)) = data.get("kind") {
            a.kind = kind.clone();
        }
        let flag = |key: &str| data.get(key).and_then(Value::as_bool);
        a.is_bike = flag("is_bike").unwrap_or(false);
        a.team_corroborated = flag("team_corroborated").unwrap_or(false);
        a.is_competition = flag("is_competition").unwrap_or(false);
        a.model_suggested = flag("model_suggested").unwrap_or(false);
        let number = |key: &str| data.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        a.plate_conf = number("plate_conf");
        a.number_conf = number("number_conf");
        a.vlm_conf = number("vlm_conf");
        let list = |key: &str| -> Vec<String> {
            data.get(key)
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default()
        };
        a.sponsors = list("sponsors");
        a.text = list("text");
        a
    }

    /// The model, unless it is only a suggestion from the car's shape.
    pub fn confirmed_model(&self) -> Option<&str> {
        if self.model_suggested {
            None
        } else {
            given(&self.model)
        }
    }

    /// A short human label for the review UI.
    pub fn title(&self) -> String {
        let model = self.confirmed_model().unwrap_or("");
        let mut make = given(&self.make).unwrap_or("");
        // The model often already carries the make ("Holden Commodore").
        if !make.is_empty() && model.to_lowercase().starts_with(&make.to_lowercase()) {
            make = "";
        }
        // Sentence case on the colour's first character only.
        let colour = given(&self.colour).unwrap_or("");
        let mut chars = colour.chars();
        let colour: String = match chars.next() {
            Some(first) => first.to_uppercase().chain(chars).collect(),
            None => String::new(),
        };
        let bits: Vec<&str> = [colour.as_str(), make, model]
            .into_iter()
            .filter(|b| !b.is_empty())
            .collect();
        let mut label = if !bits.is_empty() {
            bits.join(" ")
        } else if given(&self.plate).is_some() || given(&self.race_number).is_some() {
            capitalize(&self.kind)
        } else {
            format!("{}, not identified", capitalize(&self.kind))
        };
        if let Some(number) = given(&self.race_number) {
            label = format!("#{number} {label}");
        }
        label
    }
}

/// First character upper, the rest lower.
fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.as_str().to_lowercase().chars())
            .collect(),
        None => String::new(),
    }
}

/// A small (number, confidence, source) tuple: one reading OCR handed back.
#[derive(Debug, Clone, PartialEq)]
pub struct OcrReading {
    pub number: Option<String>,
    pub confidence: f64,
    pub source: String,
}

impl Default for OcrReading {
    fn default() -> Self {
        OcrReading {
            number: None,
            confidence: 0.0,
            source: "ocr".to_string(),
        }
    }
}

/// A number the model saw, and where on the car it was.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NumberCandidate {
    pub number: String,
    /// "door", "bonnet", "roof", "windscreen", "nose" or "other".
    pub location: String,
}

/// How prominent a number's panel is. A door or bonnet panel is the largest
/// the car carries; a nose board is regularly a different, decorative number.
fn location_rank(location: &str) -> i32 {
    match location.trim().to_lowercase().as_str() {
        "door" | "bonnet" => 4,
        "roof" => 3,
        "windscreen" => 2,
        "nose" => 1,
        _ => 0,
    }
}

/// Pick the race number from every number the model saw, deterministically:
/// (a) one the album's entry list has, (b) the most prominent panel (door,
/// then bonnet, then roof, windscreen, nose, other), (c) the number already
/// seen in this burst, which settles a tie between equally prominent panels.
/// Equal scores keep the model's own order, so the answer never depends on
/// hash iteration.
pub fn choose_race_number(
    candidates: &[NumberCandidate],
    listed: impl Fn(&str) -> bool,
    seen: &std::collections::HashMap<String, u32>,
) -> Option<String> {
    let mut unique: Vec<(&str, i32)> = Vec::new();
    for candidate in candidates {
        let number = candidate.number.trim();
        if number.is_empty() {
            continue;
        }
        let rank = location_rank(&candidate.location);
        match unique.iter_mut().find(|(n, _)| *n == number) {
            Some((_, best)) => *best = (*best).max(rank),
            None => unique.push((number, rank)),
        }
    }
    let mut pool: Vec<(&str, i32)> = unique
        .iter()
        .copied()
        .filter(|(number, _)| listed(number))
        .collect();
    if pool.is_empty() {
        pool = unique;
    }
    let best_rank = pool.iter().map(|(_, rank)| *rank).max()?;
    let mut pool: Vec<(&str, i32)> = pool.into_iter().filter(|(_, r)| *r == best_rank).collect();
    if pool.len() > 1 {
        let best_seen = pool
            .iter()
            .map(|(number, _)| seen.get(*number).copied().unwrap_or(0))
            .max()
            .unwrap_or(0);
        if best_seen > 0 {
            pool.retain(|(number, _)| seen.get(*number).copied().unwrap_or(0) == best_seen);
        }
    }
    pool.first().map(|(number, _)| (*number).to_string())
}

/// Decide the competition number from the two readers.
///
/// The model wins whenever it gave a number: it reads stylised, angled and
/// partly hidden digits far better than OCR, and an OCR read is never allowed
/// to override or outvote it. OCR only fills in when the model gave none, and
/// then only above `ocr_accept_confidence`; such a read is marked down (its
/// confidence scaled by [`OCR_ONLY_SCALE`]) so it lands in review as the
/// weaker source.
///
/// `vlm_confidence` is the model's own confidence in the number (already
/// lowered for nose-board reads by `map_vehicle`); `ocr_accept_confidence`
/// is `Settings.ocr_accept_confidence` (default 0.80).
pub fn merge_number(
    ocr_reading: &OcrReading,
    vlm_number: Option<&str>,
    vlm_confidence: f64,
    ocr_accept_confidence: f64,
) -> (Option<String>, Option<String>, f64) {
    let ocr_number = given(&ocr_reading.number);
    // Treat an empty vlm_number the same as a missing ocr number.
    let vlm_number = vlm_number.filter(|s| !s.is_empty());
    let vlm_confidence = if vlm_confidence.is_finite() {
        vlm_confidence.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let source = if ocr_reading.source.is_empty() {
        "ocr".to_string()
    } else {
        ocr_reading.source.clone()
    };

    if let Some(v) = vlm_number {
        if ocr_number == Some(v) {
            // Independent agreement is worth more than either one's confidence.
            let confidence = (ocr_reading.confidence.max(vlm_confidence).max(0.7) + 0.2).min(1.0);
            return (
                Some(v.to_string()),
                Some(format!("{source}+vlm")),
                confidence,
            );
        }
        // The model's answer stands, even under a confident OCR read: OCR's
        // race-number reads are wrong far more often.
        return (Some(v.to_string()), Some("vlm".to_string()), vlm_confidence);
    }
    if let Some(o) = ocr_number {
        if ocr_reading.confidence >= ocr_accept_confidence {
            return (
                Some(o.to_string()),
                Some(source),
                (ocr_reading.confidence * OCR_ONLY_SCALE).clamp(0.0, 1.0),
            );
        }
        // Below the floor, a lone OCR read is noise, not a number.
    }
    (None, None, 0.0)
}

/// How much a lone OCR read is scaled down: it fills a blank, but always
/// reads as the weaker source than a model answer.
pub const OCR_ONLY_SCALE: f64 = 0.75;

/// Words a model-reported name needs at least one of, to count as
/// corroborated rather than invented. "Racing" or "Team" prove nothing.
const GENERIC_TEAM_WORDS: [&str; 6] = [
    "RACING",
    "TEAM",
    "MOTORSPORT",
    "MOTORSPORTS",
    "AUTO",
    "GARAGE",
];

/// Is a model-reported name actually supported by text that was read?
pub fn corroborated(claim: Option<&str>, evidence: &[String]) -> bool {
    let Some(claim) = claim.filter(|c| !c.is_empty()) else {
        return false;
    };
    let haystack: String = evidence
        .join(" ")
        .to_uppercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    if haystack.is_empty() {
        return false;
    }
    // Match on the distinctive words only.
    let words: Vec<String> = claim
        .split_whitespace()
        .map(|word| {
            word.to_uppercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|w| w.chars().count() >= 4 && !GENERIC_TEAM_WORDS.contains(&w.as_str()))
        .collect();
    if words.is_empty() {
        return false;
    }
    words.iter().any(|w| haystack.contains(w.as_str()))
}

#[cfg(test)]
mod merge_number_tests {
    use super::*;

    fn ocr(number: Option<&str>, confidence: f64) -> OcrReading {
        OcrReading {
            number: number.map(str::to_string),
            confidence,
            source: "ocr".into(),
        }
    }

    #[test]
    fn a_confident_vlm_number_keeps_its_own_confidence() {
        let (number, source, confidence) = merge_number(&ocr(None, 0.0), Some("88"), 0.95, 0.8);
        assert_eq!(number.as_deref(), Some("88"));
        assert_eq!(source.as_deref(), Some("vlm"));
        assert_eq!(confidence, 0.95, "a 0.95 read can clear review");
    }

    #[test]
    fn a_weak_vlm_number_is_kept_but_stays_uncertain() {
        let (number, source, confidence) = merge_number(&ocr(None, 0.0), Some("88"), 0.2, 0.8);
        assert_eq!(number.as_deref(), Some("88"));
        assert_eq!(source.as_deref(), Some("vlm"));
        assert_eq!(confidence, 0.2, "a weak read stays visibly uncertain");
    }

    #[test]
    fn agreement_takes_the_better_of_the_two_readers() {
        let (_, source, confidence) = merge_number(&ocr(Some("7"), 0.5), Some("7"), 0.9, 0.8);
        assert_eq!(source.as_deref(), Some("ocr+vlm"));
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn a_confident_ocr_read_never_overrides_the_model() {
        let (number, source, confidence) =
            merge_number(&ocr(Some("12"), 0.99), Some("88"), 0.7, 0.8);
        assert_eq!(number.as_deref(), Some("88"));
        assert_eq!(source.as_deref(), Some("vlm"));
        assert_eq!(confidence, 0.7);
    }

    #[test]
    fn ocr_only_fills_a_blank_above_the_floor_and_reads_as_the_weaker_source() {
        let (number, source, confidence) = merge_number(&ocr(Some("12"), 0.9), None, 0.0, 0.8);
        assert_eq!(number.as_deref(), Some("12"));
        assert_eq!(source.as_deref(), Some("ocr"));
        assert_eq!(confidence, 0.9 * OCR_ONLY_SCALE);
        assert!(confidence < 0.8, "a lone OCR read stays in review");
    }

    #[test]
    fn a_lone_ocr_read_below_the_floor_is_no_number() {
        let (number, source, confidence) = merge_number(&ocr(Some("12"), 0.79), None, 0.0, 0.8);
        assert_eq!((number, source), (None, None));
        assert_eq!(confidence, 0.0);
    }

    #[test]
    fn an_out_of_range_vlm_confidence_is_clamped() {
        let (_, _, high) = merge_number(&ocr(None, 0.0), Some("7"), 9.0, 0.8);
        let (_, _, low) = merge_number(&ocr(None, 0.0), Some("7"), -1.0, 0.8);
        assert_eq!((high, low), (1.0, 0.0));
    }
}

#[cfg(test)]
mod choose_race_number_tests {
    use super::*;
    use std::collections::HashMap;

    fn candidate(number: &str, location: &str) -> NumberCandidate {
        NumberCandidate {
            number: number.into(),
            location: location.into(),
        }
    }

    fn none(_: &str) -> bool {
        false
    }

    #[test]
    fn an_entry_list_number_wins_over_a_bigger_panel() {
        let candidates = [
            candidate("99", "door"),
            candidate("7", "windscreen"),
            candidate("42", "nose"),
        ];
        let listed = |n: &str| n == "7";
        assert_eq!(
            choose_race_number(&candidates, listed, &HashMap::new()).as_deref(),
            Some("7")
        );
    }

    #[test]
    fn no_list_match_falls_back_to_the_most_prominent_panel() {
        let candidates = [
            candidate("12", "windscreen"),
            candidate("5", "bonnet"),
            candidate("3", "roof"),
        ];
        assert_eq!(
            choose_race_number(&candidates, none, &HashMap::new()).as_deref(),
            Some("5")
        );
    }

    #[test]
    fn a_number_seen_in_the_burst_settles_a_prominence_tie() {
        let candidates = [candidate("12", "door"), candidate("5", "door")];
        let mut seen = HashMap::new();
        seen.insert("5".to_string(), 3);
        seen.insert("12".to_string(), 1);
        assert_eq!(
            choose_race_number(&candidates, none, &seen).as_deref(),
            Some("5")
        );
        // Without a burst history the model's own order is the tiebreak.
        assert_eq!(
            choose_race_number(&candidates, none, &HashMap::new()).as_deref(),
            Some("12")
        );
    }

    #[test]
    fn a_duplicate_keeps_its_most_prominent_location() {
        let candidates = [candidate("8", "nose"), candidate("8", "door")];
        let other = [candidate("9", "door")];
        assert_eq!(
            choose_race_number(&candidates, none, &HashMap::new()).as_deref(),
            Some("8")
        );
        assert_eq!(
            choose_race_number(&other, none, &HashMap::new()).as_deref(),
            Some("9")
        );
    }
}

#[cfg(test)]
mod suggested_model_tests {
    use super::*;
    use serde_json::json;

    fn parse(value: Value) -> VehicleAnalysis {
        VehicleAnalysis::from_map(value.as_object().unwrap())
    }

    #[test]
    fn suggested_model_is_left_out_of_title() {
        let suggested = parse(
            json!({"make": "Ford", "model": "Focus", "colour": "red", "model_suggested": true}),
        );
        assert_eq!(suggested.title(), "Red Ford");
        let confirmed = parse(json!({"make": "Ford", "model": "Focus", "colour": "red"}));
        assert_eq!(confirmed.title(), "Red Ford Focus");
        let missing_flag =
            parse(json!({"make": "Ford", "model": "Focus", "model_suggested": false}));
        assert_eq!(missing_flag.title(), "Ford Focus");
    }
}
