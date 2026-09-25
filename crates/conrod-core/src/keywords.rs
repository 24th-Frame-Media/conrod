//! Turning what was read off a vehicle into keywords worth searching.
//!
//! The number, the plate, the car, the team, the colour -- what a
//! photographer would type into Lightroom's search box.

use crate::analysis::{given, VehicleAnalysis};
use crate::mapping::NumberMap;
use crate::text::casefold;
use std::collections::HashSet;

/// The keyword settings, as `Settings.keyword_prefix` / `write_plate_keyword`.
#[derive(Debug, Clone, Default)]
pub struct KeywordOptions {
    pub prefix: String,
    pub write_plate: bool,
}

fn dedupe_casefold(
    keywords: impl IntoIterator<Item = String>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
) {
    for keyword in keywords {
        if seen.insert(casefold(&keyword)) {
            out.push(keyword);
        }
    }
}

/// Keywords for one detected vehicle.
pub fn for_vehicle(
    a: &VehicleAnalysis,
    options: &KeywordOptions,
    numbers: Option<&NumberMap>,
) -> Vec<String> {
    let prefix = &options.prefix;
    let mut out: Vec<String> = Vec::new();
    let add = |out: &mut Vec<String>, value: &str| {
        let value = value.trim();
        if !value.is_empty() {
            out.push(format!("{prefix}{value}"));
        }
    };

    if let Some(number) = given(&a.race_number) {
        add(&mut out, number);
        add(&mut out, &format!("#{number}"));
        add(&mut out, &format!("Car {number}"));
        // An empty entry list has nothing to look up, so skip it.
        if let Some(map) = numbers.filter(|m| !m.is_empty()) {
            // The entry list is authoritative about who a number is.
            for keyword in map.keywords_for(number, prefix) {
                if !out.contains(&keyword) {
                    out.push(keyword);
                }
            }
        }
    }

    if let Some(plate) = given(&a.plate).filter(|_| options.write_plate) {
        add(&mut out, plate);
        if let Some(state) = given(&a.plate_state) {
            add(&mut out, state);
        }
    }

    for value in [&a.make, &a.colour, &a.body_type] {
        if let Some(v) = given(value) {
            add(&mut out, v);
        }
    }
    match (given(&a.make), given(&a.model)) {
        (Some(make), Some(model)) => {
            // Both the bare model and the qualified one, for either search.
            add(&mut out, model);
            add(&mut out, &format!("{make} {model}"));
        }
        (_, Some(model)) => add(&mut out, model),
        _ => {}
    }

    // An uncorroborated team is the model's guess; it stays out of the file
    // until something read, or a person, backs it.
    if let Some(team) = given(&a.team) {
        if a.team_corroborated || a.number_source.as_deref() == Some("manual") {
            add(&mut out, team);
        }
    }
    for sponsor in &a.sponsors {
        add(&mut out, sponsor);
    }
    if a.is_competition {
        add(&mut out, "Motorsport");
    }
    if a.is_bike {
        add(&mut out, "Motorcycle");
    }

    let mut ordered = Vec::new();
    dedupe_casefold(out, &mut HashSet::new(), &mut ordered);
    ordered
}

/// Keywords for a whole frame: the union across its vehicles.
pub fn for_frame(
    analyses: &[VehicleAnalysis],
    options: &KeywordOptions,
    numbers: Option<&NumberMap>,
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for a in analyses {
        dedupe_casefold(for_vehicle(a, options, numbers), &mut seen, &mut ordered);
    }
    ordered
}

/// A one-line description, for writers who want a caption as well.
pub fn caption_for(analyses: &[VehicleAnalysis]) -> String {
    analyses
        .iter()
        .map(VehicleAnalysis::title)
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join("; ")
}
