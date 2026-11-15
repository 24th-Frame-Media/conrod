//! Everything tunable, persisted to `settings.json` in the data directory.
//!
//! Reading is tolerant, since the file on disk may predate fields this
//! struct has added or removed: unknown keys are ignored, a value of the
//! wrong type is ignored rather than stored and left to fail later, and the
//! sharpness thresholds are retired when they were set against an older
//! scale.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

pub const SHARP_AT: f64 = 0.825;
pub const BLURRED_BELOW: f64 = 0.606;
/// Bumped whenever the focus scale is re-derived; see `load`.
pub const FOCUS_SCALE: i64 = 3;
/// Bumped by the burst-gap migration: a file without it predates per-album
/// gaps. See [`migrate_burst_gap`].
pub const BURST_GAP_SCALE: i64 = 1;
/// The old default for `burst_gap`. The engine used to ignore it as a sentinel
/// (and grouped at 1.5 s), so stored files carrying it mean the old default,
/// not four seconds. The migration rewrites it once.
pub const LEGACY_IGNORED_BURST_GAP: f64 = 4.0;
/// Default Ollama endpoint, shared by every crate that talks to it.
pub const DEFAULT_VLM_HOST: &str = "http://127.0.0.1:11434";

/// The gap to group with: a positive, finite setting is honoured as-is;
/// anything else is [`crate::bursts::DEFAULT_BURST_GAP_SECONDS`].
pub fn burst_gap_or_default(gap: f64) -> f64 {
    if gap.is_finite() && gap > 0.0 {
        gap
    } else {
        crate::bursts::DEFAULT_BURST_GAP_SECONDS
    }
}

/// One-time migration of a stored settings map (the global file or an album's
/// `settings_json`): before per-album gaps, 4.0 was the ignored default and
/// every album grouped at 1.5 s. Such a map has no `burst_gap_scale`, so its
/// 4.0 becomes 1.5 and the marker is written; afterwards a 4.0 is a real
/// choice and is left alone. Returns whether anything was rewritten.
pub fn migrate_burst_gap(map: &mut Map<String, Value>) -> bool {
    if map.get("burst_gap_scale").and_then(Value::as_i64) == Some(BURST_GAP_SCALE) {
        return false;
    }
    let legacy = map.get("burst_gap").and_then(Value::as_f64)
        == Some(LEGACY_IGNORED_BURST_GAP);
    if legacy {
        map.insert(
            "burst_gap".into(),
            Value::from(crate::bursts::DEFAULT_BURST_GAP_SECONDS),
        );
    }
    map.insert("burst_gap_scale".into(), Value::from(BURST_GAP_SCALE));
    true
}

/// The most workers each pool may use, by what it waits on. Documented so the
/// Settings screen can show the cap rather than letting a user set 40.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct WorkerCaps {
    /// The cull pool (read, detect, sharpness, faces): past ~16 workers the
    /// hyperthreads and E-cores add little (10.1, 12.3, 13.8 and 14.4
    /// frames/s at 8, 12, 16 and 20 workers on a 20-thread laptop).
    pub cull: i64,
    /// Thumbnail backfill: disk- and JPEG-encoder bound, not cores.
    pub preview: i64,
    /// YOLO sessions: each one's weights live in VRAM on a GPU build, so this
    /// stays at two; a third session only fights the first two for the GPU.
    pub detect: i64,
    /// Identify workers: each one holds an Ollama conversation, so this is
    /// Ollama's own `OLLAMA_NUM_PARALLEL` (4 by default). More workers only
    /// queue on the server.
    pub identify: i64,
    /// Cores this machine has.
    pub cores: i64,
}

/// Clamp a stored worker count into `1..=cap`, falling back to `fallback` for a
/// nonsense value.
pub fn clamp_workers(setting: i64, cap: i64, fallback: i64) -> i64 {
    if setting >= 1 {
        setting.min(cap)
    } else {
        fallback.clamp(1, cap)
    }
}

impl Default for WorkerCaps {
    fn default() -> Self {
        let cores = std::thread::available_parallelism().map_or(8, |n| n.get()) as i64;
        WorkerCaps {
            cull: 16,
            preview: 4,
            detect: 2,
            identify: 4,
            cores,
        }
    }
}

