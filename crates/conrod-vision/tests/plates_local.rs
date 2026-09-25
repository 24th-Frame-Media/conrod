//! Plate reading checked against recorded snapshots, on real vehicle
//! crops. Local-only: needs tools/gen_plates_local.py's fixture and the two
//! ONNX models it depends on (open-image-models' and fast-plate-ocr's own
//! caches -- present once those tools have been run at least once).
//!
//! Plate text, state and the best roundel number are compared. The general OCR
//! runs on PP-OCRv4 like the fixture (see tools/gen_plates_local.py); without
//! those model files the OCR half is skipped and only plate text is checked.

use conrod_vision::detect::Device;
use conrod_vision::imageops::Rgb;
use conrod_vision::ocr::Ocr;
use conrod_vision::plates::{scan_regions, PlateDetector, PlateOptions, PlateReader};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Measured 136/150 (91%): the misses are one-character glyph flips between oar-ocr and RapidOCR.
const MIN_TEXT_PERCENT: usize = 88;

fn ocr_models() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default());
    [
        std::env::var_os("CONROD_MODELS").map(PathBuf::from),
        Some(home.join(".conrod/models")),
        Some(
            home.join("Downloads/Conrod-0.2.11-win64/Conrod/_internal/rapidocr_onnxruntime/models"),
        ),
    ]
    .into_iter()
    .flatten()
    .find(|d| d.join("ch_PP-OCRv4_det_infer.onnx").is_file())
}

fn cache_dir() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join(".cache")
}

/// A truncating cast, clamped at zero for a crop box.
fn trunc(v: f64) -> usize {
    (v as i64).max(0) as usize
}

#[test]
#[ignore = "real photos: cargo test --release -p conrod-vision -- --ignored"]
fn plate_text_matches_recorded_snapshot_on_real_crops() {
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/plates_local.json");
    let detector_model = cache_dir().join(
        "open-image-models/yolo-v9-t-640-license-plate-end2end/yolo-v9-t-640-license-plates-end2end.onnx",
    );
    let reader_model = cache_dir()
        .join("fast-plate-ocr/global-plates-mobile-vit-v2-model/global_mobile_vit_v2_ocr.onnx");
    let (Ok(text), true, true) = (
        std::fs::read_to_string(&fixture_path),
        detector_model.exists(),
        reader_model.exists(),
    ) else {
        eprintln!("no local plates fixture or models; see tools/gen_plates_local.py");
        return;
    };
    let fixture: Value = serde_json::from_str(&text).unwrap();
    let opts = PlateOptions::default();
    let ocr = ocr_models().map(|dir| Ocr::load(&dir).unwrap());
    let mut detector = PlateDetector::load(&detector_model, Device::Cpu).unwrap();
    let mut reader = PlateReader::load(&reader_model, Device::Cpu).unwrap();

    let (mut total, mut matched, mut state_ok, mut number_ok, mut snapshot_numbers) =
        (0, 0, 0, 0, 0);
    let mut mismatches = Vec::new();

    for case in fixture["cases"].as_array().unwrap() {
        let crop_path = case["crop"].as_str().unwrap();
        let Ok(bytes) = std::fs::read(crop_path) else {
            continue;
        };
        let Ok(crop) = Rgb::decode_jpeg(&bytes, 1) else {
            continue;
        };

        let native = opts.plate_native_search.then(|| {
            let preview_path = case["preview"].as_str().unwrap();
            let bytes = std::fs::read(preview_path).ok()?;
            let frame = Rgb::decode_jpeg(&bytes, 1).ok()?;
            let box_: Vec<f64> = case["box"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            Some(frame.crop(
                trunc(box_[0]),
                trunc(box_[1]),
                trunc(box_[2]),
                trunc(box_[3]),
            ))
        });
        let native = native.flatten();

        let (reading, numbers) = scan_regions(
            &crop,
            native.as_ref(),
            &opts,
            &mut detector,
            &mut reader,
            ocr.as_ref(),
        )
        .unwrap();
        let want_number = case["numbers"][0][0].as_str();
        snapshot_numbers += usize::from(want_number.is_some());
        number_ok += usize::from(numbers.first().map(|n| n.0.as_str()) == want_number);
        state_ok += usize::from(reading.state.as_deref() == case["state"].as_str());

        total += 1;
        let want_text = case["text"].as_str();
        if reading.text.as_deref() == want_text {
            matched += 1;
        } else {
            mismatches.push(format!(
                "{crop_path}: got {:?}, want {want_text:?}",
                reading.text
            ));
        }
    }

    eprintln!(
        "plates on {total} crops: text {matched}, state {state_ok}, best roundel number {number_ok} (recorded snapshot read one on {snapshot_numbers})"
    );
    for mismatch in &mismatches {
        eprintln!("  mismatch: {mismatch}");
    }
    assert!(
        total == 0 || matched * 100 >= total * MIN_TEXT_PERCENT,
        "{matched}/{total}"
    );
}
