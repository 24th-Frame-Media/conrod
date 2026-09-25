//! The pure text logic in `plates.rs` (`interpret`, `trim_to_format`,
//! `looks_like_plate`) against `conrod/plates.py`, on synthetic cases.
//! `fixtures/plates_text.json` is committed -- generate it with
//! `tools/gen_plates_local.py --committed`.

use conrod_vision::plates::{interpret, looks_like_plate, trim_to_format, PlateOptions};
use serde_json::Value;
use std::path::Path;

fn fixture() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/plates_text.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap()
}

#[test]
fn looks_like_plate_matches_recorded_snapshot() {
    for case in fixture()["looks_like_plate"].as_array().unwrap() {
        let token = case["token"].as_str().unwrap();
        assert_eq!(
            looks_like_plate(token),
            case["want"].as_bool().unwrap(),
            "looks_like_plate({token:?})"
        );
    }
}

#[test]
fn trim_to_format_matches_recorded_snapshot() {
    for case in fixture()["trim_to_format"].as_array().unwrap() {
        let token = case["token"].as_str().unwrap();
        assert_eq!(
            trim_to_format(token),
            case["want"].as_str().unwrap(),
            "trim_to_format({token:?})"
        );
    }
}

#[test]
fn interpret_matches_recorded_snapshot() {
    let opts = PlateOptions::default();
    for case in fixture()["interpret"].as_array().unwrap() {
        let lines: Vec<(String, f64)> = case["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|line| {
                let line = line.as_array().unwrap();
                (
                    line[0].as_str().unwrap().to_string(),
                    line[1].as_f64().unwrap(),
                )
            })
            .collect();
        let got = interpret(&lines, &opts);
        let want = &case["want"];
        assert_eq!(got.text.as_deref(), want["text"].as_str(), "{lines:?}");
        assert_eq!(got.state.as_deref(), want["state"].as_str(), "{lines:?}");
        assert_eq!(
            got.confidence,
            want["confidence"].as_f64().unwrap(),
            "{lines:?}"
        );
        let candidates: Vec<&str> = want["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(got.candidates, candidates, "{lines:?}");
    }
}
