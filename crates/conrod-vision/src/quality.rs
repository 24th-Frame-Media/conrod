//! NIMA technical quality: is this a well-exposed, noise-free, in-focus
//! picture, apart from what it is a picture of? (idealo/image-quality-assessment,
//! the MobileNet "technical" weights, Apache-2.0.) Opt-in: sharpness already
//! answers "is the subject sharp", so this is only a tiebreak, never the score.
//!
//! The model outputs a 10-bin distribution (mean opinion score histogram,
//! bins 1..10); the quality score is that distribution's mean, same as the
//! NIMA paper.

use crate::imageops::{Filter, Rgb};
use ort::ep;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

/// What NIMA-MobileNet was trained on: 224px square.
pub const INPUT_EDGE: usize = 224;
/// Keras's MobileNet preprocessing: pixels to [-1, 1].
const SCALE: f32 = 127.5;

pub struct QualityModel {
    session: Session,
    pub device: &'static str,
}

impl QualityModel {
    pub fn load(model: &Path, gpu: bool) -> Result<QualityModel, String> {
        let open = |gpu: bool| -> Result<Session, String> {
            let builder = Session::builder().map_err(|e| e.to_string())?;
            let builder = if gpu {
                builder
                    .with_execution_providers([ep::DirectML::default().build().error_on_failure()])
            } else {
                builder
                    .with_intra_threads(1)
                    .map_err(|e| e.to_string())?
                    .with_execution_providers([ep::CPU::default().build()])
            };
            builder
                .map_err(|e| e.to_string())?
                .commit_from_file(model)
                .map_err(|e| e.to_string())
        };
        match open(gpu) {
            Ok(session) => Ok(QualityModel {
                session,
                device: if gpu { "DirectML" } else { "CPU" },
            }),
            Err(_) if gpu => Ok(QualityModel {
                session: open(false)?,
                device: "CPU",
            }),
            Err(e) => Err(e),
        }
    }

    /// The subject crop's technical quality, 0..1 (the 10-bin mean, normalised
    /// from 1..10 down to 0..1 so it sits on the same scale as sharpness).
    pub fn score(&mut self, image: &Rgb) -> Result<f64, String> {
        let input = prepare(image);
        let tensor = Tensor::from_array(([1usize, INPUT_EDGE, INPUT_EDGE, 3], input))
            .map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| e.to_string())?;
        let (_, bins) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if bins.len() != 10 {
            return Err(format!("expected 10 bins, got {}", bins.len()));
        }
        let total: f64 = bins.iter().map(|&p| f64::from(p)).sum();
        if total <= 0.0 {
            return Err("zero-probability NIMA output".into());
        }
        let mean: f64 = bins
            .iter()
            .enumerate()
            .map(|(i, &p)| (i + 1) as f64 * f64::from(p) / total)
            .sum();
        Ok(((mean - 1.0) / 9.0).clamp(0.0, 1.0))
    }
}

/// Squashed to 224x224 (NIMA's own preprocessing resizes rather than crops)
/// and scaled to [-1, 1], channels-last (NHWC), the Keras export's own layout.
fn prepare(image: &Rgb) -> Vec<f32> {
    let square = image.resize(INPUT_EDGE, INPUT_EDGE, Filter::Bilinear);
    square
        .data
        .iter()
        .map(|&px| f32::from(px) / SCALE - 1.0)
        .collect()
}
