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
use crate::vlm::{self, pick_service, HostPools, VlmClient};
use conrod_core::decision::{self, DecisionTransport, InProcess, Request, Response};
use conrod_core::moments::{Moment, MomentReading, MomentVerdict};
use conrod_core::settings::{MomentsService, Settings};
use serde_json::{json, Value};
use std::sync::atomic::AtomicUsize;
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
        vlm::guard_remote(OPENROUTER_URL, "OpenRouter decision endpoint")
            .map_err(|e| e.to_string())?;
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
        vlm::guard_remote(url, "OpenCode decision endpoint").map_err(|e| e.to_string())?;
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

/// The OpenCode key a moments service may borrow: the key of an enabled
/// OpenCode vision service, so one OpenCode account serves both lanes.
fn vision_opencode_key(settings: &Settings) -> String {
    settings
        .vlm_services
        .iter()
        .filter(|s| s.enabled && s.provider.starts_with("opencode"))
        .map(|s| s.api_key.trim())
        .find(|key| !key.is_empty())
        .unwrap_or("")
        .to_string()
}

/// How long one cloud decision may take before the next service is tried.
const DECISION_TIMEOUT: Duration = Duration::from_secs(60);

/// The model id a service uses when its own is blank.
fn default_model(provider: &str) -> &'static str {
    match provider {
        "opencode" | "opencode-go" => "jev-1.13",
        _ => "perplexity/pplx-decider-flash-v1.3:free",
    }
}

/// The transport one enabled cloud service speaks. The local model is built
/// from the engine's seat instead (see [`ServiceJudge::new`]); a provider this
/// build does not know is skipped, never turned into a moment.
fn cloud_transport(
    service: &MomentsService,
    shared_opencode: &str,
) -> Option<Arc<dyn DecisionTransport>> {
    let model = match service.model.trim() {
        "" => default_model(&service.provider).to_string(),
        model => model.to_string(),
    };
    match service.provider.as_str() {
        "openrouter" => Some(Arc::new(OpenRouterTransport {
            key: service.api_key.clone(),
            model,
            timeout: DECISION_TIMEOUT,
        })),
        "opencode" | "opencode-go" => {
            let key = if service.api_key.trim().is_empty() {
                shared_opencode.to_string()
            } else {
                service.api_key.clone()
            };
            Some(Arc::new(OpenCodeTransport {
                key,
                model,
                timeout: DECISION_TIMEOUT,
            }))
        }
        _ => None,
    }
}

/// One enabled cloud service and the transport it resolved to.
struct CloudService {
    id: String,
    transport: Arc<dyn DecisionTransport>,
}

/// The moment lane's judge: the usable local model first, then every enabled
/// cloud service once per question in `moments_strategy` order, then (when the
/// album allows it) the car reader. A service that is down is skipped and
/// never retried for the same frame: one dead service never drops a moment
/// while another can answer, and total exhaustion is "not judged", never a
/// failed scan.
pub struct ServiceJudge {
    name: &'static str,
    local: Option<Arc<dyn DecisionTransport>>,
    cloud: Vec<CloudService>,
    pools: HostPools,
    turn: AtomicUsize,
    strategy: String,
    fallback: Option<Box<dyn MomentJudge>>,
}

impl ServiceJudge {
    fn new(
        settings: &Settings,
        vlm: Option<(VlmClient, IoSettings)>,
        in_process: InProcess,
    ) -> ServiceJudge {
        let local_enabled = settings
            .moments_services
            .iter()
            .any(|service| service.enabled && service.provider == "in_process");
        // The seat is a candidate only when the model is installed and the
        // graphics-memory check let it load; `is_available` already says so.
        let local = (local_enabled && in_process.is_available())
            .then(|| Arc::new(in_process) as Arc<dyn DecisionTransport>);
        let shared_opencode = vision_opencode_key(settings);
        let cloud: Vec<CloudService> = settings
            .moments_services
            .iter()
            .filter(|service| service.enabled)
            .filter_map(|service| {
                cloud_transport(service, &shared_opencode).map(|transport| CloudService {
                    id: service.id.clone(),
                    transport,
                })
            })
            .collect();
        let fallback = vlm
            .filter(|_| settings.moments_vlm_fallback && settings.use_vlm)
            .map(|(client, settings)| {
                Box::new(VlmJudge { client, settings }) as Box<dyn MomentJudge>
            });
        let name = if local.is_none() && cloud.is_empty() {
            "vlm"
        } else {
            "services"
        };
        ServiceJudge {
            name,
            local,
            cloud,
            pools: HostPools::default(),
            turn: AtomicUsize::new(0),
            strategy: settings.moments_strategy.clone(),
            fallback,
        }
    }

