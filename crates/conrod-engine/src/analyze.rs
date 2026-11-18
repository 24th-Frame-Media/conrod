//! One vehicle crop in, everything readable off it out.
//!
//! Registration and roundel numbers are read first, a roundel beats
//! whole-crop OCR, a plate-shaped number is discarded, OCR and the vision
//! model are reconciled by `merge_number`, and the known-vehicle registry
//! only fills blanks. Two notable choices: the whole-crop OCR runs once and
//! feeds both the number and the visible text, and a reader that fails costs
//! only that reader's answer rather than the run (only a stopped or
//! misconfigured vision model ends it).

use conrod_core::analysis::{corroborated, merge_number, OcrReading, VehicleAnalysis};
use conrod_core::entrylist::{self, EntryList};
use conrod_core::registry::{self, KnownVehicle};
use conrod_core::settings::Settings;
use conrod_io::settings::Settings as IoSettings;
use conrod_io::vlm::{self, VehicleDescription, VlmClient, VlmError};
use conrod_vision::imageops::Rgb;
use conrod_vision::{ocr, plates};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

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
    /// The number gates in the order they ran, for debugging a wrong number.
    pub trace: Vec<String>,
    /// The model's parsed reply, for the eval harness; null when it was not asked.
    pub vlm_reply: serde_json::Value,
    /// How many asks the model's answer took (a truncated reply is re-asked).
    pub vlm_attempts: u32,
    /// The exact question the vision model was asked; null when it was not asked.
    pub vlm_prompt: Option<String>,
}

pub fn analyze(
    crop: &Rgb,
    native: Option<&Rgb>,
    kind: &str,
    is_bike: bool,
    ctx: &Context,
    readers: &mut Readers,
) -> Result<Analysed, String> {
    analyze_with(crop, native, kind, is_bike, ctx, readers, None)
}

