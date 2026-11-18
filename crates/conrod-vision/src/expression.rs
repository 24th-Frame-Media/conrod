//! Expressions, for portraits, weddings and events: is anyone blinking, is
//! anyone smiling, how far is the head turned.
//!
//! Two ONNX conversions of Google's MediaPipe Face Landmarker models
//! (Apache-2.0, `huggingface.co/t9749/mediapipe-face-landmarker-onnx`, from
//! `face_landmarker.task` float16 v1): the landmark model (478 points on a
//! 256 px square crop, 4.9 MB) and the blendshape model (52 ARKit-style
//! coefficients from 146 of those points, 1.9 MB). The blendshape model is
//! optional; without it eye openness comes from the eye aspect ratio and the
//! smile from mouth-corner geometry, both documented at their thresholds.
//!
//! The crop is squared to 1.5x the face box and de-rotated by the eye line, so
//! the landmark model sees an upright face; head pose is read back from the
//! landmarks in image space. The exporter validated the pair against the
//! official MediaPipe task on 300 LFW photos: 0.8 % of face width landmark
//! error, 0.010 mean blendshape difference.

use crate::faces::{session_with, Face};
use crate::imageops::Rgb;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

/// The 478-point landmark model, 4.9 MB. Same folder as YuNet.
pub const LANDMARKS: &str = "face_landmarks_detector.onnx";
/// The 52-coefficient blendshape model, 1.9 MB.
pub const BLENDSHAPES: &str = "face_blendshapes.onnx";

/// The landmark model's square input.
pub const CROP: usize = 256;
/// Side of the crop square as a multiple of the face box's long edge.
const MARGIN: f64 = 1.5;

/// MediaPipe's blendshape order (`face_blendshapes_graph.cc`). Indices below
/// refer to this order, which is the order the model emits.
const BLENDSHAPE_NAMES: [&str; 52] = [
    "_neutral",
    "browDownLeft",
    "browDownRight",
    "browInnerUp",
    "browOuterUpLeft",
    "browOuterUpRight",
    "cheekPuff",
    "cheekSquintLeft",
    "cheekSquintRight",
    "eyeBlinkLeft",
    "eyeBlinkRight",
    "eyeLookDownLeft",
    "eyeLookDownRight",
    "eyeLookInLeft",
    "eyeLookInRight",
    "eyeLookOutLeft",
    "eyeLookOutRight",
    "eyeLookUpLeft",
    "eyeLookUpRight",
    "eyeSquintLeft",
    "eyeSquintRight",
    "eyeWideLeft",
    "eyeWideRight",
    "jawForward",
    "jawLeft",
    "jawOpen",
    "jawRight",
    "mouthClose",
    "mouthDimpleLeft",
    "mouthDimpleRight",
    "mouthFrownLeft",
    "mouthFrownRight",
    "mouthFunnel",
    "mouthLeft",
    "mouthLowerDownLeft",
    "mouthLowerDownRight",
    "mouthPressLeft",
    "mouthPressRight",
    "mouthPucker",
    "mouthRight",
    "mouthRollLower",
    "mouthRollUpper",
    "mouthShrugLower",
    "mouthShrugUpper",
    "mouthSmileLeft",
    "mouthSmileRight",
    "mouthStretchLeft",
    "mouthStretchRight",
    "mouthUpperUpLeft",
    "mouthUpperUpRight",
    "noseSneerLeft",
    "noseSneerRight",
];

/// The 146 landmark indices the blendshape model reads, in MediaPipe's order.
const BLENDSHAPE_SUBSET: [usize; 146] = [
    0, 1, 4, 5, 6, 7, 8, 10, 13, 14, 17, 21, 33, 37, 39, 40, 46, 52, 53, 54, 55, 58, 61, 63, 65,
    66, 67, 70, 78, 80, 81, 82, 84, 87, 88, 91, 93, 95, 103, 105, 107, 109, 127, 132, 133, 136,
    144, 145, 146, 148, 149, 150, 152, 153, 154, 155, 157, 158, 159, 160, 161, 162, 163, 168, 172,
    173, 176, 178, 181, 185, 191, 195, 197, 234, 246, 249, 251, 263, 267, 269, 270, 276, 282, 283,
    284, 285, 288, 291, 293, 295, 296, 297, 300, 308, 310, 311, 312, 314, 317, 318, 321, 323, 324,
    332, 334, 336, 338, 356, 361, 362, 365, 373, 374, 375, 377, 378, 379, 380, 381, 382, 384, 385,
    386, 387, 388, 389, 390, 397, 398, 400, 402, 405, 409, 415, 454, 466, 468, 469, 470, 471, 472,
    473, 474, 475, 476, 477,
];

