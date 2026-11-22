//! Who judges a frame's moments. A [`MomentJudge`] takes one photograph (and,
//! for a burst, several) and the shoot's vocabulary, and answers with what is
//! clearly visible and how sure it is.
//!
//! Two backends: `vlm`, the vision model already in the identify lane (it names
//! tags but has no calibrated probability), and `decider`, a decision model
//! that answers calibrated probabilities through a [`DecisionTransport`]
//! (OpenCode's System One model JEV, OpenRouter's Decisions API, or an
//! in-process model). A failure is an `Err`; callers treat it as "no moment"
//! and never block a scan or demote a frame because of it.

use crate::settings::Settings as IoSettings;
use crate::vlm::{self, VlmClient};
use conrod_core::decision::{self, DecisionTransport, InProcess, Request, Response};
use conrod_core::moments::{Moment, MomentReading, MomentVerdict};
use conrod_core::settings::Settings;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;

/// What the vision model's "I am confident" stands for. It is a stand-in, not
/// a probability: the readings are marked uncalibrated.
pub const VLM_CONFIDENT_P: f64 = 0.9;
/// What its tags are worth when it says it is unsure: a "maybe", never a lift.
pub const VLM_UNSURE_P: f64 = 0.6;
/// The vision model must claim at least this confidence for its tags to be
/// treated as sure.
pub const VLM_CONFIDENT_AT: f64 = 0.7;

pub trait MomentJudge: Send + Sync {
    fn name(&self) -> &'static str;

    /// One photograph, as a `data:image/jpeg;base64,...` URL.
    fn judge(&self, context: &str, vocab: &[Moment], image: &str) -> Result<MomentVerdict, String>;

    /// One photograph plus a detail crop of the subject. The whole frame comes first, so a
    /// judge that can only take one image (the vision model) sees the ground and the scene;
    /// a judge that can take several (the decision model) also sees the car up close.
    fn judge_pair(
        &self,
        context: &str,
        vocab: &[Moment],
        frame: &str,
        crop: Option<&str>,
    ) -> Result<MomentVerdict, String> {
        let _ = crop;
        self.judge(context, vocab, frame)
    }

    /// Which frame of a burst shows the peak: P per frame, in order. A backend
    /// that cannot say answers `Ok(None)`.
    fn peak(&self, _context: &str, _images: &[String]) -> Result<Option<Vec<f64>>, String> {
        Ok(None)
    }
}

/// The base64 body of a data URL (what the vision model providers want).
pub fn strip_data_url(url: &str) -> &str {
    url.split_once(";base64,").map_or(url, |(_, body)| body)
}

/// The prompt the vision model gets: the preset's vocabulary as data, and
/// the same discipline as the vehicle prompt (only what is clearly visible).
pub fn vlm_prompt(context: &str, vocab: &[Moment]) -> String {
    let list: Vec<String> = vocab
        .iter()
        .map(|m| format!("- {}: {}", m.key, m.ask))
        .collect();
    format!(
        "You are judging one photograph for a photographer. {context}\n\
         Report only moments that are clearly visible in the photograph. Never guess.\n\
         moments: the tags from this list that clearly apply; [] when none does.\n{}\n\
         confidence: how sure you are, from 0 to 1, that the tags you listed are really visible.\n\
         Reply with one JSON object only.",
        list.join("\n")
    )
}

/// The reply's shape: tags restricted to the vocabulary.
pub fn vlm_schema(vocab: &[Moment]) -> Value {
    json!({
        "type": "object",
        "properties": {
            "moments": {"type": "array", "items": {"type": "string", "enum": vocab.iter().map(|m| m.key).collect::<Vec<_>>()}},
            "confidence": {"type": "number"},
        },
        "required": ["moments", "confidence"],
    })
}

