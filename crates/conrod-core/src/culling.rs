//! Respecting the cull.
//!
//! Port of `conrod/culling.py`. `read_culls` and `filter_frames` themselves
//! stay out -- both are exiftool calls -- but their pure decision logic does
//! not need a separate wrapper here: turning a raw tag row into a [`Cull`]
//! is [`cull_from_tags`], and turning a [`Cull`] into a keep/skip verdict is
//! [`Cull::passes`]. `filter_frames` itself is just those two calls run over
//! a list and tallied, which needs no port of its own.

use crate::py;
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
        Value::String(s) => py::float(s),
        // float(True) == 1.0 in Python, since bool is an int subtype.
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        _ => None,
    }
}

/// Turn one exiftool tag row into a [`Cull`] -- the pure decision inside
/// `read_culls`. `Rating` wins over `XMP:Rating` when both are present;
/// anything that will not parse as a number is treated as unrated, exactly
/// as Python's `except (TypeError, ValueError): value = 0` does.
pub fn cull_from_tags(row: &Map<String, Value>) -> Cull {
    // `if rating is None: rating = row.get("XMP:Rating")` -- a *value* of
    // JSON null falls through exactly like a missing key does.
    let rating = match row.get("Rating") {
        None | Some(Value::Null) => row.get("XMP:Rating"),
        some => some,
    };
    // int(float(rating)): truncates toward zero, not the nearest star.
    let value = rating
        .and_then(to_float)
        .map(|f| f.trunc() as i32)
        .unwrap_or(0);
    let label = match row.get("Label") {
        Some(v) if py::truthy(v) => py::str_of(v),
        _ => String::new(),
    };
    Cull {
        rating: value.max(0),
        label,
        rejected: value <= REJECTED,
    }
}
