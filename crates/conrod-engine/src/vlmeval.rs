//! Production-path VLM eval: each labelled crop goes through `analyze::analyze`,
//! the function the identify lane calls for every vehicle, and the answer is
//! written as `target/vlm/results/production__app.jsonl` for `scripts/vlm_bench.py`.
//!
//! `cargo test -p conrod-engine --release vlm_production_eval -- --ignored --nocapture`

use crate::analyze::{analyze, Context, Readers, Analysed};
use conrod_core::entrylist;
use conrod_core::models;
use conrod_core::registry::KnownVehicle;
use conrod_core::settings::Settings;
use conrod_io::settings::Settings as IoSettings;
use conrod_io::vlm;
use conrod_vision::detect::Device;
use conrod_vision::imageops::Rgb;
use conrod_vision::{ocr, plates};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const ROOT: &str = r"E:\code\photo\conrod";
const VLM_DIR: &str = r"E:\code\photo\conrod\target\vlm";
const ENTRY_LIST: &str = r"E:\code\photo\conrod\target\vlm\entry_list_bathurst_2026.json";

fn settings_for(group: &str) -> Settings {
    let mut s = Settings::default();
    if group == "carmeet" {
        s.scan_profile = "motorsport-meet".into();
    } else {
        s.scan_profile = "motorsport".into();
        s.entry_list = ENTRY_LIST.into();
    }
    s
}

fn plate_options(s: &Settings) -> plates::PlateOptions {
    plates::PlateOptions {
        plate_conf: s.plate_conf as f32,
        plate_reader: s.plate_reader,
        plate_reader_min_conf: s.plate_reader_min_conf as f32,
        plate_native_search: s.plate_native_search,
        plate_native_lower: s.plate_native_lower,
        plate_tile_edge: s.plate_tile_edge.max(32) as usize,
        plate_tile_overlap: s.plate_tile_overlap,
        plate_pad_x: s.plate_pad_x,
        plate_pad_y: s.plate_pad_y,
        plate_ocr_edge: s.plate_ocr_edge.max(32) as usize,
        plate_min_len: s.plate_min_len.max(1) as usize,
        plate_max_len: s.plate_max_len.max(1) as usize,
        max_plates_per_vehicle: s.max_plates_per_vehicle.max(1) as usize,
        number_min_len: s.number_min_len.max(1) as usize,
        number_max_len: s.number_max_len.max(1) as usize,
    }
}

fn decode(path: &str) -> Result<Rgb, String> {
    let full = PathBuf::from(ROOT).join(path);
    let bytes = std::fs::read(&full).map_err(|e| format!("read {}: {e}", full.display()))?;
    Rgb::decode_jpeg(&bytes, 1)
}

/// Crops whose full request, raw model reply and gate trace are kept in the
/// results: 001 (reply cut off), 017 (OCR number over the model's), 022
/// (make/model lost), 053 (a six-letter personalised plate).
const DEBUG_IDS: [&str; 4] = ["001", "017", "022", "053"];

/// The exact request the app sends for this crop, rebuilt without a call so
/// the eval can show what differs from `scripts/vlm_bench.py`.
fn debug_request(s: &Settings, crop: &Rgb, racing: bool) -> Value {
    let io = IoSettings::from_core(s);
    let candidates = entrylist::load(&PathBuf::from(ENTRY_LIST))
        .map(|l| l.candidates())
        .unwrap_or_default();
    let hints = vlm::VlmHints { racing, candidates };
    let prompt = vlm::vehicle_prompt(&io, false, &hints);
    let schema = vlm::schema(s.read_plates && !racing);
    let image = vlm::encode_for_model(crop, s.vlm_input_edge as u32);
    json!({
        "prompt": prompt,
        "schema": schema,
        "image": {
            "w": crop.width, "h": crop.height,
            "long_edge": s.vlm_input_edge, "jpeg_b64_len": image.len(),
        },
    })
}

fn record(a: &conrod_core::analysis::VehicleAnalysis) -> Value {
    json!({
        "number": a.race_number,
        "plate": a.plate,
        "make": a.make,
        "model": a.model,
        "evidence": a.model_evidence,
        "visible_text": a.text,
        "sponsors": a.sponsors,
        "team": a.team,
        "drivers": a.driver.iter().cloned().collect::<Vec<String>>(),
        "colour": a.colour,
        "body_type": a.body_type,
        "country": a.country,
        "is_competition": a.is_competition,
        "number_source": a.number_source,
        "number_conf": a.number_conf,
        "model_suggested": a.model_suggested,
        "team_corroborated": a.team_corroborated,
    })
}

