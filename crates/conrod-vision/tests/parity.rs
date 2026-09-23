//! The sharpness measure against the Python implementation, on the same pixels.
//!
//! `fixtures/sharpness.json` + `fixtures/sharpness/*.png` are synthetic and
//! committed. `fixtures/sharpness_local.json`, when present, points at the
//! photographer's own crops (tools/gen_golden.py --local) and is the real gate.

use conrod_core::ridge::SharpModel;
use conrod_vision::imageops::Gray;
use conrod_vision::sharpness;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn read(name: &str) -> Option<Value> {
    let text = std::fs::read_to_string(fixtures().join(name)).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

fn floats(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

/// Compare one result; returns the largest score difference seen.
fn check(label: &str, got: &sharpness::Sharpness, want: &Value, tolerance: f64) -> f64 {
    assert_eq!(
        got.measured,
        want["measured"].as_bool().unwrap(),
        "{label}: measured"
    );
    assert_eq!(
        got.uncertain,
        want["uncertain"].as_bool().unwrap(),
        "{label}: uncertain"
    );
    assert_eq!(
        got.learned,
        want["learned"].as_bool().unwrap(),
        "{label}: learned"
    );
    let close = |a: f64, b: f64, what: &str| {
        assert!(
            (a - b).abs() <= tolerance,
            "{label}: {what} got {a}, want {b}"
        );
    };
    close(got.score, want["score"].as_f64().unwrap(), "score");
    close(
        got.background,
        want["background"].as_f64().unwrap(),
        "background",
    );
    close(
        got.heuristic,
        want["heuristic"].as_f64().unwrap(),
        "heuristic",
    );
    let bands = floats(&want["bands"]);
    assert_eq!(got.bands.len(), bands.len(), "{label}: bands");
    for (a, b) in got.bands.iter().zip(&bands) {
        close(*a, *b, "band");
    }
    let features = floats(&want["features"]);
    assert_eq!(got.features.len(), features.len(), "{label}: features");
    for (i, (a, b)) in got.features.iter().zip(&features).enumerate() {
        close(*a, *b, sharpness::FEATURE_NAMES[i]);
    }
    // Discrete calls: exact, unless the score sits right on a threshold.
    assert_eq!(
        got.panning,
        want["panning"].as_bool().unwrap(),
        "{label}: panning"
    );
    assert_eq!(
        got.sharp_end,
        want["sharp_end"].as_str().unwrap(),
        "{label}: sharp_end"
    );
    (got.score - want["score"].as_f64().unwrap()).abs()
}

#[test]
fn synthetic_crops_match_python() {
    let fixture = read("sharpness.json").expect("sharpness fixture");
    let model: SharpModel = serde_json::from_value(fixture["learned_model"].clone()).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let name = case["image"].as_str().unwrap();
        let image = Gray::open(&fixtures().join(name)).unwrap();
        let bbox = case["box"].as_array().map(|b| {
            let v = floats(&Value::Array(b.clone()));
            [v[0], v[1], v[2], v[3]]
        });
        // Lossless pixels, Pillow-exact resampling: rounding error only.
        check(
            name,
            &sharpness::measure(&image, bbox, None),
            &case["plain"],
            1e-4,
        );
        check(
            &format!("{name} (learned)"),
            &sharpness::measure(&image, bbox, Some(&model)),
            &case["learned"],
            1e-4,
        );
    }
    for v in fixture["verdicts"].as_array().unwrap() {
        let score = v["score"].as_f64().unwrap();
        let (sharp_at, blurred) = (sharpness::SHARP_AT, sharpness::BLURRED_BELOW);
        assert_eq!(
            sharpness::verdict_for(score, sharp_at, blurred),
            v["verdict"].as_str().unwrap()
        );
        assert_eq!(
            sharpness::rating_for(score, sharp_at, blurred),
            v["rating"].as_str().unwrap()
        );
        assert_eq!(
            u64::from(sharpness::stars_for(score)),
            v["stars"].as_u64().unwrap()
        );
    }
}

/// The photographer's real crops. JPEG decoders differ by a grey level here
/// and there (Pillow uses libjpeg-turbo), so this allows more, and reports
/// how far apart the two ever got.
#[test]
fn real_crops_match_python_when_available() {
    let Some(fixture) = read("sharpness_local.json") else {
        eprintln!("no local fixture; run tools/gen_golden.py --local");
        return;
    };
    let mut worst = 0.0f64;
    let mut flips = 0;
    let cases = fixture["cases"].as_array().unwrap();
    for case in cases {
        let path = case["image"].as_str().unwrap();
        let Ok(image) = Gray::open(Path::new(path)) else {
            continue;
        };
        let b = floats(&case["box"]);
        let got = sharpness::measure(&image, Some([b[0], b[1], b[2], b[3]]), None);
        let want = &case["plain"];
        worst = worst.max((got.score - want["score"].as_f64().unwrap()).abs());
        let stars_want = sharpness::stars_for(want["score"].as_f64().unwrap());
        if sharpness::stars_for(got.score) != stars_want
            || got.panning != want["panning"].as_bool().unwrap()
        {
            flips += 1;
        }
    }
    eprintln!(
        "{} real crops: worst score gap {worst:.5}, {flips} star or pan changes",
        cases.len()
    );
    assert!(worst < 0.01, "worst score gap {worst}");
    assert!(flips * 100 <= cases.len(), "{flips} decisions changed");
}
