//! The handful of Python string and number semantics the port has to keep.
//!
//! Small on purpose: only what a fixture has shown to matter.

use serde_json::Value;

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
/// ponytail: ASCII only. Python also accepts other scripts' digits and
/// superscripts; race numbers are ASCII in practice. Widen if one turns up.
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
