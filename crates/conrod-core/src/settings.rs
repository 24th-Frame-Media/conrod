//! Everything tunable, persisted to `settings.json` in the data directory.
//!
//! Port of `Settings` in `conrod/config.py`. The file is shared with the
//! Python app during the migration, so it is read the way Python reads it:
//! unknown keys ignored, a value of the wrong type ignored (Python would
//! store it and fail later; ignoring is the safe half of that), and the
//! sharpness thresholds retired when they were set against an older scale.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub const SHARP_AT: f64 = 0.825;
pub const BLURRED_BELOW: f64 = 0.606;
/// Bumped whenever the focus scale is re-derived; see `load`.
pub const FOCUS_SCALE: i64 = 3;

/// `CONROD_HOME`, else `%USERPROFILE%\.conrod` -- deliberately not
/// LOCALAPPDATA, which sandboxed hosts redirect into a per-app container.
pub fn data_root() -> PathBuf {
    if let Some(home) = std::env::var_os("CONROD_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home);
    }
    let profile = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .unwrap_or_default();
    PathBuf::from(profile).join(".conrod")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub detect_model: String,
    pub detect_imgsz: i64,
    pub detect_conf: f64,
    pub min_box_fraction: f64,
    pub max_vehicles_per_frame: i64,
    pub include_cars: bool,
    pub include_bikes: bool,
    pub include_trucks: bool,
    pub crop_padding: f64,
    pub dominant_subject_fraction: f64,
    pub crop_min_edge: i64,
    pub crop_max_edge: i64,
    pub read_plates: bool,
    pub plate_model: String,
    pub plate_conf: f64,
    pub plate_ocr_edge: i64,
    pub plate_pad_x: f64,
    pub plate_pad_y: f64,
    pub plate_reader: bool,
    pub plate_reader_model: String,
    pub plate_reader_min_conf: f64,
    pub plate_native_search: bool,
    pub plate_native_lower: f64,
    pub plate_tile_edge: i64,
    pub plate_tile_overlap: f64,
    pub plate_min_len: i64,
    pub plate_max_len: i64,
    pub max_plates_per_vehicle: i64,
    pub read_numbers: bool,
    pub ocr_accept_confidence: f64,
    pub number_min_len: i64,
    pub number_max_len: i64,
    pub read_text: bool,
    pub text_min_confidence: f64,
    pub text_min_length: i64,
    pub max_text_items: i64,
    pub use_vlm: bool,
    pub vlm_provider: String,
    pub vlm_model: String,
    pub vlm_host: String,
    pub vlm_extra_hosts: String,
    pub vlm_api_key: String,
    pub anthropic_key_kind: String,
    pub vlm_max_retries: i64,
    pub vlm_timeout: f64,
    pub vlm_input_edge: i64,
    pub normalise_names: bool,
    pub burst_second_look: bool,
    pub group_by_burst: bool,
    pub burst_gap: f64,
    pub sharp_at: f64,
    pub blurred_below: f64,
    pub focus_scale: i64,
    pub auto_reject_below_stars: i64,
    pub import_existing_ratings: bool,
    pub cull_blurred: bool,
    pub mark_burst_picks: bool,
    pub pick_label: String,
    pub use_known_vehicles: bool,
    pub identify_make_model: bool,
    pub identify_colour: bool,
    pub identify_team: bool,
    pub group_vehicles: bool,
    pub respect_culling: bool,
    pub skip_rejected: bool,
    pub write_rating: bool,
    pub write_label: bool,
    pub overwrite_rating: bool,
    pub overwrite_label: bool,
    pub min_rating: i64,
    pub require_label: String,
    pub close_to_tray: bool,
    pub write_sidecar_for_raw: bool,
    pub overwrite_caption: bool,
    pub keyword_prefix: String,
    pub write_plate_keyword: bool,
    pub write_caption: bool,
    pub analysis_workers: i64,
    pub preview_workers: i64,
    pub detect_workers: i64,
    pub workers: i64,
    pub extra: Map<String, Value>,
    /// New in the Rust app: what kind of shoot a new scan defaults to.
    /// Python ignores the key, and drops it if it saves the file.
    pub scan_profile: String,
}

