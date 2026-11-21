//! Parity with the official `pplx_decider` reference (BF16, Transformers).
//!
//! `prompts_match_the_reference` needs no weights. `probabilities_match_the_reference` runs the
//! real model and is ignored unless the runtime and weights are present:
//!
//! ```text
//! $env:CONROD_DECIDER_RUNTIME = "<folder with llama.dll, mtmd.dll, ggml*.dll>"
//! $env:CONROD_DECIDER_MODEL   = "<decider-flash-v1.2-q8_0.gguf or the int4 file>"
//! $env:CONROD_DECIDER_MMPROJ  = "<mmproj-decider-flash-v1.2-f16.gguf>"
//! cargo test --release -p conrod-decider --test parity -- --ignored --nocapture
//! ```
//!
//! The fixture is `tests/fixtures/reference.json`, made by `scripts/decider/reference_fixture.py`.

use conrod_decider::protocol;
use conrod_decider::*;
use serde::Deserialize;
use std::path::PathBuf;

const REFERENCE: &str = include_str!("fixtures/reference.json");
const IMAGE_TOKENS: &str = "<|vision_start|><|image_pad|><|vision_end|>";

/// Typed, not `serde_json::Value`: a `Value` map would sort the questions, and their order is
/// part of the prompt.
#[derive(Deserialize)]
struct Reference {
    temperature: f64,
    codes: Vec<String>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    id: String,
    request: Request,
    images: Vec<String>,
    prompt: String,
    logits: Ordered<Vec<f32>>,
    input_tokens: u64,
}

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn reference() -> Reference {
    serde_json::from_str(REFERENCE).unwrap()
}

fn request_of(case: &Case) -> (Request, Vec<image::RgbImage>) {
    let pixels = case
        .images
        .iter()
        .map(|name| image::open(fixtures().join(name)).unwrap().to_rgb8())
        .collect();
    (case.request.clone(), pixels)
}

#[test]
fn prompts_match_the_reference() {
    let reference = reference();
    assert!(reference.cases.len() >= 5);
    for case in &reference.cases {
        let (request, pixels) = request_of(case);
        let ours = protocol::prompt(&request, &reference.codes, IMAGE_TOKENS, pixels.len());
        assert_eq!(ours, case.prompt, "prompt of {}", case.id);
    }
}

#[test]
fn the_temperature_and_codes_are_the_checkpoints() {
    let reference = reference();
    assert_eq!(reference.temperature, DEFAULT_TEMPERATURE);
    assert_eq!(reference.codes.len(), 255);
    assert_eq!(reference.codes[254], "JT");
}

fn env_path(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("set {name}")))
}

#[test]
#[ignore = "needs CONROD_DECIDER_RUNTIME / _MODEL / _MMPROJ"]
fn probabilities_match_the_reference() {
    let paths = DeciderPaths::new(
        env_path("CONROD_DECIDER_RUNTIME"),
        env_path("CONROD_DECIDER_MODEL"),
        env_path("CONROD_DECIDER_MMPROJ"),
    );
    let decider = Decider::load(
        &paths,
        &LoadOptions {
            // The reference ran with the checkpoint's cap.
            max_pixels: MAX_PIXELS,
            max_tokens: 4096,
            ..LoadOptions::default()
        },
    )
    .unwrap();
    let reference = reference();
    let temperature = reference.temperature;
    let (mut worst, mut sum, mut count, mut flips) = (0f64, 0f64, 0usize, 0usize);
    for case in &reference.cases {
        let (request, pixels) = request_of(case);
        let (response, timing) = decider.decide_timed_with(&request, &pixels).unwrap();
        let usage = response.usage.unwrap_or_default();
        println!(
            "{}: {} tokens (reference {}), {:?}",
            case.id,
            usage.input_tokens,
            case.input_tokens,
            timing.encode + timing.decode
        );
        // Images are resized by different code (Pillow vs ours): allow a few positions.
        assert!(
            (usage.input_tokens as i64 - case.input_tokens as i64).abs() <= 8,
            "token count of {}",
            case.id
        );
        let ours = decider.logits_with(&request, &pixels).unwrap();
        for (id, question) in request.questions.iter() {
            let theirs = case.logits.get(id).unwrap();
            let mine = ours.get(id).unwrap();
            let p = protocol::softmax(mine, temperature);
            let q = protocol::softmax(theirs, temperature);
            let gap = p
                .iter()
                .zip(&q)
                .map(|(a, b)| (a - b).abs())
                .fold(0f64, f64::max);
            worst = worst.max(gap);
            sum += gap;
            count += 1;
            let top = |v: &[f64]| {
                v.iter()
                    .enumerate()
                    .fold(0, |b, (i, x)| if *x > v[b] { i } else { b })
            };
            let mut sorted = q.clone();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
            let margin = sorted[0] - sorted.get(1).copied().unwrap_or(0.0);
            if top(&p) != top(&q) {
                flips += 1;
                assert!(
                    margin < 0.1,
                    "{id} of {}: argmax flip on a clear answer",
                    case.id
                );
            }
            println!("  {id} [{}]: max |dp| {gap:.4}", question.kind());
        }
    }
    println!(
        "questions {count}, mean gap {:.4}, worst {worst:.4}, argmax flips {flips}",
        sum / count as f64
    );
    // int8 (Q8_0): mean ~0.007; int4 is looser and checked by the same fixture via the env vars.
    let (max_worst, max_mean) = if std::env::var("CONROD_DECIDER_LOOSE").is_ok() {
        (0.2, 0.04)
    } else {
        (0.08, 0.015)
    };
    assert!(worst < max_worst, "worst probability gap {worst}");
    assert!(
        sum / (count as f64) < max_mean,
        "mean gap {}",
        sum / count as f64
    );
}
