//! Checks the Rust port against what the Python implementation did.
//!
//! The fixtures are written by `tools/gen_golden.py`. A failure here after
//! regenerating them means the two implementations have diverged.

use conrod_core::{framing, ridge};
use serde_json::{Map, Value};
use std::fs;
use std::path::Path;

fn fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(format!("{name}.json"));
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("fixture is valid JSON")
}

fn floats(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

fn matrix(value: &Value) -> Vec<Vec<f64>> {
    value.as_array().unwrap().iter().map(floats).collect()
}

/// numpy and this sum in different orders, so equal to rounding error and no
/// closer; 1e-8 is far below anything a rating could notice.
fn assert_close(got: f64, want: f64, what: &str) {
    let tolerance = 1e-8 * want.abs().max(1.0);
    assert!(
        (got - want).abs() <= tolerance,
        "{what}: got {got}, want {want}"
    );
}

fn assert_all_close(got: &[f64], want: &[f64], what: &str) {
    assert_eq!(got.len(), want.len(), "{what}: length");
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        assert_close(*g, *w, &format!("{what}[{i}]"));
    }
}

#[test]
fn framing_matches_python() {
    let fixture = fixture("framing");
    let cases = fixture["cases"].as_array().unwrap();
    assert!(cases.len() > 50, "fixture looks truncated");
    for case in cases {
        let bbox = case["box"].as_array().map(|b| {
            let n = floats(&Value::Array(b.clone()));
            [n[0], n[1], n[2], n[3]]
        });
        let (w, h) = (
            case["width"].as_i64().unwrap(),
            case["height"].as_i64().unwrap(),
        );
        let got = framing::assess(bbox, w, h);
        let label = format!("{bbox:?} in {w}x{h}");
        assert_eq!(
            u64::from(got.sides),
            case["sides"].as_u64().unwrap(),
            "{label}: sides"
        );
        // Same operations in the same order, so bit-identical, not just close.
        assert_eq!(
            got.factor,
            case["factor"].as_f64().unwrap(),
            "{label}: factor"
        );
        assert_eq!(
            got.cut_off(),
            case["cut_off"].as_bool().unwrap(),
            "{label}: cut_off"
        );
        assert_eq!(
            framing::describe(&got),
            case["describe"].as_str().unwrap(),
            "{label}: text"
        );
    }
}

#[test]
fn ridge_matches_python() {
    let fixture = fixture("ridge");
    let mut fitted = 0;
    let mut refused = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let vectors = matrix(&case["vectors"]);
        let stars = floats(&case["stars"]);
        let probes = matrix(&case["probe"]);
        let want = &case["model"];
        let kind = case["kind"].as_str().unwrap();

        match kind {
            "sharp_model" => match ridge::fit_sharp(&vectors, &stars) {
                None => {
                    assert!(want.is_null(), "Rust refused a fit Python made");
                    refused += 1;
                }
                Some(model) => {
                    assert!(!want.is_null(), "Rust fitted what Python refused");
                    assert_eq!(
                        model.trained_on,
                        want["trained_on"].as_u64().unwrap() as usize
                    );
                    assert_all_close(&model.mean, &floats(&want["mean"]), "mean");
                    assert_all_close(&model.spread, &floats(&want["spread"]), "spread");
                    assert_all_close(&model.weights, &floats(&want["weights"]), "weights");
                    assert_close(
                        model.intercept,
                        want["intercept"].as_f64().unwrap(),
                        "intercept",
                    );
                    let expected = floats(&case["predictions"]);
                    for (probe, want) in probes.iter().zip(&expected) {
                        let got = ridge::predict_sharp(&model, probe).unwrap();
                        assert_close(got, *want, "prediction");
                    }
                    // A model Python wrote must load and predict identically.
                    let loaded: ridge::SharpModel = serde_json::from_value(want.clone()).unwrap();
                    for (probe, want) in probes.iter().zip(&expected) {
                        assert_close(
                            ridge::predict_sharp(&loaded, probe).unwrap(),
                            *want,
                            "loaded",
                        );
                    }
                    fitted += 1;
                }
            },
            "taste" => match ridge::fit_taste(&vectors, &stars) {
                None => {
                    assert!(want.is_null());
                    refused += 1;
                }
                Some(model) => {
                    assert_all_close(&model.weights, &floats(&want["weights"]), "taste weights");
                    let expected: Vec<i64> = case["predictions"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_i64().unwrap())
                        .collect();
                    for (probe, want) in probes.iter().zip(&expected) {
                        assert_eq!(
                            i64::from(ridge::predict_taste(&model, probe).unwrap()),
                            *want
                        );
                    }
                    fitted += 1;
                }
            },
            other => panic!("unknown fixture kind {other}"),
        }
    }
    assert_eq!(
        (fitted, refused),
        (5, 2),
        "every fixture case must be exercised"
    );
}

