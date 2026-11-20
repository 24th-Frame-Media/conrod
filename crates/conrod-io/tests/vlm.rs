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
