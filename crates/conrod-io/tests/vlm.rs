use conrod_io::settings::Settings;
use conrod_io::vlm::{
    anthropic_auth, build_request, parse_anthropic_response, parse_gemini_response,
    parse_ollama_response, parse_openai_response, schema, vehicle_prompt, VlmHints,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vlm.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn settings_for(provider: &str) -> Settings {
    let mut settings = Settings::default();
    match provider {
        "ollama" => {
            settings.vlm_model = "qwen2.5vl:7b".into();
            settings.vlm_host = "http://127.0.0.1:11434".into();
        }
        "openai" => {
            settings.vlm_provider = "openai".into();
            settings.vlm_model = "gpt-4o".into();
            settings.vlm_api_key = "sk-test-123".into();
        }
        "anthropic" => {
            settings.vlm_provider = "anthropic".into();
            settings.vlm_model = "claude-sonnet-5".into();
            settings.vlm_api_key = "sk-ant-test".into();
        }
        "anthropic-claude-code" => {
            settings.vlm_provider = "anthropic".into();
            settings.vlm_model = "claude-sonnet-5".into();
            settings.vlm_api_key = "sk-ant-oat01-test".into();
        }
        "gemini" => {
            settings.vlm_provider = "gemini".into();
            settings.vlm_model = "gemini-2.0-flash".into();
            settings.vlm_api_key = "AIzaTest".into();
        }
        "openrouter" => {
            settings.vlm_provider = "openrouter".into();
            settings.vlm_model = "qwen/qwen3-vl-235b-a22b-instruct".into();
            settings.vlm_api_key = "sk-or-test".into();
        }
        "opencode" => {
            settings.vlm_provider = "opencode".into();
            settings.vlm_model = "deepseek-v4.1-flash".into();
            settings.vlm_api_key = "sk-oc-test".into();
        }
        _ => unreachable!(),
    }
    settings
}

fn headers(request: &Value) -> Vec<(String, String)> {
    request["headers"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.as_str().unwrap().into()))
        .collect()
}

fn query(request: &Value) -> Vec<(String, String)> {
    request["query"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.as_str().unwrap().into()))
        .collect()
}

fn sorted(mut pairs: Vec<(String, String)>) -> Vec<(String, String)> {
    pairs.sort();
    pairs
}

#[test]
fn ollama_request_caps_the_context_and_keeps_the_model_loaded() {
    let request = build_request(
        &settings_for("ollama"),
        "describe it",
        &["b64img".to_string()],
        &json!({"type": "object"}),
        600,
    )
    .unwrap();
    assert_eq!(request.json["options"]["num_ctx"], json!(8192));
    assert_eq!(request.json["keep_alive"], json!("2m"));
    assert_eq!(request.json["options"]["num_predict"], json!(600));
}

#[test]
fn provider_requests_match_recorded_fixture() {
    let payload = fixture();
    let images = vec!["b64img".to_string()];
    for case in payload["requests"].as_array().unwrap() {
        let provider = case["provider"].as_str().unwrap();
        let settings = settings_for(provider);
        let request = build_request(
            &settings,
            "describe it",
            &images,
            &json!({
                "type": "object",
                "properties": {"make": {"type": ["string", "null"]}},
                "required": ["make"],
            }),
            500,
        )
        .unwrap();
        let expected = &case["request"];
        assert_eq!(
            request.url,
            expected["url"].as_str().unwrap(),
            "{provider} URL"
        );
        assert_eq!(
            sorted(request.headers),
            sorted(headers(expected)),
            "{provider} headers"
        );
        assert_eq!(
            sorted(request.query),
            sorted(query(expected)),
            "{provider} query"
        );
        assert_eq!(request.json, expected["json"], "{provider} body");
    }
}