#[test]
fn a_model_from_another_feature_version_is_ignored() {
    let model = ridge::SharpModel {
        version: ridge::FEATURE_VERSION + 1,
        trained_on: 1,
        mean: vec![0.0],
        spread: vec![1.0],
        weights: vec![1.0],
        intercept: 0.0,
    };
    assert_eq!(ridge::predict_sharp(&model, &[1.0]), None);
}

#[test]
fn a_vector_of_the_wrong_length_is_not_guessed_at() {
    let model = ridge::SharpModel {
        version: ridge::FEATURE_VERSION,
        trained_on: 1,
        mean: vec![0.0, 0.0],
        spread: vec![1.0, 1.0],
        weights: vec![1.0, 1.0],
        intercept: 0.0,
    };
    assert_eq!(ridge::predict_sharp(&model, &[1.0]), None);
}

// --- bursts -------------------------------------------------------------------

use conrod_core::{analysis::VehicleAnalysis, bursts, keywords, mapping::NumberMap, marques};

fn opt_f64(v: &Value) -> Option<f64> {
    v.as_f64()
}

fn opt_str(v: &Value) -> Option<&str> {
    v.as_str()
}

#[test]
fn bursts_match_python() {
    let fixture = fixture("bursts");
    for case in fixture["cases"].as_array().unwrap() {
        let tags = case["tags"].as_object().unwrap();
        assert_eq!(
            bursts::camera_of(tags, "fallback cam"),
            case["camera"].as_str().unwrap(),
            "camera of {tags:?}"
        );
        let got = bursts::taken_at(tags);
        let want = opt_f64(&case["taken"]);
        assert_eq!(got, want, "taken_at {tags:?}");
    }

    let rows: Vec<bursts::Tags> = fixture["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_object().unwrap().clone())
        .collect();
    let frames = bursts::describe(
        &rows,
        fixture["fallback"].as_str().unwrap(),
        bursts::BURST_GAP_SECONDS,
    );
    let want = fixture["frames"].as_array().unwrap();
    assert_eq!(frames.len(), want.len());
    for (got, want) in frames.iter().zip(want) {
        assert_eq!(got.path, want["path"].as_str().unwrap());
        assert_eq!(got.camera, want["camera"].as_str().unwrap(), "{}", got.path);
        assert_eq!(got.taken, opt_f64(&want["taken"]), "{}", got.path);
        assert_eq!(
            u64::from(got.burst),
            want["burst"].as_u64().unwrap(),
            "{}",
            got.path
        );
    }

    let collected = bursts::collect(&frames);
    let want = fixture["bursts"].as_array().unwrap();
    assert_eq!(collected.len(), want.len());
    for (got, want) in collected.iter().zip(want) {
        assert_eq!(u64::from(got.key), want["key"].as_u64().unwrap());
        assert_eq!(got.camera, want["camera"].as_str().unwrap());
        let paths: Vec<&str> = want["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p.as_str().unwrap())
            .collect();
        assert_eq!(got.frames, paths);
        assert_eq!(got.started, opt_f64(&want["started"]));
        assert_eq!(got.ended, opt_f64(&want["ended"]));
    }
}

#[test]
fn marques_match_python() {
    for case in fixture("marques")["cases"].as_array().unwrap() {
        let (make, model) = (opt_str(&case["make"]), opt_str(&case["model"]));
        assert_eq!(
            marques::correct_make(make, model).as_deref(),
            opt_str(&case["out"]),
            "{make:?} {model:?}"
        );
    }
}

#[test]
fn entry_lists_match_python() {
    let fixture = fixture("mapping");
    let map = NumberMap::parse(fixture["csv"].as_str().unwrap()).unwrap();
    // The fixture's rows were key-sorted by the JSON writer, so compare as
    // sets here; the lookups below carry the real column order.
    let want = fixture["rows"].as_object().unwrap();
    assert_eq!(map.rows.len(), want.len());
    for (key, cells) in want {
        let got: std::collections::BTreeMap<&str, &str> = map.rows[key]
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let want: std::collections::BTreeMap<&str, &str> = cells
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str().unwrap()))
            .collect();
        assert_eq!(got, want, "row {key}");
    }
    for lookup in fixture["lookups"].as_array().unwrap() {
        let (number, prefix) = (
            lookup["number"].as_str().unwrap(),
            lookup["prefix"].as_str().unwrap(),
        );
        let want: Vec<&str> = lookup["keywords"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect();
        assert_eq!(
            map.keywords_for(number, prefix),
            want,
            "{number:?} {prefix:?}"
        );
        assert_eq!(
            map.describe(number),
            lookup["describe"].as_str().unwrap(),
            "{number:?}"
        );
    }
    assert!(fixture["bad_has_error"].as_bool().unwrap());
    assert!(NumberMap::parse("driver,team\nA,B\n").is_err());
    assert!(NumberMap::parse("").unwrap().is_empty());
}

#[test]
fn keywords_and_titles_match_python() {
    let fixture = fixture("keywords");
    let map = NumberMap::parse(fixture["csv"].as_str().unwrap()).unwrap();
    let strings = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap().to_string())
            .collect()
    };
    for case in fixture["cases"].as_array().unwrap() {
        let a = VehicleAnalysis::from_map(case["analysis"].as_object().unwrap());
        let options = keywords::KeywordOptions {
            prefix: case["prefix"].as_str().unwrap().into(),
            write_plate: case["write_plate"].as_bool().unwrap(),
        };
        let label = format!("{} / {:?}", case["analysis"], options.prefix);
        assert_eq!(a.title(), case["title"].as_str().unwrap(), "title {label}");
        assert_eq!(
            keywords::for_vehicle(&a, &options, None),
            strings(&case["keywords"]),
            "{label}"
        );
        assert_eq!(
            keywords::for_vehicle(&a, &options, Some(&map)),
            strings(&case["keywords_with_map"]),
            "with map {label}"
        );
    }
    let frame = &fixture["frame"];
    let analyses: Vec<VehicleAnalysis> = frame["analyses"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| VehicleAnalysis::from_map(a.as_object().unwrap()))
        .collect();
    let options = keywords::KeywordOptions {
        prefix: String::new(),
        write_plate: true,
    };
    assert_eq!(
        keywords::for_frame(&analyses, &options, Some(&map)),
        strings(&frame["keywords"])
    );
    assert_eq!(
        keywords::caption_for(&analyses),
        frame["caption"].as_str().unwrap()
    );
}