/// [`analyze`], with a callback for the vision prompt: it fires just before the
/// model is asked, so a live feed can show the question while the model thinks.
pub fn analyze_with(
    crop: &Rgb,
    native: Option<&Rgb>,
    kind: &str,
    is_bike: bool,
    ctx: &Context,
    readers: &mut Readers,
    on_prompt: Option<&dyn Fn(&str)>,
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
                    // ponytail: only the native-crop box translates to full-frame pixels for free
                    // (see operations.rs); a crop-space box would need cut()'s resize undone too.
                    if reading.bbox_native {
                        a.plate_bbox = reading.bbox;
                    }
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
    let mut ocr_reading = if s.read_numbers {
        ocr_number(&roundels, &tokens, a.plate.as_deref(), s)
    } else {
        OcrReading::default()
    };

    let (entries, entry_warning) = entry_list(&s.entry_list);
    failures.extend(entry_warning);

    // 3. The semantic pass.
    let policy = conrod_core::profile::ShootPreset::parse(&s.scan_profile).policy();
    let racing = policy.profile == conrod_core::profile::ScanProfile::Motorsport
        && policy.vlm.numbers;
    let mut described = VehicleDescription::default();
    let mut vlm_reply = serde_json::Value::Null;
    let mut vlm_attempts = 0;
    let mut vlm_prompt = None;
    let mut vlm_ran = false;
    if let (true, Some((client, io))) = (s.use_vlm, describer.as_ref()) {
        let hints = vlm::VlmHints {
            racing,
            candidates: entries
                .as_deref()
                .map(EntryList::candidates)
                .unwrap_or_default(),
        };
        // Build the question here as well as inside `describe_with_reply`, so a
        // live feed can show it while the model is still thinking. It is a pure
        // function of the settings and hints, so the two copies are identical.
        let prompt = vlm::vehicle_prompt(io, is_bike, &hints);
        if let Some(on_prompt) = on_prompt {
            on_prompt(&prompt);
        }
        vlm_prompt = Some(prompt);
        match vlm::describe_with_reply(client, crop, io, is_bike, &hints) {
            Ok((d, reply, attempts)) => {
                vlm_ran = true;
                vlm_reply = reply;
                vlm_attempts = attempts;
                a.vlm_conf = d.confidence;
                a.model_evidence = d.model_evidence.clone();
                a.model_suggested = d.model_suggested;
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
                a.driver = d.driver.clone();
                a.country = d.country.clone();
                a.is_competition = d.is_competition;
                described = d;
            }
            Err(e @ (VlmError::Stopped | VlmError::Misconfigured(_))) => return Err(e.to_string()),
            Err(e) => failures.push(format!("vehicle description: {e}")),
        }
    }

    // A plate read by the model sees the whole vehicle, so it is kept over the
    // plate reader's crop-level pass, which occasionally clips a character
    // ("78S" for "78SQ"). The reader still fills the field when the model gave
    // none.
    let reader_plate = a.plate.clone();
    if let Some(plate) = described.plate.clone() {
        if reader_plate.as_deref() != Some(plate.as_str()) {
            a.plate = Some(plate);
            a.plate_state = None;
            a.plate_conf = described.confidence.max(0.5);
        }
    }

    // Whole-crop OCR is the weakest number read: off a race car it takes digits
    // out of a sponsor's livery ("253" off a road-going GT-R). Where the model
    // neither claims a number nor this is a race shoot, it is not a number.
    if vlm_ran
        && !racing
        && ocr_reading.source == "ocr"
        && described.race_number.as_deref() != ocr_reading.number.as_deref()
    {
        ocr_reading = OcrReading::default();
    }

    // 4. Reconcile the number. An entry-list number the model read is trusted
    // on its own, and beats an OCR read the list does not have (OCR over-reads
    // digits off sponsor and livery text).
    let listed = |n: &str| entries.as_deref().is_some_and(|list| list.get(n).is_some());
    // With a race list, a whole-crop OCR digit run the list does not have is a
    // livery or sponsor read ("220" off a GR Supra), not a number.
    if ocr_reading.source == "ocr"
        && entries.as_deref().is_some_and(|list| !list.is_empty())
        && ocr_reading.number.as_deref().is_some_and(|n| !listed(n))
    {
        ocr_reading = OcrReading::default();
    }
    let vlm_number = s
        .read_numbers
        .then(|| {
            trusted_vlm_number(
                &described,
                &roundels,
                &ocr_reading,
                a.plate.as_deref(),
                &tokens,
                entries.as_deref(),
            )
        })
        .flatten();
    let ocr_listed = ocr_reading.number.as_deref().is_some_and(listed);
    let (number, source, confidence) = match vlm_number.as_deref() {
        Some(v) if listed(v) && !ocr_listed => merge_number(
            &OcrReading::default(),
            Some(v),
            described.number_confidence,
            s.ocr_accept_confidence,
        ),
        _ => merge_number(
            &ocr_reading,
            vlm_number.as_deref(),
            described.number_confidence,
            s.ocr_accept_confidence,
        ),
    };
    let mut trace = vec![
        format!(
            "vlm {vlm_attempts} ask(s): number {:?} ({:?}), make {:?}, model {:?}, evidence {:?}, competition {}",
            described.race_number,
            described.number_confidence,
            described.make,
            described.model,
            described.model_evidence,
            described.is_competition
        ),
        format!(
            "ocr number {:?} from {} (listed={ocr_listed}), roundels {roundels:?}",
            ocr_reading.number, ocr_reading.source
        ),
        format!("trusted vlm number {vlm_number:?}"),
        format!("merged {number:?} from {source:?} at {confidence:.2}"),
        format!(
            "plate model {:?} / reader {reader_plate:?} -> {:?}",
            described.plate, a.plate
        ),
    ];
    // Agreement can score above the model's own confidence; keep that ceiling,
    // so a nose-board read keeps its penalty even when OCR agrees.
    let from_vlm = vlm_number.is_some() && number == vlm_number;
    let confidence = if from_vlm {
        confidence.min(described.number_confidence)
    } else {
        confidence
    };
    a.race_number = number;
    a.number_source = source;
    a.number_conf = confidence;

    // 4b. A driver name on the car identifies the entry more reliably than a
    // single number read: car 2's nose board says 1, but "Wood" and "Evans" on
    // the windscreen are entry 2. Only a unique driver match corrects the
    // number; a team alone runs two cars and proves nothing.
    if let Some(list) = entries.as_deref() {
        if !list.is_empty() {
            if let Some(entry) = evidence_entry(&a, &described, list) {
                let agrees = a
                    .race_number
                    .as_deref()
                    .is_some_and(|number| list.get(number) == Some(entry));
                if !agrees {
                    trace.push(format!(
                        "entry evidence: car {} from a driver name replaces number {:?} at {:.2}",
                        entry.number, a.race_number, a.number_conf
                    ));
                    a.race_number = Some(entry.number.clone());
                    a.number_source = Some("entry".into());
                    a.number_conf = 0.9;
                }
            }
        }
    }

    let entry_team = entries
        .as_deref()
        .is_some_and(|list| apply_entry(&mut a, list));
    trace.push(format!(
        "entry list: team {:?} (from list={entry_team}), make {:?}, model {:?}",
        a.team, a.make, a.model
    ));

    // 5. Free text, minus anything already captured as a field.
    a.team_corroborated = entry_team;
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
            .chain(&a.driver)
            .chain(&a.country)
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
        a.team_corroborated = entry_team || corroborated(a.team.as_deref(), &found);
    }

    // Last, and only into the gaps: what was read on the day always wins.
    registry::fill(&mut a, ctx.known);

    Ok(Analysed {
        analysis: a,
        failures,
        trace,
        vlm_reply,
        vlm_attempts,
        vlm_prompt,
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
        Some(n) => {
            if Some(n) == plate || plates::looks_like_plate(n) {
                return OcrReading::default();
            }
            if let Some(p) = plate {
                let p_digits: String = p.chars().filter(|c| c.is_ascii_digit()).collect();
                if !p_digits.is_empty()
                    && (p_digits == n
                        || p_digits.trim_start_matches('0') == n.trim_start_matches('0'))
                {
                    return OcrReading::default();
                }
            }
            reading
        }
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
    plate: Option<&str>,
    tokens: &[ocr::Token],
    list: Option<&EntryList>,
) -> Option<String> {
    let claim = described.race_number.clone()?;
    let digits: String = claim.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 3 {
        return None;
    }
    if let Ok(y) = digits.parse::<u32>() {
        if (1900..=2099).contains(&y) {
            return None;
        }
    }
    if let Some(p) = plate {
        let p_digits: String = p.chars().filter(|c| c.is_ascii_digit()).collect();
        if !p_digits.is_empty()
            && (p_digits == digits
                || p_digits.trim_start_matches('0') == digits.trim_start_matches('0'))
        {
            return None;
        }
    }
    if list.is_some_and(|l| l.get(&claim).is_some()) {
        return Some(claim);
    }
    if !tokens.is_empty()
        && digits.len() >= 3
        && roundels.is_empty()
        && ocr_number.number.as_deref() != Some(&claim)
    {
        return None;
    }
    let disowned = !described.is_competition
        && roundels.is_empty()
        && ocr_number.number.as_deref() != Some(&claim);
    (!disowned).then_some(claim)
}

type EntryCache = Option<(String, Option<SystemTime>, Option<Arc<EntryList>>)>;
static ENTRY_LIST: Mutex<EntryCache> = Mutex::new(None);

/// The race entry list at `path`, loaded once and reloaded when the file's
/// modified time changes. The warning is only returned by the load that failed,
/// so a bad file is reported once rather than per crop.
fn entry_list(path: &str) -> (Option<Arc<EntryList>>, Option<String>) {
    let path = path.trim();
    if path.is_empty() {
        return (None, None);
    }
    let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let mut cache = ENTRY_LIST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((p, t, list)) = cache.as_ref() {
        if p == path && *t == stamp {
            return (list.clone(), None);
        }
    }
    let (list, warning) = match entrylist::load(Path::new(path)) {
        Ok(list) => (Some(Arc::new(list)), None),
        Err(e) => (None, Some(format!("entry list ignored: {e}"))),
    };
    *cache = Some((path.to_string(), stamp, list.clone()));
    (list, warning)
}

/// Fills the car from the entry list, where the final number is in it: team,
/// drivers, make/model (the list wins over a missing, suggested or differing
/// model read) and sponsors spelled as the list has them. A number missing from
/// The one entry a driver name on the car points at. The model's visible text
/// is the strongest source; its own driver/team fields are the fallback. Team
/// names alone never decide because one team runs two cars.
fn evidence_entry<'a>(
    a: &VehicleAnalysis,
    described: &VehicleDescription,
    list: &'a EntryList,
) -> Option<&'a conrod_core::entrylist::Entry> {
    let mut evidence = described.livery_text.clone();
    evidence.extend(a.driver.iter().cloned());
    evidence.extend(a.team.iter().cloned());
    list.from_evidence(&evidence)
}

