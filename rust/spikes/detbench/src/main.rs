//! Spike E: how fast can a frame go from JPEG to boxes, in Rust, per device?
//!
//!     detbench <detector_local.json> <model.onnx>
//!
//! The fixture is the one tools/export_onnx.py writes: real preview frames
//! and the boxes ultralytics found on them. For every combination of decode
//! (full, or the JPEG's own 1/2 and 1/4 scaling) and device (CPU, DirectML,
//! CUDA) this reports time per stage and how many of ultralytics' boxes were
//! reproduced -- so a faster path has to prove it still finds the same cars.

use ort::ep;
use ort::session::Session;
use ort::value::Tensor;
use serde_json::Value;
use std::fs;
use std::io::BufReader;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

const IMGSZ: usize = 960;
const STRIDE: usize = 32;
const CONF: f32 = 0.25;
const IOU: f32 = 0.7;
const CLASSES: [usize; 5] = [0, 2, 3, 5, 7];

struct Prepared {
    tensor: Vec<f32>,
    height: usize,
    width: usize,
    /// Rescale back to the original frame: (gain, pad_x, pad_y).
    gain: f64,
    pad: (f64, f64),
    frame: (usize, usize),
}

#[derive(Clone, Debug)]
struct Det {
    cls: usize,
    conf: f32,
    bbox: [f64; 4],
}

/// Decode, optionally letting the JPEG decoder scale by 1/2, 1/4 or 1/8 in
/// the DCT, which is most of the saving: a 6960x4640 frame at 1/4 is 1740x1160
/// and still bigger than the 960x640 the detector wants.
fn decode(path: &str, denominator: u16) -> (Vec<u8>, usize, usize) {
    let file = fs::File::open(path).expect("open preview");
    let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(file));
    decoder.read_info().expect("jpeg header");
    let info = decoder.info().unwrap();
    if denominator > 1 {
        decoder
            .scale(info.width / denominator, info.height / denominator)
            .expect("scale");
    }
    let pixels = decoder.decode().expect("decode");
    let info = decoder.info().unwrap();
    (pixels, info.width as usize, info.height as usize)
}

/// cv2.resize INTER_LINEAR, RGB u8: two taps per axis, pixel-centre aligned,
/// no antialiasing -- what ultralytics' letterbox does.
fn resize_linear(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    let taps = |dst: usize, src_len: usize| -> Vec<(usize, usize, f32)> {
        let scale = src_len as f64 / dst as f64;
        (0..dst)
            .map(|d| {
                let f = (d as f64 + 0.5) * scale - 0.5;
                let mut i = f.floor() as isize;
                let mut frac = (f - i as f64) as f32;
                if i < 0 {
                    i = 0;
                    frac = 0.0;
                }
                let i = i as usize;
                if i >= src_len - 1 {
                    (src_len - 1, src_len - 1, 0.0)
                } else {
                    (i, i + 1, frac)
                }
            })
            .collect()
    };
    let xs = taps(dw, sw);
    let ys = taps(dh, sh);
    let mut out = vec![0u8; dw * dh * 3];
    for (y, &(y0, y1, fy)) in ys.iter().enumerate() {
        for (x, &(x0, x1, fx)) in xs.iter().enumerate() {
            for c in 0..3 {
                let p = |yy: usize, xx: usize| f32::from(src[(yy * sw + xx) * 3 + c]);
                let top = p(y0, x0) * (1.0 - fx) + p(y0, x1) * fx;
                let bottom = p(y1, x0) * (1.0 - fx) + p(y1, x1) * fx;
                out[(y * dw + x) * 3 + c] = (top * (1.0 - fy) + bottom * fy).round() as u8;
            }
        }
    }
    out
}

