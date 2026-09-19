//! Checks the Rust port against what the Python implementation did.
//!
//! The fixtures are written by `tools/gen_golden.py`. A failure here after
//! regenerating them means the two implementations have diverged.

use conrod_core::{framing, ridge};
use serde_json::Value;
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
