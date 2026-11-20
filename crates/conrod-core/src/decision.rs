//! The Decisions request / response schema (Perplexity's decider models,
//! https://huggingface.co/perplexity-ai/pplx-decider-flash-v1.2), shared by
//! every transport: OpenRouter's Decisions API, an in-process model, a mock.
//!
//! Only the shapes live here, plus the builders and readers that turn a
//! shoot's moment vocabulary into a request and an answer back into readings.
//! No IO: a [`DecisionTransport`] carries the request.
//!
//! The model card: `noul` answers P(true); `choice` answers a probability
//! per option; `score` answers the expected level index (levels listed lowest
//! first) with its probabilities. Images are data URLs, at most 1.6 MP each.

use crate::moments::{Moment, MomentReading, MomentVerdict};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;

/// One question. `criteria` differs by kind, as the model card lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    /// P(true) for a yes/no question.
    Noul {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
    },
    /// A probability per option (`option -> description`).
    Choice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
        criteria: BTreeMap<String, Option<String>>,
    },
    /// An ordered rubric, lowest level first.
    Score {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<String>,
        criteria: Vec<String>,
    },
}

/// A request: the context, the questions about it, optional images (data URLs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub state: Value,
    pub questions: BTreeMap<String, Question>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}

/// One answer. Which fields are filled depends on the question's kind.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    /// `noul`: P(true).
    #[serde(default)]
    pub noul: Option<f64>,
    /// `choice`: the likeliest option.
    #[serde(default)]
    pub choice: Option<String>,
    /// `score`: the expected level index.
    #[serde(default)]
    pub score: Option<f64>,
    /// `choice`: option -> p; `score`: p per level (an object or a list).
    #[serde(default)]
    pub probabilities: Option<Value>,
    #[serde(default)]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Response {
    #[serde(default)]
    pub answers: BTreeMap<String, Answer>,
    #[serde(default)]
    pub usage: Option<Value>,
}

/// Something that can answer a [`Request`]. `Err` is a transport or model
/// failure; callers treat it as "no moment" and never block a scan on it.
pub trait DecisionTransport: Send + Sync {
    fn name(&self) -> &'static str;
    fn decide(&self, request: &Request) -> Result<Response, String>;
}

/// How the in-process model answers.
pub type DecideFn = dyn Fn(&Request) -> Result<Response, String> + Send + Sync;

/// The in-process model's seat. The decider crate fills it with
/// [`InProcess::with`]; until then every request fails with a plain message.
#[derive(Clone, Default)]
pub struct InProcess(Option<Arc<DecideFn>>);

impl InProcess {
    pub fn unavailable() -> InProcess {
        InProcess(None)
    }

    pub fn with(
        decide: impl Fn(&Request) -> Result<Response, String> + Send + Sync + 'static,
    ) -> InProcess {
        InProcess(Some(Arc::new(decide)))
    }
}

impl DecisionTransport for InProcess {
    fn name(&self) -> &'static str {
        "in_process"
    }

    fn decide(&self, request: &Request) -> Result<Response, String> {
        match &self.0 {
            Some(decide) => decide(request),
            None => Err("The in-process decision model is not installed".into()),
        }
    }
}

/// The question id for a moment tag.
pub fn moment_id(key: &str) -> String {
    format!("m_{key}")
}

/// The id of the overall-strength question.
pub const STRENGTH_ID: &str = "strength";
/// The rubric for the strength question, lowest first.
pub const STRENGTH_LEVELS: [&str; 4] = [
    "no moment worth keeping",
    "ordinary frame",
    "good moment",
    "peak moment",
];

/// One request per frame: a `noul` question per tag in the preset's
/// vocabulary (a select-all-that-apply decision, where joint questions help),
/// and one `score` question for the frame's overall moment strength.
/// `context` is the shoot's one-line guidance.
pub fn moment_request(context: &str, vocab: &[Moment], images: Vec<String>) -> Request {
    let mut questions = BTreeMap::new();
    for moment in vocab {
        questions.insert(
            moment_id(moment.key),
            Question::Noul {
                instructions: Some(moment.ask.to_string()),
            },
        );
    }
    questions.insert(
        STRENGTH_ID.to_string(),
        Question::Score {
            instructions: Some(
                "How strong is the best moment in this photograph, for a keeper at this kind of shoot?"
                    .into(),
            ),
            criteria: STRENGTH_LEVELS.iter().map(|s| (*s).to_string()).collect(),
        },
    );
    Request {
        state: json!(format!(
            "{context} Answer only from what is clearly visible in the photograph."
        )),
        questions,
        images,
    }
}