fn prepare(path: &str, frame: (usize, usize), denominator: u16) -> Prepared {
    let (pixels, w, h) = decode(path, denominator);
    // The long edge to IMGSZ, measured on the *original* frame so a scaled
    // decode lands on the same letterbox as a full one.
    let (fh, fw) = frame;
    let r = (IMGSZ as f64 / fh as f64).min(IMGSZ as f64 / fw as f64);
    let (new_w, new_h) = (
        (fw as f64 * r).round() as usize,
        (fh as f64 * r).round() as usize,
    );
    let resized = if (w, h) == (new_w, new_h) {
        pixels
    } else {
        resize_linear(&pixels, w, h, new_w, new_h)
    };
    let dw = ((IMGSZ - new_w) % STRIDE) as f64 / 2.0;
    let dh = ((IMGSZ - new_h) % STRIDE) as f64 / 2.0;
    let (top, bottom) = ((dh - 0.1).round() as usize, (dh + 0.1).round() as usize);
    let (left, right) = ((dw - 0.1).round() as usize, (dw + 0.1).round() as usize);
    let (in_w, in_h) = (new_w + left + right, new_h + top + bottom);

    let plane = in_w * in_h;
    let mut tensor = vec![114.0 / 255.0; plane * 3];
    for y in 0..new_h {
        for x in 0..new_w {
            let s = (y * new_w + x) * 3;
            let d = (y + top) * in_w + (x + left);
            for c in 0..3 {
                tensor[c * plane + d] = f32::from(resized[s + c]) / 255.0;
            }
        }
    }
    let gain = (in_h as f64 / fh as f64).min(in_w as f64 / fw as f64);
    let pad_x = ((in_w as f64 - (fw as f64 * gain).round()) / 2.0 - 0.1).round();
    let pad_y = ((in_h as f64 - (fh as f64 * gain).round()) / 2.0 - 0.1).round();
    Prepared {
        tensor,
        height: in_h,
        width: in_w,
        gain,
        pad: (pad_x, pad_y),
        frame,
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

/// [1, 84, N] -> boxes: argmax over all classes, then the class filter, then
/// class-wise NMS, then back to frame pixels. Same as tools/export_onnx.py.
fn postprocess(raw: &[f32], anchors: usize, p: &Prepared) -> Vec<Det> {
    let mut found: Vec<Det> = Vec::new();
    for a in 0..anchors {
        let (mut best, mut cls) = (f32::MIN, 0);
        for c in 0..80 {
            let s = raw[(4 + c) * anchors + a];
            if s > best {
                best = s;
                cls = c;
            }
        }
        if best <= CONF || !CLASSES.contains(&cls) {
            continue;
        }
        let (cx, cy, w, h) = (
            raw[a],
            raw[anchors + a],
            raw[2 * anchors + a],
            raw[3 * anchors + a],
        );
        found.push(Det {
            cls,
            conf: best,
            bbox: [
                f64::from(cx - w / 2.0),
                f64::from(cy - h / 2.0),
                f64::from(cx + w / 2.0),
                f64::from(cy + h / 2.0),
            ],
        });
    }
    found.sort_by(|a, b| b.conf.total_cmp(&a.conf));
    let mut kept: Vec<Det> = Vec::new();
    for d in found {
        if kept
            .iter()
            .all(|k| k.cls != d.cls || iou(&k.bbox, &d.bbox) <= f64::from(IOU))
        {
            kept.push(d);
        }
    }
    let (fh, fw) = (p.frame.0 as f64, p.frame.1 as f64);
    for d in &mut kept {
        let b = &mut d.bbox;
        b[0] = ((b[0] - p.pad.0) / p.gain).clamp(0.0, fw);
        b[2] = ((b[2] - p.pad.0) / p.gain).clamp(0.0, fw);
        b[1] = ((b[1] - p.pad.1) / p.gain).clamp(0.0, fh);
        b[3] = ((b[3] - p.pad.1) / p.gain).clamp(0.0, fh);
    }
    kept
}

fn infer(session: &mut Session, p: &Prepared) -> Vec<Det> {
    let input = Tensor::from_array(([1usize, 3, p.height, p.width], p.tensor.clone())).unwrap();
    let outputs = session.run(ort::inputs![input]).unwrap();
    let (shape, data) = outputs[0].try_extract_tensor::<f32>().unwrap();
    postprocess(data, shape[2] as usize, p)
}

fn matches(reference: &[Det], ours: &[Det], threshold: f64) -> usize {
    let mut used = vec![false; ours.len()];
    let mut hit = 0;
    for r in reference {
        let best = ours
            .iter()
            .enumerate()
            .filter(|(j, o)| !used[*j] && o.cls == r.cls)
            .map(|(j, o)| (j, iou(&r.bbox, &o.bbox)))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((j, v)) = best {
            if v >= threshold {
                used[j] = true;
                hit += 1;
            }
        }
    }
    hit
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let fixture: Value = serde_json::from_str(&fs::read_to_string(&args[1]).unwrap()).unwrap();
    let model = &args[2];
    let frames: Vec<(String, (usize, usize), Vec<Det>)> = fixture["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            let shape = f["shape"].as_array().unwrap();
            let boxes = f["boxes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| {
                    let v: Vec<f64> = b["box"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|x| x.as_f64().unwrap())
                        .collect();
                    Det {
                        cls: b["cls"].as_u64().unwrap() as usize,
                        conf: b["conf"].as_f64().unwrap() as f32,
                        bbox: [v[0], v[1], v[2], v[3]],
                    }
                })
                .collect();
            (
                f["frame"].as_str().unwrap().to_string(),
                (
                    shape[0].as_u64().unwrap() as usize,
                    shape[1].as_u64().unwrap() as usize,
                ),
                boxes,
            )
        })
        .collect();
    let reference_total: usize = frames.iter().map(|f| f.2.len()).sum();
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    println!(
        "{} frames, {reference_total} reference boxes, {threads} threads",
        frames.len()
    );

    // Decode + letterbox, per scale: single-frame latency and parallel throughput.
    let mut prepared_by_scale = Vec::new();
    for denominator in [1u16, 2, 4] {
        let times: Vec<f64> = frames
            .iter()
            .take(10)
            .map(|(path, frame, _)| {
                let t = Instant::now();
                let _ = prepare(path, *frame, denominator);
                t.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        let start = Instant::now();
        let next = AtomicUsize::new(0);
        let mut slots: Vec<Option<Prepared>> = (0..frames.len()).map(|_| None).collect();
        let results = std::sync::Mutex::new(&mut slots);
        std::thread::scope(|s| {
            for _ in 0..threads {
                s.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= frames.len() {
                        break;
                    }
                    let p = prepare(&frames[i].0, frames[i].1, denominator);
                    results.lock().unwrap()[i] = Some(p);
                });
            }
        });
        let wall = start.elapsed().as_secs_f64();
        println!(
            "decode 1/{denominator}: {:.0} ms/frame single, {:.1} frames/s on {threads} threads",
            median(times),
            frames.len() as f64 / wall
        );
        prepared_by_scale.push((
            denominator,
            slots.into_iter().map(Option::unwrap).collect::<Vec<_>>(),
        ));
    }

    for device in ["cpu", "directml", "cuda"] {
        let builder = Session::builder().unwrap();
        let built = match device {
            "cpu" => builder.with_execution_providers([ep::CPU::default().build()]),
            "directml" => builder
                .with_execution_providers([ep::DirectML::default().build().error_on_failure()]),
            _ => builder.with_execution_providers([ep::CUDA::default().build().error_on_failure()]),
        };
        let committed = match built {
            Ok(mut b) => b.commit_from_file(model).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        let mut session = match committed {
            Ok(s) => s,
            Err(e) => {
                println!("{device}: unavailable ({e})");
                continue;
            }
        };
        for (denominator, prepared) in &prepared_by_scale {
            let _ = infer(&mut session, &prepared[0]); // warm-up: kernels, allocations
            let mut times = Vec::new();
            let mut hit = 0;
            let mut loose = 0;
            let mut total = 0;
            for (p, (_, _, reference)) in prepared.iter().zip(&frames) {
                let t = Instant::now();
                let ours = infer(&mut session, p);
                times.push(t.elapsed().as_secs_f64() * 1000.0);
                hit += matches(reference, &ours, 0.9);
                loose += matches(reference, &ours, 0.5);
                total += ours.len();
            }
            println!(
                "{device:>8} decode 1/{denominator}: infer+post {:.0} ms/frame, matched {hit}/{reference_total} at IoU 0.9, {loose} at 0.5 (ours {total})",
                median(times)
            );
        }
    }
}
