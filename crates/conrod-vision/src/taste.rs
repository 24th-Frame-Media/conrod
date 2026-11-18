//! The photographer's taste: a learned within-burst ranking signal.
//!
//! The experiment behind this (`target/taste/report.md`) put a listwise head on
//! DINOv2-small's CLS token plus handcrafted measurements and got 0.265 exact /
//! 0.365 near-identical against 0.235 for Laplacian sharpness and 0.208 for a
//! coin toss. That is a good *sort* and a bad *pick*: three wrong keepers for
//! every right one. Conrod therefore stores this as an order only -- it never
//! decides `burst_pick`.
//!
//! What is scored here is one frame at a time, because that is what the head
//! can be asked at review time. The experiment's burst-relative inputs (burst
//! position, within-burst ranks, cosine to the burst mean) and its two
//! no-reference IQA models (MUSIQ + CLIP-IQA+, 260 MB and 320 ms/frame) are
//! therefore **dropped**. The head is `taste-head.json` in the models folder;
//! `scripts/taste/train_head.py` retrains it from the photographer's delivered
//! frames. Missing head or missing tower means taste is simply unavailable.
//!
//! The tower is onnx-community's DINOv2-small export (Apache-2.0, Meta DINOv2),
//! `onnx/model.onnx`, 89 MB:
//! <https://huggingface.co/onnx-community/dinov2-small/resolve/main/onnx/model.onnx>

use crate::detect::Device;
use crate::imageops::{Filter, Rgb};
use ort::ep;
use ort::session::Session;
use ort::value::Tensor;
use serde_json::Value;
use std::path::Path;

/// What the tower was trained on: 224px square, ImageNet statistics.
pub const INPUT_EDGE: usize = 224;
/// The experiment's handcrafted features were measured on 512px previews.
pub const FEATURE_EDGE: usize = 512;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

/// The handcrafted measurements, in the order `features()` returns them. The
/// head names the ones it wants, so a future retrain can drop or add one.
pub const HANDCRAFTED: [&str; 8] = [
    "sharp_lap",
    "luma_mean",
    "luma_std",
    "clip_hi",
    "clip_lo",
    "sat_mean",
    "colourful",
    "aspect",
];

/// A linear head over standardised per-frame features, plus the embedding.
pub struct Head {
    feature_order: Vec<String>,
    mean: Vec<f32>,
    std: Vec<f32>,
    weights: Vec<f32>,
    bias: f32,
}

impl Head {
    /// Parse `taste-head.json`. Every vector must line up with `feature_order`.
    pub fn parse(text: &str) -> Result<Head, String> {
        let doc: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let list = |key: &str| -> Result<Vec<f32>, String> {
            doc[key]
                .as_array()
                .ok_or_else(|| format!("taste head has no {key}"))?
                .iter()
                .map(|v| {
                    v.as_f64()
                        .map(|x| x as f32)
                        .ok_or_else(|| format!("{key} holds a non-number"))
                })
                .collect()
        };
        let feature_order: Vec<String> = doc["feature_order"]
            .as_array()
            .ok_or("taste head has no feature_order")?
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect();
        let (mean, std, weights) = (list("mean")?, list("std")?, list("weights")?);
        let n = feature_order.len();
        if n == 0 || mean.len() != n || std.len() != n || weights.len() != n {
            return Err(format!(
                "taste head is {n} features but mean/std/weights are {}/{}/{}",
                mean.len(),
                std.len(),
                weights.len()
            ));
        }
        if std.iter().any(|s| !s.is_finite() || *s <= 0.0) {
            return Err("taste head has a zero spread".into());
        }
        Ok(Head {
            feature_order,
            mean,
            std,
            weights,
            bias: doc["bias"].as_f64().unwrap_or(0.0) as f32,
        })
    }

    pub fn feature_order(&self) -> &[String] {
        &self.feature_order
    }

    /// One embedding + the named handcrafted values -> 0..1 taste, or `None`
    /// when a feature the head wants is missing (a hand-edited head file).
    pub fn score(&self, embedding: &[f32], handcrafted: &[(String, f32)]) -> Option<f32> {
        if embedding.is_empty() {
            return None;
        }
        let mut z = self.bias;
        for (i, name) in self.feature_order.iter().enumerate() {
            let raw = if let Some(index) = name.strip_prefix("emb") {
                *embedding.get(index.parse::<usize>().ok()?)?
            } else {
                handcrafted
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| *v)?
            };
            if !raw.is_finite() {
                return None;
            }
            z += self.weights[i] * ((raw - self.mean[i]) / self.std[i]);
        }
        Some(1.0 / (1.0 + (-z).exp()))
    }
}

