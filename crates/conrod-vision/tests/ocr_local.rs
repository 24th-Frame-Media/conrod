//! The OCR port against RapidOCR (PP-OCRv4) on real vehicle crops.
//!
//! Local-only: needs `fixtures/ocr_local.json` (tools/gen_ocr_local.py)
//! and the PP-OCRv4 model files, looked up in `$CONROD_MODELS`, the data
//! directory's `models/`, then the Python release's bundled copy. Without
//! either it prints why and returns, so CI (which has neither) is unaffected.

use conrod_core::settings::Settings;
use conrod_vision::imageops::Rgb;
use conrod_vision::ocr::{read_number, visible_text, Ocr};
use serde_json::Value;
use std::path::{Path, PathBuf};

fn models_dir() -> Option<PathBuf> {
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

fn norm(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_uppercase()
}

#[test]
#[ignore = "real photos: cargo test --release -p conrod-vision -- --ignored"]
fn ocr_matches_rapidocr_on_real_crops() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/ocr_local.json");
    let (Ok(text), Some(models)) = (std::fs::read_to_string(&fixture), models_dir()) else {
        eprintln!("no local OCR fixture or PP-OCRv4 models; see tools/gen_ocr_local.py");
        return;
    };
    let fixture: Value = serde_json::from_str(&text).unwrap();
    let ocr = Ocr::load(&models).unwrap();
    let settings = Settings::default();

    let (mut n, mut lines_equal, mut number_equal, mut text_equal, mut with_number) =
        (0, 0, 0, 0, 0);
    let (mut number_kept, mut number_extra) = (0, 0);
    let mut differing = Vec::new();
    let started = std::time::Instant::now();
    for case in fixture["cases"].as_array().unwrap() {
        let Ok(bytes) = std::fs::read(case["crop"].as_str().unwrap()) else {
            continue;
        };
        let crop = Rgb::decode_jpeg(&bytes, 1).unwrap();
        let tokens = ocr.read(&crop).unwrap();
        n += 1;

        let want_lines: Vec<String> = case["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| norm(l[0].as_str().unwrap()))
            .filter(|t| !t.is_empty())
            .collect();
        let got_lines: Vec<String> = tokens
            .iter()
            .map(|t| norm(&t.text))
            .filter(|t| !t.is_empty())
            .collect();
        let mut a = want_lines.clone();
        let mut b = got_lines.clone();
        a.sort();
        b.sort();
        lines_equal += usize::from(a == b);

        let want_number = case["number"][0].as_str().map(str::to_owned);
        let got_number = read_number(&tokens, &settings).map(|(n, _)| n);
        with_number += usize::from(want_number.is_some());
        number_equal += usize::from(want_number == got_number);
        number_kept += usize::from(want_number.is_some() && want_number == got_number);
        number_extra += usize::from(want_number.is_none() && got_number.is_some());

        let want_text: Vec<String> = case["text"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect();
        let got_text = visible_text(&tokens, &settings, &[]);
        text_equal += usize::from(want_text == got_text);

        if a != b || want_number != got_number {
            differing.push(format!(
                "{}: python {:?} -> {:?} | rust {:?} -> {:?}",
                Path::new(case["crop"].as_str().unwrap())
                    .file_name()
                    .unwrap()
                    .to_string_lossy(),
                want_lines,
                want_number,
                got_lines,
                got_number
            ));
        }
    }
    eprintln!(
        "OCR on {n} crops ({:.0} ms/crop): identical lines {lines_equal}, race number equal {number_equal} (python read one on {with_number}, rust matched {number_kept}; rust-only reads {number_extra}), visible text {text_equal}",
        started.elapsed().as_secs_f64() * 1000.0 / n as f64
    );
    for d in differing.iter().take(14) {
        eprintln!("  {d}");
    }
    // Measured on 120 crops: 57 of 62 (92%). The rest are detector-level
    // differences from oar-ocr vs RapidOCR's own DB post-processing; Rust also
    // reads real numbers Python misses (crop 38361's orange 7).
    assert!(
        number_kept * 100 >= with_number * 85,
        "recovered {number_kept}/{with_number} of Python's numbers"
    );
}