/// What the answers say: every tag's P(true) (calibrated), and the strength.
pub fn read_moments(response: &Response, vocab: &[Moment]) -> MomentVerdict {
    let readings = vocab
        .iter()
        .filter_map(|moment| {
            let p = response.answers.get(&moment_id(moment.key))?.noul?;
            p.is_finite().then(|| MomentReading {
                tag: moment.key.to_string(),
                p: p.clamp(0.0, 1.0),
                calibrated: true,
            })
        })
        .collect();
    let levels = (STRENGTH_LEVELS.len() - 1) as f64;
    let strength = response
        .answers
        .get(STRENGTH_ID)
        .and_then(|a| a.score)
        .filter(|s| s.is_finite())
        .map(|s| (s / levels).clamp(0.0, 1.0));
    MomentVerdict {
        backend: "decider".into(),
        readings,
        strength,
        peak: None,
    }
}

/// The option name for frame `i` (0-based): sorts in frame order up to 99.
pub fn frame_option(i: usize) -> String {
    format!("f{:02}", i + 1)
}

pub const PEAK_ID: &str = "peak";

/// One `choice` request over a burst: which of these frames shows the peak of
/// the action? `images` are the burst's frames in order, downscaled by the
/// caller (about 8 at most).
pub fn peak_request(context: &str, images: Vec<String>) -> Request {
    let criteria = (0..images.len())
        .map(|i| (frame_option(i), Some(format!("image {}", i + 1))))
        .collect();
    let mut questions = BTreeMap::new();
    questions.insert(
        PEAK_ID.to_string(),
        Question::Choice {
            instructions: Some(
                "Which of these frames shows the peak of the action or moment?".into(),
            ),
            criteria,
        },
    );
    Request {
        state: json!(format!(
            "{context} These images are consecutive frames of one burst, in order."
        )),
        questions,
        images,
    }
}

/// P(peak) for each of `frames` frames, in order; a missing option is 0.
/// An answer that is not a probability for every frame reads as no opinion
/// (`None`), so a broken reply never decides a pick.
pub fn read_peak(response: &Response, frames: usize) -> Option<Vec<f64>> {
    let answer = response.answers.get(PEAK_ID)?;
    let probabilities = answer.probabilities.as_ref()?;
    let ps: Vec<f64> = match probabilities {
        Value::Object(map) => (0..frames)
            .map(|i| {
                map.get(&frame_option(i))
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0)
            })
            .collect(),
        Value::Array(list) if list.len() == frames => {
            list.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect()
        }
        _ => return None,
    };
    ps.iter()
        .all(|p| p.is_finite() && (0.0..=1.0001).contains(p))
        .then_some(ps)
}