pub struct TasteModel {
    session: Session,
    head: Head,
    /// What is actually running, for the status area to say.
    pub device: &'static str,
}

impl TasteModel {
    pub fn load(tower: &Path, head: &Path, device: Device) -> Result<TasteModel, String> {
        let text = std::fs::read_to_string(head).map_err(|e| e.to_string())?;
        let head = Head::parse(&text)?;
        let threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .clamp(1, 8);
        let open = |gpu: bool| -> Result<Session, String> {
            let builder = Session::builder().map_err(|e| e.to_string())?;
            let builder = if gpu {
                builder
                    .with_execution_providers([ep::DirectML::default().build().error_on_failure()])
            } else {
                builder
                    .with_intra_threads(threads)
                    .map_err(|e| e.to_string())?
                    .with_execution_providers([ep::CPU::default().build()])
            };
            builder
                .map_err(|e| e.to_string())?
                .commit_from_file(tower)
                .map_err(|e| e.to_string())
        };
        match device {
            Device::Cpu => Ok(TasteModel {
                session: open(false)?,
                head,
                device: "CPU",
            }),
            Device::DirectMl => Ok(TasteModel {
                session: open(true)?,
                head,
                device: "DirectML",
            }),
            Device::Auto => match open(true) {
                Ok(session) => Ok(TasteModel {
                    session,
                    head,
                    device: "DirectML",
                }),
                Err(_) => Ok(TasteModel {
                    session: open(false)?,
                    head,
                    device: "CPU",
                }),
            },
        }
    }

    /// The learned taste of one frame, 0..1. `None` when the frame cannot be
    /// read or scored; a caller sorts the frames that do have a score.
    pub fn score(&mut self, image: &Rgb) -> Option<f32> {
        if image.width == 0 || image.height == 0 {
            return None;
        }
        let small = long_edge(image, FEATURE_EDGE);
        let handcrafted = handcrafted(&small);
        let embedding = self.embed(&small).ok()?;
        self.head.score(&embedding, &handcrafted)
    }

    /// DINOv2's CLS token, L2-normalised, from a squash to 224x224 (the
    /// experiment's preprocessing; `similarity.rs` centre-crops instead).
    fn embed(&mut self, image: &Rgb) -> Result<Vec<f32>, String> {
        let square = image.resize(INPUT_EDGE, INPUT_EDGE, Filter::Lanczos);
        let plane = INPUT_EDGE * INPUT_EDGE;
        let mut data = vec![0f32; 3 * plane];
        for (i, px) in square.data.as_chunks::<3>().0.iter().enumerate() {
            for c in 0..3 {
                data[c * plane + i] = (f32::from(px[c]) / 255.0 - MEAN[c]) / STD[c];
            }
        }
        let tensor = Tensor::from_array(([1usize, 3, INPUT_EDGE, INPUT_EDGE], data))
            .map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| e.to_string())?;
        let (shape, raw) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        let vector: &[f32] = if shape.len() == 3 {
            &raw[..shape[2] as usize]
        } else {
            raw
        };
        let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm <= 0.0 {
            return Err("zero-norm embedding".into());
        }
        Ok(vector.iter().map(|v| v / norm).collect())
    }
}

/// Scale so the long edge is `edge`, keeping the aspect. Upscales too: a 420px
/// cached thumbnail is normalised to the 512px the features were measured on.
fn long_edge(image: &Rgb, edge: usize) -> Rgb {
    let longest = image.width.max(image.height);
    if longest == edge || longest == 0 {
        return image.clone();
    }
    let scale = edge as f64 / longest as f64;
    let w = ((image.width as f64 * scale).round() as usize).max(1);
    let h = ((image.height as f64 * scale).round() as usize).max(1);
    image.resize(w, h, Filter::Lanczos)
}

