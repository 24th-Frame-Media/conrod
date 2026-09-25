//! Small string/JSON-value helpers shared by the analysis modules.

use serde_json::Value;
use std::collections::HashMap;
use std::hash::Hash;

/// Render a decoded JSON value the way it should read as plain text: a
/// string as itself, a bool as "True"/"False" and null as "None" (matching
/// what exiftool's own JSON round-trips through), everything else via its
/// normal `Display`.
pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        other => other.to_string(),
    }
}

/// Whether a decoded JSON value carries anything: `null`/`false`/`0`/`""`/
/// `[]`/`{}` all count as empty.
pub fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Case-insensitive fold for keyword de-duplication: lowercase, with the
/// German sharp s folded to "ss".
pub fn casefold(text: &str) -> String {
    text.to_lowercase().replace('ß', "ss")
}

/// A frequency count that keeps first-seen order: [`Tally::ranked`] sorts by
/// count, breaking ties by insertion order rather than by however a hash
/// happens to land. Used wherever callers need to break a tie on "whichever
/// was seen first".
#[derive(Debug, Clone)]
pub struct Tally<T: Hash + Eq + Clone> {
    order: Vec<T>,
    counts: HashMap<T, i64>,
}

impl<T: Hash + Eq + Clone> Tally<T> {
    pub fn new() -> Self {
        Tally {
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

    /// Keys in the order they were first added.
    pub fn keys(&self) -> &[T] {
        &self.order
    }

    /// The count for `key`, or 0 if it was never added.
    pub fn get(&self, key: &T) -> i64 {
        self.counts.get(key).copied().unwrap_or(0)
    }

    /// Counts high to low; equal counts keep first-seen order.
    pub fn ranked(&self) -> Vec<(T, i64)> {
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

impl<T: Hash + Eq + Clone> Default for Tally<T> {
    fn default() -> Self {
        Self::new()
    }
}