// Landmark indices, as MediaPipe numbers its 478 points. Subject's right eye
// is 33/133, the left 362/263; 61 and 291 are the mouth corners.
const EYE_R_OUTER: usize = 33;
const EYE_R_INNER: usize = 133;
const EYE_R_UPPER: [usize; 2] = [159, 158];
const EYE_R_LOWER: [usize; 2] = [145, 153];
const EYE_L_INNER: usize = 362;
const EYE_L_OUTER: usize = 263;
const EYE_L_UPPER: [usize; 2] = [386, 385];
const EYE_L_LOWER: [usize; 2] = [374, 380];
const MOUTH_R: usize = 61;
const MOUTH_L: usize = 291;
const LIP_UPPER: usize = 13;
const LIP_LOWER: usize = 14;
const NOSE_TIP: usize = 1;
const CHIN: usize = 152;

/// Eye aspect ratio at or below which an eye reads shut, and at or above which
/// it reads fully open: a blink takes it from about 0.30 to 0.10 on MediaPipe's
/// geometry (Soukupova and Cech's measure). Only used without the blendshape
/// model.
pub const EAR_CLOSED: f32 = 0.12;
pub const EAR_OPEN: f32 = 0.30;
/// Corner lift, as a fraction of the inter-ocular distance, that reads as no
/// smile and as a full one. Fitted by eye on the sample frames; only used
/// without the blendshape model.
pub const SMILE_LIFT_LOW: f32 = 0.005;
pub const SMILE_LIFT_HIGH: f32 = 0.05;
/// Lower eye-open than this reads as a blink, on the blendshape eye closes:
/// the contact sheet measured blinks at 0.03-0.26 and open eyes at 0.78-0.99.
/// It only confirms an OCEC "closed": on a turned head the mesh reads a shut
/// eye that the photographer delivered, so it never culls on its own.
pub const CLOSED_EYE_BELOW: f32 = 0.20;
/// A mesh that agrees with the detector's landmarks at least this well is
/// trusted over the binary OCEC verdict.
pub const TRUST_CONFIDENCE: f32 = 0.50;

/// What a face is doing, all in 0..1 except the head angles in degrees.
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    /// The subject's left eye (MediaPipe's left, the image's right side).
    pub eye_open_l: f32,
    pub eye_open_r: f32,
    /// Mean of `mouthSmileLeft` and `mouthSmileRight`.
    pub smile: f32,
    pub jaw_open: f32,
    /// Mean of the inner and two outer brow-raise coefficients.
    pub brow_up: f32,
    /// Degrees, positive when the face turns to the subject's left.
    pub yaw: f32,
    /// Degrees, positive when the chin lifts (looking up).
    pub pitch: f32,
    /// Degrees, positive when the head tilts to the image's right.
    pub roll: f32,
    /// How well the 478-point mesh lands on the detector's five landmarks: 1
    /// when they agree, 0 when it is a false positive (see [`mesh_agreement`]).
    pub confidence: f32,
}

impl Expression {
    /// The lower of the two eyes, which is what a keeper check wants.
    pub fn eye_open_min(&self) -> f32 {
        self.eye_open_l.min(self.eye_open_r)
    }
}

/// The square the landmark model sees, and the map back to the frame: crop
/// pixel `(u, v)` samples the frame at `centre + R(angle) * (u - 128, v - 128) *
/// scale`.
pub struct Crop {
    cx: f64,
    cy: f64,
    scale: f64,
    angle: f64,
}

impl Crop {
    /// The frame pixel a crop pixel samples.
    pub fn to_image(&self, (u, v): (f64, f64)) -> (f64, f64) {
        let (dx, dy) = (
            (u - CROP as f64 / 2.0) * self.scale,
            (v - CROP as f64 / 2.0) * self.scale,
        );
        let (s, c) = self.angle.sin_cos();
        (self.cx + dx * c - dy * s, self.cy + dx * s + dy * c)
    }
}