/// a non-empty list is kept but trusted half as much, and fills nothing.
/// Returns whether the team came from the list.
fn apply_entry(a: &mut VehicleAnalysis, list: &EntryList) -> bool {
    if list.is_empty() {
        return false;
    }
    let Some(entry) = a.race_number.as_deref().and_then(|n| list.get(n)) else {
        if a.race_number.is_some() {
            a.number_conf *= 0.5;
        }
        canonical_sponsors(a, &list.sponsors());
        return false;
    };
    if let Some(team) = &entry.team {
        a.team = Some(team.clone());
    }
    if !entry.drivers.is_empty() {
        a.driver = Some(entry.drivers.join(", "));
    }
    let differs = |have: &Option<String>, want: &Option<String>| {
        want.as_deref()
            .is_some_and(|w| !have.as_deref().is_some_and(|h| h.eq_ignore_ascii_case(w)))
    };
    let disputed = a.make.is_none()
        || a.model.is_none()
        || a.model_suggested
        || differs(&a.make, &entry.make)
        || differs(&a.model, &entry.model);
    if disputed && (entry.make.is_some() || entry.model.is_some()) {
        if let Some(make) = &entry.make {
            a.make = Some(make.clone());
        }
        if let Some(model) = &entry.model {
            a.model = Some(model.clone());
            a.model_suggested = false;
            a.model_evidence = Some("entry_list".into());
        }
    }
    if entry.sponsors.is_empty() {
        canonical_sponsors(a, &list.sponsors());
    } else {
        canonical_sponsors(a, &entry.sponsors);
    }
    entry.team.is_some()
}