#[test]
#[ignore]
fn vlm_production_eval() {
    let dir = PathBuf::from(VLM_DIR);
    let mut crops: Vec<Value> = serde_json::from_str(
        &std::fs::read_to_string(dir.join("crops.json")).expect("read crops.json"),
    )
    .expect("parse crops.json");
    // `CONROD_VLM_IDS=053,062` re-runs just those crops while debugging one reading.
    if let Ok(only) = std::env::var("CONROD_VLM_IDS") {
        let keep: Vec<&str> = only
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        if !keep.is_empty() {
            crops.retain(|c| c["id"].as_str().is_some_and(|id| keep.contains(&id)));
        }
    }
    let base = Settings::default();

    let plate_models = match (
        base.read_plates,
        models::find(models::PLATE_DETECTOR),
        models::find(models::PLATE_READER),
    ) {
        (true, Some(det), Some(rd)) => Some((
            plates::PlateDetector::load(&det, Device::Cpu).expect("plate detector"),
            plates::PlateReader::load(&rd, Device::Cpu).expect("plate reader"),
        )),
        (true, _, _) => {
            println!("plate models not installed: plates unread, as the app would");
            None
        }
        _ => None,
    };
    let text_engine = match models::ocr_dir().map(|d| ocr::Ocr::load(&d)) {
        Some(Ok(engine)) => Some(Arc::new(engine)),
        Some(Err(e)) => {
            println!("OCR load failed: text unread, as the app would ({e})");
            None
        }
        None => {
            println!("OCR models not installed: text unread, as the app would");
            None
        }
    };
    let mut readers = Readers {
        plates: plate_models,
        ocr: text_engine,
        vlm: base.use_vlm.then(|| {
            (
                vlm::VlmClient::new(Arc::new(vlm::RealClock::default())),
                IoSettings::from_core(&base),
            )
        }),
    };
    let plate_opts = plate_options(&base);
    let known: HashMap<String, KnownVehicle> = HashMap::new();

    let out_path = dir.join("results").join("production__app.jsonl");
    let mut out = std::fs::File::create(&out_path).expect("create results file");
    for c in &crops {
        let id = c["id"].as_str().unwrap_or_default().to_string();
        let group = c["group"].as_str().unwrap_or("motorsport").to_string();
        let path = c["crop"].as_str().unwrap_or_default().to_string();
        let s = settings_for(&group);
        let t0 = Instant::now();
        let outcome: Result<(Analysed, f64, Value), String> = decode(&path).and_then(|crop| {
            if let Some((_, io)) = readers.vlm.as_mut() {
                *io = IoSettings::from_core(&s);
            }
            let ctx = Context {
                settings: &s,
                plates: &plate_opts,
                known: &known,
            };
            let done = analyze(&crop, None, "car", false, &ctx, &mut readers)?;
            let elapsed = t0.elapsed().as_secs_f64();
            let debug = if DEBUG_IDS.contains(&id.as_str()) {
                debug_request(&s, &crop, group == "motorsport")
            } else {
                Value::Null
            };
            Ok((done, elapsed, debug))
        });
        let latency = match &outcome {
            Ok((_, elapsed, _)) => (elapsed * 1000.0).round() / 1000.0,
            Err(_) => (t0.elapsed().as_secs_f64() * 1000.0).round() / 1000.0,
        };
        let rec = match outcome {
            Ok((done, _, debug)) => {
                let obj = record(&done.analysis);
                let debug = if debug.is_null() {
                    Value::Null
                } else {
                    json!({
                        "request": debug,
                        "attempts": done.vlm_attempts,
                        "raw_reply": done.vlm_reply,
                    })
                };
                json!({
                    "id": id, "group": group, "model": "qwen2.5vl:7b", "variant": "app",
                    "obj": obj, "json_ok": true, "text": obj.to_string(),
                    "latency": latency, "attempts": done.vlm_attempts,
                    "note": done.failures.join("; "), "error": null,
                    "trace": done.trace, "debug": debug,
                })
            }
            Err(e) => json!({
                "id": id, "group": group, "model": "qwen2.5vl:7b", "variant": "app",
                "obj": null, "json_ok": false, "text": "",
                "latency": latency, "attempts": 0, "note": "", "error": e,
            }),
        };
        println!("app {id} {group} {latency:.1}ms json={} {}", rec["json_ok"], rec["error"]);
        writeln!(out, "{}", rec).expect("write record");
        out.flush().expect("flush");
    }
    println!("wrote {}", out_path.display());
}
