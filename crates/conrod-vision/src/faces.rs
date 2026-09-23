//! Faces and eyes, for the portrait and event scan types.
//!
//! YuNet (OpenCV zoo, MIT, 230 KB): a face box and five landmarks -- the eyes,
//! the nose, the mouth corners. The decode is OpenCV's `FaceDetectorYN`:
//! per stride, score = sqrt(cls * obj), box and landmarks as offsets from the
//! grid cell, then NMS. Eye sharpness is the existing tile measure on a patch
//! around each eye landmark, which is where a portrait is judged.

use crate::imageops::{Gray, Rgb};
use crate::sharpness;
use ort::ep;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

pub const INPUT: usize = 640;
const STRIDES: [usize; 3] = [8, 16, 32];
pub const SCORE: f32 = 0.6;
const NMS_IOU: f32 = 0.3;

#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    pub score: f32,
    /// Pixels of the image passed in.
    pub bbox: [f64; 4],
    /// Right eye then left eye, as YuNet orders them (the subject's right).
    pub eyes: [(f64, f64); 2],
}

pub struct FaceDetector {
    session: Session,
}

impl FaceDetector {
    pub fn load(model: &Path) -> Result<FaceDetector, String> {
        let build = |gpu: bool| -> Result<Session, String> {
            let b = Session::builder().map_err(|e| e.to_string())?;
            let b = if gpu {
                b.with_execution_providers([ep::DirectML::default().build().error_on_failure()])
            } else {
                b.with_execution_providers([ep::CPU::default().build()])
            };
            b.map_err(|e| e.to_string())?
                .commit_from_file(model)
                .map_err(|e| e.to_string())
        };
        let session = build(true).or_else(|_| build(false))?;
        Ok(FaceDetector { session })
    }

    /// Faces in an upright image. The image is scaled so its long edge is
    /// 640 and padded at the right and bottom, which OpenCV leaves to the
    /// caller and this does for them.
    pub fn detect(&mut self, image: &Rgb) -> Result<Vec<Face>, String> {
        if image.width == 0 || image.height == 0 {
            return Ok(Vec::new());
        }
        let scale = INPUT as f64 / image.width.max(image.height) as f64;
        let (w, h) = (
            ((image.width as f64 * scale).round() as usize).clamp(1, INPUT),
            ((image.height as f64 * scale).round() as usize).clamp(1, INPUT),
        );
        let small = image.resize_cv2_linear(w, h);
        // BGR, 0..255, no normalisation: what FaceDetectorYN feeds it.
        let plane = INPUT * INPUT;
        let mut tensor = vec![0f32; plane * 3];
        for y in 0..h {
            for x in 0..w {
                let s = (y * w + x) * 3;
                for c in 0..3 {
                    tensor[c * plane + y * INPUT + x] = f32::from(small.data[s + 2 - c]);
                }
            }
        }
        let input =
            Tensor::from_array(([1usize, 3, INPUT, INPUT], tensor)).map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![input])
            .map_err(|e| e.to_string())?;
        let get = |name: &str| -> Result<Vec<f32>, String> {
            let (_, data) = outputs[name]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            Ok(data.to_vec())
        };

        let mut found: Vec<Face> = Vec::new();
        for stride in STRIDES {
            let (cls, obj) = (
                get(&format!("cls_{stride}"))?,
                get(&format!("obj_{stride}"))?,
            );
            let (bbox, kps) = (
                get(&format!("bbox_{stride}"))?,
                get(&format!("kps_{stride}"))?,
            );
            found.extend(decode_stride(stride, &cls, &obj, &bbox, &kps, scale)?);
        }
        found.sort_by(|a, b| b.score.total_cmp(&a.score));
        let mut kept: Vec<Face> = Vec::new();
        for f in found {
            if kept
                .iter()
                .all(|k| iou(&k.bbox, &f.bbox) <= f64::from(NMS_IOU))
            {
                kept.push(f);
            }
        }
        Ok(kept)
    }
}

