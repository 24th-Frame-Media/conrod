//! One vehicle crop in, everything readable off it out (`conrod/analyze.py::analyze`).
//!
//! Same order and rules as Python: registration and roundel numbers first, a
//! roundel beats whole-crop OCR, a plate-shaped number is discarded, OCR and the
//! vision model are reconciled by `merge_number`, and the known-vehicle registry
//! only fills blanks. Two deliberate differences: the whole-crop OCR runs once
//! and feeds both the number and the visible text, and a reader that fails costs
//! that reader's answer rather than the run (only a stopped or misconfigured
//! vision model ends it).

use conrod_core::analysis::{corroborated, merge_number, OcrReading, VehicleAnalysis};
use conrod_core::registry::{self, KnownVehicle};
use conrod_core::settings::Settings;
use conrod_io::settings::Settings as IoSettings;
use conrod_io::vlm::{self, VehicleDescription, VlmClient, VlmError};
use conrod_vision::imageops::Rgb;
use conrod_vision::{ocr, plates};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// The readers one worker has loaded; a reader whose setting is off is `None`.
#[derive(Default)]
pub struct Readers {
    pub plates: Option<(plates::PlateDetector, plates::PlateReader)>,
    pub ocr: Option<Arc<ocr::Ocr>>,
    pub vlm: Option<(VlmClient, IoSettings)>,
}

/// What does not change between crops of one run.
pub struct Context<'a> {
    pub settings: &'a Settings,
    pub plates: &'a plates::PlateOptions,
    /// Plate registry, keyed by normalised plate; empty when it is switched off.
    pub known: &'a HashMap<String, KnownVehicle>,
}

pub struct Analysed {
    pub analysis: VehicleAnalysis,
    /// Readers that failed on this crop, for the status area's log.
    pub failures: Vec<String>,
}

pub fn analyze(
    crop: &Rgb,
    native: Option<&Rgb>,
    kind: &str,
    is_bike: bool,
    ctx: &Context,
    readers: &mut Readers,
) -> Result<Analysed, String> {
    let s = ctx.settings;
    let mut a = VehicleAnalysis {
        kind: kind.into(),
        is_bike,
        ..VehicleAnalysis::default()
    };
    let mut failures = Vec::new();
    let Readers {
        plates: plate_models,
        ocr: text_engine,
        vlm: describer,
    } = readers;

    // 1. Registration, and any number roundels the same detector picked up.
    let mut roundels: Vec<(String, f64)> = Vec::new();
    if let (true, Some((detector, reader))) = (s.read_plates, plate_models.as_mut()) {
        match plates::scan_regions(
            crop,
            native,
            ctx.plates,
            detector,
            reader,
            text_engine.as_deref(),
        ) {
            Ok((reading, numbers)) => {
                roundels = numbers;
                if reading.text.is_some() {
                    a.plate = reading.text;
                    a.plate_state = reading.state;
                    a.plate_conf = reading.confidence;
                }
            }
            Err(e) => failures.push(format!("plate read: {e}")),
        }
    }

    // Whole-crop OCR, once: the number reads it and so does the visible text.
    let tokens = match text_engine.as_deref() {
        Some(engine) if s.read_text || (s.read_numbers && roundels.is_empty()) => {
            engine.read(crop).unwrap_or_else(|e| {
                failures.push(format!("text read: {e}"));
                Vec::new()
            })
        }
        _ => Vec::new(),
    };

    // 2. Competition number.
    let ocr_number = if s.read_numbers {
        ocr_number(&roundels, &tokens, a.plate.as_deref(), s)
    } else {
        OcrReading::default()
    };

    // 3. The semantic pass.
    let mut described = VehicleDescription::default();
    if let (true, Some((client, io))) = (s.use_vlm, describer.as_ref()) {
        match vlm::describe(client, crop, io, is_bike) {
            Ok(d) => {
                a.vlm_conf = d.confidence;
                if s.identify_make_model {
                    a.make = d.make.clone();
                    a.model = d.model.clone();
                    a.body_type = d.body_type.clone();
                }
                if s.identify_colour {
                    a.colour = d.colour.clone();
                }
                if s.identify_team {
                    a.team = d.team.clone();
                    a.sponsors = d.sponsors.clone();
                }
                a.is_competition = d.is_competition;
                described = d;
            }
            Err(e @ (VlmError::Stopped | VlmError::Misconfigured(_))) => return Err(e.to_string()),
            Err(e) => failures.push(format!("vehicle description: {e}")),
        }
    }

    // 4. Reconcile the number.
    let vlm_number = trusted_vlm_number(&described, &roundels, &ocr_number);
    let (number, source, confidence) =
        merge_number(&ocr_number, vlm_number.as_deref(), s.ocr_accept_confidence);
    a.race_number = number;
    a.number_source = source;
    a.number_conf = confidence;

    // 5. Free text, minus anything already captured as a field.
    if s.read_text {
        let key = |t: &str| -> String {
            t.to_uppercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect()
        };
        let mut seen: HashSet<String> = a
            .sponsors
            .iter()
            .chain(&a.team)
            .chain(&a.plate)
            .chain(&a.plate_state)
            .chain(&a.race_number)
            .map(|t| key(t))
            .collect();
        let exclude: Vec<String> = seen.iter().cloned().collect();
        let found = ocr::visible_text(&tokens, s, &exclude);
        // The model's livery reading and OCR's often overlap; keep the union.
        a.text = described
            .livery_text
            .iter()
            .chain(&found)
            .filter(|t| {
                let k = key(t);
                !k.is_empty() && seen.insert(k)
            })
            .take(s.max_text_items.max(0) as usize)
            .cloned()
            .collect();
        // Only OCR counts as evidence for a team name the model reported:
        // its own livery text would "confirm" its own invention.
        a.team_corroborated = corroborated(a.team.as_deref(), &found);
    }

    // Last, and only into the gaps: what was read on the day always wins.
    registry::fill(&mut a, ctx.known);

    Ok(Analysed {
        analysis: a,
        failures,
    })
}