/// A vision-model reply as readings. Tags outside the vocabulary are dropped;
/// a model that says it is unsure produces maybes, never sure moments.
pub fn read_vlm_reply(reply: &Value, vocab: &[Moment]) -> MomentVerdict {
    let confidence = reply
        .get("confidence")
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let p = if confidence >= VLM_CONFIDENT_AT {
        VLM_CONFIDENT_P
    } else {
        VLM_UNSURE_P
    };
    let mut readings: Vec<MomentReading> = Vec::new();
    for tag in reply
        .get("moments")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(tag) = tag.as_str().map(str::trim) else {
            continue;
        };
        if vocab.iter().any(|m| m.key == tag) && !readings.iter().any(|r| r.tag == tag) {
            readings.push(MomentReading {
                tag: tag.to_string(),
                p,
                calibrated: false,
            });
        }
    }
    MomentVerdict {
        backend: "vlm".into(),
        readings,
        strength: None,
        peak: None,
    }
}

/// The vision model as a judge.
pub struct VlmJudge {
    pub client: VlmClient,
    pub settings: IoSettings,
}

impl MomentJudge for VlmJudge {
    fn name(&self) -> &'static str {
        "vlm"
    }

    fn judge(&self, context: &str, vocab: &[Moment], image: &str) -> Result<MomentVerdict, String> {
        let reply = self
            .client
            .call(
                &self.settings,
                &vlm_prompt(context, vocab),
                &[strip_data_url(image).to_string()],
                &vlm_schema(vocab),
                200,
            )
            .map_err(|e| e.to_string())?;
        Ok(read_vlm_reply(&reply, vocab))
    }
}

/// The decider as a judge, over any transport.
pub struct DeciderJudge {
    pub transport: Arc<dyn DecisionTransport>,
}

impl MomentJudge for DeciderJudge {
    fn name(&self) -> &'static str {
        "decider"
    }

    fn judge(&self, context: &str, vocab: &[Moment], image: &str) -> Result<MomentVerdict, String> {
        let request = decision::moment_request(context, vocab, vec![image.to_string()]);
        let response = self.transport.decide(&request)?;
        Ok(decision::read_moments(&response, vocab))
    }

    fn judge_pair(
        &self,
        context: &str,
        vocab: &[Moment],
        frame: &str,
        crop: Option<&str>,
    ) -> Result<MomentVerdict, String> {
        // The llama.cpp multimodal path takes any number of images in one request, so the
        // model can see the whole frame (and the ground under the wheels) as well as the car.
        let mut images = vec![frame.to_string()];
        if let Some(crop) = crop {
            images.push(crop.to_string());
        }
        let request = decision::moment_request(context, vocab, images);
        let response = self.transport.decide(&request)?;
        Ok(decision::read_moments(&response, vocab))
    }

    fn peak(&self, context: &str, images: &[String]) -> Result<Option<Vec<f64>>, String> {
        if images.len() < 2 {
            return Ok(None);
        }
        let request = decision::peak_request(context, images.to_vec());
        let response = self.transport.decide(&request)?;
        Ok(decision::read_peak(&response, images.len()))
    }
}

pub const OPENROUTER_URL: &str = "https://openrouter.ai/api/alpha/decisions";
/// OpenCode's System One endpoint: JEV, a typed decision model that also takes
/// PNG/JPEG images as `image_url` parts inside `state`. Billed to the OpenCode
/// balance, so it is on the pay-as-you-go host, not the Go plan host.
pub const OPENCODE_DECISION_URL: &str = "https://opencode.ai/inference/systemone/v1/systemone";

/// The [`Request`] as both decision APIs take it: the images moved into `state`
/// as `image_url` parts (a top-level `images` field is rejected 400).
pub fn decisions_body(model: &str, request: &Request) -> Value {
    let state = if request.images.is_empty() {
        request.state.clone()
    } else {
        let mut parts = vec![request.state.clone()];
        parts.extend(
            request
                .images
                .iter()
                .map(|url| json!({"type": "image_url", "image_url": {"url": url}})),
        );
        Value::Array(parts)
    };
    json!({"model": model, "state": state, "questions": request.questions})
}

/// A decision API's reply as a [`Response`]; a non-2xx says why, never the key.
pub fn parse_decisions(provider: &str, status: u16, body: &[u8]) -> Result<Response, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| format!("{provider} answered {status} with no JSON"))?;
    if !(200..300).contains(&status) {
        let said = value
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or("no message");
        return Err(format!("{provider} answered {status}: {said}"));
    }
    // Answers sit at the top level, or under `data` on some gateways.
    let answers = if value.get("answers").is_some() {
        value
    } else {
        value.get("data").cloned().unwrap_or(value)
    };
    serde_json::from_value(answers).map_err(|e| format!("{provider} reply was not a decision: {e}"))
}

