//! Finding vehicles and people in a frame.
//!
//! Runs on ONNX Runtime instead of ultralytics + torch. The model is YOLO11s
//! exported by `tools/export_onnx.py`, decoded to reproduce ultralytics box
//! for box: rectangular letterbox (ultralytics' predict() sets rect=True),
//! argmax over all 80 classes then the class filter, class-wise NMS, and
//! ultralytics' rescale. Then a few of the app's own rules: drop tiny boxes, drop boxes inside a
//! bigger one, keep the largest few, pad each into the crop to analyse.

use crate::imageops::Rgb;
use ort::ep;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

pub const IMGSZ: usize = 960;
const STRIDE: usize = 32;
const PAD: f32 = 114.0 / 255.0;
/// Offsets each class apart so NMS never compares boxes of different classes.
const CLASS_OFFSET: f64 = 7680.0;

/// COCO ids.
pub const PERSON: usize = 0;
pub const VEHICLES: [usize; 4] = [2, 3, 5, 7];
pub const PETS: [usize; 2] = [15, 16];

pub fn class_name(id: usize) -> &'static str {
    match id {
        0 => "person",
        2 => "car",
        3 => "motorcycle",
        5 => "bus",
        7 => "truck",
        15 => "cat",
        16 => "dog",
        _ => "vehicle",
    }
}

/// The detector settings from `conrod/config.py`, with its defaults.
#[derive(Debug, Clone)]
pub struct DetectOptions {
    pub conf: f32,
    pub iou: f32,
    pub classes: Vec<usize>,
    /// Boxes whose longer side is under this share of the frame's short edge
    /// are background traffic, not the subject.
    pub min_box_fraction: f64,
    pub max_per_frame: usize,
    pub crop_padding: f64,
    /// A subject this much of the frame is analysed as the whole frame.
    pub dominant_subject_fraction: f64,
}