/// The number the crop itself gives: a roundel read (a localised, upscaled
/// region) beats whole-crop OCR, and anything plate-shaped is thrown away, since
/// a registration taken for a number would be wrong in a way that is hard to see.
fn ocr_number(
    roundels: &[(String, f64)],
    tokens: &[ocr::Token],
    plate: Option<&str>,
    s: &Settings,
) -> OcrReading {
    let reading = match roundels.first() {
        Some((token, score)) => OcrReading {
            number: Some(token.clone()),
            confidence: (score + 0.15).min(1.0),
            source: "roundel".into(),
        },
        None => match ocr::read_number(tokens, s) {
            Some((number, confidence)) => OcrReading {
                number: Some(number),
                confidence,
                source: "ocr".into(),
            },
            None => OcrReading::default(),
        },
    };
    match reading.number.as_deref() {
        Some(n) if Some(n) == plate || plates::looks_like_plate(n) => OcrReading::default(),
        _ => reading,
    }
}

/// The model will put a competition number on a road car if asked hard enough
/// (a highway-patrol wagon came back as #220). When it has itself said this is
/// not a competition vehicle its number is no evidence, unless a roundel
/// detection or the OCR read agrees.
fn trusted_vlm_number(
    described: &VehicleDescription,
    roundels: &[(String, f64)],
    ocr_number: &OcrReading,
) -> Option<String> {
    let claim = described.race_number.clone()?;
    let disowned = !described.is_competition
        && roundels.is_empty()
        && ocr_number.number.as_deref() != Some(&claim);
    (!disowned).then_some(claim)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(text: &str, confidence: f64) -> ocr::Token {
        ocr::Token {
            text: text.into(),
            confidence,
            area: 0.05,
        }
    }

    #[test]
    fn a_roundel_beats_whole_crop_ocr() {
        let s = Settings::default();
        let r = ocr_number(&[("77".into(), 0.6)], &[token("12", 0.99)], None, &s);
        assert_eq!(
            (r.number.as_deref(), r.source.as_str()),
            (Some("77"), "roundel")
        );
        assert!((r.confidence - 0.75).abs() < 1e-9);
    }

    #[test]
    fn a_plate_shaped_number_is_discarded() {
        let s = Settings::default();
        assert_eq!(
            ocr_number(&[], &[token("12", 0.9)], Some("12"), &s).number,
            None
        );
        assert_eq!(
            ocr_number(&[("12".into(), 0.9)], &[], Some("12"), &s).number,
            None
        );
        assert_eq!(
            ocr_number(&[], &[token("12", 0.9)], Some("ABC123"), &s)
                .number
                .as_deref(),
            Some("12")
        );
    }

    #[test]
    fn the_models_number_on_a_road_car_is_no_evidence() {
        let road_car = VehicleDescription {
            race_number: Some("220".into()),
            is_competition: false,
            ..VehicleDescription::default()
        };
        let none = OcrReading::default();
        assert_eq!(trusted_vlm_number(&road_car, &[], &none), None);
        // ...unless OCR read the same number, or a roundel was found.
        let same = OcrReading {
            number: Some("220".into()),
            confidence: 0.5,
            source: "ocr".into(),
        };
        assert_eq!(
            trusted_vlm_number(&road_car, &[], &same).as_deref(),
            Some("220")
        );
        assert_eq!(
            trusted_vlm_number(&road_car, &[("5".into(), 0.5)], &none).as_deref(),
            Some("220")
        );
        let racer = VehicleDescription {
            is_competition: true,
            ..road_car
        };
        assert_eq!(
            trusted_vlm_number(&racer, &[], &none).as_deref(),
            Some("220")
        );
    }

    #[test]
    fn readers_are_optional_and_the_registry_only_fills_blanks() {
        let s = Settings {
            use_vlm: false,
            read_plates: false,
            read_numbers: false,
            read_text: false,
            ..Settings::default()
        };
        let mut known = HashMap::new();
        known.insert(
            "ABC123".to_string(),
            KnownVehicle {
                make: Some("Holden".into()),
                ..KnownVehicle::default()
            },
        );
        let ctx = Context {
            settings: &s,
            plates: &plates::PlateOptions::default(),
            known: &known,
        };
        let out = analyze(
            &Rgb {
                width: 1,
                height: 1,
                data: vec![0; 3],
            },
            None,
            "car",
            false,
            &ctx,
            &mut Readers::default(),
        )
        .unwrap();
        assert!(out.failures.is_empty());
        assert_eq!(out.analysis.race_number, None);
        assert_eq!(
            out.analysis.make, None,
            "no plate was read, so nothing is filled"
        );
    }
}