/// The experiment's `handcrafted()` from `scripts/taste/thumbs.py`, on the
/// 512px image: 5-point Laplacian variance, luma mean/std, clipping fractions,
/// saturation, colourfulness, and the aspect ratio.
pub fn handcrafted(image: &Rgb) -> Vec<(String, f32)> {
    let (w, h) = (image.width, image.height);
    let n = (w * h).max(1) as f64;
    let mut sum = 0f64;
    let mut sum2 = 0f64;
    let mut hi = 0usize;
    let mut lo = 0usize;
    let mut sat = 0f64;
    let mut ch_sum = [0f64; 3];
    let mut ch_sum2 = [0f64; 3];
    let mut luma = vec![0f32; w * h];
    for (i, px) in image.data.as_chunks::<3>().0.iter().enumerate() {
        let rgb = [
            f64::from(px[0]) / 255.0,
            f64::from(px[1]) / 255.0,
            f64::from(px[2]) / 255.0,
        ];
        let y = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
        luma[i] = y as f32;
        sum += y;
        sum2 += y * y;
        if y > 0.97 {
            hi += 1;
        }
        if y < 0.02 {
            lo += 1;
        }
        let mx = rgb[0].max(rgb[1]).max(rgb[2]);
        let mn = rgb[0].min(rgb[1]).min(rgb[2]);
        sat += (mx - mn) / mx.max(1e-6);
        for c in 0..3 {
            ch_sum[c] += rgb[c];
            ch_sum2[c] += rgb[c] * rgb[c];
        }
    }
    // Population variance of the 5-point Laplacian over the interior, exactly
    // numpy's `lap.var()`.
    let mut lap_n = 0f64;
    let mut lap_sum = 0f64;
    let mut lap_sum2 = 0f64;
    if w >= 3 && h >= 3 {
        for y in 1..h - 1 {
            for x in 1..w - 1 {
                let i = y * w + x;
                let v = f64::from(luma[i - w] + luma[i + w] + luma[i - 1] + luma[i + 1])
                    - 4.0 * f64::from(luma[i]);
                lap_n += 1.0;
                lap_sum += v;
                lap_sum2 += v * v;
            }
        }
    }
    let lap_var = if lap_n > 0.0 {
        (lap_sum2 - lap_sum * lap_sum / lap_n) / lap_n
    } else {
        0.0
    };
    let luma_mean = sum / n;
    let luma_std = (sum2 / n - luma_mean * luma_mean).max(0.0).sqrt();
    let mean = [ch_sum[0] / n, ch_sum[1] / n, ch_sum[2] / n];
    let var = |c: usize| (ch_sum2[c] / n - mean[c] * mean[c]).max(0.0);
    let colourful = (var(0) + var(1) + var(2)).sqrt()
        + 0.3
            * ((mean[0] - mean[1]).powi(2)
                + (mean[0] - mean[2]).powi(2)
                + (mean[1] - mean[2]).powi(2))
            .sqrt();
    let values = [
        lap_var.max(0.0).ln_1p(),
        luma_mean,
        luma_std,
        hi as f64 / n,
        lo as f64 / n,
        sat / n,
        colourful,
        w as f64 / h.max(1) as f64,
    ];
    HANDCRAFTED
        .iter()
        .zip(values)
        .map(|(name, v)| ((*name).to_string(), v as f32))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: usize, h: usize, seed: u32) -> Rgb {
        let data = (0..w * h)
            .flat_map(|i| {
                let x = (i % w) as u32;
                let y = (i / w) as u32;
                let base = x.wrapping_mul(seed).wrapping_add(y * 37);
                [base as u8, (base / 3) as u8, (base / 7) as u8]
            })
            .collect();
        Rgb::new(w, h, data)
    }

    fn head_json(n: usize) -> String {
        let mut order: Vec<String> = (0..n - 1).map(|i| format!("emb{i}")).collect();
        order.push("luma_mean".into());
        let mut weights = vec![0.0f32; n];
        weights[n - 1] = 2.0;
        serde_json::json!({
            "version": 1,
            "feature_order": order,
            "mean": vec![0.0f32; n],
            "std": vec![1.0f32; n],
            "weights": weights,
            "bias": -1.0,
        })
        .to_string()
    }

    #[test]
    fn a_head_is_a_monotone_score_over_the_features_it_names() {
        let head = Head::parse(&head_json(4)).unwrap();
        let handcrafted = vec![("luma_mean".to_string(), 0.5)];
        let emb = vec![0.1, 0.2, 0.3];
        let low = head.score(&emb, &handcrafted).unwrap();
        let high = head
            .score(&emb, &[("luma_mean".to_string(), 0.75)])
            .unwrap();
        assert!((0.0..=1.0).contains(&low) && high > low, "{low} {high}");
        // 0.5 * 2 - 1 = 0 -> 0.5; 0.75 * 2 - 1 = 0.5 -> sigmoid(0.5)
        assert!((low - 0.5).abs() < 1e-6);
        assert!((high - 0.62245935).abs() < 1e-5);
    }

    #[test]
    fn a_head_missing_a_feature_names_the_problem() {
        assert!(Head::parse("{}").is_err());
        let doc = head_json(3).replace("\"emb1\"", "\"emb9\"");
        let head = Head::parse(&doc).unwrap();
        let none = head.score(&[0.0, 0.0], &[("luma_mean".into(), 0.5)]);
        assert_eq!(none, None, "emb9 is outside the embedding");
        let short = Head::parse(&head_json(4)).unwrap();
        assert_eq!(short.score(&[], &[("luma_mean".into(), 0.5)]), None);
    }

    #[test]
    fn handcrafted_reads_a_flat_grey_as_flat_and_a_pattern_as_sharp() {
        let flat = Rgb::new(64, 48, vec![128u8; 64 * 48 * 3]);
        let f = handcrafted(&flat);
        let get = |name: &str| f.iter().find(|(k, _)| k == name).unwrap().1;
        assert!(get("sharp_lap") < 1e-6, "no detail: no Laplacian energy");
        assert!((get("luma_mean") - 128.0 / 255.0).abs() < 1e-5);
        assert_eq!(get("luma_std"), 0.0);
        assert_eq!(get("clip_hi"), 0.0);
        assert!((get("aspect") - 64.0 / 48.0).abs() < 1e-5);
        assert_eq!(f.len(), HANDCRAFTED.len());

        let pattern = picture(64, 48, 5);
        let p = handcrafted(&pattern);
        let sharp = p.iter().find(|(k, _)| k == "sharp_lap").unwrap().1;
        assert!(sharp > 0.0, "a pattern has Laplacian variance");
        let bright = Rgb::new(8, 8, vec![255u8; 8 * 8 * 3]);
        let b = handcrafted(&bright);
        let clip = b.iter().find(|(k, _)| k == "clip_hi").unwrap().1;
        assert!((clip - 1.0).abs() < 1e-6, "white is fully clipped");
    }

    /// The real models and ~20 real previews. `cargo test -p conrod-vision
    /// --release taste -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs ~/.conrod/models/taste-head.json and the DINOv2 tower"]
    fn scores_real_previews_in_a_sensible_order() {
        let root =
            Path::new(&std::env::var("USERPROFILE").unwrap_or_default()).join(".conrod/models");
        let (tower, head) = (
            root.join("dinov2-small-tower.onnx"),
            root.join("taste-head.json"),
        );
        if !tower.is_file() || !head.is_file() {
            eprintln!("no taste models in {}; nothing to check", root.display());
            return;
        }
        let thumbs = Path::new(r"E:\code\photo\conrod\target\taste\thumbs");
        let Ok(dir) = std::fs::read_dir(thumbs) else {
            eprintln!("no experiment thumbnails; nothing to check");
            return;
        };
        let mut files: Vec<_> = dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        files.sort();
        files.truncate(20);
        assert_eq!(files.len(), 20, "the experiment kept 6680 thumbnails");
        let mut model = TasteModel::load(&tower, &head, Device::Cpu).unwrap();
        let started = std::time::Instant::now();
        let mut scores = Vec::new();
        for path in &files {
            let bytes = std::fs::read(path).unwrap();
            let image = Rgb::decode_jpeg(&bytes, 1).unwrap();
            let score = model.score(&image).expect("a readable frame scores");
            assert!((0.0..=1.0).contains(&score));
            scores.push(score);
        }
        let ms = started.elapsed().as_millis() as f64 / files.len() as f64;
        let mean = scores.iter().sum::<f32>() / scores.len() as f32;
        eprintln!(
            "20 frames: {ms:.1} ms/frame (decode + tower + head), taste mean {mean:.3}, range {:.3}..{:.3}",
            scores.iter().cloned().fold(f32::INFINITY, f32::min),
            scores.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        );
        for (path, score) in files.iter().zip(&scores) {
            eprintln!(
                "  {} {score:.4}",
                path.file_stem().unwrap_or_default().to_string_lossy()
            );
        }
        assert!(
            scores.iter().any(|s| (*s - mean).abs() > 1e-4),
            "the head discriminates"
        );
    }
}