/// Spells each read sponsor as the list does where they match; unmatched reads
/// are kept as read, and nothing is added from the list.
fn canonical_sponsors(a: &mut VehicleAnalysis, known: &[String]) {
    let mut out: Vec<String> = Vec::new();
    for read in &a.sponsors {
        let name = known.iter().find(|k| same_sponsor(read, k)).unwrap_or(read);
        if !out.contains(name) {
            out.push(name.clone());
        }
    }
    a.sponsors = out;
}

fn same_sponsor(read: &str, known: &str) -> bool {
    let (r, k) = (sponsor_key(read), sponsor_key(known));
    if r.is_empty() || k.is_empty() {
        return false;
    }
    let (short, long) = if r.len() <= k.len() { (&r, &k) } else { (&k, &r) };
    (short.len() >= 4 && long.contains(short.as_str())) || similar(&r, &k)
}

fn sponsor_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Edit distance ratio of at least 0.85 between the longer string's length.
fn similar(a: &str, b: &str) -> bool {
    let longest = a.chars().count().max(b.chars().count());
    longest > 0 && 1.0 - edit_distance(a, b) as f64 / longest as f64 >= 0.85
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
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

    const TWO_ENTRIES: &str = "number,team,drivers,make,model,sponsors\n\
        7,Team Alpha,Ann Ash; Ben Birch,Ford,Mustang,Mobil 1;Optus\n\
        42,Team Beta,Cal Cole,Toyota,GR Supra,Liqui Moly\n";

    #[test]
    fn a_number_in_the_list_fills_the_car_from_it() {
        let list = entrylist::parse(TWO_ENTRIES).unwrap();
        let mut a = VehicleAnalysis {
            race_number: Some("7".into()),
            number_conf: 0.9,
            make: Some("Ford".into()),
            model: Some("Focus".into()),
            model_suggested: true,
            sponsors: vec!["MOBIL 1".into(), "Random Co".into()],
            ..VehicleAnalysis::default()
        };
        assert!(apply_entry(&mut a, &list));
        assert_eq!(a.team.as_deref(), Some("Team Alpha"));
        assert_eq!(a.driver.as_deref(), Some("Ann Ash, Ben Birch"));
        assert_eq!(a.make.as_deref(), Some("Ford"));
        assert_eq!(a.model.as_deref(), Some("Mustang"));
        assert!(!a.model_suggested);
        assert_eq!(a.model_evidence.as_deref(), Some("entry_list"));
        assert_eq!(a.sponsors, ["Mobil 1", "Random Co"]);
        assert_eq!(a.number_conf, 0.9);
    }

    #[test]
    fn a_number_missing_from_the_list_is_kept_but_doubted() {
        let list = entrylist::parse(TWO_ENTRIES).unwrap();
        let mut a = VehicleAnalysis {
            race_number: Some("99".into()),
            number_conf: 0.8,
            ..VehicleAnalysis::default()
        };
        assert!(!apply_entry(&mut a, &list));
        assert_eq!(a.race_number.as_deref(), Some("99"));
        assert!((a.number_conf - 0.4).abs() < 1e-9);
        assert_eq!(a.team, None);
        assert_eq!(a.driver, None);
    }

    #[test]
    fn a_driver_name_corrects_a_conflicting_number() {
        let list = entrylist::parse(
            r#"[{"number":"2","drivers":["Ryan Wood","Jaxon Evans"]},
                {"number":"1","drivers":["Chaz Mostert"]}]"#,
        )
        .unwrap();
        let a = VehicleAnalysis {
            race_number: Some("1".into()),
            number_conf: 0.95,
            driver: Some("Wood".into()),
            ..VehicleAnalysis::default()
        };
        let entry = evidence_entry(&a, &VehicleDescription::default(), &list).unwrap();
        assert_eq!(entry.number, "2");
        // A sponsor name that happens to match a driver is not enough unless
        // it names the driver; the entry list method itself is tested there.
        assert!(evidence_entry(
            &VehicleAnalysis::default(),
            &VehicleDescription::default(),
            &list
        )
        .is_none());
    }

    #[test]
    fn leading_zeros_find_the_entry() {
        let list = entrylist::parse(TWO_ENTRIES).unwrap();
        let mut a = VehicleAnalysis {
            race_number: Some("0042".into()),
            number_conf: 0.9,
            ..VehicleAnalysis::default()
        };
        assert!(apply_entry(&mut a, &list));
        assert_eq!(a.team.as_deref(), Some("Team Beta"));
        assert_eq!(a.model.as_deref(), Some("GR Supra"));
    }

    #[test]
    fn an_empty_list_changes_nothing() {
        let list = entrylist::parse("number,team\n").unwrap();
        let mut a = VehicleAnalysis {
            race_number: Some("7".into()),
            number_conf: 0.9,
            ..VehicleAnalysis::default()
        };
        assert!(!apply_entry(&mut a, &list));
        assert_eq!(a.race_number.as_deref(), Some("7"));
        assert_eq!(a.number_conf, 0.9);
        assert_eq!(a.team, None);
    }

    #[test]
    fn an_empty_path_or_a_missing_file_is_no_list() {
        assert!(entry_list("").0.is_none());
        assert!(entry_list("   ").0.is_none());
        let missing = std::env::temp_dir().join("conrod-no-such-entry-list.csv");
        let (list, warning) = entry_list(&missing.to_string_lossy());
        assert!(list.is_none());
        assert!(warning.is_some());
    }

    #[test]
    fn sponsor_spellings_are_matched_loosely() {
        assert!(same_sponsor("Optus Sport", "Optus"));
        assert!(same_sponsor("Liqui-Moly", "Liqui Moly"));
        assert!(!same_sponsor("Castrol", "Mobil 1"));
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
        assert_eq!(trusted_vlm_number(&road_car, &[], &none, None, &[], None), None);
        // ...unless OCR read the same number, or a roundel was found.
        let same = OcrReading {
            number: Some("220".into()),
            confidence: 0.5,
            source: "ocr".into(),
        };
        assert_eq!(
            trusted_vlm_number(&road_car, &[], &same, None, &[], None).as_deref(),
            Some("220")
        );
        assert_eq!(
            trusted_vlm_number(&road_car, &[("5".into(), 0.5)], &none, None, &[], None).as_deref(),
            Some("220")
        );
        let racer = VehicleDescription {
            is_competition: true,
            ..road_car
        };
        assert_eq!(
            trusted_vlm_number(&racer, &[], &none, None, &[], None).as_deref(),
            Some("220")
        );
    }

    #[test]
    fn a_listed_number_is_trusted_without_ocr_agreement() {
        let list = entrylist::parse("number,team\n142,Team Gamma\n").unwrap();
        let racer = VehicleDescription {
            race_number: Some("142".into()),
            is_competition: true,
            ..VehicleDescription::default()
        };
        let none = OcrReading::default();
        let tok = vec![token("GAMMA", 0.9)];
        assert_eq!(
            trusted_vlm_number(&racer, &[], &none, None, &tok, Some(&list)).as_deref(),
            Some("142")
        );
        assert_eq!(trusted_vlm_number(&racer, &[], &none, None, &tok, None), None);
    }

    #[test]
    fn trusted_vlm_number_rejects_hallucinations_and_plate_substrings() {
        let racer = VehicleDescription {
            race_number: Some("100".into()),
            is_competition: true,
            ..VehicleDescription::default()
        };
        let none = OcrReading::default();
        // Rejected because "100" is part of registration plate "TZ100"
        assert_eq!(
            trusted_vlm_number(&racer, &[], &none, Some("TZ100"), &[], None),
            None
        );

        // Rejected because 4-digit year
        let year_car = VehicleDescription {
            race_number: Some("2023".into()),
            is_competition: true,
            ..VehicleDescription::default()
        };
        assert_eq!(trusted_vlm_number(&year_car, &[], &none, None, &[], None), None);

        // Rejected uncorroborated 3-digit number when OCR tokens are present
        let tok = vec![token("FORD", 0.9)];
        assert_eq!(trusted_vlm_number(&racer, &[], &none, None, &tok, None), None);
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