pub fn parse_openrouter(status: u16, body: &[u8]) -> Result<Response, String> {
    parse_decisions("OpenRouter", status, body)
}

pub fn parse_opencode(status: u16, body: &[u8]) -> Result<Response, String> {
    parse_decisions("OpenCode", status, body)
}

pub struct OpenRouterTransport {
    pub key: String,
    pub model: String,
    pub timeout: Duration,
}

impl DecisionTransport for OpenRouterTransport {
    fn name(&self) -> &'static str {
        "openrouter"
    }

    fn decide(&self, request: &Request) -> Result<Response, String> {
        if self.key.trim().is_empty() {
            return Err("No OpenRouter key is set for the decision model".into());
        }
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .timeout_global(Some(self.timeout))
                .build(),
        );
        let mut reply = agent
            .post(OPENROUTER_URL)
            .header("Authorization", &format!("Bearer {}", self.key.trim()))
            .send_json(decisions_body(&self.model, request))
            .map_err(|e| {
                format!(
                    "OpenRouter could not be reached: {}",
                    e.to_string().replace(self.key.trim(), "***")
                )
            })?;
        let status = reply.status().as_u16();
        let body = reply.body_mut().read_to_vec().map_err(|e| e.to_string())?;
        parse_openrouter(status, &body)
    }
}

/// OpenCode's System One endpoint, whose decision model is JEV. It takes the
/// same body as OpenRouter's Decisions API. JEV is billed to the Console
/// (pay-as-you-go) balance, so no Go subscription header or session id is sent.
pub struct OpenCodeTransport {
    pub key: String,
    pub model: String,
    pub timeout: Duration,
}

impl OpenCodeTransport {
    /// One POST. `decide_at` takes the URL so a test can point it at a local
    /// server and see exactly which headers the credits API receives.
    fn decide_at(&self, url: &str, request: &Request) -> Result<Response, String> {
        if self.key.trim().is_empty() {
            return Err("No OpenCode key is set for the decision model".into());
        }
        let agent = ureq::Agent::new_with_config(
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .timeout_global(Some(self.timeout))
                .build(),
        );
        let mut reply = agent
            .post(url)
            .header("Authorization", &format!("Bearer {}", self.key.trim()))
            .header("User-Agent", vlm::OPENCODE_USER_AGENT)
            .send_json(decisions_body(&self.model, request))
            .map_err(|e| {
                format!(
                    "OpenCode could not be reached: {}",
                    e.to_string().replace(self.key.trim(), "***")
                )
            })?;
        let status = reply.status().as_u16();
        let body = reply.body_mut().read_to_vec().map_err(|e| e.to_string())?;
        parse_opencode(status, &body)
    }
}

impl DecisionTransport for OpenCodeTransport {
    fn name(&self) -> &'static str {
        "opencode"
    }

    fn decide(&self, request: &Request) -> Result<Response, String> {
        self.decide_at(OPENCODE_DECISION_URL, request)
    }
}

/// The OpenCode key the decider uses: its own, else the key of an enabled
/// OpenCode vision service, so one OpenCode account serves both lanes.
fn opencode_key(settings: &Settings) -> String {
    let own = settings.moments_opencode_key.trim();
    if !own.is_empty() {
        return own.to_string();
    }
    settings
        .vlm_services
        .iter()
        .filter(|s| s.enabled && s.provider.starts_with("opencode"))
        .map(|s| s.api_key.trim())
        .find(|key| !key.is_empty())
        .unwrap_or("")
        .to_string()
}