/// The frame a peak answer picks, when one stands out: the likeliest frame,
/// if it clears `floor` and beats the runner-up. Otherwise none: a flat
/// answer carries no tie-break.
pub fn peak_frame(ps: &[f64], floor: f64) -> Option<usize> {
    let (best, p) = ps
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    let runner_up = ps
        .iter()
        .copied()
        .enumerate()
        .filter(|(i, _)| *i != best)
        .map(|(_, p)| p)
        .fold(0.0, f64::max);
    (p >= floor && p > runner_up).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moments::vocabulary;
    use crate::profile::ShootPreset;

    #[test]
    fn a_moment_request_asks_one_noul_per_tag_and_one_strength_score() {
        for preset in ShootPreset::ALL {
            let vocab = vocabulary(preset);
            if vocab.is_empty() {
                continue;
            }
            let req = moment_request(
                preset.prompt_context(),
                vocab,
                vec!["data:image/jpeg;base64,AA".into()],
            );
            assert_eq!(req.questions.len(), vocab.len() + 1, "{preset:?}");
            for moment in vocab {
                match &req.questions[&moment_id(moment.key)] {
                    Question::Noul { instructions } => {
                        assert_eq!(instructions.as_deref(), Some(moment.ask), "{preset:?}")
                    }
                    other => panic!("{preset:?} {}: {other:?}", moment.key),
                }
            }
            match &req.questions[STRENGTH_ID] {
                Question::Score { criteria, .. } => assert_eq!(criteria.len(), 4),
                other => panic!("{other:?}"),
            }
            assert!(req.state.as_str().unwrap().contains("clearly visible"));
        }
    }

    #[test]
    fn the_request_serialises_to_the_model_cards_schema() {
        let vocab = vocabulary(ShootPreset::Wedding);
        let req = moment_request("Weddings.", vocab, vec!["data:image/jpeg;base64,AA".into()]);
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["questions"]["m_first_kiss"]["type"], "noul");
        assert!(v["questions"]["m_first_kiss"]["instructions"]
            .as_str()
            .unwrap()
            .contains("kissing"));
        assert!(v["questions"]["m_first_kiss"].get("criteria").is_none());
        assert_eq!(v["questions"]["strength"]["type"], "score");
        assert_eq!(
            v["questions"]["strength"]["criteria"][0],
            "no moment worth keeping"
        );
        assert_eq!(v["images"][0], "data:image/jpeg;base64,AA");
        // No images: the field is left out.
        let bare = serde_json::to_value(moment_request("x", vocab, vec![])).unwrap();
        assert!(bare.get("images").is_none());
        // And it parses back.
        let back: Request = serde_json::from_value(v).unwrap();
        assert_eq!(back, req);
    }

    fn answer(noul: f64) -> Answer {
        Answer {
            noul: Some(noul),
            ..Answer::default()
        }
    }

    #[test]
    fn a_response_reads_into_calibrated_readings_and_a_strength() {
        let vocab = vocabulary(ShootPreset::Wedding);
        let mut answers = BTreeMap::new();
        answers.insert("m_first_kiss".to_string(), answer(0.97));
        answers.insert("m_confetti".to_string(), answer(0.2));
        answers.insert(
            STRENGTH_ID.to_string(),
            Answer {
                score: Some(2.4),
                ..Answer::default()
            },
        );
        let verdict = read_moments(
            &Response {
                answers,
                usage: None,
            },
            vocab,
        );
        assert_eq!(verdict.backend, "decider");
        assert_eq!(verdict.readings.len(), 2, "only answered tags");
        assert!(verdict.readings.iter().all(|r| r.calibrated));
        assert!((verdict.strength.unwrap() - 0.8).abs() < 1e-9);
        // Parsed from the wire shape too.
        let wire = r#"{"answers":{"m_vows":{"noul":0.9},"strength":{"score":3.0,"probabilities":[0.0,0.0,0.1,0.9],"legend":["a","b","c","d"],"confidence":0.9}},"usage":{"input_tokens":1700,"output_tokens":0,"sequences":1}}"#;
        let parsed: Response = serde_json::from_str(wire).unwrap();
        let verdict = read_moments(&parsed, vocab);
        assert_eq!(verdict.readings[0].tag, "vows");
        assert_eq!(verdict.strength, Some(1.0));
        // Junk is dropped, never panics.
        let junk: Response =
            serde_json::from_str(r#"{"answers":{"m_vows":{"noul":null}}}"#).unwrap();
        assert!(read_moments(&junk, vocab).readings.is_empty());
        assert!(read_moments(&Response::default(), vocab).strength.is_none());
    }

    #[test]
    fn a_burst_peak_request_is_one_choice_over_the_frames() {
        let frames: Vec<String> = (0..3)
            .map(|i| format!("data:image/jpeg;base64,{i}"))
            .collect();
        let req = peak_request("Rodeo.", frames.clone());
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["questions"]["peak"]["type"], "choice");
        assert_eq!(
            v["questions"]["peak"]["criteria"]
                .as_object()
                .unwrap()
                .len(),
            3
        );
        assert_eq!(v["questions"]["peak"]["criteria"]["f01"], "image 1");
        assert_eq!(req.images, frames);
    }

    #[test]
    fn peak_probabilities_parse_and_the_pick_needs_a_clear_winner() {
        let wire = r#"{"answers":{"peak":{"choice":"f02","probabilities":{"f01":0.1,"f02":0.8,"f03":0.1},"confidence":0.8}}}"#;
        let ps = read_peak(&serde_json::from_str(wire).unwrap(), 3).unwrap();
        assert_eq!(ps, vec![0.1, 0.8, 0.1]);
        assert_eq!(peak_frame(&ps, 0.5), Some(1));
        // Not clear enough, or flat: no tie-break.
        assert_eq!(peak_frame(&[0.4, 0.35, 0.25], 0.5), None);
        assert_eq!(peak_frame(&[0.5, 0.5], 0.4), None);
        assert_eq!(peak_frame(&[], 0.5), None);
        // A list works, a wrong-sized or out-of-range one is no opinion.
        let list: Response =
            serde_json::from_str(r#"{"answers":{"peak":{"probabilities":[0.2,0.8]}}}"#).unwrap();
        assert_eq!(read_peak(&list, 2), Some(vec![0.2, 0.8]));
        assert_eq!(read_peak(&list, 3), None);
        let bad: Response =
            serde_json::from_str(r#"{"answers":{"peak":{"probabilities":{"f01":7.0}}}}"#).unwrap();
        assert_eq!(read_peak(&bad, 1), None);
        assert_eq!(read_peak(&Response::default(), 2), None);
    }

    #[test]
    fn the_in_process_seat_fails_plainly_until_the_model_is_installed() {
        let seat = InProcess::unavailable();
        let req = moment_request("x", vocabulary(ShootPreset::Wedding), vec![]);
        assert!(seat.decide(&req).unwrap_err().contains("not installed"));
        let filled = InProcess::with(|r| {
            assert!(!r.questions.is_empty());
            Ok(Response::default())
        });
        assert_eq!(filled.name(), "in_process");
        assert!(filled.decide(&req).unwrap().answers.is_empty());
    }
}
