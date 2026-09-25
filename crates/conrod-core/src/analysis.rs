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
    pub race_number: Option<String>,
    pub number_source: Option<String>,
    pub number_conf: f64,
    pub team: Option<String>,
    pub driver: Option<String>,
    pub country: Option<String>,
    pub team_corroborated: bool,
    pub sponsors: Vec<String>,
    pub text: Vec<String>,
    pub is_competition: bool,
    pub vlm_conf: f64,
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
            race_number: None,
            number_source: None,
            number_conf: 0.0,
            team: None,
            driver: None,
            country: None,
            team_corroborated: false,
            sponsors: Vec::new(),
            text: Vec::new(),
            is_competition: false,
            vlm_conf: 0.0,
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
        if let Some(Value::String(kind)) = data.get("kind") {
            a.kind = kind.clone();
        }
        let flag = |key: &str| data.get(key).and_then(Value::as_bool);
        a.is_bike = flag("is_bike").unwrap_or(false);
        a.team_corroborated = flag("team_corroborated").unwrap_or(false);
        a.is_competition = flag("is_competition").unwrap_or(false);
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

    /// A short human label for the review UI.
    pub fn title(&self) -> String {
        let model = given(&self.model).unwrap_or("");
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

/// Decide the competition number from two disagreeing readers.
///
/// `ocr_accept_confidence` is `Settings.ocr_accept_confidence` (default
/// 0.80): the OCR reading's own floor for winning outright.
pub fn merge_number(
    ocr_reading: &OcrReading,
    vlm_number: Option<&str>,
    ocr_accept_confidence: f64,
) -> (Option<String>, Option<String>, f64) {
    let ocr_number = given(&ocr_reading.number);
    // Treat an empty vlm_number the same as a missing ocr number.
    let vlm_number = vlm_number.filter(|s| !s.is_empty());
    let source = if ocr_reading.source.is_empty() {
        "ocr".to_string()
    } else {
        ocr_reading.source.clone()
    };

    if let (Some(o), Some(v)) = (ocr_number, vlm_number) {
        if o == v {
            // Independent agreement is worth more than either one's confidence.
            let confidence = (ocr_reading.confidence.max(0.7) + 0.2).min(1.0);
            return (
                Some(o.to_string()),
                Some(format!("{source}+vlm")),
                confidence,
            );
        }
    }
    if let Some(o) = ocr_number {
        if ocr_reading.confidence >= ocr_accept_confidence {
            return (Some(o.to_string()), Some(source), ocr_reading.confidence);
        }
    }
    if let Some(v) = vlm_number {
        // The model reads stylised and angled numbers far better than OCR,
        // so it wins where OCR was not already confident.
        return (Some(v.to_string()), Some("vlm".to_string()), 0.7);
    }
    if let Some(o) = ocr_number {
        // Weak, but better than nothing -- the low score sends it to review.
        return (Some(o.to_string()), Some(source), ocr_reading.confidence);
    }
    (None, None, 0.0)
}

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