/// A face box squared, de-rotated by the eye line and resampled to 256 px,
/// bilinear and black outside the frame (as `faces::align` does).
fn crop(image: &Rgb, face: &Face) -> (Rgb, Crop) {
    let [x0, y0, x1, y1] = face.bbox;
    let (side, angle) = crop_geometry(face);
    let cx = (x0 + x1) / 2.0;
    let cy = (y0 + y1) / 2.0;
    let scale = side / CROP as f64;
    let c = Crop {
        cx,
        cy,
        scale,
        angle,
    };
    let mut out = vec![0u8; CROP * CROP * 3];
    let at = |x: usize, y: usize| -> &[u8] {
        &image.data[(y * image.width + x) * 3..(y * image.width + x) * 3 + 3]
    };
    for v in 0..CROP {
        for u in 0..CROP {
            let (x, y) = c.to_image((u as f64 + 0.5, v as f64 + 0.5));
            let (x, y) = (x - 0.5, y - 0.5);
            if x < 0.0 || y < 0.0 || x > (image.width - 1) as f64 || y > (image.height - 1) as f64 {
                continue;
            }
            let (xi, yi) = (x as usize, y as usize);
            let (x1, y1) = (
                (xi + 1).min(image.width - 1),
                (yi + 1).min(image.height - 1),
            );
            let (fx, fy) = (x - xi as f64, y - yi as f64);
            let i = (v * CROP + u) * 3;
            for ch in 0..3 {
                let top = f64::from(at(xi, yi)[ch]) * (1.0 - fx) + f64::from(at(x1, yi)[ch]) * fx;
                let bottom =
                    f64::from(at(xi, y1)[ch]) * (1.0 - fx) + f64::from(at(x1, y1)[ch]) * fx;
                out[i + ch] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    (Rgb::new(CROP, CROP, out), c)
}

/// The crop's side in frame pixels and its rotation, from the face box and the
/// eye line.
fn crop_geometry(face: &Face) -> (f64, f64) {
    let [x0, y0, x1, y1] = face.bbox;
    let side = (x1 - x0).max(y1 - y0).max(8.0) * MARGIN;
    let [(rx, ry), (lx, ly)] = face.eyes;
    let angle = if (lx - rx).hypot(ly - ry) > 1.0 {
        (ly - ry).atan2(lx - rx)
    } else {
        0.0
    };
    (side, angle)
}

/// MediaPipe's landmark model and, when installed, its blendshape model.
pub struct ExpressionModel {
    landmarks: Session,
    blendshapes: Option<Session>,
}

impl ExpressionModel {
    /// Loads the landmark model, DirectML if it loads and the CPU otherwise.
    pub fn load(model: &Path) -> Result<ExpressionModel, String> {
        Ok(ExpressionModel {
            landmarks: session_with(model, true)?,
            blendshapes: None,
        })
    }

    /// The same, on the CPU only, for a benchmark or a machine where a GPU
    /// session is not wanted.
    pub fn load_cpu(model: &Path) -> Result<ExpressionModel, String> {
        Ok(ExpressionModel {
            landmarks: session_with(model, false)?,
            blendshapes: None,
        })
    }

    /// Adds the 52 blendshape coefficients. Without it, eye openness and the
    /// smile fall back to landmark geometry.
    pub fn load_blendshapes(&mut self, model: &Path) -> Result<(), String> {
        self.blendshapes = Some(session_with(model, true)?);
        Ok(())
    }

    pub fn load_blendshapes_cpu(&mut self, model: &Path) -> Result<(), String> {
        self.blendshapes = Some(session_with(model, false)?);
        Ok(())
    }

    pub fn has_blendshapes(&self) -> bool {
        self.blendshapes.is_some()
    }

    /// What `face` in `image` is doing. The box comes from [`crate::faces`],
    /// which does the detecting; this only reads the crop.
    pub fn read(&mut self, image: &Rgb, face: &Face) -> Result<Expression, String> {
        let (crop, map) = crop(image, face);
        let points = self.landmarks(&crop)?;
        let confidence = mesh_agreement(&map, &points, face);

        // Crop pixels for the coefficients and the fallbacks (rotation does
        // not change an aspect ratio); frame pixels for the pose, which is
        // about how the head sits in the photograph.
        let frame_points: Vec<(f64, f64)> = points.iter().map(|&p| map.to_image(p)).collect();
        let (yaw, pitch, roll) = head_pose(&frame_points);
        let mut expression = Expression {
            eye_open_l: 1.0,
            eye_open_r: 1.0,
            smile: 0.0,
            jaw_open: 0.0,
            brow_up: 0.0,
            yaw,
            pitch,
            roll,
            confidence,
        };

        match self.blendshapes.as_mut() {
            Some(session) => {
                let scores = blend(session, &points)?;
                let coef = |name: &str| -> f32 {
                    BLENDSHAPE_NAMES
                        .iter()
                        .position(|n| *n == name)
                        .and_then(|i| scores.get(i).copied())
                        .unwrap_or(0.0)
                        .clamp(0.0, 1.0)
                };
                expression.eye_open_l = 1.0 - coef("eyeBlinkLeft");
                expression.eye_open_r = 1.0 - coef("eyeBlinkRight");
                expression.smile = (coef("mouthSmileLeft") + coef("mouthSmileRight")) / 2.0;
                expression.jaw_open = coef("jawOpen");
                expression.brow_up =
                    (coef("browInnerUp") + coef("browOuterUpLeft") + coef("browOuterUpRight"))
                        / 3.0;
            }
            None => {
                let eyes = eye_aspect_ratios(&points);
                expression.eye_open_r = openness(eyes[0]);
                expression.eye_open_l = openness(eyes[1]);
                expression.smile = smile_from_landmarks(&points);
                expression.jaw_open = jaw_from_landmarks(&points);
                expression.brow_up = 0.0;
            }
        }
        Ok(expression)
    }

    /// The 478 landmarks in crop pixels.
    fn landmarks(&mut self, crop: &Rgb) -> Result<Vec<(f64, f64)>, String> {
        let mut tensor = vec![0f32; CROP * CROP * 3];
        for (i, px) in crop.data.as_chunks::<3>().0.iter().enumerate() {
            for c in 0..3 {
                tensor[i * 3 + c] = f32::from(px[c]) / 255.0;
            }
        }
        let input =
            Tensor::from_array(([1usize, CROP, CROP, 3], tensor)).map_err(|e| e.to_string())?;
        let out = self
            .landmarks
            .run(ort::inputs![input])
            .map_err(|e| e.to_string())?;
        let (_, data) = out[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        let data = data.to_vec();
        // 478 points, and possibly a score squeezed in at the end.
        let flat = if data.len() % 3 == 1 {
            &data[..data.len() - 1]
        } else {
            data.as_slice()
        };
        if flat.len() < 478 * 3 {
            return Err(format!(
                "landmark model returned {} values, expected 478 x 3 (+1)",
                data.len()
            ));
        }
        // Some exports emit the model's normalised coordinates; scale them.
        let wide = flat.iter().fold(0f32, |m, v| m.max(v.abs()));
        let scale = if wide <= 2.0 { CROP as f64 } else { 1.0 };
        Ok(flat
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| (f64::from(p[0]) * scale, f64::from(p[1]) * scale))
            .collect())
    }
}

/// How well the 478-point mesh lands on the face the detector found: the mean
/// distance, in face widths, between the mesh's eye, nose and mouth-corner
/// points and YuNet's five, 1 when they agree and 0 when they are a third of a
/// face width apart. A wheel or a dog that YuNet scored as a face puts the mesh
/// somewhere else entirely, so this is the figure to trust a face's
/// coefficients by.
pub fn mesh_agreement(map: &Crop, points: &[(f64, f64)], face: &Face) -> f32 {
    let pairs = [
        (EYE_R_OUTER, 0),
        (EYE_L_OUTER, 1),
        (NOSE_TIP, 2),
        (MOUTH_R, 3),
        (MOUTH_L, 4),
    ];
    let width = (face.bbox[2] - face.bbox[0]).max(1.0);
    let mut error = 0.0;
    for (mesh, detector) in pairs {
        let (x, y) = map.to_image(points[mesh]);
        let (u, v) = face.landmarks[detector];
        error += (x - u).hypot(y - v);
    }
    let mean = error / pairs.len() as f64 / width;
    (1.0 - mean / 0.33).clamp(0.0, 1.0) as f32
}

/// The 52 blendshape coefficients for a face's crop-space landmarks.
fn blend(session: &mut Session, points: &[(f64, f64)]) -> Result<Vec<f32>, String> {
    let mut tensor = Vec::with_capacity(146 * 2);
    for i in BLENDSHAPE_SUBSET {
        let (x, y) = points
            .get(i)
            .copied()
            .ok_or_else(|| format!("landmark {i} missing from the crop"))?;
        tensor.extend_from_slice(&[x as f32, y as f32]);
    }
    let input = Tensor::from_array(([1usize, 146, 2], tensor)).map_err(|e| e.to_string())?;
    let out = session
        .run(ort::inputs![input])
        .map_err(|e| e.to_string())?;
    let (_, data) = out[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| e.to_string())?;
    Ok(data.to_vec())
}

/// Eye aspect ratio, right then left: the mean of two vertical eyelid
/// openings over the eye's width (Soukupova and Cech).
pub fn eye_aspect_ratios(points: &[(f64, f64)]) -> [f32; 2] {
    let dist = |a: usize, b: usize| {
        let (ax, ay) = points[a];
        let (bx, by) = points[b];
        (ax - bx).hypot(ay - by)
    };
    let ear = |upper: [usize; 2], lower: [usize; 2], inner: usize, outer: usize| -> f32 {
        let width = dist(inner, outer);
        if width < 1e-6 {
            return 0.0;
        }
        ((dist(upper[0], lower[0]) + dist(upper[1], lower[1])) / (2.0 * width)) as f32
    };
    [
        ear(EYE_R_UPPER, EYE_R_LOWER, EYE_R_INNER, EYE_R_OUTER),
        ear(EYE_L_UPPER, EYE_L_LOWER, EYE_L_INNER, EYE_L_OUTER),
    ]
}

/// Eye aspect ratio to 0..1 openness, [`EAR_CLOSED`] to [`EAR_OPEN`].
pub fn openness(ear: f32) -> f32 {
    ((ear - EAR_CLOSED) / (EAR_OPEN - EAR_CLOSED)).clamp(0.0, 1.0)
}

/// A smile without the blendshape model: how far the mouth corners sit above
/// the lips, as a fraction of the inter-ocular distance.
pub fn smile_from_landmarks(points: &[(f64, f64)]) -> f32 {
    let (rx, ry) = points[EYE_R_OUTER];
    let (lx, ly) = points[EYE_L_OUTER];
    let iod = (rx - lx).hypot(ry - ly);
    if iod < 1e-6 {
        return 0.0;
    }
    let lip = (points[LIP_UPPER].1 + points[LIP_LOWER].1) / 2.0;
    let corners = (points[MOUTH_R].1 + points[MOUTH_L].1) / 2.0;
    // Image y grows downwards, so a corner above the lips has the larger lift.
    let lift = ((lip - corners) / iod) as f32;
    ((lift - SMILE_LIFT_LOW) / (SMILE_LIFT_HIGH - SMILE_LIFT_LOW)).clamp(0.0, 1.0)
}

/// A jaw opening without the blendshape model: lip separation over the
/// eye-line-to-chin distance, 0 at a closed mouth and 1 at a wide one.
fn jaw_from_landmarks(points: &[(f64, f64)]) -> f32 {
    let (_, ry) = points[EYE_R_OUTER];
    let (_, ly) = points[EYE_L_OUTER];
    let eye_y = (ry + ly) / 2.0;
    let span = points[CHIN].1 - eye_y;
    if span < 1e-6 {
        return 0.0;
    }
    let gap = (points[LIP_LOWER].1 - points[LIP_UPPER].1) / span;
    ((gap as f32 - 0.02) / 0.18).clamp(0.0, 1.0)
}

/// Yaw, pitch and roll in degrees from the 478 landmarks, in frame pixels.
///
/// Roll is the eye line's angle. Yaw is the nose tip's offset from the eye
/// midpoint along the eye line, as a fraction of half the eye distance: a
/// documented linear proxy, about 45 degrees at 1.0. Pitch is where the nose
/// tip sits between the eye line and the chin, 0.37 of the way on the frontal
/// faces in the sample; the chin side foreshortens as the chin lifts, so a
/// larger fraction reads as looking up (about 45 degrees for the next fifth of
/// the span). Good to a few degrees on frontal faces, a rough sign on profiles;
/// not a substitute for PnP against a 3D model.
pub fn head_pose(points: &[(f64, f64)]) -> (f32, f32, f32) {
    let (ro_x, ro_y) = points[EYE_R_OUTER];
    let (lo_x, lo_y) = points[EYE_L_OUTER];
    let (ex, ey) = ((ro_x + lo_x) / 2.0, (ro_y + lo_y) / 2.0);
    let eye_vec = (lo_x - ro_x, lo_y - ro_y);
    let eye_dist = eye_vec.0.hypot(eye_vec.1);
    let roll = eye_vec.1.atan2(eye_vec.0).to_degrees() as f32;
    if eye_dist < 1e-6 {
        return (0.0, 0.0, roll);
    }
    let line = (eye_vec.0 / eye_dist, eye_vec.1 / eye_dist);
    let normal = (-line.1, line.0);
    let (nx, ny) = points[NOSE_TIP];
    let along = (nx - ex) * line.0 + (ny - ey) * line.1;
    let yaw = ((along / (eye_dist / 2.0)) * 45.0).clamp(-90.0, 90.0) as f32;

    let down = (points[CHIN].0 - ex) * normal.0 + (points[CHIN].1 - ey) * normal.1;
    let nose_down = (nx - ex) * normal.0 + (ny - ey) * normal.1;
    let pitch = if down.abs() < 1e-6 {
        0.0
    } else {
        (((nose_down / down) - 0.37) / 0.20 * 45.0).clamp(-45.0, 45.0) as f32
    };
    (yaw, pitch, roll)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frontal face with the points the pose, the ratio and the smile need,
    /// everything else at the crop centre.
    fn frontal() -> Vec<(f64, f64)> {
        let mut p = vec![(128.0, 128.0); 478];
        p[EYE_R_OUTER] = (88.0, 110.0);
        p[EYE_R_INNER] = (120.0, 110.0);
        p[EYE_L_INNER] = (136.0, 110.0);
        p[EYE_L_OUTER] = (168.0, 110.0);
        p[NOSE_TIP] = (128.0, 140.0);
        p[CHIN] = (128.0, 200.0);
        p[LIP_UPPER] = (128.0, 168.0);
        p[LIP_LOWER] = (128.0, 172.0);
        p[MOUTH_R] = (108.0, 170.0);
        p[MOUTH_L] = (148.0, 170.0);
        // Open right eye: lids 6 px apart over a 40 px eye.
        p[EYE_R_UPPER[0]] = (100.0, 104.0);
        p[EYE_R_UPPER[1]] = (112.0, 104.0);
        p[EYE_R_LOWER[0]] = (100.0, 116.0);
        p[EYE_R_LOWER[1]] = (112.0, 116.0);
        p[EYE_L_UPPER[0]] = (144.0, 104.0);
        p[EYE_L_UPPER[1]] = (156.0, 104.0);
        p[EYE_L_LOWER[0]] = (144.0, 116.0);
        p[EYE_L_LOWER[1]] = (156.0, 116.0);
        p
    }

    #[test]
    fn a_frontal_face_has_no_yaw_or_roll() {
        let (yaw, _, roll) = head_pose(&frontal());
        assert!(yaw.abs() < 0.5, "{yaw}");
        assert!(roll.abs() < 0.5, "{roll}");
    }

    #[test]
    fn yaw_follows_the_nose_and_roll_the_eye_line() {
        let mut turned = frontal();
        turned[NOSE_TIP] = (128.0 + 20.0, 140.0);
        let (yaw, _, _) = head_pose(&turned);
        assert!((yaw - 22.5).abs() < 0.5, "{yaw}");

        let mut tilted = frontal();
        let (angle, sin, cos) = (
            10f64.to_radians(),
            10f64.to_radians().sin(),
            10f64.to_radians().cos(),
        );
        for i in [EYE_R_OUTER, EYE_L_OUTER] {
            let (dx, dy) = (tilted[i].0 - 128.0, tilted[i].1 - 110.0);
            tilted[i] = (128.0 + dx * cos - dy * sin, 110.0 + dx * sin + dy * cos);
        }
        let (_, _, roll) = head_pose(&tilted);
        assert!((roll - angle.to_degrees() as f32).abs() < 1.0, "{roll}");
    }

    #[test]
    fn a_chin_lift_compresses_the_chin_side_and_reads_positive_pitch() {
        let (_, frontal_pitch, _) = head_pose(&frontal());
        let mut up = frontal();
        // The chin rises towards the eye line in projection.
        up[CHIN] = (128.0, 170.0);
        let (_, pitch, _) = head_pose(&up);
        assert!(pitch > frontal_pitch, "{pitch} vs {frontal_pitch}");
        assert!(pitch > 0.0, "{pitch}");
    }

    #[test]
    fn aspect_ratio_reads_open_eyes_open_and_closed_eyes_shut() {
        let open = eye_aspect_ratios(&frontal());
        assert!((open[0] - 0.375).abs() < 1e-3, "{open:?}");
        assert!(openness(open[0]) > 0.9);

        let mut closed = frontal();
        for i in EYE_R_UPPER.into_iter().chain(EYE_R_LOWER) {
            closed[i].1 = 110.0;
        }
        let shut = eye_aspect_ratios(&closed);
        assert!(shut[0] < EAR_CLOSED, "{shut:?}");
        assert_eq!(openness(shut[0]), 0.0);
    }

    #[test]
    fn lifted_mouth_corners_read_as_a_smile() {
        assert_eq!(smile_from_landmarks(&frontal()), 0.0);
        let mut smiling = frontal();
        for i in [MOUTH_R, MOUTH_L] {
            smiling[i].1 -= 8.0;
        }
        let smile = smile_from_landmarks(&smiling);
        assert!(smile > 0.5, "{smile}");
    }

    #[test]
    fn the_blendshape_order_is_mediapipes() {
        assert_eq!(BLENDSHAPE_NAMES.len(), 52);
        assert_eq!(BLENDSHAPE_NAMES[9], "eyeBlinkLeft");
        assert_eq!(BLENDSHAPE_NAMES[44], "mouthSmileLeft");
        assert_eq!(BLENDSHAPE_NAMES[25], "jawOpen");
        assert_eq!(BLENDSHAPE_SUBSET.len(), 146);
        assert!(BLENDSHAPE_SUBSET.iter().all(|i| *i < 478));
    }

    #[test]
    fn crop_is_square_upright_and_maps_back_to_the_face_centre() {
        let image = Rgb::new(400, 300, vec![90; 400 * 300 * 3]);
        let face = Face {
            score: 0.9,
            bbox: [100.0, 60.0, 180.0, 140.0],
            eyes: [(130.0, 100.0), (150.0, 100.0)],
            landmarks: [
                (130.0, 100.0),
                (150.0, 100.0),
                (0.0, 0.0),
                (0.0, 0.0),
                (0.0, 0.0),
            ],
        };
        let (out, map) = crop(&image, &face);
        assert_eq!((out.width, out.height), (CROP, CROP));
        let (x, y) = map.to_image((128.0, 128.0));
        assert!((x - 140.0).abs() < 1e-9 && (y - 100.0).abs() < 1e-9);
        // 1.5x the 80 px box.
        assert!((map.scale * CROP as f64 - 120.0).abs() < 1e-9);
    }

    #[test]
    fn the_models_report_the_expected_inputs_and_outputs() {
        use ort::value::ValueType;
        let describe = |session: &Session| -> Vec<(String, String)> {
            session
                .inputs()
                .iter()
                .chain(session.outputs())
                .map(|o| {
                    let ty = match o.dtype() {
                        ValueType::Tensor { ty, shape, .. } => format!("{ty:?} {shape:?}"),
                        other => format!("{other:?}"),
                    };
                    (o.name().to_string(), ty)
                })
                .collect()
        };
        if let Some(path) = installed(LANDMARKS) {
            let session = session_with(&path, false).unwrap();
            let io = describe(&session);
            eprintln!("landmarks: {io:#?}");
            assert!(io.iter().any(|(_, ty)| ty.contains("1434")), "{io:?}");
        }
        if let Some(path) = installed(BLENDSHAPES) {
            let session = session_with(&path, false).unwrap();
            let io = describe(&session);
            eprintln!("blendshapes: {io:#?}");
            assert!(io.iter().any(|(_, ty)| ty.contains("146")), "{io:?}");
            assert!(io.iter().any(|(_, ty)| ty.contains("[52]")), "{io:?}");
        }
    }

    #[test]
    fn the_blendshape_model_is_scale_invariant() {
        let Some(path) = installed(BLENDSHAPES) else {
            eprintln!("no {BLENDSHAPES}; see docs");
            return;
        };
        let mut session = session_with(&path, false).unwrap();
        let base: Vec<(f64, f64)> = (0..478)
            .map(|i| {
                let t = i as f64 / 478.0;
                (40.0 + 176.0 * t, 60.0 + 140.0 * (t * 3.0).sin())
            })
            .collect();
        let big: Vec<(f64, f64)> = base.iter().map(|(x, y)| (x * 10.0, y * 10.0)).collect();
        let small = blend(&mut session, &base).unwrap();
        let large = blend(&mut session, &big).unwrap();
        let worst = small
            .iter()
            .zip(&large)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        eprintln!("blendshape scale drift: {worst}");
        assert!(worst < 0.05, "the input is not normalised: {worst}");
    }

    #[test]
    fn mesh_agreement_separates_a_face_from_a_stray_mesh() {
        let image = Rgb::new(400, 300, vec![90; 400 * 300 * 3]);
        let face = Face {
            score: 0.9,
            bbox: [100.0, 60.0, 180.0, 140.0],
            eyes: [(130.0, 100.0), (150.0, 100.0)],
            landmarks: [
                (130.0, 100.0),
                (150.0, 100.0),
                (140.0, 115.0),
                (128.0, 128.0),
                (152.0, 128.0),
            ],
        };
        let (_, map) = crop(&image, &face);
        let to_crop = |(x, y): (f64, f64)| {
            let (dx, dy) = (x - map.cx, y - map.cy);
            let (s, c) = map.angle.sin_cos();
            (
                (dx * c + dy * s) / map.scale + CROP as f64 / 2.0,
                (-dx * s + dy * c) / map.scale + CROP as f64 / 2.0,
            )
        };
        let mut points = vec![(0.0, 0.0); 478];
        for (mesh, detector) in [
            (EYE_R_OUTER, 0),
            (EYE_L_OUTER, 1),
            (NOSE_TIP, 2),
            (MOUTH_R, 3),
            (MOUTH_L, 4),
        ] {
            points[mesh] = to_crop(face.landmarks[detector]);
        }
        assert!(mesh_agreement(&map, &points, &face) > 0.99);
        // A mesh half a face away is a false positive, not a face.
        points.iter_mut().for_each(|(x, _)| *x += 80.0);
        assert!(mesh_agreement(&map, &points, &face) < 0.01);
    }

    /// Models are optional locally; each test skips without them.
    fn installed(name: &str) -> Option<std::path::PathBuf> {
        let dir =
            std::path::PathBuf::from(std::env::var("USERPROFILE").ok()?).join(".conrod/models");
        let path = dir.join(name);
        path.is_file().then_some(path)
    }

    #[test]
    fn the_landmark_model_takes_a_256_square_and_returns_478_points() {
        let Some(path) = installed(LANDMARKS) else {
            eprintln!("no {LANDMARKS}; see docs");
            return;
        };
        let mut model = ExpressionModel::load_cpu(&path).unwrap();
        let image = Rgb::new(CROP, CROP, vec![128; CROP * CROP * 3]);
        let points = model.landmarks(&image).unwrap();
        assert_eq!(points.len(), 478);
        assert!(points.iter().all(|(x, y)| x.is_finite() && y.is_finite()));
    }

    #[test]
    fn the_blendshape_model_takes_146_points_and_returns_52() {
        let Some(path) = installed(BLENDSHAPES) else {
            eprintln!("no {BLENDSHAPES}; see docs");
            return;
        };
        let mut session = session_with(&path, false).unwrap();
        let mut points = vec![(0.0, 0.0); 478];
        for (i, p) in points.iter_mut().enumerate() {
            let t = i as f64 / 478.0;
            *p = (40.0 + 176.0 * t, 40.0 + 176.0 * t);
        }
        let scores = blend(&mut session, &points).unwrap();
        assert_eq!(scores.len(), 52);
        assert!(scores.iter().all(|s| s.is_finite()));
        eprintln!(
            "blendshape range {:?}",
            (
                scores.iter().cloned().fold(f32::MIN, f32::max),
                scores.iter().cloned().fold(f32::MAX, f32::min)
            )
        );
    }
}
