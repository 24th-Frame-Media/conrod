//! YuNet decoder parity on the optional local OpenCV fixture.
//!
//! The fixture names private frames and is intentionally git-ignored. Generate
//! it with `tools/gen_faces_local.py`; this test silently skips without it or
//! the local YuNet model.

use conrod_vision::faces::FaceDetector;
use conrod_vision::imageops::Rgb;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn iou(a: &[f64; 4], b: &[f64]) -> f64 {
    let ix = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let iy = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = ix * iy;
    let union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

fn model_path() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
        .join(".conrod/models/face_detection_yunet_2023mar.onnx")
}

#[test]
fn faces_match_opencv_when_available() {
    let fixture_path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/faces_local.json");
    let (Ok(text), true) = (
        std::fs::read_to_string(&fixture_path),
        model_path().exists(),
    ) else {
        eprintln!("no local faces fixture or model; see tools/gen_faces_local.py");
        return;
    };
    let fixture: Value = serde_json::from_str(&text).unwrap();
    let mut detector = FaceDetector::load(&model_path()).unwrap();
    let mut expected = 0usize;
    let mut matched = 0usize;

    for case in fixture["cases"].as_array().unwrap() {
        let path = Path::new(case["frame"].as_str().unwrap());
        let Ok(image) = Rgb::open(path) else { continue };
        let got = detector.detect(&image).unwrap();
        let mut used = vec![false; got.len()];
        for want in case["faces"].as_array().unwrap() {
            expected += 1;
            let bbox: Vec<f64> = want["bbox"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap())
                .collect();
            let eyes: Vec<Vec<f64>> = want["eyes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| {
                    p.as_array()
                        .unwrap()
                        .iter()
                        .map(|v| v.as_f64().unwrap())
                        .collect()
                })
                .collect();
            let Some((i, face)) = got.iter().enumerate().find(|(i, face)| {
                !used[*i]
                    && iou(&face.bbox, &bbox) >= 0.95
                    && (face.score - want["score"].as_f64().unwrap() as f32).abs() < 0.02
            }) else {
                continue;
            };
            let eye_gap = face
                .eyes
                .iter()
                .zip(&eyes)
                .map(|(&(x, y), p)| (x - p[0]).hypot(y - p[1]))
                .fold(0.0, f64::max);
            if eye_gap <= 4.0 {
                used[i] = true;
                matched += 1;
            }
        }
    }

    eprintln!("YuNet faces matched {matched}/{expected}");
    if expected == 0 {
        eprintln!("local faces fixture references removed previews; regenerate with tools/gen_faces_local.py");
        return;
    }
    assert!(matched * 100 >= expected * 95, "{matched}/{expected}");
}
