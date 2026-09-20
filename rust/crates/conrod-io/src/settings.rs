//! The handful of `conrod/config.py::Settings` fields the VLM and writer
//! ports need. Not a port of the whole dataclass -- nothing else in this
//! crate reads settings yet, and the other ~30 fields (culling thresholds,
//! detector switches, and so on) belong to code that has not moved here.

#[derive(Debug, Clone)]
pub struct Settings {
    pub vlm_provider: String,
    pub vlm_model: String,
    pub vlm_host: String,
    pub vlm_extra_hosts: String,
    pub vlm_api_key: String,
    pub anthropic_key_kind: String,
    pub vlm_max_retries: u32,
    pub vlm_timeout: f64,
    pub vlm_input_edge: u32,
    pub number_min_len: usize,
    pub number_max_len: usize,
    pub read_plates: bool,
    pub read_numbers: bool,

    pub write_sidecar_for_raw: bool,
    pub overwrite_caption: bool,
    pub write_rating: bool,
    pub write_label: bool,
    pub overwrite_rating: bool,
    pub overwrite_label: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            vlm_provider: "ollama".into(),
            vlm_model: "qwen2.5vl:7b".into(),
            vlm_host: "http://127.0.0.1:11434".into(),
            vlm_extra_hosts: String::new(),
            vlm_api_key: String::new(),
            anthropic_key_kind: "auto".into(),
            vlm_max_retries: 4,
            vlm_timeout: 180.0,
            vlm_input_edge: 1568,
            number_min_len: 1,
            number_max_len: 3,
            read_plates: true,
            read_numbers: true,

            write_sidecar_for_raw: true,
            overwrite_caption: false,
            write_rating: true,
            write_label: true,
            overwrite_rating: false,
            overwrite_label: false,
        }
    }
}

impl Settings {
    /// Adapt the shared application settings to the IO crate's smaller API.
    /// Values are clamped before narrowing because settings files are user
    /// editable and negative integers must not wrap into huge limits.
    pub fn from_core(settings: &conrod_core::settings::Settings) -> Self {
        Self {
            vlm_provider: settings.vlm_provider.clone(),
            vlm_model: settings.vlm_model.clone(),
            vlm_host: settings.vlm_host.clone(),
            vlm_extra_hosts: settings.vlm_extra_hosts.clone(),
            vlm_api_key: settings.vlm_api_key.clone(),
            anthropic_key_kind: settings.anthropic_key_kind.clone(),
            vlm_max_retries: settings.vlm_max_retries.max(1) as u32,
            vlm_timeout: settings.vlm_timeout,
            vlm_input_edge: settings.vlm_input_edge.max(1) as u32,
            number_min_len: settings.number_min_len.max(0) as usize,
            number_max_len: settings.number_max_len.max(0) as usize,
            read_plates: settings.read_plates,
            read_numbers: settings.read_numbers,
            write_sidecar_for_raw: settings.write_sidecar_for_raw,
            overwrite_caption: settings.overwrite_caption,
            write_rating: settings.write_rating,
            write_label: settings.write_label,
            overwrite_rating: settings.overwrite_rating,
            overwrite_label: settings.overwrite_label,
        }
    }

    /// Every configured Ollama endpoint, `vlm_host` first, deduplicated.
    /// Port of `Settings.ollama_hosts` (`conrod/config.py`).
    pub fn ollama_hosts(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut hosts = Vec::new();
        for host in std::iter::once(self.vlm_host.as_str()).chain(self.vlm_extra_hosts.split(',')) {
            let host = host.trim().trim_end_matches('/');
            if !host.is_empty() && seen.insert(host.to_string()) {
                hosts.push(host.to_string());
            }
        }
        hosts
    }
}
