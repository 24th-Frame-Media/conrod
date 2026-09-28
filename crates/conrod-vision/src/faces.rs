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
    /// All five: right eye, left eye, nose tip, right and left mouth corner.
    pub landmarks: [(f64, f64); 5],
}

/// DirectML if it loads, else the CPU.
fn session(model: &Path) -> Result<Session, String> {
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
    build(true).or_else(|_| build(false))
}

pub struct FaceDetector {
    session: Session,
    /// OCEC, when its model is installed; without it eyes are never judged.
    eyes: Option<Session>,
}

impl FaceDetector {
    pub fn load(model: &Path) -> Result<FaceDetector, String> {
        Ok(FaceDetector {
            session: session(model)?,
            eyes: None,
        })
    }

    /// Also judge whether eyes are open, with OCEC (PINTO0309, MIT).
    pub fn load_eyes(&mut self, model: &Path) -> Result<(), String> {
        self.eyes = Some(session(model)?);
        Ok(())
    }

    /// OCEC's `prob_open` for each eye of a face found in `image`, right then
    /// left. `None` without the model or for an eye too small to judge.
    pub fn eyes_open(&mut self, image: &Rgb, face: &Face) -> [Option<f32>; 2] {
        let Some(session) = self.eyes.as_mut() else {
            return [None, None];
        };
        let patches = face.eyes.map(|eye| eye_patch(image, face, eye));
        let present: Vec<&Rgb> = patches.iter().flatten().collect();
        if present.is_empty() {
            return [None, None];
        }
        let plane = EYE_W * EYE_H;
        let mut tensor = vec![0f32; present.len() * plane * 3];
        for (n, patch) in present.iter().enumerate() {
            for (i, px) in patch.data.chunks_exact(3).enumerate() {
                for c in 0..3 {
                    tensor[(n * 3 + c) * plane + i] = f32::from(px[c]) / 255.0;
                }
            }
        }
        let Ok(input) = Tensor::from_array(([present.len(), 3, EYE_H, EYE_W], tensor)) else {
            return [None, None];
        };
        let probs: Vec<f32> = match session.run(ort::inputs![input]) {
            Ok(out) => match out[0].try_extract_tensor::<f32>() {
                Ok((_, data)) => data.to_vec(),
                Err(_) => return [None, None],
            },
            Err(_) => return [None, None],
        };
        let mut next = probs.into_iter();
        patches.map(|p| p.and_then(|_| next.next()))
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
            landmarks: [at(0), at(1), at(2), at(3), at(4)],
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

/// OCEC's input, height x width.
const EYE_H: usize = 24;
const EYE_W: usize = 40;
/// Below this `prob_open` an eye counts as closed.
pub const EYES_CLOSED_BELOW: f32 = 0.5;

/// A 40x24 RGB patch centred on one eye landmark, about an eye wide: OCEC was
/// trained on detector eye boxes, which are wider than tall at that ratio.
fn eye_patch(image: &Rgb, face: &Face, (x, y): (f64, f64)) -> Option<Rgb> {
    let [(rx, ry), (lx, ly)] = face.eyes;
    let gap = (rx - lx)
        .hypot(ry - ly)
        .max((face.bbox[2] - face.bbox[0]) / 3.0);
    let w = gap * 0.6;
    let h = w * EYE_H as f64 / EYE_W as f64;
    let x0 = (x - w / 2.0).max(0.0) as usize;
    let y0 = (y - h / 2.0).max(0.0) as usize;
    let x1 = ((x + w / 2.0).max(0.0) as usize).min(image.width);
    let y1 = ((y + h / 2.0).max(0.0) as usize).min(image.height);
    if x1 < x0 + 8 || y1 < y0 + 5 {
        return None;
    }
    Some(image.crop(x0, y0, x1, y1).resize_cv2_linear(EYE_W, EYE_H))
}

/// Where SFace wants the five landmarks in its 112x112 input (OpenCV's
/// `FaceRecognizerSF::alignCrop`, the ArcFace template).
const TEMPLATE: [(f64, f64); 5] = [
    (38.2946, 51.6963),
    (73.5318, 51.5014),
    (56.0252, 71.7366),
    (41.5493, 92.3655),
    (70.7299, 92.2041),
];
pub const ALIGNED: usize = 112;
/// SFace's length, and so a stored face vector's version: DINOv2's are 384.
pub const IDENTITY_DIM: usize = 128;
/// Cosine at or above which two SFace vectors are one person (OpenCV's
/// threshold for SFace).
pub const SAME_PERSON: f32 = 0.363;

/// The least-squares similarity (rotation, scale, shift) taking `from` onto
/// `to`: `(a, b, tx, ty)` with `x' = a x - b y + tx`, `y' = b x + a y + ty`.
fn similarity(from: &[(f64, f64); 5], to: &[(f64, f64); 5]) -> (f64, f64, f64, f64) {
    let mean = |p: &[(f64, f64); 5]| {
        let (x, y) = p.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
        (x / 5.0, y / 5.0)
    };
    let ((fx, fy), (tx, ty)) = (mean(from), mean(to));
    let (mut dot, mut cross, mut norm) = (0.0, 0.0, 0.0);
    for ((x, y), (u, v)) in from.iter().zip(to) {
        let (px, py, qx, qy) = (x - fx, y - fy, u - tx, v - ty);
        dot += px * qx + py * qy;
        cross += px * qy - py * qx;
        norm += px * px + py * py;
    }
    let (a, b) = if norm > 0.0 {
        (dot / norm, cross / norm)
    } else {
        (1.0, 0.0)
    };
    (a, b, tx - (a * fx - b * fy), ty - (b * fx + a * fy))
}

/// The face warped upright onto SFace's 112x112 template by its landmarks,
/// bilinear, black outside the image (what `cv2.warpAffine` does).
pub fn align(image: &Rgb, face: &Face) -> Rgb {
    let (a, b, tx, ty) = similarity(&face.landmarks, &TEMPLATE);
    let det = a * a + b * b;
    let mut out = vec![0u8; ALIGNED * ALIGNED * 3];
    if det == 0.0 || image.width == 0 || image.height == 0 {
        return Rgb::new(ALIGNED, ALIGNED, out);
    }
    let (w, h) = (image.width, image.height);
    let px = |x: usize, y: usize, c: usize| f64::from(image.data[(y * w + x) * 3 + c]);
    for v in 0..ALIGNED {
        for u in 0..ALIGNED {
            let (du, dv) = (u as f64 - tx, v as f64 - ty);
            let (x, y) = ((a * du + b * dv) / det, (a * dv - b * du) / det);
            if x < 0.0 || y < 0.0 || x > (w - 1) as f64 || y > (h - 1) as f64 {
                continue;
            }
            let (x0, y0) = (x as usize, y as usize);
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            let (fx, fy) = (x - x0 as f64, y - y0 as f64);
            for c in 0..3 {
                let top = px(x0, y0, c) * (1.0 - fx) + px(x1, y0, c) * fx;
                let bottom = px(x0, y1, c) * (1.0 - fx) + px(x1, y1, c) * fx;
                out[(v * ALIGNED + u) * 3 + c] = (top * (1.0 - fy) + bottom * fy).round() as u8;
            }
        }
    }
    Rgb::new(ALIGNED, ALIGNED, out)
}

/// SFace (OpenCV zoo, Apache-2.0): who a face is, as a unit vector compared
/// by dot product against [`SAME_PERSON`].
pub struct FaceEmbedder {
    session: Session,
}

impl FaceEmbedder {
    pub fn load(model: &Path) -> Result<FaceEmbedder, String> {
        Ok(FaceEmbedder {
            session: session(model)?,
        })
    }

    /// The identity vector of a face found in `image`.
    pub fn embed(&mut self, image: &Rgb, face: &Face) -> Result<Vec<f32>, String> {
        let aligned = align(image, face);
        // RGB 0..255, no normalisation: OpenCV's blobFromImage(swapRB=true).
        let plane = ALIGNED * ALIGNED;
        let mut tensor = vec![0f32; plane * 3];
        for (i, px) in aligned.data.chunks_exact(3).enumerate() {
            for c in 0..3 {
                tensor[c * plane + i] = f32::from(px[c]);
            }
        }
        let input = Tensor::from_array(([1usize, 3, ALIGNED, ALIGNED], tensor))
            .map_err(|e| e.to_string())?;
        let out = self
            .session
            .run(ort::inputs![input])
            .map_err(|e| e.to_string())?;
        let (_, data) = out[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        Ok(unit(data.to_vec()))
    }
}

fn unit(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
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

    fn face_at(landmarks: [(f64, f64); 5]) -> Face {
        Face {
            score: 1.0,
            bbox: [0.0, 0.0, 200.0, 200.0],
            eyes: [landmarks[0], landmarks[1]],
            landmarks,
        }
    }

    #[test]
    fn a_face_already_on_the_template_aligns_unchanged() {
        let (a, b, tx, ty) = similarity(&TEMPLATE, &TEMPLATE);
        assert!((a - 1.0).abs() < 1e-9 && b.abs() < 1e-9 && tx.abs() < 1e-9 && ty.abs() < 1e-9);
        // Twice the size and shifted: the transform halves it and moves it back.
        let big = TEMPLATE.map(|(x, y)| (x * 2.0 + 10.0, y * 2.0 + 30.0));
        let (a, b, tx, ty) = similarity(&big, &TEMPLATE);
        assert!((a - 0.5).abs() < 1e-9 && b.abs() < 1e-9);
        assert!((tx + 5.0).abs() < 1e-9 && (ty + 15.0).abs() < 1e-9);
    }

    #[test]
    fn alignment_samples_the_face_into_a_112_square() {
        // Dark left of x=120, light right of it; the template face at twice size.
        let (w, h) = (300, 300);
        let data = (0..w * h)
            .flat_map(|i| [if i % w < 120 { 0u8 } else { 200 }; 3])
            .collect();
        let image = Rgb::new(w, h, data);
        let face = face_at(TEMPLATE.map(|(x, y)| (x * 2.0, y * 2.0)));
        let out = align(&image, &face);
        assert_eq!((out.width, out.height), (ALIGNED, ALIGNED));
        let at = |x: usize, y: usize| out.data[(y * ALIGNED + x) * 3];
        // Image x 120 lands on template x 60.
        assert_eq!(at(50, 56), 0);
        assert_eq!(at(70, 56), 200);
    }

    #[test]
    fn eye_patches_are_ocec_sized_and_tiny_eyes_are_skipped() {
        let image = Rgb::new(400, 400, vec![128; 400 * 400 * 3]);
        let o = (0.0, 0.0);
        let face = face_at([(150.0, 150.0), (250.0, 150.0), o, o, o]);
        let patch = eye_patch(&image, &face, face.eyes[0]).unwrap();
        assert_eq!((patch.width, patch.height), (EYE_W, EYE_H));
        let mut tiny = face_at([(10.0, 10.0), (12.0, 10.0), o, o, o]);
        tiny.bbox = [8.0, 8.0, 14.0, 14.0];
        assert!(eye_patch(&image, &tiny, tiny.eyes[0]).is_none());
    }

    #[test]
    fn identity_vectors_are_unit_length() {
        let v = unit(vec![3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        assert_eq!(unit(vec![0.0; 3]), vec![0.0; 3]);
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