// --- grouping ------------------------------------------------------------------

use conrod_core::grouping;
use std::collections::{HashMap, HashSet};

fn strvec(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn strset(v: &Value) -> HashSet<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

fn out_map(v: &Value) -> HashMap<i64, i64> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, val)| (k.parse::<i64>().unwrap(), val.as_i64().unwrap()))
        .collect()
}

#[test]
fn grouping_signature_helpers_match_python() {
    for case in fixture("grouping")["signature"].as_array().unwrap() {
        let (a, b) = (case["a"].as_str().unwrap(), case["b"].as_str().unwrap());
        let min_colour = case["min_colour"].as_f64().unwrap();
        let max_bits = case["max_bits"].as_i64().unwrap();
        let label = format!("{a:?} / {b:?}");
        assert_eq!(
            grouping::shape_distance(a, b),
            case["shape_distance"].as_i64().unwrap(),
            "shape_distance {label}"
        );
        assert_eq!(
            grouping::colour_matches(a, b, min_colour),
            case["colour_matches"].as_bool().unwrap(),
            "colour_matches {label}"
        );
        assert_eq!(
            grouping::similar(a, b, max_bits, min_colour),
            case["similar"].as_bool().unwrap(),
            "similar {label}"
        );
    }
}

#[test]
fn grouping_plate_helpers_match_python() {
    let fixture = fixture("grouping");
    let plate = &fixture["plate"];
    for case in plate["tidy_plate"].as_array().unwrap() {
        assert_eq!(
            grouping::tidy_plate(opt_str(&case["value"])),
            opt_str(&case["out"]).map(str::to_string),
            "tidy_plate {:?}",
            case["value"]
        );
    }
    for case in plate["near_plate"].as_array().unwrap() {
        let (a, b) = (case["a"].as_str().unwrap(), case["b"].as_str().unwrap());
        assert_eq!(
            grouping::near_plate(a, b),
            case["out"].as_bool().unwrap(),
            "near_plate {a:?}/{b:?}"
        );
    }
    for case in plate["nearly_seen"].as_array().unwrap() {
        let seen = strset(&case["seen"]);
        assert_eq!(
            grouping::nearly_seen(opt_str(&case["plate"]), &seen),
            case["out"].as_bool().unwrap(),
            "nearly_seen {:?}",
            case["plate"]
        );
    }
    for case in plate["plate_verdict"].as_array().unwrap() {
        let seen = strset(&case["seen"]);
        assert_eq!(
            grouping::plate_verdict(opt_str(&case["plate"]), &seen),
            case["out"].as_bool(),
            "plate_verdict {:?}",
            case["plate"]
        );
    }
    for case in plate["same_make"].as_array().unwrap() {
        assert_eq!(
            grouping::same_make(opt_str(&case["a"]), opt_str(&case["b"])),
            case["out"].as_bool().unwrap(),
            "same_make {:?}/{:?}",
            case["a"],
            case["b"]
        );
    }
}

