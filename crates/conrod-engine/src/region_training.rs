//! Where the per-region sharpness models live.
//!
//! The trainer that wrote them went with the Train tab (be1df63); `rescore`
//! still reads whatever model files that left behind.
use crate::desktop::Desktop;
use std::path::PathBuf;

pub(crate) fn path(d: &Desktop, r: &str) -> PathBuf {
    d.root.join("models").join(if r == "vehicle" {
        "sharpness.json".into()
    } else {
        format!("sharpness-{r}.json")
    })
}
