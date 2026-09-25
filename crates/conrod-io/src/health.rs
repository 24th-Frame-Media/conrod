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

/// Query Ollama's `/api/tags` on `host` and return discovered models with metadata and vision capability flags.
pub fn ollama_models(host: &str) -> Value {
    let host = host.trim();
    let host_str = if host.is_empty() {
        conrod_core::settings::DEFAULT_VLM_HOST
    } else {
        host
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(3)))
        .build()
        .into();
    let url = format!("{}/api/tags", host_str.trim_end_matches('/'));
    let resp = match agent.get(&url).call() {
        Ok(mut r) => {
            if r.status().as_u16() != 200 {
                return json!({
                    "ok": false,
                    "host": host_str,
                    "online": false,
                    "error": format!("Ollama returned status {}", r.status().as_u16()),
                    "models": []
                });
            }
            match r.body_mut().read_json::<Value>() {
                Ok(v) => v,
                Err(e) => {
                    return json!({
                        "ok": false,
                        "host": host_str,
                        "online": false,
                        "error": format!("Could not parse Ollama response: {e}"),
                        "models": []
                    });
                }
            }
        }
        Err(e) => {
            return json!({
                "ok": false,
                "host": host_str,
                "online": false,
                "error": format!("Could not connect to Ollama: {e}"),
                "models": []
            });
        }
    };

    let models: Vec<Value> = resp["models"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|m| {
            let name = m["name"].as_str()?;
            let size = m["size"].as_u64().unwrap_or(0);
            let modified_at = m["modified_at"].as_str().unwrap_or("");
            let param_size = m["details"]["parameter_size"].as_str().unwrap_or("");

            let caps_has_vision = m["capabilities"]
                .as_array()
                .is_some_and(|c| c.iter().any(|cap| cap.as_str() == Some("vision")));
            let families_has_vision = m["details"]["families"].as_array().is_some_and(|fams| {
                fams.iter().any(|f| {
                    let fam = f.as_str().unwrap_or("");
                    fam == "clip" || fam.contains("vl") || fam.contains("vision")
                })
            });
            let name_lower = name.to_lowercase();
            let name_has_vision = name_lower.contains("vl")
                || name_lower.contains("vision")
                || name_lower.contains("minicpm-v")
                || name_lower.contains("llava");

            let vision = caps_has_vision || families_has_vision || name_has_vision;

            Some(json!({
                "name": name,
                "size": size,
                "modified_at": modified_at,
                "parameter_size": param_size,
                "vision": vision,
            }))
        })
        .collect();

    json!({
        "ok": true,
        "host": host_str,
        "online": true,
        "models": models
    })
}