    fn judgeable(&self) -> bool {
        self.local.is_some() || !self.cloud.is_empty() || self.fallback.is_some()
    }

    /// One request down the failover order: local once, then each cloud
    /// service once from a rotated start. `Err` keeps the last reason for the
    /// fallback and the log.
    fn decide_with(&self, request: &Request) -> Result<Response, String> {
        let mut last = "No moment service answered".to_string();
        if let Some(local) = &self.local {
            match local.decide(request) {
                Ok(response) => return Ok(response),
                Err(why) => last = why,
            }
        }
        if !self.cloud.is_empty() {
            let ids: Vec<String> = self
                .cloud
                .iter()
                .map(|service| service.id.clone())
                .collect();
            let pool = self.pools.pool_for(ids.clone());
            let least_busy = self.strategy.is_empty() || self.strategy == "least_busy";
            let held = least_busy.then(|| pool.acquire());
            let start = pick_service(&self.strategy, &ids, held.as_deref(), &self.turn);
            // As vision does: only the first pick holds a least-busy token;
            // failover tries go unpooled rather than wait on a busy service.
            if let Some(id) = held {
                pool.release(id);
            }
            for step in 0..ids.len() {
                let service = &self.cloud[(start + step) % self.cloud.len()];
                match service.transport.decide(request) {
                    Ok(response) => return Ok(response),
                    Err(why) => last = why,
                }
            }
        }
        Err(last)
    }
}

impl MomentJudge for ServiceJudge {
    fn name(&self) -> &'static str {
        self.name
    }

    fn judge(&self, context: &str, vocab: &[Moment], image: &str) -> Result<MomentVerdict, String> {
        let request = decision::moment_request(context, vocab, vec![image.to_string()]);
        match self.decide_with(&request) {
            Ok(response) => Ok(decision::read_moments(&response, vocab)),
            Err(why) => match &self.fallback {
                Some(fallback) => fallback.judge(context, vocab, image),
                None => Err(format!("not judged: {why}")),
            },
        }
    }

    fn judge_pair(
        &self,
        context: &str,
        vocab: &[Moment],
        frame: &str,
        crop: Option<&str>,
    ) -> Result<MomentVerdict, String> {
        let mut images = vec![frame.to_string()];
        if let Some(crop) = crop {
            images.push(crop.to_string());
        }
        let request = decision::moment_request(context, vocab, images);
        match self.decide_with(&request) {
            Ok(response) => Ok(decision::read_moments(&response, vocab)),
            Err(why) => match &self.fallback {
                Some(fallback) => fallback.judge_pair(context, vocab, frame, crop),
                None => Err(format!("not judged: {why}")),
            },
        }
    }

    fn peak(&self, context: &str, images: &[String]) -> Result<Option<Vec<f64>>, String> {
        if images.len() < 2 {
            return Ok(None);
        }
        let request = decision::peak_request(context, images.to_vec());
        match self.decide_with(&request) {
            Ok(response) => Ok(decision::read_peak(&response, images.len())),
            Err(why) => match &self.fallback {
                Some(fallback) => fallback.peak(context, images),
                None => Err(format!("not judged: {why}")),
            },
        }
    }
}