#[test]
fn grouping_paint_helpers_match_python() {
    let fixture = fixture("grouping");
    let swatch = &fixture["swatch"];
    for case in swatch["rgb"].as_array().unwrap() {
        let want = case["out"].as_array().map(|a| {
            (
                a[0].as_u64().unwrap() as u8,
                a[1].as_u64().unwrap() as u8,
                a[2].as_u64().unwrap() as u8,
            )
        });
        assert_eq!(
            grouping::rgb(case["value"].as_str().unwrap()),
            want,
            "rgb {:?}",
            case["value"]
        );
    }
    for case in swatch["swatch_matches"].as_array().unwrap() {
        let max_swatch = case["max_swatch"].as_i64().unwrap();
        assert_eq!(
            grouping::swatch_matches(opt_str(&case["a"]), opt_str(&case["b"]), max_swatch),
            case["out"].as_bool().unwrap(),
            "swatch_matches {:?}/{:?}",
            case["a"],
            case["b"]
        );
    }
    for case in fixture["median_hex"].as_array().unwrap() {
        let values = strvec(&case["values"]);
        assert_eq!(
            grouping::median_hex(&values),
            opt_str(&case["out"]).map(str::to_string),
            "median_hex {values:?}"
        );
    }
}

#[test]
fn grouping_voting_helpers_match_python() {
    let fixture = fixture("grouping");
    for case in fixture["edit_distance"].as_array().unwrap() {
        let (a, b) = (case["a"].as_str().unwrap(), case["b"].as_str().unwrap());
        let limit = case["limit"].as_u64().unwrap() as usize;
        assert_eq!(
            grouping::edit_distance(a, b, limit),
            case["out"].as_u64().unwrap() as usize,
            "edit_distance {a:?}/{b:?}"
        );
    }
    for case in fixture["accumulate"].as_array().unwrap() {
        let members: Vec<Map<String, Value>> = case["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_object().unwrap().clone())
            .collect();
        let key = case["key"].as_str().unwrap();
        assert_eq!(
            grouping::accumulate(&members, key),
            strvec(&case["out"]),
            "accumulate {key}"
        );
    }
    for case in fixture["vote"].as_array().unwrap() {
        let values: Vec<Option<String>> = case["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().map(str::to_string))
            .collect();
        let (value, hits) = grouping::vote(&values);
        assert_eq!(
            value,
            opt_str(&case["value"]).map(str::to_string),
            "vote value {values:?}"
        );
        assert_eq!(hits, case["hits"].as_f64().unwrap(), "vote hits {values:?}");
    }
    for case in fixture["proposed_makes"].as_array().unwrap() {
        let members: Vec<Map<String, Value>> = case["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_object().unwrap().clone())
            .collect();
        let mut got: Vec<String> = grouping::proposed_makes(&members).into_iter().collect();
        got.sort();
        assert_eq!(
            got,
            strvec(&case["out"]),
            "proposed_makes {:?}",
            case["members"]
        );
    }
    for case in fixture["plain"].as_array().unwrap() {
        assert_eq!(
            grouping::plain(case["text"].as_str().unwrap()),
            case["out"].as_str().unwrap()
        );
    }
}

#[test]
fn grouping_own_reading_matches_python() {
    let fixture = fixture("grouping");
    for case in fixture["own_reading"]["remember_own_reading"]
        .as_array()
        .unwrap()
    {
        let mut current = case["before"].as_object().unwrap().clone();
        grouping::remember_own_reading(&mut current);
        assert_eq!(
            Value::Object(current),
            case["after"].clone(),
            "remember_own_reading {:?}",
            case["before"]
        );
    }
    for case in fixture["own_reading"]["use_own_reading"]
        .as_array()
        .unwrap()
    {
        let mut parsed = case["before"].as_object().unwrap().clone();
        grouping::use_own_reading(&mut parsed);
        assert_eq!(
            Value::Object(parsed),
            case["after"].clone(),
            "use_own_reading {:?}",
            case["before"]
        );
    }
}

#[test]
fn grouping_consensus_matches_python() {
    for case in fixture("grouping")["consensus"].as_array().unwrap() {
        let members: Vec<Map<String, Value>> = case["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_object().unwrap().clone())
            .collect();
        let got = grouping::consensus(&members);
        let want = &case["out"];
        let label = format!("{:?}", case["members"]);
        assert_eq!(
            got.make,
            opt_str(&want["make"]).map(str::to_string),
            "make {label}"
        );
        assert_eq!(
            got.model,
            opt_str(&want["model"]).map(str::to_string),
            "model {label}"
        );
        assert_eq!(
            got.colour,
            opt_str(&want["colour"]).map(str::to_string),
            "colour {label}"
        );
        assert_eq!(
            got.race_number,
            opt_str(&want["race_number"]).map(str::to_string),
            "race_number {label}"
        );
        assert_eq!(
            got.plate,
            opt_str(&want["plate"]).map(str::to_string),
            "plate {label}"
        );
        assert_eq!(
            got.colour_hex,
            opt_str(&want["colour_hex"]).map(str::to_string),
            "colour_hex {label}"
        );
        assert_eq!(
            got.team,
            opt_str(&want["team"]).map(str::to_string),
            "team {label}"
        );
        assert_eq!(got.sponsors, strvec(&want["sponsors"]), "sponsors {label}");
        assert_eq!(
            got.livery_text,
            strvec(&want["livery_text"]),
            "livery_text {label}"
        );
        assert_eq!(
            got.agreement,
            want["agreement"].as_f64().unwrap(),
            "agreement {label}"
        );
        assert_eq!(
            got.size,
            want["size"].as_u64().unwrap() as usize,
            "size {label}"
        );
        assert_eq!(got.disputed, strvec(&want["disputed"]), "disputed {label}");
    }
}

#[test]
fn cluster_by_look_matches_python() {
    for case in fixture("grouping")["cluster_by_look"].as_array().unwrap() {
        let rows: Vec<grouping::LookRow> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| grouping::LookRow {
                det_id: r["det_id"].as_i64().unwrap(),
                vector: r["vector"]
                    .as_array()
                    .map(|a| a.iter().map(|v| v.as_f64().unwrap()).collect()),
                frame_index: r["frame_index"].as_i64().unwrap(),
                burst: r["burst"].as_i64(),
                plate: r["plate"].as_str().map(str::to_string),
            })
            .collect();
        let same_car = case["same_car"].as_f64().unwrap();
        let got = grouping::cluster_by_look(&rows, same_car);
        assert_eq!(
            got,
            out_map(&case["out"]),
            "cluster_by_look {:?}",
            case["rows"]
        );
    }
}

#[test]
fn cluster_matches_python() {
    for case in fixture("grouping")["cluster"].as_array().unwrap() {
        let rows: Vec<grouping::SignatureRow> = case["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| grouping::SignatureRow {
                det_id: r["det_id"].as_i64().unwrap(),
                signature: r["signature"].as_str().unwrap().to_string(),
                frame_index: r["frame_index"].as_i64().unwrap(),
                swatch: r["swatch"].as_str().map(str::to_string),
                cls: r["cls"].as_str().map(str::to_string),
                make: r["make"].as_str().map(str::to_string),
                plate: r["plate"].as_str().map(str::to_string),
                burst: r["burst"].as_i64(),
            })
            .collect();
        let o = &case["options"];
        let options = grouping::ClusterOptions {
            max_bits: o["max_bits"].as_i64().unwrap(),
            min_colour: o["min_colour"].as_f64().unwrap(),
            frame_window: o["frame_window"].as_i64().unwrap(),
            max_swatch: o["max_swatch"].as_i64().unwrap(),
        };
        let got = grouping::cluster(&rows, &options);
        assert_eq!(got, out_map(&case["out"]), "cluster {:?}", case["rows"]);
    }
}

#[test]
fn a_broken_stored_analysis_is_the_default_not_an_error() {
    for raw in [
        None,
        Some(""),
        Some("not json"),
        Some("[1,2]"),
        Some("{\"make\": 5}"),
    ] {
        let a = VehicleAnalysis::from_json(raw);
        assert_eq!(a.make, None);
        assert_eq!(a.kind, "car");
    }
}

// --- normalise ------------------------------------------------------------

use conrod_core::normalise::{self, Reading};

fn reading_from_json(v: &Value) -> Reading {
    Reading {
        make: v["make"].as_str().unwrap().to_string(),
        model: v["model"].as_str().unwrap().to_string(),
        count: v["count"].as_i64().unwrap(),
        stated: v["stated"].as_bool().unwrap(),
    }
}

fn readings_from_json(v: &Value) -> Vec<Reading> {
    v.as_array()
        .unwrap()
        .iter()
        .map(reading_from_json)
        .collect()
}

fn assert_reading_lists_eq(got: &[Reading], want: &Value, what: &str) {
    let want: Vec<Reading> = readings_from_json(want);
    assert_eq!(got, want.as_slice(), "{what}");
}

#[test]
fn normalise_matches_python() {
    let fixture = fixture("normalise");

    for case in fixture["readings_from"].as_array().unwrap() {
        let texts: Vec<String> = case["texts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect();
        assert_reading_lists_eq(
            &normalise::readings_from(&texts),
            &case["out"],
            "readings_from",
        );
    }

    for case in fixture["readings_of"].as_array().unwrap() {
        let members: Vec<Map<String, Value>> = case["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m.as_object().unwrap().clone())
            .collect();
        assert_reading_lists_eq(
            &normalise::readings_of(&members),
            &case["out"],
            "readings_of",
        );
    }

    for case in fixture["key"].as_array().unwrap() {
        assert_eq!(
            normalise::key(case["a"].as_str().unwrap()),
            case["key_a"].as_str().unwrap()
        );
        assert_eq!(
            normalise::key(case["b"].as_str().unwrap()),
            case["key_b"].as_str().unwrap()
        );
    }

    for case in fixture["observed"].as_array().unwrap() {
        let readings = readings_from_json(&case["readings"]);
        assert_eq!(
            normalise::observed(&readings),
            case["out"].as_str().unwrap()
        );
    }

    for case in fixture["plurality_make"].as_array().unwrap() {
        let readings = readings_from_json(&case["readings"]);
        assert_eq!(
            normalise::plurality_make(&readings).as_deref(),
            opt_str(&case["out"])
        );
    }

    for case in fixture["acceptable_make"].as_array().unwrap() {
        let readings = readings_from_json(&case["readings"]);
        assert_eq!(
            normalise::acceptable_make(opt_str(&case["make"]), &readings).as_deref(),
            opt_str(&case["out"])
        );
    }

    for case in fixture["acceptable_model"].as_array().unwrap() {
        let readings = readings_from_json(&case["readings"]);
        assert_eq!(
            normalise::acceptable_model(opt_str(&case["model"]), &readings).as_deref(),
            opt_str(&case["out"])
        );
    }

    assert_eq!(
        normalise::MAJORITY_SETTLES,
        fixture["majority_settles"].as_f64().unwrap()
    );

    for case in fixture["settle_without_model"].as_array().unwrap() {
        let readings = readings_from_json(&case["readings"]);
        let out = normalise::settle_without_model(&readings)
            .expect("every settle_without_model fixture case is expected to settle");
        assert_eq!(out.make.as_deref(), opt_str(&case["make"]));
        assert_eq!(out.model.as_deref(), opt_str(&case["model"]));
        assert_eq!(out.rejected, strings(&case["rejected"]));
    }

    for case in fixture["cache_key"].as_array().unwrap() {
        let readings = readings_from_json(&case["readings"]);
        assert_eq!(
            normalise::cache_key(&readings),
            case["out"].as_str().unwrap()
        );
    }

    for case in fixture["reconcile"].as_array().unwrap() {
        let readings = readings_from_json(&case["readings"]);
        let out = normalise::reconcile(
            opt_str(&case["make_in"]),
            opt_str(&case["model_in"]),
            &readings,
        );
        assert_eq!(out.make.as_deref(), opt_str(&case["make"]));
        assert_eq!(out.model.as_deref(), opt_str(&case["model"]));
        assert_eq!(out.rejected, strings(&case["rejected"]));
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

// --- registry --------------------------------------------------------------

use conrod_core::registry::{self, KnownVehicle, Member, Row};

fn known_vehicle_from_json(v: &Value) -> KnownVehicle {
    KnownVehicle {
        make: opt_str(&v["make"]).map(str::to_string),
        model: opt_str(&v["model"]).map(str::to_string),
        colour: opt_str(&v["colour"]).map(str::to_string),
        body_type: opt_str(&v["body_type"]).map(str::to_string),
        team: opt_str(&v["team"]).map(str::to_string),
        sponsors: strings(&v["sponsors"]),
        race_number: opt_str(&v["race_number"]).map(str::to_string),
    }
}

fn known_from_json(v: &Value) -> HashMap<String, KnownVehicle> {
    v.as_object()
        .unwrap()
        .iter()
        .map(|(k, entry)| (k.clone(), known_vehicle_from_json(entry)))
        .collect()
}

fn member_from_json(v: &Value) -> Member {
    Member {
        plate: opt_str(&v["row"]["plate"]).map(str::to_string),
        attributes: v["parsed"].as_object().unwrap().clone(),
    }
}

fn members_from_json(v: &Value) -> Vec<Member> {
    v.as_array().unwrap().iter().map(member_from_json).collect()
}

fn row_from_tuple(v: &Value) -> Row {
    let t = v.as_array().unwrap();
    Row {
        plate: t[0].as_str().unwrap().to_string(),
        make: opt_str(&t[1]).map(str::to_string),
        model: opt_str(&t[2]).map(str::to_string),
        colour: opt_str(&t[3]).map(str::to_string),
        body_type: opt_str(&t[4]).map(str::to_string),
        team: opt_str(&t[5]).map(str::to_string),
        sponsors: opt_str(&t[6]).map(str::to_string),
        race_number: opt_str(&t[7]).map(str::to_string),
    }
}

#[test]
fn registry_matches_python() {
    let fixture = fixture("registry");

    for case in fixture["normalise"].as_array().unwrap() {
        assert_eq!(
            registry::normalise(opt_str(&case["plate"])),
            case["out"].as_str().unwrap()
        );
    }

    for case in fixture["near_plate"].as_array().unwrap() {
        assert_eq!(
            registry::near_plate(case["a"].as_str().unwrap(), case["b"].as_str().unwrap()),
            case["out"].as_bool().unwrap()
        );
    }

    for case in fixture["fill"].as_array().unwrap() {
        let mut a = VehicleAnalysis::from_map(case["analysis"].as_object().unwrap());
        let known = known_from_json(&case["known"]);
        let filled = registry::fill(&mut a, &known);
        assert_eq!(filled, case["filled"].as_bool().unwrap());
        assert_eq!(
            a,
            VehicleAnalysis::from_map(case["result"].as_object().unwrap())
        );
    }

    for case in fixture["agreed"].as_array().unwrap() {
        let members = members_from_json(&case["members"]);
        let got = registry::agreed(&members);
        match &case["out"] {
            Value::Null => assert!(got.is_none()),
            want => {
                let got = got.expect("Python found an agreed reading");
                assert_eq!(got.plate, want["plate"].as_str().unwrap());
                assert_eq!(got.aliases, strings(&want["aliases"]));
                assert_eq!(got.make.as_deref(), opt_str(&want["make"]));
                assert_eq!(got.model.as_deref(), opt_str(&want["model"]));
                assert_eq!(got.colour.as_deref(), opt_str(&want["colour"]));
                assert_eq!(got.body_type.as_deref(), opt_str(&want["body_type"]));
                assert_eq!(got.team.as_deref(), opt_str(&want["team"]));
                assert_eq!(got.race_number.as_deref(), opt_str(&want["race_number"]));
                match &want["sponsors"] {
                    Value::Null => assert_eq!(got.sponsors, None),
                    s => assert_eq!(got.sponsors, Some(strings(s))),
                }
            }
        }
    }

    for case in fixture["majority"].as_array().unwrap() {
        let members = members_from_json(&case["members"]);
        let field = case["field"].as_str().unwrap();
        if field == "sponsors" {
            let want = match &case["out"] {
                Value::Null => None,
                s => Some(strings(s)),
            };
            assert_eq!(registry::majority_sponsors(&members), want);
        } else {
            assert_eq!(
                registry::majority(&members, field).as_deref(),
                opt_str(&case["out"])
            );
        }
    }

    for case in fixture["split"].as_array().unwrap() {
        assert_eq!(registry::split_field(&case["value"]), strings(&case["out"]));
    }

    for case in fixture["text"].as_array().unwrap() {
        assert_eq!(
            registry::text_of(&case["value"]).as_deref(),
            opt_str(&case["out"])
        );
    }

    // to_csv / from_csv, split across parse_csv + merge_csv_row + to_csv.
    let seed_rows: Vec<Row> = fixture["seed_rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(row_from_tuple)
        .collect();
    assert_eq!(
        registry::to_csv(&seed_rows),
        fixture["initial_csv"].as_str().unwrap()
    );

    let csv_in = fixture["csv_in"].as_str().unwrap();
    let (given_rows, skipped) = registry::parse_csv(csv_in).unwrap();
    assert_eq!(
        skipped as u64,
        fixture["from_csv_skipped"].as_u64().unwrap()
    );
    assert_eq!(
        given_rows.len() as u64,
        fixture["from_csv_written"].as_u64().unwrap()
    );

    let mut final_rows = seed_rows.clone();
    for given in &given_rows {
        let existing = final_rows.iter().find(|r| r.plate == given.plate).cloned();
        let merged = registry::merge_csv_row(given, existing.as_ref());
        match final_rows.iter_mut().find(|r| r.plate == merged.plate) {
            Some(slot) => *slot = merged,
            None => final_rows.push(merged),
        }
    }
    assert_eq!(
        registry::to_csv(&final_rows),
        fixture["csv_out"].as_str().unwrap()
    );

    assert_eq!(
        registry::parse_csv("driver,team\nA,B\n").is_err(),
        fixture["bad_column_error"].as_bool().unwrap()
    );
    assert_eq!(
        registry::parse_csv("").is_err(),
        fixture["empty_error"].as_bool().unwrap()
    );
}

// --- culling ----------------------------------------------------------------

use conrod_core::culling::{self, Cull, CullSettings};

#[test]
fn culling_matches_python() {
    let fixture = fixture("culling");

    assert_eq!(
        culling::REJECTED as i64,
        fixture["rejected_const"].as_i64().unwrap()
    );

    for case in fixture["read_culls"].as_array().unwrap() {
        let row = case["row"].as_object().unwrap().clone();
        let got = culling::cull_from_tags(&row);
        assert_eq!(
            got.rating as i64,
            case["rating"].as_i64().unwrap(),
            "{row:?}"
        );
        assert_eq!(got.label, case["label"].as_str().unwrap(), "{row:?}");
        assert_eq!(got.rejected, case["rejected"].as_bool().unwrap(), "{row:?}");
    }

    for case in fixture["sidecar"].as_array().unwrap() {
        let got = culling::sidecar_for(Path::new(case["image"].as_str().unwrap()));
        // Python's pathlib normalises "/" to the platform separator when
        // stringified; Rust's Path keeps whichever one the input used. Both
        // name the same path, so normalise before comparing.
        let got = got.to_string_lossy().replace('\\', "/");
        let want = case["sidecar"].as_str().unwrap().replace('\\', "/");
        assert_eq!(got, want);
    }

    for case in fixture["passes"].as_array().unwrap() {
        let cull = Cull {
            rating: case["cull"]["rating"].as_i64().unwrap() as i32,
            label: case["cull"]["label"].as_str().unwrap().to_string(),
            rejected: case["cull"]["rejected"].as_bool().unwrap(),
        };
        let settings = CullSettings {
            skip_rejected: case["settings"]["skip_rejected"].as_bool().unwrap(),
            min_rating: case["settings"]["min_rating"].as_i64().unwrap() as i32,
            require_label: case["settings"]["require_label"]
                .as_str()
                .unwrap()
                .to_string(),
        };
        let (ok, reason) = cull.passes(&settings);
        assert_eq!(ok, case["ok"].as_bool().unwrap());
        assert_eq!(reason, case["reason"].as_str().unwrap());
    }
}

// --- analyze: merge_number / corroborated -----------------------------------

use conrod_core::analysis::{corroborated, merge_number, OcrReading};

#[test]
fn merge_matches_python() {
    let fixture = fixture("merge");
    let ocr_accept_confidence = fixture["ocr_accept_confidence"].as_f64().unwrap();

    for case in fixture["merge_number"].as_array().unwrap() {
        let ocr_reading = OcrReading {
            number: opt_str(&case["ocr"]["number"]).map(str::to_string),
            confidence: case["ocr"]["confidence"].as_f64().unwrap(),
            source: case["ocr"]["source"].as_str().unwrap().to_string(),
        };
        let (number, source, confidence) = merge_number(
            &ocr_reading,
            opt_str(&case["vlm_number"]),
            ocr_accept_confidence,
        );
        let want = case["out"].as_array().unwrap();
        assert_eq!(number.as_deref(), opt_str(&want[0]), "{case}");
        assert_eq!(source.as_deref(), opt_str(&want[1]), "{case}");
        assert_eq!(confidence, want[2].as_f64().unwrap(), "{case}");
    }

    for case in fixture["corroborated"].as_array().unwrap() {
        let evidence = strings(&case["evidence"]);
        assert_eq!(
            corroborated(opt_str(&case["claim"]), &evidence),
            case["out"].as_bool().unwrap(),
            "{case}"
        );
    }
}

#[test]
fn settings_match_python() {
    use conrod_core::settings::Settings;
    let fixture = fixture("settings");
    let as_map = |s: &Settings| {
        let mut v = serde_json::to_value(s).unwrap();
        let m = v.as_object_mut().unwrap();
        m.remove("workers");
        m.remove("scan_profile"); // Rust-only
        v
    };
    // Python stores 1 for 1.0 and vice versa; compare numbers as numbers.
    let same = |a: &Value, b: &Value| -> bool {
        a.as_object().unwrap().iter().all(|(k, va)| {
            let vb = &b[k];
            match (va.as_f64(), vb.as_f64()) {
                (Some(x), Some(y)) => x == y,
                _ => va == vb,
            }
        }) && a.as_object().unwrap().len() == b.as_object().unwrap().len()
    };
    assert!(
        same(&as_map(&Settings::default()), &fixture["defaults"]),
        "defaults differ"
    );
    let dir = std::env::temp_dir().join(format!("conrod-settings-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    for case in fixture["cases"].as_array().unwrap() {
        std::fs::write(&path, case["stored"].to_string()).unwrap();
        let loaded = Settings::load(&path);
        assert!(
            same(&as_map(&loaded), &case["loaded"]),
            "load of {}",
            case["stored"]
        );
        let hosts: Vec<&str> = case["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap())
            .collect();
        assert_eq!(loaded.ollama_hosts(), hosts);
        let classes: Vec<usize> = case["classes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_u64().unwrap() as usize)
            .collect();
        assert_eq!(loaded.vehicle_classes(), classes);
    }
    std::fs::remove_dir_all(&dir).ok();
}
