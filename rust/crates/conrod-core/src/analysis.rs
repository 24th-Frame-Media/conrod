//! What was read off one vehicle.
//!
//! Port of the `VehicleAnalysis` data model in `conrod/analyze.py`. Stored as
//! JSON in `detections.attributes`, which also carries keys this struct does
//! not own (grouping writes its own), so reading is tolerant: unknown keys are
//! ignored and a value of the wrong type falls back to the default.

use crate::py;
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
            team_corroborated: false,
            sponsors: Vec::new(),
            text: Vec::new(),
            is_competition: false,
            vlm_conf: 0.0,
        }
    }
}

/// A Python-truthy string: present and not empty.
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
            py::capitalize(&self.kind)
        } else {
            format!("{}, not identified", py::capitalize(&self.kind))
        };
        if let Some(number) = given(&self.race_number) {
            label = format!("#{number} {label}");
        }
        label
    }
}
