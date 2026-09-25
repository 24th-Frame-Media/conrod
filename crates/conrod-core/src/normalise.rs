//! Settling a group's noisy readings into one canonical name.
//!
//! Arbitrating between readings with a real call to a locally-installed
//! vision model is out of scope here. What lives here is everything pure
//! around it: deciding whether the call is even needed
//! ([`settle_without_model`]), the key a pending call is cached under
//! ([`cache_key`]), and the checking-back of the model's answer against what
//! was actually read ([`reconcile`]).

use crate::analysis::given;
use crate::marques;
use crate::text::Tally;
use serde_json::{Map, Value};
use std::collections::HashMap;

/// One distinct thing the per-frame readers said, and how often.
///
/// The count is the whole point: deduplicating readings and handing a model
/// a plain list threw away the evidence for which one is right.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    pub make: String,
    pub model: String,
    pub count: i64,
    /// Whether `make` is a marque a reader actually reported, rather than a
    /// guess made by splitting a bare string on its first space. A guessed
    /// make can never be counted as a vote for one -- see `plurality_make`.
    pub stated: bool,
}

impl Default for Reading {
    fn default() -> Self {
        Reading {
            make: String::new(),
            model: String::new(),
            count: 1,
            stated: true,
        }
    }
}

impl Reading {
    pub fn text(&self) -> String {
        [self.make.as_str(), self.model.as_str()]
            .into_iter()
            .filter(|b| !b.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Readings from bare strings, each seen once. For callers without tallies.
pub fn readings_from(texts: &[String]) -> Vec<Reading> {
    texts
        .iter()
        .map(|text| {
            let mut parts = text.splitn(2, ' ');
            let make = parts.next().unwrap_or("").to_string();
            let model = parts.next().unwrap_or("").to_string();
            Reading {
                make,
                model,
                count: 1,
                stated: false,
            }
        })
        .collect()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Canonical {
    pub make: Option<String>,
    pub model: Option<String>,
    pub rejected: Vec<String>,
}

impl Canonical {
    /// True once either half of an identity is filled in.
    pub fn is_truthy(&self) -> bool {
        given(&self.make).is_some() || given(&self.model).is_some()
    }
}

/// What each frame's own reader said, one line per distinct reading,
/// deduplicated and carrying how often each was seen.
pub fn readings_of(members: &[Map<String, Value>]) -> Vec<Reading> {
    // A reader's own text field, if it is a non-empty string.
    let field = |member: &Map<String, Value>, key: &str| -> Option<String> {
        match member.get(key) {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        }
    };

    let mut counts: Tally<String> = Tally::new();
    let mut forms: HashMap<String, Tally<(String, String)>> = HashMap::new();
    for member in members {
        let mut make = field(member, "own_make")
            .or_else(|| field(member, "make"))
            .unwrap_or_default();
        make = make.trim().to_string();
        let mut model = field(member, "own_model")
            .or_else(|| field(member, "model"))
            .unwrap_or_default();
        model = model.trim().to_string();
        // The model often already carries the make ("Holden Holden Commodore").
        if !make.is_empty() && model.to_lowercase().starts_with(&make.to_lowercase()) {
            make.clear();
        }
        let line = [make.as_str(), model.as_str()]
            .into_iter()
            .filter(|b| !b.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .trim()
            .to_string();
        if line.is_empty() {
            continue;
        }
        let k = key(&line);
        counts.add(k.clone(), 1);
        forms.entry(k).or_default().add((make, model), 1);
    }

    let mut out = Vec::new();
    for (k, total) in counts.ranked() {
        let (make, model) = forms[&k].ranked()[0].0.clone();
        out.push(Reading {
            make,
            model,
            count: total,
            stated: true,
        });
    }
    out
}

/// What two readings have to share to be the same reading: lowercase,
/// punctuation and separators dropped entirely (not turned into spaces), so
/// "XJ-S" matches "XJS" and "Cooper-S" matches "Cooper S".
pub fn key(reading: &str) -> String {
    reading
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

pub fn observed(readings: &[Reading]) -> String {
    readings
        .iter()
        .map(Reading::text)
        .collect::<Vec<_>>()
        .join(" | ")
        .to_lowercase()
}

/// The make the most frames named, when one of them clearly leads. `None` on
/// a tie, which is a real answer: nothing should be allowed to claim a win
/// that was not won.
pub fn plurality_make(readings: &[Reading]) -> Option<String> {
    let mut tally: Tally<String> = Tally::new();
    let mut spelling: HashMap<String, String> = HashMap::new();
    for reading in readings {
        if !reading.stated {
            continue;
        }
        let make = if !reading.make.is_empty() {
            reading.make.clone()
        } else {
            let model = (!reading.model.is_empty()).then_some(reading.model.as_str());
            marques::correct_make(None, model).unwrap_or_default()
        };
        let make = make.trim().to_string();
        if make.is_empty() {
            continue;
        }
        let lower = make.to_lowercase();
        tally.add(lower.clone(), reading.count);
        spelling.entry(lower).or_insert(make);
    }
    if tally.is_empty() {
        return None;
    }
    let ranked = tally.ranked();
    if ranked.len() > 1 && ranked[0].1 == ranked[1].1 {
        return None;
    }
    spelling.get(&ranked[0].0).cloned()
}

/// A make is acceptable if it was read, or if a read nameplate implies it --
/// `marques`'s job, done from the other direction.
pub fn acceptable_make(make: Option<&str>, readings: &[Reading]) -> Option<String> {
    let make = make.filter(|m| !m.is_empty())?;
    let haystack = observed(readings);
    if haystack.contains(&make.to_lowercase()) {
        return Some(make.to_string());
    }
    for reading in readings {
        let text = reading.text();
        let text_opt = (!text.is_empty()).then_some(text.as_str());
        if let Some(implied) = marques::correct_make(None, text_opt) {
            if implied.to_lowercase() == make.to_lowercase() {
                return Some(make.to_string());
            }
        }
    }
    None
}

/// Keep the words of the model that were read, drop the ones that were not
/// -- word by word, so joining "Falcon" from one reading to "FG" from
/// another is fine, but inventing "MkII" out of nothing is not.
pub fn acceptable_model(model: Option<&str>, readings: &[Reading]) -> Option<String> {
    let model = model.filter(|m| !m.is_empty())?;
    let haystack = observed(readings);
    let replaced = model.replace('-', " ");
    let kept: Vec<&str> = replaced
        .split_whitespace()
        .filter(|w| haystack.contains(&w.to_lowercase()))
        .collect();
    if kept.is_empty() {
        return None;
    }
    let tidied = kept.join(" ");

    // Give the spelling back: splitting on the hyphen to check words is
    // fine, rejoining with a space is not -- "X-Trail" must not come back
    // as "X Trail".
    for reading in readings {
        let text = reading.text();
        for candidate in [text.as_str(), reading.model.as_str()] {
            if !candidate.is_empty() && key(candidate) == key(&tidied) {
                return Some(candidate.to_string());
            }
        }
    }
    Some(tidied)
}

/// A reading this much of the group agreed on is not a disagreement to
/// arbitrate, it is the answer.
pub const MAJORITY_SETTLES: f64 = 0.7;

/// Whether the readings already agree well enough (or there is only one)
/// that no model call is needed -- the part of `canonical()` before it ever
/// builds a request.
pub fn settle_without_model(readings: &[Reading]) -> Option<Canonical> {
    if readings.len() < 2 {
        // Nothing to reconcile, and asking a model to "tidy" a lone reading
        // is how a reading gets embellished.
        return Some(Canonical::default());
    }
    let total: i64 = readings.iter().map(|r| r.count).sum();
    let top = &readings[0];
    if total > 0 && (top.count as f64) / (total as f64) >= MAJORITY_SETTLES {
        let make = if !top.make.is_empty() {
            Some(top.make.clone())
        } else {
            let model = (!top.model.is_empty()).then_some(top.model.as_str());
            marques::correct_make(None, model)
        };
        let model = (!top.model.is_empty()).then(|| top.model.clone());
        return Some(Canonical {
            make,
            model,
            rejected: Vec::new(),
        });
    }
    None
}

/// The cache key a pending model call is looked up (and stored) by.
pub fn cache_key(readings: &[Reading]) -> String {
    readings
        .iter()
        .map(|r| format!("{}x {}", r.count, r.text()))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Check the model's raw answer back against what was actually read.
///
/// `make` and `model` are the response's fields, already extracted (a
/// non-string JSON value is `None` -- that conversion happens before this is
/// called, same as `canonical()`'s own `.strip() if isinstance(..., str)`).
/// This is the checking-back logic that follows the HTTP call in
/// `canonical()`: a plurality make overrules a model that disagrees with it,
/// and otherwise only words that were actually observed survive.
pub fn reconcile(make: Option<&str>, model: Option<&str>, readings: &[Reading]) -> Canonical {
    let make = make.map(str::trim).filter(|s| !s.is_empty());
    let model = model.map(str::trim).filter(|s| !s.is_empty());

    // The frames are the evidence and the count is what makes them evidence:
    // where one marque leads, the answer has to be that marque.
    let leader = plurality_make(readings);
    if let (Some(leader), Some(make_val)) = (&leader, make) {
        if key(make_val) != key(leader) {
            let top = &readings[0];
            let top_model = if key(&top.make) == key(leader) {
                // An empty string here really is stored as an empty string,
                // not treated as absent.
                Some(top.model.clone())
            } else {
                None
            };
            // ponytail: a missing model renders as the literal word "None"
            // in the rejected text -- kept as-is, the recorded snapshots
            // depend on this exact wording.
            let model_repr = model.unwrap_or("None");
            let rejected_text = format!("{make_val} {model_repr}").trim().to_string();
            return Canonical {
                make: Some(leader.clone()),
                model: top_model,
                rejected: vec![rejected_text],
            };
        }
    }

    let mut rejected: Vec<String> = Vec::new();
    let kept_make = acceptable_make(make, readings);
    if let Some(m) = make {
        if kept_make.is_none() {
            rejected.push(m.to_string());
        }
    }
    let kept_model = acceptable_model(model, readings);
    if let Some(m) = model {
        if kept_model.as_deref() != Some(m) {
            rejected.push(m.to_string());
        }
    }

    // A model without its make is half an answer; `marques` can often
    // supply the other half from the nameplate alone.
    let mut kept_make = kept_make;
    if kept_model.is_some() && kept_make.is_none() {
        kept_make = marques::correct_make(None, kept_model.as_deref());
    }

    // The make repeated into the model is the one mistake worth stripping
    // without asking.
    let mut kept_model = kept_model;
    if let (Some(km), Some(mo)) = (kept_make.clone(), kept_model.clone()) {
        if mo.to_lowercase().starts_with(&km.to_lowercase()) {
            let stripped = mo[km.len()..].trim().to_string();
            kept_model = if stripped.is_empty() {
                None
            } else {
                Some(stripped)
            };
        }
    }

    Canonical {
        make: kept_make,
        model: kept_model,
        rejected,
    }
}