impl Default for Settings {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism().map_or(8, |n| n.get()) as i64;
        Settings {
            detect_model: "yolo11s.pt".into(),
            detect_imgsz: 960,
            detect_conf: 0.25,
            min_box_fraction: 0.08,
            max_vehicles_per_frame: 8,
            include_cars: true,
            include_bikes: true,
            include_trucks: true,
            crop_padding: 0.18,
            dominant_subject_fraction: 0.45,
            crop_min_edge: 320,
            crop_max_edge: 2048,
            read_plates: true,
            plate_model: "yolo-v9-t-640-license-plate-end2end".into(),
            plate_conf: 0.35,
            plate_ocr_edge: 700,
            plate_pad_x: 0.06,
            plate_pad_y: 0.18,
            plate_reader: true,
            plate_reader_model: "global-plates-mobile-vit-v2-model".into(),
            plate_reader_min_conf: 0.75,
            plate_native_search: true,
            plate_native_lower: 0.55,
            plate_tile_edge: 1280,
            plate_tile_overlap: 0.25,
            plate_min_len: 2,
            plate_max_len: 8,
            max_plates_per_vehicle: 2,
            read_numbers: true,
            ocr_accept_confidence: 0.80,
            number_min_len: 1,
            number_max_len: 3,
            read_text: true,
            text_min_confidence: 0.55,
            text_min_length: 2,
            max_text_items: 12,
            use_vlm: true,
            vlm_provider: "ollama".into(),
            vlm_model: "qwen2.5vl:7b".into(),
            vlm_host: "http://127.0.0.1:11434".into(),
            vlm_extra_hosts: String::new(),
            vlm_api_key: String::new(),
            anthropic_key_kind: "auto".into(),
            vlm_max_retries: 4,
            vlm_timeout: 180.0,
            vlm_input_edge: 1568,
            normalise_names: true,
            burst_second_look: true,
            group_by_burst: true,
            burst_gap: 4.0,
            sharp_at: SHARP_AT,
            blurred_below: BLURRED_BELOW,
            focus_scale: FOCUS_SCALE,
            auto_reject_below_stars: 2,
            import_existing_ratings: true,
            cull_blurred: true,
            mark_burst_picks: true,
            pick_label: "Blue".into(),
            use_known_vehicles: true,
            identify_make_model: true,
            identify_colour: true,
            identify_team: true,
            group_vehicles: true,
            respect_culling: true,
            skip_rejected: true,
            write_rating: true,
            write_label: true,
            overwrite_rating: false,
            overwrite_label: false,
            min_rating: 0,
            require_label: String::new(),
            close_to_tray: true,
            write_sidecar_for_raw: true,
            overwrite_caption: false,
            keyword_prefix: String::new(),
            write_plate_keyword: true,
            write_caption: false,
            analysis_workers: 3,
            preview_workers: 4,
            detect_workers: 2,
            workers: (cpus - 2).max(2),
            extra: Map::new(),
            scan_profile: "motorsport".into(),
        }
    }
}

/// Same JSON kind, with ints and floats both counting as numbers (Python
/// happily stores 1 where 1.0 is meant).
fn same_kind(a: &Value, b: &Value) -> bool {
    matches!(
        (a, b),
        (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Object(_), Value::Object(_))
    )
}

impl Settings {
    pub fn path() -> PathBuf {
        data_root().join("settings.json")
    }

    /// Read settings, never failing: a missing or broken file is the defaults.
    pub fn load(path: &Path) -> Settings {
        let stored: Map<String, Value> = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let mut settings = Settings::default().apply(&stored);
        // A threshold is a number about a scale; one set against an older
        // scale now means something nobody asked for, so it is retired.
        if stored.get("focus_scale").and_then(Value::as_i64) != Some(FOCUS_SCALE) {
            settings.sharp_at = SHARP_AT;
            settings.blurred_below = BLURRED_BELOW;
            settings.focus_scale = FOCUS_SCALE;
        }
        settings
    }

    /// Overlay known keys whose values have the right type.
    pub fn apply(self, updates: &Map<String, Value>) -> Settings {
        let Value::Object(mut current) = serde_json::to_value(&self).unwrap() else {
            unreachable!("Settings serialises to an object")
        };
        for (key, value) in updates {
            if let Some(slot) = current.get_mut(key) {
                if same_kind(slot, value) {
                    *slot = value.clone();
                }
            }
        }
        serde_json::from_value(Value::Object(current)).unwrap_or(self)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self).unwrap())
    }

    /// Every configured Ollama host, the main one first, de-duplicated.
    pub fn ollama_hosts(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for host in std::iter::once(self.vlm_host.as_str()).chain(self.vlm_extra_hosts.split(',')) {
            let host = host.trim().trim_end_matches('/');
            if !host.is_empty() && !out.iter().any(|h| h == host) {
                out.push(host.to_string());
            }
        }
        out
    }

    /// COCO vehicle ids to ask the detector for, per the include switches.
    pub fn vehicle_classes(&self) -> Vec<usize> {
        let mut wanted = Vec::new();
        if self.include_cars {
            wanted.push(2);
        }
        if self.include_bikes {
            wanted.push(3);
        }
        if self.include_trucks {
            wanted.extend([5, 7]);
        }
        if wanted.is_empty() {
            vec![2, 3, 5, 7]
        } else {
            wanted
        }
    }
}