/// The caps for this machine.
pub fn worker_caps() -> WorkerCaps {
    WorkerCaps::default()
}

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
    /// Every VLM endpoint to spread calls over. Empty on an older file; `load`
    /// seeds it from the single-provider fields above, which stay for that.
    pub vlm_services: Vec<VlmService>,
    /// "least_busy", "round_robin" or "random".
    pub vlm_strategy: String,
    pub vlm_max_retries: i64,
    pub vlm_timeout: f64,
    pub vlm_input_edge: i64,
    pub normalise_names: bool,
    pub burst_second_look: bool,
    pub group_by_burst: bool,
    /// Seconds between two frames of one camera that still belong to one
    /// burst. Per album; [`DEFAULT_BURST_GAP_SECONDS`] when unset.
    pub burst_gap: f64,
    /// Set by the one-time burst-gap migration (see `migrate_burst_gap`): once
    /// present, a stored 4.0 is a real choice and is never rewritten.
    pub burst_gap_scale: i64,
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
    /// What kind of shoot a new scan defaults to.
    pub scan_profile: String,
    /// Motorsport only: also look at people (faces, eyes, naming) and judge
    /// a frame by its main subject. Other profiles ignore it.
    pub include_people: bool,
    /// Score the subject crop with NIMA technical quality and use it only as
    /// a burst tiebreak after sharpness. Opt-in: off until the model is
    /// installed and a photographer turns it on.
    pub use_quality_score: bool,
    /// How hard blur culls: "strict", "moderate" or "lenient". Unknown text
    /// reads as "moderate".
    pub blur_strictness: String,
    /// Path to a race entry list (CSV or JSON). Empty means none.
    pub entry_list: String,
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
            vlm_host: DEFAULT_VLM_HOST.into(),
            vlm_extra_hosts: String::new(),
            vlm_api_key: String::new(),
            anthropic_key_kind: "auto".into(),
            vlm_services: Vec::new(),
            vlm_strategy: "least_busy".into(),
            vlm_max_retries: 4,
            vlm_timeout: 180.0,
            vlm_input_edge: 1568,
            normalise_names: true,
            burst_second_look: true,
            group_by_burst: true,
            burst_gap: crate::bursts::DEFAULT_BURST_GAP_SECONDS,
            burst_gap_scale: BURST_GAP_SCALE,
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
            include_people: false,
            use_quality_score: false,
            blur_strictness: "moderate".into(),
            entry_list: String::new(),
        }
    }
}

/// One VLM server: a provider at a host (with its key), and the model
/// selected on that server.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VlmService {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub host: String,
    pub api_key: String,
    pub key_kind: String,
    pub enabled: bool,
}

impl Default for VlmService {
    fn default() -> Self {
        VlmService {
            id: String::new(),
            provider: "ollama".into(),
            model: String::new(),
            host: String::new(),
            api_key: String::new(),
            key_kind: "auto".into(),
            enabled: true,
        }
    }
}

/// Same JSON kind, with ints and floats both counting as numbers -- a file
/// on disk may store `1` where `1.0` is meant, or vice versa.
fn same_kind(a: &Value, b: &Value) -> bool {
    matches!(
        (a, b),
        (Value::Bool(_), Value::Bool(_))
            | (Value::Number(_), Value::Number(_))
            | (Value::String(_), Value::String(_))
            | (Value::Object(_), Value::Object(_))
            | (Value::Array(_), Value::Array(_))
    )
}

impl Settings {
    pub fn path() -> PathBuf {
        data_root().join("settings.json")
    }

    /// Read settings, never failing: a missing or broken file is the defaults.
    pub fn load(path: &Path) -> Settings {
        let mut stored: Map<String, Value> = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        // A file written by an older build: its 4.0 burst gap meant the default
        // (1.5 s), not four seconds. Rewrite it once, in place.
        let migrated = !stored.is_empty() && migrate_burst_gap(&mut stored);
        let mut settings = Settings::default().apply(&stored);
        // A threshold is a number about a scale; one set against an older
        // scale now means something nobody asked for, so it is retired.
        if stored.get("focus_scale").and_then(Value::as_i64) != Some(FOCUS_SCALE) {
            settings.sharp_at = SHARP_AT;
            settings.blurred_below = BLURRED_BELOW;
            settings.focus_scale = FOCUS_SCALE;
        }
        settings.seed_vlm_services();
        if migrated {
            let _ = settings.save(path);
        }
        settings
    }