impl Default for DetectOptions {
    fn default() -> Self {
        DetectOptions {
            conf: 0.25,
            iou: 0.7,
            classes: VEHICLES.to_vec(),
            min_box_fraction: 0.08,
            max_per_frame: 8,
            crop_padding: 0.18,
            dominant_subject_fraction: 0.45,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub class_id: usize,
    pub conf: f32,
    /// The detector's own box, frame pixels.
    pub bbox: [f64; 4],
    /// The padded box actually analysed.
    pub crop_box: [f64; 4],
}

impl Detection {
    pub fn area(&self) -> f64 {
        (self.bbox[2] - self.bbox[0]) * (self.bbox[3] - self.bbox[1])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    /// DirectML if a DirectX 12 GPU will take it, else the CPU.
    Auto,
    Cpu,
    DirectMl,
}

pub struct Detector {
    session: Session,
    /// What is actually running, for the status area to say.
    pub device: &'static str,
}

/// Open an ONNX session with the requested provider. Shared by detector,
/// plate detector/reader, and the similarity embedder.
pub(crate) fn open_session(
    model: &Path,
    device: Device,
) -> Result<(Session, &'static str), String> {
    let open = |gpu: bool| -> Result<Session, String> {
        let fail = |e: ort::Error<ort::session::builder::SessionBuilder>| e.to_string();
        let mut builder = Session::builder().map_err(|e| e.to_string())?;
        if gpu {
            builder = builder
                .with_execution_providers([ep::DirectML::default().build().error_on_failure()])
                .map_err(fail)?;
        } else {
            // A share of the cores, so several CPU sessions can run side by side.
            let threads = (std::thread::available_parallelism().map_or(4, |n| n.get()) / 4).max(1);
            builder = builder.with_intra_threads(threads).map_err(fail)?;
            builder = builder
                .with_execution_providers([ep::CPU::default().build()])
                .map_err(fail)?;
        }
        builder.commit_from_file(model).map_err(|e| e.to_string())
    };
    match device {
        Device::Cpu => Ok((open(false)?, "CPU")),
        Device::DirectMl => Ok((open(true)?, "DirectML")),
        Device::Auto => match open(true) {
            Ok(session) => Ok((session, "DirectML")),
            Err(_) => Ok((open(false)?, "CPU")),
        },
    }
}

impl Detector {
    pub fn load(model: &Path, device: Device) -> Result<Detector, String> {
        let (session, device) = open_session(model, device)?;
        Ok(Detector { session, device })
    }

    /// Find subjects in one upright frame, with detect.py's rules applied.
    pub fn detect(
        &mut self,
        frame: &Rgb,
        options: &DetectOptions,
    ) -> Result<Vec<Detection>, String> {
        self.detect_letterboxed(letterbox(frame), options)
    }

    /// `detect` for a frame already run through `letterbox`. That resize is
    /// the costly, parallelisable half, so a caller sharing this detector
    /// behind a lock does it before taking the lock and holds it only for
    /// the network.
    pub fn detect_letterboxed(
        &mut self,
        input: Letterboxed,
        options: &DetectOptions,
    ) -> Result<Vec<Detection>, String> {
        let (width, height) = input.frame;
        let boxes = self.run(input, options)?;
        Ok(filter(boxes, width, height, options))
    }

    /// What the network itself found, as ultralytics would report it:
    /// (class, confidence, box in frame pixels).
    pub fn raw(
        &mut self,
        frame: &Rgb,
        options: &DetectOptions,
    ) -> Result<Vec<(usize, f32, [f64; 4])>, String> {
        self.run(letterbox(frame), options)
    }

    fn run(
        &mut self,
        mut input: Letterboxed,
        options: &DetectOptions,
    ) -> Result<Vec<(usize, f32, [f64; 4])>, String> {
        let dims = [1usize, 3, input.height, input.width];
        let tensor = Tensor::from_array((dims, std::mem::take(&mut input.tensor)))
            .map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| e.to_string())?;
        let (shape, raw) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        Ok(decode(raw, shape[2] as usize, &input, options))
    }
}

pub struct Letterboxed {
    pub tensor: Vec<f32>,
    pub width: usize,
    pub height: usize,
    gain: f64,
    pad: (f64, f64),
    frame: (usize, usize),
}

/// ultralytics' LetterBox(auto=True): long edge to IMGSZ, each side padded
/// only up to the next multiple of 32, centred with its -0.1/+0.1 rounding.
pub fn letterbox(frame: &Rgb) -> Letterboxed {
    let (fw, fh) = (frame.width, frame.height);
    let r = (IMGSZ as f64 / fh as f64).min(IMGSZ as f64 / fw as f64);
    let (new_w, new_h) = (
        (fw as f64 * r).round() as usize,
        (fh as f64 * r).round() as usize,
    );
    let resized = if (new_w, new_h) == (fw, fh) {
        frame.clone()
    } else {
        frame.resize_cv2_linear(new_w, new_h)
    };
    let dw = ((IMGSZ - new_w) % STRIDE) as f64 / 2.0;
    let dh = ((IMGSZ - new_h) % STRIDE) as f64 / 2.0;
    let (top, bottom) = ((dh - 0.1).round() as usize, (dh + 0.1).round() as usize);
    let (left, right) = ((dw - 0.1).round() as usize, (dw + 0.1).round() as usize);
    let (width, height) = (new_w + left + right, new_h + top + bottom);
    let plane = width * height;
    let mut tensor = vec![PAD; plane * 3];
    for y in 0..new_h {
        for x in 0..new_w {
            let s = (y * new_w + x) * 3;
            let d = (y + top) * width + x + left;
            for c in 0..3 {
                tensor[c * plane + d] = f32::from(resized.data[s + c]) / 255.0;
            }
        }
    }
    // ultralytics' scale_boxes recomputes these from the padded input.
    let gain = (height as f64 / fh as f64).min(width as f64 / fw as f64);
    let pad_x = ((width as f64 - (fw as f64 * gain).round()) / 2.0 - 0.1).round();
    let pad_y = ((height as f64 - (fh as f64 * gain).round()) / 2.0 - 0.1).round();
    Letterboxed {
        tensor,
        width,
        height,
        gain,
        pad: (pad_x, pad_y),
        frame: (fw, fh),
    }
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

/// `[1, 84, N]` -> boxes in frame pixels.
fn decode(
    raw: &[f32],
    anchors: usize,
    input: &Letterboxed,
    options: &DetectOptions,
) -> Vec<(usize, f32, [f64; 4])> {
    let mut found: Vec<(usize, f32, [f64; 4])> = Vec::new();
    for a in 0..anchors {
        let (mut best, mut cls) = (f32::MIN, 0);
        for c in 0..80 {
            let s = raw[(4 + c) * anchors + a];
            if s > best {
                best = s;
                cls = c;
            }
        }
        if best <= options.conf || !options.classes.contains(&cls) {
            continue;
        }
        let (cx, cy, w, h) = (
            raw[a],
            raw[anchors + a],
            raw[2 * anchors + a],
            raw[3 * anchors + a],
        );
        let b = [cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0].map(f64::from);
        found.push((cls, best, b));
    }
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut kept: Vec<(usize, f32, [f64; 4])> = Vec::new();
    for d in found {
        let shifted = |b: &[f64; 4], c: usize| b.map(|v| v + c as f64 * CLASS_OFFSET);
        if kept
            .iter()
            .all(|k| iou(&shifted(&k.2, k.0), &shifted(&d.2, d.0)) <= f64::from(options.iou))
        {
            kept.push(d);
        }
    }
    kept.truncate(300);
    let (fw, fh) = (input.frame.0 as f64, input.frame.1 as f64);
    for d in &mut kept {
        let b = &mut d.2;
        b[0] = ((b[0] - input.pad.0) / input.gain).clamp(0.0, fw);
        b[2] = ((b[2] - input.pad.0) / input.gain).clamp(0.0, fw);
        b[1] = ((b[1] - input.pad.1) / input.gain).clamp(0.0, fh);
        b[3] = ((b[3] - input.pad.1) / input.gain).clamp(0.0, fh);
    }
    kept
}

/// detect.py's rules on top of the detector's boxes.
fn filter(
    boxes: Vec<(usize, f32, [f64; 4])>,
    width: usize,
    height: usize,
    options: &DetectOptions,
) -> Vec<Detection> {
    let min_edge = width.min(height) as f64 * options.min_box_fraction;
    let mut found: Vec<Detection> = boxes
        .into_iter()
        .filter(|(_, _, b)| (b[2] - b[0]).max(b[3] - b[1]) >= min_edge)
        .map(|(class_id, conf, bbox)| Detection {
            class_id,
            conf,
            bbox,
            crop_box: expand_box(bbox, width, height, options),
        })
        .collect();
    // Largest first, stable sort so ties keep their detection order.
    found.sort_by(|a, b| b.area().total_cmp(&a.area()));
    let mut kept: Vec<Detection> = Vec::new();
    for det in found {
        if kept
            .iter()
            .all(|bigger| overlap_fraction(&det.bbox, &bigger.bbox) <= 0.75)
        {
            kept.push(det);
        }
    }
    kept.truncate(options.max_per_frame);
    kept
}

/// How much of `inner` lies inside `outer`.
fn overlap_fraction(inner: &[f64; 4], outer: &[f64; 4]) -> f64 {
    let (ix1, iy1) = (inner[0].max(outer[0]), inner[1].max(outer[1]));
    let (ix2, iy2) = (inner[2].min(outer[2]), inner[3].min(outer[3]));
    if ix2 <= ix1 || iy2 <= iy1 {
        return 0.0;
    }
    let area = (inner[2] - inner[0]) * (inner[3] - inner[1]);
    if area > 0.0 {
        (ix2 - ix1) * (iy2 - iy1) / area
    } else {
        0.0
    }
}

/// Pad a box proportionally; a subject filling the frame is the whole frame.
pub fn expand_box(b: [f64; 4], width: usize, height: usize, options: &DetectOptions) -> [f64; 4] {
    let (w, h) = (width as f64, height as f64);
    let frame_area = (w * h).max(1.0);
    if (b[2] - b[0]) * (b[3] - b[1]) / frame_area >= options.dominant_subject_fraction {
        return [0.0, 0.0, w, h];
    }
    let (px, py) = (
        (b[2] - b[0]) * options.crop_padding,
        (b[3] - b[1]) * options.crop_padding,
    );
    [
        (b[0] - px).max(0.0),
        (b[1] - py).max(0.0),
        (b[2] + px).min(w),
        (b[3] + py).min(h),
    ]
}
