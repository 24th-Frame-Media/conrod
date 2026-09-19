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