    /// An older file has only the single-provider fields: turn them into one
    /// service, plus one Ollama service per extra host with the same model.
    pub fn seed_vlm_services(&mut self) {
        if !self.vlm_services.is_empty() {
            return;
        }
        let main = VlmService {
            id: "vlm-1".into(),
            provider: self.vlm_provider.clone(),
            model: self.vlm_model.clone(),
            host: self.vlm_host.clone(),
            api_key: self.vlm_api_key.clone(),
            key_kind: self.anthropic_key_kind.clone(),
            enabled: true,
        };
        let extra: Vec<String> = self.ollama_hosts().into_iter().skip(1).collect();
        self.vlm_services.push(main);
        for host in extra {
            let id = format!("vlm-{}", self.vlm_services.len() + 1);
            self.vlm_services.push(VlmService {
                id,
                provider: "ollama".into(),
                model: self.vlm_model.clone(),
                host,
                ..VlmService::default()
            });
        }
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
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Cull-pool workers: `workers` when set, else most of the machine, a
    /// fifth left for the UI. Past ~16 workers the hyperthreads and E-cores add
    /// little (10.1, 12.3, 13.8 and 14.4 frames/s at 8, 12, 16 and 20 workers
    /// on a 20-thread laptop).
    pub fn cull_workers(&self) -> i64 {
        let caps = worker_caps();
        let default = (std::thread::available_parallelism().map_or(8, |n| n.get() as i64) * 4 / 5)
            .clamp(2, caps.cull);
        clamp_workers(self.workers, caps.cull, default)
    }

    /// Identify workers: `analysis_workers` when set, else a fifth of the
    /// cores. Each worker holds one Ollama conversation, so the cap is
    /// Ollama's own parallelism.
    pub fn identify_worker_count(&self) -> i64 {
        let caps = worker_caps();
        let default = (std::thread::available_parallelism().map_or(4, |n| n.get() as i64) / 5)
            .clamp(1, caps.identify);
        clamp_workers(self.analysis_workers, caps.identify, default)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_without_blur_strictness_load_as_moderate() {
        let settings: Settings = serde_json::from_str(r#"{"scan_profile":"motorsport"}"#).unwrap();
        assert_eq!(settings.blur_strictness, "moderate");
    }

    #[test]
    fn the_old_ignored_burst_gap_becomes_the_one_default_exactly_once() {
        let mut old: Map<String, Value> = serde_json::from_str(r#"{"burst_gap": 4.0}"#).unwrap();
        assert!(migrate_burst_gap(&mut old));
        assert_eq!(old["burst_gap"], 1.5, "existing albums keep grouping at 1.5 s");
        assert_eq!(old["burst_gap_scale"], BURST_GAP_SCALE);
        assert!(!migrate_burst_gap(&mut old), "the marker makes it one-time");
        // From now on 4.0 is a real choice and is left alone.
        old.insert("burst_gap".into(), Value::from(4.0));
        assert!(!migrate_burst_gap(&mut old));
        let settings = Settings::default().apply(&old);
        assert_eq!(settings.burst_gap, 4.0);

        // A map with no gap at all just gains the marker.
        let mut fresh: Map<String, Value> = Map::new();
        assert!(migrate_burst_gap(&mut fresh));
        assert_eq!(fresh.get("burst_gap"), None);
    }

    #[test]
    fn loading_a_file_from_before_album_gaps_rewrites_it() {
        let root = std::env::temp_dir().join(format!("conrod-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        std::fs::write(&path, r#"{"burst_gap": 4.0, "focus_scale": 3}"#).unwrap();
        let loaded = Settings::load(&path);
        assert_eq!(loaded.burst_gap, crate::bursts::DEFAULT_BURST_GAP_SECONDS);
        let rewritten: Map<String, Value> =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(rewritten["burst_gap"], 1.5, "the migration is persisted");
        assert_eq!(rewritten["burst_gap_scale"], BURST_GAP_SCALE);
        // A later deliberate 4.0 survives a reload.
        std::fs::write(&path, r#"{"burst_gap": 4.0, "burst_gap_scale": 1, "focus_scale": 3}"#)
            .unwrap();
        assert_eq!(Settings::load(&path).burst_gap, 4.0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_non_positive_burst_gap_means_the_default() {
        assert_eq!(
            burst_gap_or_default(0.0),
            crate::bursts::DEFAULT_BURST_GAP_SECONDS
        );
        assert_eq!(
            burst_gap_or_default(-3.0),
            crate::bursts::DEFAULT_BURST_GAP_SECONDS
        );
        assert_eq!(
            burst_gap_or_default(f64::NAN),
            crate::bursts::DEFAULT_BURST_GAP_SECONDS
        );
        assert_eq!(burst_gap_or_default(4.0), 4.0);
        assert_eq!(
            Settings::default().burst_gap,
            crate::bursts::DEFAULT_BURST_GAP_SECONDS,
            "the stored default is the one documented default"
        );
    }

    #[test]
    fn worker_counts_are_clamped_to_their_documented_caps() {
        let caps = worker_caps();
        assert_eq!(caps.cull, 16);
        assert_eq!(caps.identify, 4, "Ollama's parallel default");
        assert_eq!(caps.detect, 2, "VRAM");
        assert_eq!(clamp_workers(40, caps.cull, 4), 16);
        assert_eq!(clamp_workers(0, caps.identify, 3), 3);
        assert_eq!(clamp_workers(-2, caps.preview, 40), 4);
        assert_eq!(clamp_workers(2, caps.cull, 4), 2);
    }
}