#[test]
fn gemini_key_travels_in_a_header_never_in_the_url() {
    let request = build_request(
        &settings_for("gemini"),
        "describe it",
        &["b64img".to_string()],
        &json!({"type": "object"}),
        500,
    )
    .unwrap();
    // A query string is written to every proxy and server log it passes; the
    // key must never be there.
    assert!(
        !request.url.contains("AIzaTest"),
        "the key is in the URL: {}",
        request.url
    );
    assert!(
        request.query.iter().all(|(k, _)| k != "key"),
        "the key is still a query parameter"
    );
    assert!(
        request
            .headers
            .iter()
            .any(|(k, v)| k == "x-goog-api-key" && v == "AIzaTest"),
        "the key must ride in x-goog-api-key: {:?}",
        request.headers
    );
}

#[test]
fn provider_responses_match_recorded_fixture() {
    let payload = fixture();
    for case in payload["responses"].as_array().unwrap() {
        let provider = case["provider"].as_str().unwrap();
        let body = &case["body"];
        let result = match provider {
            "ollama" => parse_ollama_response(body),
            "openai" => parse_openai_response(body),
            "anthropic" => parse_anthropic_response(body),
            "gemini" => parse_gemini_response(body),
            _ => unreachable!(),
        };
        if case["ok"].as_bool().unwrap() {
            assert_eq!(result.unwrap(), case["result"], "{provider} response");
        } else {
            assert!(result.is_err(), "{provider} should reject {body}");
        }
    }
}

#[test]
fn settings_hosts_and_anthropic_auth_match_fixture() {
    let payload = fixture();
    for case in payload["ollama_hosts"].as_array().unwrap() {
        let settings = Settings {
            vlm_host: case["vlm_host"].as_str().unwrap().into(),
            vlm_extra_hosts: case["vlm_extra_hosts"].as_str().unwrap().into(),
            ..Settings::default()
        };
        let expected: Vec<String> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|host| host.as_str().unwrap().into())
            .collect();
        assert_eq!(settings.ollama_hosts(), expected);
    }
    for case in payload["anthropic_key_kind"].as_array().unwrap() {
        let settings = Settings {
            vlm_api_key: case["vlm_api_key"].as_str().unwrap().into(),
            anthropic_key_kind: case["anthropic_key_kind"].as_str().unwrap().into(),
            ..Settings::default()
        };
        let expected: Vec<(String, String)> = case["expected_auth"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| (key.clone(), value.as_str().unwrap().into()))
            .collect();
        assert_eq!(anthropic_auth(&settings), expected);
    }
}

#[test]
fn schema_is_stable() {
    let req = schema(true)["required"].as_array().unwrap().clone();
    assert_eq!(req.len(), 11);
    assert!(req.iter().any(|v| v.as_str() == Some("is_competition")));
    assert!(req.iter().any(|v| v.as_str() == Some("plate")));
    assert!(req.iter().any(|v| v.as_str() == Some("action")));
    let without = schema(false)["required"].as_array().unwrap().clone();
    assert_eq!(without.len(), 10);
    assert!(!without.iter().any(|v| v.as_str() == Some("plate")));
}

#[test]
fn vehicle_prompt_matches_the_album_targets() {
    let mut settings = Settings::default();
    let hints = VlmHints::default();
    let baseline = vehicle_prompt(&settings, false, &hints);
    // The shoot preset's sentence is deliberately absent: it costs the number.
    assert!(!baseline.contains("Motorsport: capture vehicle identity"));
    assert!(baseline.contains("never infer unseen details."));
    assert!(baseline.contains("Reply with one JSON object only."));
    assert!(baseline.contains("- plate: the registration plate"));
    assert!(!baseline.contains("{CANDIDATES}"));
    assert!(baseline.len() < 2000, "local prompt should stay concise");
    settings.scan_profile = "motorsport-rally".into();
    assert!(vehicle_prompt(&settings, false, &hints).len() < 2000);
    settings.read_numbers = false;
    let road = vehicle_prompt(&settings, false, &hints);
    assert!(road.contains("Set number to null."));
    settings.read_plates = false;
    let bike = vehicle_prompt(&settings, true, &hints);
    assert!(bike.contains("motorcycle and its rider"));
    assert!(!bike.contains("- plate:"));
}