/// Which transport carries the decision model's questions, from the album's
/// `moments_transport`: `auto` (the default) prefers the in-process model when
/// one is seated, else OpenCode when a key is set, else OpenRouter; `auto`,
/// `in_process`, `opencode` and `openrouter` are otherwise strict. `Err` says
/// what is missing.
fn transport_for(
    settings: &Settings,
    in_process: InProcess,
) -> Result<Arc<dyn DecisionTransport>, String> {
    let openrouter_key = settings.moments_openrouter_key.trim().to_string();
    let opencode_key = opencode_key(settings);
    let openrouter = || -> Arc<dyn DecisionTransport> {
        Arc::new(OpenRouterTransport {
            key: settings.moments_openrouter_key.clone(),
            model: settings.moments_openrouter_model.clone(),
            timeout: Duration::from_secs(60),
        })
    };
    let opencode = || -> Arc<dyn DecisionTransport> {
        Arc::new(OpenCodeTransport {
            key: opencode_key.clone(),
            model: if settings.moments_opencode_model.trim().is_empty() {
                "jev-1.13".into()
            } else {
                settings.moments_opencode_model.clone()
            },
            timeout: Duration::from_secs(60),
        })
    };
    match settings.moments_transport.as_str() {
        "in_process" => Ok(Arc::new(in_process)),
        "openrouter" if !openrouter_key.is_empty() => Ok(openrouter()),
        "openrouter" => {
            Err("Add an OpenRouter key to judge moments with the decision model".into())
        }
        "opencode" if !opencode_key.is_empty() => Ok(opencode()),
        "opencode" => {
            Err("Add an OpenCode key to judge moments with JEV".into())
        }
        _ if in_process.is_available() => Ok(Arc::new(in_process)),
        _ if !opencode_key.is_empty() => Ok(opencode()),
        _ if !openrouter_key.is_empty() => Ok(openrouter()),
        _ => Err(
            "Turn on the decision model in Settings (or add an OpenCode or OpenRouter key) to judge moments with it"
                .into(),
        ),
    }
}

