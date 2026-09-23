//! Provider availability, without issuing a paid inference request.
use crate::settings::Settings;
use serde_json::{json, Value};
pub fn vision(settings: &Settings, enabled: bool) -> Value {
    if !enabled {
        return json!({"name": "Vision model", "file": "vlm", "ready": true, "detail": "Disabled in settings"});
    }
    if settings.vlm_provider != "ollama" {
        return json!({"name": "Vision model", "file": "vlm", "ready": !settings.vlm_api_key.trim().is_empty(), "detail": "API key presence checked; availability is checked during identification"});
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(4)))
        .build()
        .into();
    let result = (|| -> Result<bool, String> {
        let v: Value = agent
            .get(format!(
                "{}/api/tags",
                settings.vlm_host.trim_end_matches('/')
            ))
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_json()
            .map_err(|e| e.to_string())?;
        Ok(v["models"].as_array().is_some_and(|models| {
            models.iter().any(|m| {
                m["name"].as_str().is_some_and(|n| {
                    n == settings.vlm_model
                        || n.strip_suffix(":latest") == Some(settings.vlm_model.as_str())
                })
            })
        }))
    })();
    match result {
        Ok(ready) => {
            json!({"name": "Vision model", "file": "vlm", "ready": ready, "detail": if ready { format!("{} available", settings.vlm_model) } else { format!("Install {} in Ollama", settings.vlm_model) }})
        }
        Err(e) => {
            json!({"name": "Vision model", "file": "vlm", "ready": false, "detail": format!("Ollama unavailable: {e}")})
        }
    }
}
