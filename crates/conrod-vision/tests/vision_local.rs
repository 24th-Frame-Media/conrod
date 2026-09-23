//! The similarity embedding and the colour swatch against Python, on the
//! photographer's own crops.
//!
//! `fixtures/vision_local.json`, when present, points at real crops
//! (`tools/gen_vision_local.py`) and is the parity gate that counts; without
//! it, or without the model file, this prints why and returns rather than
//! failing a build that simply has neither.
//!
//! Rust's image resampler and Pillow's `Image.BILINEAR` differ slightly on
//! JPEG crops; the embedding check uses a conservative cosine floor rather
//! than pretending pixel-level identity. The colour swatch is checked
//! separately because it is more sensitive to small pixel shifts.

use conrod_vision::detect::Device;
use conrod_vision::imageops::Rgb;
use conrod_vision::{colour, similarity};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn fixture() -> Option<Value> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vision_local.json");
    let text = std::fs::read_to_string(path).ok()?;
    Some(serde_json::from_str(&text).unwrap())
}

fn model_path() -> PathBuf {
    PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default())
        .join(".conrod/models/dinov2-small-quantized.onnx")
}

fn parse_hex(s: &str) -> Option<[i32; 3]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    Some([
        i32::from_str_radix(&s[0..2], 16).ok()?,
        i32::from_str_radix(&s[2..4], 16).ok()?,
        i32::from_str_radix(&s[4..6], 16).ok()?,
    ])
}

#[test]
fn real_crops_match_python_when_available() {
    let (Some(fixture), true) = (fixture(), model_path().exists()) else {
        eprintln!("no local vision fixture or model; run tools/gen_vision_local.py");
        return;
    };
    let mut embedder = similarity::Embedder::load(&model_path(), Device::Cpu).unwrap();

    let cases = fixture["cases"].as_array().unwrap();
    let mut worst_cosine = 1.0f32;
    let (mut colour_exact, mut colour_checked) = (0, 0);
    let mut worst_channel_gap = 0i32;

    for case in cases {
        let path = case["image"].as_str().unwrap();
        let Ok(image) = Rgb::open(Path::new(path)) else {
            continue;
        };

        let want: Vec<f32> = case["embedding"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        let got = embedder.embed(&image).unwrap();
        assert_eq!(got.len(), want.len(), "{path}: embedding length");
        let cosine = similarity::nearness(&got, &want);
        worst_cosine = worst_cosine.min(cosine);
        assert!(cosine >= 0.98, "{path}: cosine {cosine}");

        if let Some(want_hex) = case["colour"].as_str() {
            colour_checked += 1;
            let got_hex = colour::dominant(&image).unwrap_or_default();
            if got_hex == want_hex {
                colour_exact += 1;
            } else {
                let (Some(a), Some(b)) = (parse_hex(&got_hex), parse_hex(want_hex)) else {
                    panic!("{path}: unparseable colour {got_hex} vs {want_hex}");
                };
                let gap = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).max().unwrap();
                worst_channel_gap = worst_channel_gap.max(gap);
                assert!(
                    gap <= 12,
                    "{path}: colour {got_hex} vs {want_hex}, gap {gap}"
                );
            }
        }
    }

    eprintln!(
        "{} real crops: worst cosine {worst_cosine:.5}, colour exact {colour_exact}/{colour_checked}, worst channel gap on the rest {worst_channel_gap}",
        cases.len()
    );
    if colour_checked == 0 {
        eprintln!("local fixture carries no colour swatches (its job was cull-only); regenerate from an identified job with tools/gen_vision_local.py");
        return;
    }
    assert!(
        worst_channel_gap <= 12,
        "worst colour channel gap {worst_channel_gap}"
    );
}
