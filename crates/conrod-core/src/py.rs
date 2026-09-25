//! The handful of Python string and number semantics the port has to keep.
//!
//! Small on purpose: only what a fixture has shown to matter.

use serde_json::Value;
use std::collections::HashMap;
use std::hash::Hash;

/// `str(value)` for the JSON values exiftool hands back.
pub fn str_of(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// `int(text)`: surrounding whitespace allowed, a sign allowed.
pub fn int(text: &str) -> Option<i64> {
    text.trim().parse().ok()
}

/// `float(text)`: surrounding whitespace allowed, inf and nan allowed.
pub fn float(text: &str) -> Option<f64> {
    text.trim().parse().ok()
}

/// `str.casefold()`, close enough for keyword de-duplication: lowercase, and
/// the sharp s folds to "ss" as it does in Python.
pub fn casefold(text: &str) -> String {
    text.to_lowercase().replace('ß', "ss")
}

/// `str.isdigit()` for one character.
///
/// ponytail: ASCII only; race numbers are ASCII in practice. Widen if a
/// non-ASCII one turns up.
pub fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

/// `str.capitalize()`: first character upper, the rest lower.
pub fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first
            .to_uppercase()
            .chain(chars.as_str().to_lowercase().chars())
            .collect(),
        None => String::new(),
    }
}

/// `bool(value)` for a parsed JSON value: `None`/`false`/`0`/`""`/`[]`/`{}`
/// are all falsy, exactly as Python treats them.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// A `collections.Counter`-alike that remembers first-seen order.
///
/// Python's `Counter` is a plain dict, so it iterates in insertion order, and
/// `most_common()` sorts by count with a *stable* sort -- ties keep that
/// insertion order rather than coming out however a hash happens to land.
/// Both matter to callers that break ties on "whichever was seen first".
#[derive(Debug, Clone)]
pub struct Counter<T: Hash + Eq + Clone> {
    order: Vec<T>,
    counts: HashMap<T, i64>,
}

impl<T: Hash + Eq + Clone> Counter<T> {
    pub fn new() -> Self {
        Counter {
            order: Vec::new(),
            counts: HashMap::new(),
        }
    }

    pub fn add(&mut self, key: T, amount: i64) {
        if !self.counts.contains_key(&key) {
            self.order.push(key.clone());
        }
        *self.counts.entry(key).or_insert(0) += amount;
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Keys in the order they were first added -- `for key in counter`.
    pub fn keys(&self) -> &[T] {
        &self.order
    }

    /// Counts high to low; equal counts keep first-seen order --
    /// `Counter.most_common()`.
    pub fn most_common(&self) -> Vec<(T, i64)> {
        let mut items: Vec<(usize, T, i64)> = self
            .order
            .iter()
            .enumerate()
            .map(|(i, k)| (i, k.clone(), self.counts[k]))
            .collect();
        items.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
        items.into_iter().map(|(_, k, c)| (k, c)).collect()
    }
}

impl<T: Hash + Eq + Clone> Default for Counter<T> {
    fn default() -> Self {
        Self::new()
    }
}