/// The judge this album's settings ask for. `in_process` is the in-process
/// model's seat (the engine fills it from the decider service).
pub fn judge_for(
    settings: &Settings,
    vlm: Option<(VlmClient, IoSettings)>,
    in_process: InProcess,
) -> Result<Box<dyn MomentJudge>, String> {
    let judge = ServiceJudge::new(settings, vlm, in_process);
    if !judge.judgeable() {
        return Err(
            "No moment service is on, and the car reader is off, so moments cannot be judged"
                .into(),
        );
    }
    Ok(Box::new(judge))
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

    /// A transport that records each attempt and can be a dead service.
    struct Trip {
        id: &'static str,
        fail: bool,
        seen: Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl DecisionTransport for Trip {
        fn name(&self) -> &'static str {
            "mock"
        }
        fn decide(&self, _: &Request) -> Result<Response, String> {
            self.seen.lock().unwrap().push(self.id);
            if self.fail {
                Err(format!("{} is down", self.id))
            } else {
                Ok(Response::default())
            }
        }
    }

    fn trip(
        id: &'static str,
        fail: bool,
        seen: &Arc<std::sync::Mutex<Vec<&'static str>>>,
    ) -> Arc<dyn DecisionTransport> {
        Arc::new(Trip {
            id,
            fail,
            seen: seen.clone(),
        })
    }

    /// The car reader as a stand-in that records that it was asked.
    struct Fallback(Arc<std::sync::Mutex<Vec<&'static str>>>);

    impl MomentJudge for Fallback {
        fn name(&self) -> &'static str {
            "vlm"
        }
        fn judge(
            &self,
            _context: &str,
            _vocab: &[Moment],
            _image: &str,
        ) -> Result<MomentVerdict, String> {
            self.0.lock().unwrap().push("fallback");
            Ok(MomentVerdict {
                backend: "vlm".into(),
                ..MomentVerdict::default()
            })
        }
    }

    fn judge_with(
        local: Option<Arc<dyn DecisionTransport>>,
        cloud: Vec<(&'static str, Arc<dyn DecisionTransport>)>,
        strategy: &str,
        fallback: Option<Box<dyn MomentJudge>>,
    ) -> ServiceJudge {
        ServiceJudge {
            name: "services",
            local,
            cloud: cloud
                .into_iter()
                .map(|(id, transport)| CloudService {
                    id: id.into(),
                    transport,
                })
                .collect(),
            pools: HostPools::default(),
            turn: AtomicUsize::new(0),
            strategy: strategy.into(),
            fallback,
        }
    }

    fn ask(judge: &dyn MomentJudge) -> Result<MomentVerdict, String> {
        judge.judge(
            "Track day.",
            vocabulary(ShootPreset::Wedding),
            "data:image/jpeg;base64,AA",
        )
    }

    #[test]
    fn the_usable_local_model_is_asked_before_any_cloud_service() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let judge = judge_with(
            Some(trip("local", false, &seen)),
            vec![("a", trip("a", false, &seen))],
            "least_busy",
            None,
        );
        assert_eq!(ask(&judge).unwrap().backend, "decider");
        assert_eq!(*seen.lock().unwrap(), ["local"], "the cloud is not touched");
    }

    #[test]
    fn a_down_service_is_tried_once_and_the_next_takes_over() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let judge = judge_with(
            Some(trip("local", true, &seen)),
            vec![
                ("a", trip("a", true, &seen)),
                ("b", trip("b", false, &seen)),
            ],
            "least_busy",
            None,
        );
        assert_eq!(ask(&judge).unwrap().backend, "decider");
        assert_eq!(*seen.lock().unwrap(), ["local", "a", "b"]);
        // A second frame starts from the token least busy handed out; no
        // service is retried twice for one question.
        assert!(ask(&judge).is_ok());
        assert_eq!(
            *seen.lock().unwrap(),
            ["local", "a", "b", "local", "b"],
            "b's token came back first, so b is the least busy start"
        );
    }

    #[test]
    fn round_robin_cloud_services_take_turns() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let judge = judge_with(
            None,
            vec![
                ("a", trip("a", false, &seen)),
                ("b", trip("b", false, &seen)),
                ("c", trip("c", false, &seen)),
            ],
            "round_robin",
            None,
        );
        for _ in 0..4 {
            assert!(ask(&judge).is_ok());
        }
        assert_eq!(*seen.lock().unwrap(), ["a", "b", "c", "a"]);
    }

    #[test]
    fn total_exhaustion_is_the_car_reader_when_its_gate_is_on_and_not_judged_otherwise() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let with_fallback = judge_with(
            Some(trip("local", true, &seen)),
            vec![("a", trip("a", true, &seen))],
            "least_busy",
            Some(Box::new(Fallback(seen.clone()))),
        );
        assert_eq!(ask(&with_fallback).unwrap().backend, "vlm");
        assert_eq!(*seen.lock().unwrap(), ["local", "a", "fallback"]);

        let without = judge_with(
            Some(trip("local", true, &seen)),
            vec![("a", trip("a", true, &seen))],
            "least_busy",
            None,
        );
        let err = ask(&without).unwrap_err();
        assert!(err.contains("not judged"), "{err}");
    }

    #[test]
    fn the_local_card_needs_an_installed_model_that_fits_the_card() {
        let mut s = Settings {
            moments_vlm_fallback: false,
            moments_services: vec![MomentsService {
                id: "moments-local".into(),
                provider: "in_process".into(),
                enabled: true,
                ..MomentsService::default()
            }],
            ..Settings::default()
        };
        assert!(
            judge_for(&s, None, InProcess::unavailable()).is_err(),
            "enabled but not installed (or no room): not a candidate"
        );
        let seated = InProcess::with(|_| Ok(Response::default()));
        let judge = judge_for(&s, None, seated).unwrap();
        assert_eq!(judge.name(), "services");
        assert_eq!(ask(judge.as_ref()).unwrap().backend, "decider");
        // A switched-off local card is not asked even when the seat is ready.
        s.moments_services[0].enabled = false;
        assert!(judge_for(&s, None, InProcess::with(|_| Ok(Response::default()))).is_err());
    }

    #[test]
    fn a_cloud_service_resolves_by_provider_and_borrows_the_vision_key() {
        let s = Settings {
            vlm_services: vec![conrod_core::settings::VlmService {
                id: "vlm-1".into(),
                provider: "opencode-go".into(),
                api_key: "oc_sk_shared".into(),
                enabled: true,
                ..conrod_core::settings::VlmService::default()
            }],
            ..Settings::default()
        };
        assert_eq!(
            vision_opencode_key(&s),
            "oc_sk_shared",
            "one OpenCode account serves vision and moments"
        );
        let openrouter = MomentsService {
            id: "or".into(),
            provider: "openrouter".into(),
            ..MomentsService::default()
        };
        assert_eq!(
            cloud_transport(&openrouter, "").unwrap().name(),
            "openrouter"
        );
        let opencode = MomentsService {
            id: "oc".into(),
            provider: "opencode".into(),
            ..MomentsService::default()
        };
        assert_eq!(
            cloud_transport(&opencode, "shared").unwrap().name(),
            "opencode"
        );
        assert!(
            cloud_transport(
                &MomentsService {
                    provider: "in_process".into(),
                    ..MomentsService::default()
                },
                ""
            )
            .is_none(),
            "the local model is the engine's seat, not a cloud transport"
        );
        assert!(cloud_transport(
            &MomentsService {
                provider: "mystery".into(),
                ..MomentsService::default()
            },
            ""
        )
        .is_none());
        assert_eq!(strip_data_url("data:image/jpeg;base64,AAAA"), "AAAA");
        assert_eq!(strip_data_url("AAAA"), "AAAA");
    }

    #[test]
    fn without_services_or_fallback_there_is_nothing_to_judge() {
        let s = Settings {
            moments_vlm_fallback: false,
            ..Settings::default()
        };
        assert!(judge_for(&s, None, InProcess::unavailable()).is_err());
    }
}