fn decode_stride(
    stride: usize,
    cls: &[f32],
    obj: &[f32],
    bbox: &[f32],
    kps: &[f32],
    scale: f64,
) -> Result<Vec<Face>, String> {
    if cls.len() != obj.len() || bbox.len() != cls.len() * 4 || kps.len() != cls.len() * 10 {
        return Err(format!(
            "YuNet stride {stride} output lengths: cls {}, obj {}, bbox {}, kps {}",
            cls.len(),
            obj.len(),
            bbox.len(),
            kps.len()
        ));
    }
    let cols = INPUT / stride;
    let mut found = Vec::new();
    for (i, (&c, &o)) in cls.iter().zip(obj).enumerate() {
        let score = (c.clamp(0.0, 1.0) * o.clamp(0.0, 1.0)).sqrt();
        if score < SCORE {
            continue;
        }
        let (r, col) = ((i / cols) as f32, (i % cols) as f32);
        let s = stride as f32;
        let b = &bbox[i * 4..i * 4 + 4];
        let (cx, cy) = ((col + b[0]) * s, (r + b[1]) * s);
        let (bw, bh) = (b[2].exp() * s, b[3].exp() * s);
        let k = &kps[i * 10..i * 10 + 10];
        let at = |n: usize| {
            (
                f64::from((k[2 * n] + col) * s) / scale,
                f64::from((k[2 * n + 1] + r) * s) / scale,
            )
        };
        found.push(Face {
            score,
            bbox: [cx - bw / 2.0, cy - bh / 2.0, cx + bw / 2.0, cy + bh / 2.0]
                .map(|v| f64::from(v) / scale),
            eyes: [at(0), at(1)],
        });
    }
    Ok(found)
}

fn iou(a: &[f64; 4], b: &[f64; 4]) -> f64 {
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

/// How sharp the eyes are: the tile measure on a square around each eye,
/// a third of the face wide. `None` for either eye too small to judge.
///
/// ponytail: star bands were fitted on vehicles; eye scores use the same
/// scale uncalibrated. Recalibrate once portrait samples are rated in Train.
pub fn eye_sharpness(frame: &Gray, face: &Face) -> [Option<f64>; 2] {
    let side = (face.bbox[2] - face.bbox[0]) / 3.0;
    face.eyes.map(|(x, y)| {
        let x0 = (x - side / 2.0).max(0.0) as usize;
        let y0 = (y - side / 2.0).max(0.0) as usize;
        let x1 = ((x + side / 2.0) as usize).min(frame.width);
        let y1 = ((y + side / 2.0) as usize).min(frame.height);
        if x1 <= x0 + 24 || y1 <= y0 + 24 {
            return None;
        }
        let mut patch = Vec::with_capacity((x1 - x0) * (y1 - y0));
        for row in y0..y1 {
            patch.extend_from_slice(&frame.data[row * frame.width + x0..row * frame.width + x1]);
        }
        let result = sharpness::measure(&Gray::new(x1 - x0, y1 - y0, patch), None, None);
        result.measured.then_some(result.score)
    })
}

/// Measure both eyes for each detected face in one RGB frame. The grayscale
/// conversion is shared across faces so a portrait pass does one decode.
pub fn measure_regions(image: &Rgb, faces: &[Face]) -> Vec<[Option<f64>; 2]> {
    let gray = image.to_gray();
    faces
        .iter()
        .map(|face| eye_sharpness(&gray, face))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stride_decode_scores_boxes_and_landmarks() {
        let got = decode_stride(
            8,
            &[0.81, 0.25],
            &[1.0, 1.0],
            &[0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 2.0_f32.ln(), 3.0_f32.ln()],
            &[
                1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
                0.0, 0.0, 0.0, 0.0,
            ],
            1.0,
        )
        .unwrap();
        assert_eq!(got.len(), 1);
        assert!((got[0].score - 0.9).abs() < 1e-6);
        assert_eq!(got[0].bbox, [-4.0, -4.0, 4.0, 4.0]);
        assert_eq!(got[0].eyes[0], (8.0, 16.0));
    }

    #[test]
    fn stride_decode_rejects_mismatched_outputs() {
        assert!(decode_stride(8, &[1.0], &[1.0], &[], &[], 1.0).is_err());
    }

    #[test]
    fn nms_iou_handles_overlap_and_disjoint_boxes() {
        assert_eq!(iou(&[0.0, 0.0, 10.0, 10.0], &[0.0, 0.0, 10.0, 10.0]), 1.0);
        assert_eq!(iou(&[0.0, 0.0, 1.0, 1.0], &[2.0, 2.0, 3.0, 3.0]), 0.0);
    }
}
