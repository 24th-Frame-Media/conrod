//! The detector against ultralytics' own boxes on real frames. Local-only:
//! needs tools/export_onnx.py's model and rust/fixtures/detector_local.json.

use conrod_vision::detect::{DetectOptions, Detector, Device};
use conrod_vision::imageops::Rgb;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn iou(a: &[f64; 4], b: &[f64]) -> f64 {
    let ix = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let iy = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = ix * iy;
    inter / ((a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter)
}

#[test]
fn boxes_match_ultralytics_when_available() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/detector_local.json");
    let model = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
        .join(".conrod/models/yolo11s-960.onnx");
    let (Ok(text), true) = (std::fs::read_to_string(&fixture), model.exists()) else {
        eprintln!("no local detector fixture or model; see tools/export_onnx.py");
        return;
    };
    let fixture: Value = serde_json::from_str(&text).unwrap();
    if fixture["frames"]
        .as_array()
        .unwrap()
        .iter()
        .any(|frame| !Path::new(frame["frame"].as_str().unwrap()).is_file())
    {
        eprintln!("local detector fixture references removed previews; regenerate with tools/export_onnx.py");
        return;
    }
    let options = DetectOptions {
        classes: vec![0, 2, 3, 5, 7],
        ..DetectOptions::default()
    };
    let mut detector = Detector::load(&model, Device::Auto).unwrap();
    let (mut want, mut hit) = (0, 0);
    for frame in fixture["frames"].as_array().unwrap() {
        let bytes = std::fs::read(frame["frame"].as_str().unwrap()).unwrap();
        let image = Rgb::decode_jpeg(&bytes, 1).unwrap();
        let mut got = detector.raw(&image, &options).unwrap();
        for r in frame["boxes"].as_array().unwrap() {
            want += 1;
            let b: Vec<f64> = r["box"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            let cls = r["cls"].as_u64().unwrap() as usize;
            if let Some(i) = got.iter().position(|d| d.0 == cls && iou(&d.2, &b) >= 0.9) {
                got.remove(i);
                hit += 1;
            }
        }
    }
    eprintln!(
        "{} matched {hit}/{want} ultralytics boxes at IoU 0.9",
        detector.device
    );
    assert!(hit * 100 >= want * 97, "{hit}/{want}");
}
