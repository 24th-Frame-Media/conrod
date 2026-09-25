//! Is this the same car? An instance embedding from DINOv2-small (Xenova's
//! ONNX export, dynamically quantised to uint8) answers that better than a
//! shape hash or a hue histogram did. This crate does the preprocessing, the
//! session, and the embedding itself; downloading and verifying the model
//! file is a separate tool (`tools/`), since fetching it needs an HTTP
//! client this crate has no other use for and nothing here runs unattended
//! against the network.

use crate::detect::Device;
use crate::imageops::{Filter, Rgb};
use ort::ep;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

/// What the model was trained on: 224px square, ImageNet statistics.
pub const INPUT_EDGE: usize = 224;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

pub struct Embedder {
    session: Session,
    /// What is actually running, for the status area to say.
    pub device: &'static str,
}

impl Embedder {
    pub fn load(model: &Path, device: Device) -> Result<Embedder, String> {
        let open = |gpu: bool| -> Result<Session, String> {
            let builder = Session::builder().map_err(|e| e.to_string())?;
            let builder = if gpu {
                builder
                    .with_execution_providers([ep::DirectML::default().build().error_on_failure()])
            } else {
                // One thread per session: the analysis pool already runs
                // several workers, and letting each fan out again
                // oversubscribes the machine.
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
        match device {
            Device::Cpu => Ok(Embedder {
                session: open(false)?,
                device: "CPU",
            }),
            Device::DirectMl => Ok(Embedder {
                session: open(true)?,
                device: "DirectML",
            }),
            Device::Auto => match open(true) {
                Ok(session) => Ok(Embedder {
                    session,
                    device: "DirectML",
                }),
                Err(_) => Ok(Embedder {
                    session: open(false)?,
                    device: "CPU",
                }),
            },
        }
    }

    /// One crop as a unit vector: DINOv2's CLS token, its own summary of the
    /// whole image, normalised so comparing two of them is a dot product.
    pub fn embed(&mut self, image: &Rgb) -> Result<Vec<f32>, String> {
        let input = prepare(image);
        let tensor = Tensor::from_array(([1usize, 3, INPUT_EDGE, INPUT_EDGE], input))
            .map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| e.to_string())?;
        let (shape, raw) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        // (batch, tokens, features): the CLS token is token 0, i.e. the
        // vector's first `features` entries, since the batch is one image.
        let vector: &[f32] = if shape.len() == 3 {
            &raw[..shape[2] as usize]
        } else {
            raw
        };
        let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm <= 0.0 {
            return Err("zero-norm embedding".to_string());
        }
        Ok(vector.iter().map(|v| v / norm).collect())
    }
}

/// Crop to the centre square and scale, rather than squashing to 224x224. A
/// vehicle crop is wide -- stretching that into a square distorts exactly the
/// proportions that tell one car from another.
fn prepare(image: &Rgb) -> Vec<f32> {
    let side = image.width.min(image.height);
    let left = (image.width - side) / 2;
    let top = (image.height - side) / 2;
    let square = image.crop(left, top, left + side, top + side).resize(
        INPUT_EDGE,
        INPUT_EDGE,
        Filter::Bilinear,
    );

    let plane = INPUT_EDGE * INPUT_EDGE;
    let mut data = vec![0f32; 3 * plane];
    for (i, px) in square.data.as_chunks::<3>().0.iter().enumerate() {
        for c in 0..3 {
            data[c * plane + i] = (f32::from(px[c]) / 255.0 - MEAN[c]) / STD[c];
        }
    }
    data
}

/// Cosine similarity of two embeddings, 0..1 for anything realistic.
pub fn nearness(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Store as text, because that is what the detections table holds.
pub fn pack(vector: &[f32]) -> String {
    vector
        .iter()
        .map(|v| format!("{v:.5}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub fn unpack(text: &str) -> Option<Vec<f32>> {
    if text.is_empty() {
        return None;
    }
    text.split(',').map(|s| s.parse::<f32>().ok()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_unpack_roundtrips() {
        let v = vec![0.12345_f32, -0.5, 1.0, 0.0];
        let text = pack(&v);
        let back = unpack(&text).unwrap();
        for (a, b) in v.iter().zip(&back) {
            assert!((a - b).abs() < 1e-4);
        }
    }

    #[test]
    fn unpack_rejects_garbage() {
        assert_eq!(unpack(""), None);
        assert_eq!(unpack("1.0,not a number"), None);
    }

    #[test]
    fn nearness_of_identical_unit_vectors_is_one() {
        let v = vec![0.6_f32, 0.8];
        assert!((nearness(&v, &v) - 1.0).abs() < 1e-6);
    }
}
