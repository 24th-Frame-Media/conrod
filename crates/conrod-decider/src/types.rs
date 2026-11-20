//! The request and response schema: the Decisions API shape of `pplx_decider`
//! (`state`, named `questions`, optional `images` in; one answer per question out).
//!
//! Question order is part of the protocol (the model sees questions and options in
//! the order given), so maps are [`Ordered`] rather than `serde_json::Map`, which
//! would sort its keys.

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fmt;
use std::marker::PhantomData;

/// A JSON object that keeps its keys in the order they were written.
#[derive(Debug, Clone, PartialEq)]
pub struct Ordered<T>(pub Vec<(String, T)>);

impl<T> Default for Ordered<T> {
    fn default() -> Self {
        Ordered(Vec::new())
    }
}

impl<T> Ordered<T> {
    pub fn new() -> Self {
        Ordered(Vec::new())
    }
    pub fn push(&mut self, key: impl Into<String>, value: T) {
        self.0.push((key.into(), value));
    }
    pub fn get(&self, key: &str) -> Option<&T> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &T)> {
        self.0.iter().map(|(k, v)| (k, v))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<T> FromIterator<(String, T)> for Ordered<T> {
    fn from_iter<I: IntoIterator<Item = (String, T)>>(iter: I) -> Self {
        Ordered(iter.into_iter().collect())
    }
}

impl<T: Serialize> Serialize for Ordered<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Ordered<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V<T> {
            type Value = Ordered<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut out = Vec::with_capacity(map.size_hint().unwrap_or(0));
                while let Some((k, v)) = map.next_entry::<String, T>()? {
                    out.push((k, v));
                }
                Ok(Ordered(out))
            }
        }
        d.deserialize_map(V(PhantomData))
    }
}

/// One question. `instructions` and every description is plain text or any JSON
/// value (non-strings are written into the prompt as Python-style `json.dumps`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Question {
    /// Pick one of 1..=255 named options; `criteria` maps name to description.
    Choice {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<Value>,
        criteria: Ordered<Option<Value>>,
    },
    /// Yes/no; the answer is P(true). `criteria` may describe `"true"` / `"false"`.
    Noul {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        criteria: Option<NoulCriteria>,
    },
    /// An ordered scale of 2..=255 levels, lowest first.
    Score {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        instructions: Option<Value>,
        criteria: Vec<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct NoulCriteria {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#true: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#false: Option<Value>,
}

impl Question {
    pub fn choice<I, K>(instructions: &str, options: I) -> Question
    where
        I: IntoIterator<Item = (K, Option<String>)>,
        K: Into<String>,
    {
        Question::Choice {
            instructions: Some(Value::String(instructions.to_string())),
            criteria: options
                .into_iter()
                .map(|(k, d)| (k.into(), d.map(Value::String)))
                .collect(),
        }
    }
    /// A yes/no question with the default option wording.
    pub fn noul(instructions: &str) -> Question {
        Question::Noul {
            instructions: Some(Value::String(instructions.to_string())),
            criteria: None,
        }
    }
    pub fn score(instructions: &str, levels: &[&str]) -> Question {
        Question::Score {
            instructions: Some(Value::String(instructions.to_string())),
            criteria: levels.iter().map(|l| Value::String((*l).into())).collect(),
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Question::Choice { .. } => "choice",
            Question::Noul { .. } => "noul",
            Question::Score { .. } => "score",
        }
    }
    pub fn instructions(&self) -> Option<&Value> {
        match self {
            Question::Choice { instructions, .. }
            | Question::Noul { instructions, .. }
            | Question::Score { instructions, .. } => instructions.as_ref(),
        }
    }
}

/// One decision context: a state (text, or any JSON), the questions, and images.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Request {
    pub state: Value,
    pub questions: Ordered<Question>,
    /// Image sources as in the reference: a file path or a `data:image/..;base64,..` URL.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
    /// Already decoded images (in-process callers); placed after `images`.
    #[serde(skip)]
    pub pixels: Vec<image::RgbImage>,
}

impl Request {
    pub fn new(state: impl Into<Value>) -> Request {
        Request {
            state: state.into(),
            ..Request::default()
        }
    }
    pub fn ask(mut self, id: impl Into<String>, question: Question) -> Request {
        self.questions.push(id, question);
        self
    }
    pub fn with_image(mut self, image: image::RgbImage) -> Request {
        self.pixels.push(image);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Answer {
    Noul {
        /// P(true).
        noul: f64,
    },
    Choice {
        choice: String,
        probabilities: Ordered<f64>,
        confidence: f64,
    },
    Score {
        /// Expected level index.
        score: f64,
        legend: Ordered<Value>,
        probabilities: Ordered<f64>,
        confidence: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub sequences: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub answers: Ordered<Answer>,
    pub usage: Usage,
}

impl Response {
    pub fn answer(&self, id: &str) -> Option<&Answer> {
        self.answers.get(id)
    }
    /// P(true) of a `noul` answer.
    pub fn noul(&self, id: &str) -> Option<f64> {
        match self.answers.get(id)? {
            Answer::Noul { noul } => Some(*noul),
            _ => None,
        }
    }
}