#[test]
fn racing_hints_drop_the_plate_from_prompt_and_schema() {
    let settings = Settings::default();
    let racing = VlmHints {
        racing: true,
        candidates: vec!["Ford Mustang S650".into(), "Toyota GR Supra".into()],
        ..VlmHints::default()
    };
    let prompt = vehicle_prompt(&settings, false, &racing);
    assert!(!prompt.contains("- plate:"));
    assert!(prompt.contains("make/model must be one of: Ford Mustang S650, Toyota GR Supra."));
    assert!(!schema(false)["properties"]
        .as_object()
        .unwrap()
        .contains_key("plate"));
}

#[test]
fn thinking_is_off_by_default_and_back_on_when_asked() {
    let images = vec!["b64img".to_string()];
    let schema = json!({"type": "object"});

    let mut ollama = settings_for("ollama");
    let off = build_request(&ollama, "describe it", &images, &schema, 500).unwrap();
    assert_eq!(off.json["think"], false, "Ollama thinks unless told not to");
    ollama.vlm_reasoning = true;
    let on = build_request(&ollama, "describe it", &images, &schema, 500).unwrap();
    assert_eq!(on.json["think"], true);

    let mut openrouter = settings_for("openrouter");
    let off = build_request(&openrouter, "describe it", &images, &schema, 500).unwrap();
    assert_eq!(off.json["reasoning"]["enabled"], false);
    openrouter.vlm_reasoning = true;
    let on = build_request(&openrouter, "describe it", &images, &schema, 500).unwrap();
    assert_eq!(on.json["reasoning"]["enabled"], true);

    // Claude thinks before it answers by default on the newest models.
    let mut anthropic = settings_for("anthropic");
    let off = build_request(&anthropic, "describe it", &images, &schema, 500).unwrap();
    assert_eq!(off.json["thinking"]["type"], "disabled");
    anthropic.vlm_reasoning = true;
    let on = build_request(&anthropic, "describe it", &images, &schema, 500).unwrap();
    assert!(
        on.json.get("thinking").is_none(),
        "on leaves the model's own default in place"
    );

    let mut gemini = settings_for("gemini");
    gemini.vlm_model = "gemini-2.5-flash".into();
    let off = build_request(&gemini, "describe it", &images, &schema, 500).unwrap();
    assert_eq!(
        off.json["generationConfig"]["thinkingConfig"]["thinkingBudget"],
        0
    );
    gemini.vlm_reasoning = true;
    let on = build_request(&gemini, "describe it", &images, &schema, 500).unwrap();
    assert!(on.json["generationConfig"].get("thinkingConfig").is_none());
}