/// The judge this album's settings ask for. `in_process` is the in-process
/// model's seat (the engine fills it from the decider service).
pub fn judge_for(
    settings: &Settings,
    vlm: Option<(VlmClient, IoSettings)>,
    in_process: InProcess,
) -> Result<Box<dyn MomentJudge>, String> {
    match settings.moments_backend.as_str() {
        "decider" => {
            let transport = transport_for(settings, in_process)?;
            Ok(Box::new(DeciderJudge { transport }))
        }
        _ => match vlm {
            Some((client, settings)) => Ok(Box::new(VlmJudge { client, settings })),
            None => Err("The vision model is off, so moments cannot be judged".into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use conrod_core::decision::{Answer, Ordered};
    use conrod_core::moments::vocabulary;
    use conrod_core::profile::ShootPreset;
    use std::sync::Mutex;

    /// Records requests, answers from a closure: no network.
    struct Mock {
        seen: Mutex<Vec<Request>>,
        reply: Box<conrod_core::decision::DecideFn>,
    }

    impl DecisionTransport for Mock {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn decide(&self, request: &Request) -> Result<Response, String> {
            self.seen.lock().unwrap().push(request.clone());
            (self.reply)(request)
        }
    }

    fn decider(
        reply: impl Fn(&Request) -> Result<Response, String> + Send + Sync + 'static,
    ) -> (DeciderJudge, Arc<Mock>) {
        let mock = Arc::new(Mock {
            seen: Mutex::new(Vec::new()),
            reply: Box::new(reply),
        });
        (
            DeciderJudge {
                transport: mock.clone(),
            },
            mock,
        )
    }

    fn noul(p: f64) -> Answer {
        Answer {
            noul: Some(p),
            ..Answer::default()
        }
    }

    #[test]
    fn the_vlm_prompt_and_schema_come_from_the_presets_vocabulary() {
        for preset in ShootPreset::ALL {
            let vocab = vocabulary(preset);
            if vocab.is_empty() {
                continue;
            }
            let prompt = vlm_prompt(preset.prompt_context(), vocab);
            let schema = vlm_schema(vocab);
            let allowed = schema["properties"]["moments"]["items"]["enum"]
                .as_array()
                .unwrap();
            assert_eq!(allowed.len(), vocab.len(), "{preset:?}");
            for moment in vocab {
                assert!(
                    prompt.contains(&format!("- {}: {}", moment.key, moment.ask)),
                    "{preset:?}"
                );
            }
            assert!(prompt.contains("clearly visible") && prompt.contains("Never guess"));
        }
        // A wedding prompt never asks about tyre smoke, a track one never about kisses.
        let wedding = vlm_prompt("w", vocabulary(ShootPreset::Wedding));
        assert!(!wedding.contains("tyre_smoke") && wedding.contains("first_kiss"));
        let track = vlm_prompt("t", vocabulary(ShootPreset::MotorsportTrack));
        assert!(!track.contains("first_kiss") && track.contains("tyre_smoke"));
    }

    #[test]
    fn a_confident_vlm_names_tags_at_p_point_nine_uncalibrated_and_an_unsure_one_only_maybes() {
        let vocab = vocabulary(ShootPreset::Wedding);
        let sure = read_vlm_reply(
            &json!({"moments": ["first_kiss", "bogus", "first_kiss", "vows"], "confidence": 0.9}),
            vocab,
        );
        assert_eq!(sure.backend, "vlm");
        assert_eq!(sure.readings.len(), 2, "unknown and repeated tags dropped");
        assert!(sure
            .readings
            .iter()
            .all(|r| r.p == VLM_CONFIDENT_P && !r.calibrated));
        let unsure = read_vlm_reply(
            &json!({"moments": ["first_kiss"], "confidence": 0.4}),
            vocab,
        );
        assert_eq!(unsure.readings[0].p, VLM_UNSURE_P);
        assert!(
            unsure.confident(0.85).is_empty(),
            "an unsure model never ranks"
        );
        assert_eq!(unsure.maybe(0.85).len(), 1);
        assert!(
            read_vlm_reply(&json!({"moments": [], "confidence": 0.99}), vocab)
                .readings
                .is_empty()
        );
        assert!(read_vlm_reply(&json!("nonsense"), vocab)
            .readings
            .is_empty());
    }

    #[test]
    fn the_decider_judge_asks_one_request_with_the_vocabulary_and_reads_probabilities() {
        let vocab = vocabulary(ShootPreset::Wedding);
        let (judge, mock) = decider(|req| {
            let mut answers = Ordered::new();
            for id in req.questions.keys() {
                if id == "m_first_kiss" {
                    answers.insert(id.clone(), noul(0.96));
                } else if id == "strength" {
                    answers.insert(
                        id.clone(),
                        Answer {
                            score: Some(3.0),
                            ..Answer::default()
                        },
                    );
                } else {
                    answers.insert(id.clone(), noul(0.05));
                }
            }
            Ok(Response {
                answers,
                usage: None,
            })
        });
        let verdict = judge
            .judge("Weddings.", vocab, "data:image/jpeg;base64,AA")
            .unwrap();
        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "one request per frame");
        assert_eq!(seen[0].questions.len(), vocab.len() + 1);
        assert_eq!(seen[0].images, ["data:image/jpeg;base64,AA"]);
        assert_eq!(verdict.backend, "decider");
        assert_eq!(verdict.strength, Some(1.0));
        let sure = verdict.confident(0.85);
        assert_eq!(sure.len(), 1);
        assert_eq!(sure[0].tag, "first_kiss");
        assert!(sure[0].calibrated);
    }

    #[test]
    fn the_decider_sees_the_whole_frame_first_and_the_car_crop_second() {
        let vocab = vocabulary(ShootPreset::MotorsportTrack);
        let (judge, mock) = decider(|_| Ok(Response::default()));
        judge
            .judge_pair(
                "Track day.",
                vocab,
                "data:image/jpeg;base64,FRAME",
                Some("data:image/jpeg;base64,CROP"),
            )
            .unwrap();
        // With no crop it stays one image: a judge that has only the frame still works.
        judge
            .judge_pair("Track day.", vocab, "data:image/jpeg;base64,FRAME", None)
            .unwrap();
        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(
            seen[0].images,
            [
                "data:image/jpeg;base64,FRAME",
                "data:image/jpeg;base64,CROP"
            ],
            "the whole frame must come first so the ground stays visible"
        );
        assert!(seen[0].state.as_str().unwrap().contains("whole photograph"));
        assert_eq!(seen[1].images, ["data:image/jpeg;base64,FRAME"]);
    }

    #[test]
    fn a_backend_failure_is_an_error_and_never_a_moment() {
        let (judge, _) = decider(|_| Err("503 busy".into()));
        assert!(judge
            .judge(
                "x",
                vocabulary(ShootPreset::Wedding),
                "data:image/jpeg;base64,AA"
            )
            .is_err());
        assert!(judge.peak("x", &["a".into(), "b".into()]).is_err());
        let (judge, _) = decider(|_| Ok(Response::default()));
        let verdict = judge
            .judge(
                "x",
                vocabulary(ShootPreset::Wedding),
                "data:image/jpeg;base64,AA",
            )
            .unwrap();
        assert!(verdict.readings.is_empty() && verdict.strength.is_none());
    }

    #[test]
    fn the_peak_pick_is_one_choice_request_and_a_flat_reply_is_no_opinion() {
        let (judge, mock) = decider(|req| {
            assert!(req.questions.contains_key("peak"));
            let mut answers = Ordered::new();
            answers.insert(
                "peak".to_string(),
                Answer {
                    probabilities: Some(json!({"f01": 0.1, "f02": 0.7, "f03": 0.2})),
                    ..Answer::default()
                },
            );
            Ok(Response {
                answers,
                usage: None,
            })
        });
        let frames: Vec<String> = (0..3)
            .map(|i| format!("data:image/jpeg;base64,{i}"))
            .collect();
        let ps = judge.peak("Rodeo.", &frames).unwrap().unwrap();
        assert_eq!(decision::peak_frame(&ps, 0.5), Some(1));
        assert_eq!(mock.seen.lock().unwrap()[0].images.len(), 3);
        // One frame has no burst to choose in: no request at all.
        assert_eq!(judge.peak("Rodeo.", &frames[..1]).unwrap(), None);
        assert_eq!(mock.seen.lock().unwrap().len(), 1);
    }

    #[test]
    fn openrouter_gets_images_inside_state_never_as_a_top_level_field() {
        let vocab = vocabulary(ShootPreset::Wedding);
        let req =
            decision::moment_request("Weddings.", vocab, vec!["data:image/jpeg;base64,AA".into()]);
        let body = decisions_body("perplexity/pplx-decider-flash-v1.3:free", &req);
        assert!(body.get("images").is_none());
        assert_eq!(body["model"], "perplexity/pplx-decider-flash-v1.3:free");
        let state = body["state"].as_array().unwrap();
        assert!(state[0].as_str().unwrap().contains("clearly visible"));
        assert_eq!(state[1]["type"], "image_url");
        assert_eq!(state[1]["image_url"]["url"], "data:image/jpeg;base64,AA");
        assert_eq!(body["questions"]["m_first_kiss"]["type"], "noul");
        // Text only: the state stays a string.
        let bare = decisions_body("m", &decision::moment_request("x", vocab, vec![]));
        assert!(bare["state"].is_string());
    }

    #[test]
    fn openrouter_replies_parse_and_errors_never_echo_a_key() {
        let ok = br#"{"answers":{"m_first_kiss":{"noul":0.9}},"usage":{"input_tokens":10}}"#;
        assert_eq!(
            parse_openrouter(200, ok).unwrap().answers["m_first_kiss"].noul,
            Some(0.9)
        );
        let wrapped = br#"{"data":{"answers":{"m_vows":{"noul":0.4}}}}"#;
        assert_eq!(
            parse_openrouter(200, wrapped).unwrap().answers["m_vows"].noul,
            Some(0.4)
        );
        let denied = parse_openrouter(401, br#"{"error":{"message":"No auth credentials found"}}"#)
            .unwrap_err();
        assert!(denied.contains("401") && denied.contains("No auth"));
        assert!(parse_openrouter(502, b"<html>")
            .unwrap_err()
            .contains("502"));
        let transport = OpenRouterTransport {
            key: " ".into(),
            model: "m".into(),
            timeout: Duration::from_secs(1),
        };
        let req = decision::moment_request("x", vocabulary(ShootPreset::Wedding), vec![]);
        assert!(transport
            .decide(&req)
            .unwrap_err()
            .contains("No OpenRouter key"));
    }

    #[test]
    fn opencode_jev_takes_images_inside_state_and_both_model_ids() {
        let vocab = vocabulary(ShootPreset::MotorsportTrack);
        let req = decision::moment_request(
            "Track day.",
            vocab,
            vec![
                "data:image/jpeg;base64,FRAME".into(),
                "data:image/jpeg;base64,CROP".into(),
            ],
        );
        for model in ["jev-1.13", "jev-1.13-free"] {
            let body = decisions_body(model, &req);
            assert!(body.get("images").is_none(), "{model}");
            assert_eq!(body["model"], model);
            let state = body["state"].as_array().unwrap();
            assert!(state[0].as_str().unwrap().contains("whole photograph"));
            assert_eq!(state[1]["type"], "image_url");
            assert_eq!(state[1]["image_url"]["url"], "data:image/jpeg;base64,FRAME");
            assert_eq!(state[2]["image_url"]["url"], "data:image/jpeg;base64,CROP");
        }
    }

    #[test]
    fn opencode_replies_parse_and_errors_never_echo_a_key() {
        // The shape JEV actually answers with, including a score's legend.
        let ok = br#"{"model":"jev-1.13","answers":{"m_waving":{"type":"noul","noul":0.98},"strength":{"type":"score","score":2,"confidence":0.7,"legend":{"0":"none","1":"weak","2":"moderate","3":"strong"},"probabilities":{"0":0.01,"1":0.13,"2":0.71,"3":0.15}}},"usage":{"input_tokens":375,"output_tokens":86}}"#;
        let reply = parse_opencode(200, ok).unwrap();
        assert_eq!(reply.answers["m_waving"].noul, Some(0.98));
        assert_eq!(reply.answers["strength"].score, Some(2.0));
        assert_eq!(reply.usage.unwrap().input_tokens, 375);
        let denied = parse_opencode(
            401,
            br#"{"error":{"type":"invalid_request_error","message":"Invalid API key"}}"#,
        )
        .unwrap_err();
        assert!(denied.contains("401") && denied.contains("Invalid API key"));
        assert!(parse_opencode(502, b"<html>")
            .unwrap_err()
            .contains("OpenCode"));
        let transport = OpenCodeTransport {
            key: " ".into(),
            model: "jev-1.13-free".into(),
            timeout: Duration::from_secs(1),
        };
        let req = decision::moment_request("x", vocabulary(ShootPreset::Wedding), vec![]);
        assert!(transport
            .decide(&req)
            .unwrap_err()
            .contains("No OpenCode key"));
    }

    #[test]
    fn opencode_jev_posts_with_credits_auth_and_no_go_session_header() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/systemone", listener.local_addr().unwrap());
        let (tx, rx) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut buf = [0u8; 4096];
            let mut head_end = None;
            let mut content_length = 0usize;
            loop {
                let n = stream.read(&mut buf).unwrap_or(0);
                seen.extend_from_slice(&buf[..n]);
                if head_end.is_none() {
                    if let Some(pos) = seen.windows(4).position(|w| w == b"\r\n\r\n") {
                        head_end = Some(pos + 4);
                        let head = String::from_utf8_lossy(&seen[..pos]).to_lowercase();
                        content_length = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:"))
                            .and_then(|v| v.trim().parse().ok())
                            .unwrap_or(0);
                    }
                }
                if n == 0 {
                    break;
                }
                if let Some(end) = head_end {
                    // Drain the JSON body before answering: closing while the
                    // client is still writing resets the socket and loses the reply.
                    if seen.len() >= end + content_length {
                        break;
                    }
                }
            }
            let _ = tx.send(String::from_utf8_lossy(&seen).to_string());
            let body = br#"{"answers":{"m_waving":{"type":"noul","noul":0.91}}}"#;
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(body);
            let _ = stream.flush();
        });
        let transport = OpenCodeTransport {
            key: " oc_sk_credit ".into(),
            model: "jev-1.13".into(),
            timeout: Duration::from_secs(5),
        };
        let req = decision::moment_request("x", vocabulary(ShootPreset::Wedding), vec![]);
        let reply = transport.decide_at(&url, &req);
        let seen = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default();
        server.join().unwrap();
        let headers = seen.to_lowercase();
        assert!(headers.starts_with("post /systemone "), "{headers}");
        assert!(
            headers.contains("authorization: bearer oc_sk_credit"),
            "{headers}"
        );
        assert!(headers.contains("user-agent: conrod/"), "{headers}");
        assert!(
            !headers.contains("x-opencode-session"),
            "the credits API is not the Go subscription and takes no session id: {headers}"
        );
        let reply = reply.expect("a valid JEV reply");
        assert_eq!(reply.answers["m_waving"].noul, Some(0.91));
    }

    #[test]
    fn the_opencode_key_is_its_own_or_the_vision_services() {
        let mut s = Settings::default();
        assert_eq!(
            s.moments_opencode_model, "jev-1.13",
            "the paid id works as is"
        );
        assert_eq!(opencode_key(&s), "", "no key anywhere");
        s.vlm_services.push(conrod_core::settings::VlmService {
            id: "vlm-1".into(),
            provider: "opencode-go".into(),
            model: "glm-5.3-flash".into(),
            host: String::new(),
            api_key: "oc_sk_shared".into(),
            key_kind: "auto".into(),
            enabled: true,
        });
        assert_eq!(
            opencode_key(&s),
            "oc_sk_shared",
            "one OpenCode account serves vision and moments"
        );
        s.vlm_services[0].enabled = false;
        assert_eq!(opencode_key(&s), "", "a service that is off shares nothing");
        s.moments_opencode_key = " oc_sk_own ".into();
        assert_eq!(opencode_key(&s), "oc_sk_own", "its own key wins");
        // Auto prefers JEV over OpenRouter once a key exists.
        s.moments_backend = "decider".into();
        s.moments_openrouter_key = "or".into();
        assert_eq!(
            transport_for(&s, InProcess::unavailable()).unwrap().name(),
            "opencode"
        );
        // Explicit opencode without a key says what is missing.
        s.moments_opencode_key.clear();
        s.moments_transport = "opencode".into();
        let err = match transport_for(&s, InProcess::unavailable()) {
            Ok(_) => panic!("opencode without a key has no transport"),
            Err(e) => e,
        };
        assert!(err.contains("OpenCode key"));
    }

    #[test]
    fn the_judge_follows_the_albums_backend_and_transport() {
        let mut s = Settings::default();
        assert!(
            judge_for(&s, None, InProcess::unavailable()).is_err(),
            "vlm backend needs the vision model"
        );
        s.moments_backend = "decider".into();
        assert_eq!(
            s.moments_transport, "auto",
            "in-process is preferred by default"
        );
        assert!(
            judge_for(&s, None, InProcess::unavailable()).is_err(),
            "no in-process model and no key: no judge"
        );
        s.moments_openrouter_key = "k".into();
        assert_eq!(
            judge_for(&s, None, InProcess::unavailable())
                .unwrap()
                .name(),
            "decider"
        );
        // Auto prefers the seated in-process model over OpenRouter.
        let seated = InProcess::with(|_| Ok(Response::default()));
        assert_eq!(
            transport_for(&s, seated.clone()).unwrap().name(),
            "in_process"
        );
        assert_eq!(
            transport_for(&s, InProcess::unavailable()).unwrap().name(),
            "openrouter"
        );
        // Without a key it still works in-process; explicit openrouter needs the key.
        s.moments_openrouter_key.clear();
        assert_eq!(transport_for(&s, seated).unwrap().name(), "in_process");
        s.moments_transport = "openrouter".into();
        assert!(transport_for(&s, InProcess::unavailable()).is_err());
        s.moments_transport = "in_process".into();
        s.moments_openrouter_key.clear();
        let judge = judge_for(&s, None, InProcess::unavailable()).unwrap();
        let err = judge
            .judge(
                "x",
                vocabulary(ShootPreset::Wedding),
                "data:image/jpeg;base64,AA",
            )
            .unwrap_err();
        assert!(err.contains("not installed"));
        assert_eq!(strip_data_url("data:image/jpeg;base64,AAAA"), "AAAA");
        assert_eq!(strip_data_url("AAAA"), "AAAA");
    }
}
