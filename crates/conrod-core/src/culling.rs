//! Respecting the cull.
//!
//! Turning a raw exiftool tag row into a [`Cull`] is [`cull_from_tags`], and
//! turning a [`Cull`] into a keep/skip verdict is [`Cull::passes`]. Reading
//! the tags and filtering a whole file list is just those two calls run over
//! a list and tallied, and needs nothing more here.

use crate::text::{is_truthy, value_to_string};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// Lightroom, Bridge and Photo Mechanic all express "rejected" as a negative
/// rating in XMP. Capture One and most others use 0-5 with no negatives.
pub const REJECTED: i32 = -1;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cull {
    pub rating: i32,
    pub label: String,
    pub rejected: bool,
}

/// The cull settings `Cull::passes` reads, with `conrod/config.py`'s
/// defaults.
#[derive(Debug, Clone)]
pub struct CullSettings {
    pub skip_rejected: bool,
    pub min_rating: i32,
    pub require_label: String,
}

impl Default for CullSettings {
    fn default() -> Self {
        CullSettings {
            skip_rejected: true,
            min_rating: 0,
            require_label: String::new(),
        }
    }
}

impl Cull {
    pub fn passes(&self, settings: &CullSettings) -> (bool, String) {
        if settings.skip_rejected && self.rejected {
            return (false, "rejected".to_string());
        }
        if settings.min_rating > 0 && self.rating < settings.min_rating {
            return (false, format!("{} star", self.rating));
        }
        let wanted = settings.require_label.trim().to_lowercase();
        if !wanted.is_empty() && self.label.to_lowercase() != wanted {
            let label = if self.label.is_empty() {
                "none"
            } else {
                self.label.as_str()
            };
            return (false, format!("label {label}"));
        }
        (true, String::new())
    }
}

pub fn sidecar_for(image: &Path) -> PathBuf {
    image.with_extension("xmp")
}

fn to_float(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Turn one exiftool tag row into a [`Cull`]. `Rating` wins over `XMP:Rating`
/// when both are present; anything that will not parse as a number is
/// treated as unrated.
pub fn cull_from_tags(row: &Map<String, Value>) -> Cull {
    // A *value* of JSON null falls through to `XMP:Rating` exactly like a
    // missing key does.
    let rating = match row.get("Rating") {
        None | Some(Value::Null) => row.get("XMP:Rating"),
        some => some,
    };
    // Truncates toward zero, not the nearest star.
    let value = rating
        .and_then(to_float)
        .map(|f| f.trunc() as i32)
        .unwrap_or(0);
    let label = match row.get("Label") {
        Some(v) if is_truthy(v) => value_to_string(v),
        _ => String::new(),
    };
    Cull {
        rating: value.max(0),
        label,
        rejected: value <= REJECTED,
    }
}