#[test]
fn the_off_form_is_the_lowest_one_each_model_accepts() {
    let images = vec!["b64img".to_string()];
    let schema = json!({"type": "object"});
    let ask = |provider: &str, model: &str| {
        let mut settings = settings_for(provider);
        settings.vlm_model = model.into();
        build_request(&settings, "describe it", &images, &schema, 500).unwrap()
    };

    // Sonnet 5.5 rejects "disabled"; between_tools is its lowest accepted value.
    assert_eq!(
        ask("anthropic", "claude-sonnet-5-5").json["thinking"]["type"],
        "between_tools"
    );
    // Opus 5.5, Fable and Mythos cannot turn thinking off at all.
    assert!(ask("anthropic", "claude-opus-5-5")
        .json
        .get("thinking")
        .is_none());
    assert!(ask("anthropic", "claude-fable-5-2")
        .json
        .get("thinking")
        .is_none());

    // Gemini 3 and later always think: low is the lowest level, not budget 0.
    let gemini3 = ask("gemini", "gemini-3.8-flash");
    assert_eq!(
        gemini3.json["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "low"
    );
    assert!(gemini3.json["generationConfig"]["thinkingConfig"]
        .get("thinkingBudget")
        .is_none());
    // 2.5 Pro cannot reach zero and 2.0 has no thinking: send nothing.
    assert!(ask("gemini", "gemini-2.5-pro").json["generationConfig"]
        .get("thinkingConfig")
        .is_none());
    assert!(ask("gemini", "gemini-2.0-flash").json["generationConfig"]
        .get("thinkingConfig")
        .is_none());
}

#[test]
fn providers_without_a_documented_switch_send_nothing() {
    let images = vec!["b64img".to_string()];
    let schema = json!({"type": "object"});
    for provider in ["openai", "opencode"] {
        let request = build_request(
            &settings_for(provider),
            "describe it",
            &images,
            &schema,
            500,
        )
        .unwrap();
        assert!(
            request.json.get("think").is_none()
                && request.json.get("reasoning").is_none()
                && request.json.get("thinking").is_none()
                && request.json["generationConfig"]
                    .get("thinkingConfig")
                    .is_none(),
            "{provider} has no verified off switch"
        );
    }
}

#[test]
fn ollama_carries_the_think_flag_on_the_wire() {
    use conrod_io::vlm::{RealClock, VlmClient};
    use std::io::{Read, Write};
    use std::sync::Arc;

    fn capture_once() -> (String, std::thread::JoinHandle<String>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                seen.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&seen).to_string();
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text
                        .lines()
                        .find_map(|l| {
                            let l = l.to_ascii_lowercase();
                            l.strip_prefix("content-length:")
                                .map(|v| v.trim().to_string())
                        })
                        .and_then(|v| v.parse::<usize>().ok())
                        .unwrap_or(0);
                    if n == 0 || seen.len() >= end + 4 + len {
                        break;
                    }
                }
                if n == 0 {
                    break;
                }
            }
            let body = String::from_utf8_lossy(&seen).to_string();
            let body = match body.find("\r\n\r\n") {
                Some(end) => body[end + 4..].to_string(),
                None => body,
            };
            let reply = br#"{"response":"{\"ok\":true}"}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(reply);
            body
        });
        (base, server)
    }

    let (base, server) = capture_once();
    let settings = Settings {
        vlm_host: base,
        ..Settings::default()
    };
    let client = VlmClient::new(Arc::new(RealClock::default()));
    client
        .call(
            &settings,
            "describe it",
            &[],
            &json!({"type": "object"}),
            500,
        )
        .expect("the mock answers");
    let body = server.join().unwrap();
    let sent: Value = serde_json::from_str(body.trim()).expect("the wire body is JSON");
    assert_eq!(sent["think"], false, "the off switch must reach the wire");

    let (base, server) = capture_once();
    let settings = Settings {
        vlm_host: base.clone(),
        vlm_services: vec![conrod_core::settings::VlmService {
            id: "loud".into(),
            provider: "ollama".into(),
            model: "qwen2.5vl:7b".into(),
            host: base,
            reasoning: true,
            ..conrod_core::settings::VlmService::default()
        }],
        ..Settings::default()
    };
    client
        .call(
            &settings,
            "describe it",
            &[],
            &json!({"type": "object"}),
            500,
        )
        .expect("the mock answers");
    let body = server.join().unwrap();
    let sent: Value = serde_json::from_str(body.trim()).expect("the wire body is JSON");
    assert_eq!(
        sent["think"], true,
        "the service's own flag beats the global default"
    );
}

#[test]
fn openai_schema_has_all_properties_in_required() {
    let s = schema(true);
    let req = conrod_io::vlm::openai_request("gpt-4o", "sk-test", "describe", &[], &s, 500);
    let strict_schema = &req.json["response_format"]["json_schema"]["schema"];
    assert_eq!(strict_schema["additionalProperties"], false);

    let props = strict_schema["properties"].as_object().unwrap();
    let required: Vec<String> = strict_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();

    for key in props.keys() {
        assert!(
            required.contains(key),
            "Property '{key}' missing from required in OpenAI schema"
        );
    }
}
