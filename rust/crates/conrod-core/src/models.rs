//! Where the model files and ExifTool live.
//!
//! One lookup for every reader, so a developer checkout, the owner's existing
//! `~/.conrod` and an installed release all resolve the same way. In order:
//! the data directory's `models/` (Python-compatible, and what a user can
//! override), the `resources/` folder shipped beside the executable, then the
//! caches the Python libraries leave behind for the two plate models.

use crate::settings::data_root;
use std::path::PathBuf;

pub const DETECTOR: &str = "yolo11s-960.onnx";
pub const FACES: &str = "face_detection_yunet_2023mar.onnx";
pub const SIMILARITY: &str = "dinov2-small-quantized.onnx";
pub const PLATE_DETECTOR: &str = "yolo-v9-t-640-license-plates-end2end.onnx";
pub const PLATE_READER: &str = "global_mobile_vit_v2_ocr.onnx";
/// The OCR pair is found by its detector; the recogniser sits beside it.
pub const OCR_DETECTOR: &str = "ch_PP-OCRv4_det_infer.onnx";
pub const OCR_RECOGNISER: &str = "ch_PP-OCRv4_rec_infer.onnx";
const OCR_DETECTOR_V3: &str = "ch_PP-OCRv3_det_infer.onnx";

/// The plate models' homes in the Python libraries' caches, under `~/.cache`.
const LIBRARY_CACHES: [(&str, &str); 2] = [
    (
        PLATE_DETECTOR,
        "open-image-models/yolo-v9-t-640-license-plate-end2end",
    ),
    (
        PLATE_READER,
        "fast-plate-ocr/global-plates-mobile-vit-v2-model",
    ),
];

/// `<folder of the executable>/resources`, where a release keeps its assets.
pub fn resources() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("resources"))
}

fn user_home() -> PathBuf {
    PathBuf::from(
        std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .unwrap_or_default(),
    )
}

/// Folders that may hold `name`, best first.
fn candidates(name: &str) -> Vec<PathBuf> {
    let mut dirs = vec![data_root().join("models")];
    dirs.extend(resources().map(|r| r.join("models")));
    if let Some((_, cache)) = LIBRARY_CACHES.iter().find(|(n, _)| *n == name) {
        dirs.push(user_home().join(".cache").join(cache));
    }
    dirs
}

/// The full path of a model file, if it exists anywhere we look.
pub fn find(name: &str) -> Option<PathBuf> {
    candidates(name)
        .into_iter()
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// The folder holding the OCR models (PP-OCRv4, else the older v3), which the
/// OCR engine wants as a directory.
pub fn ocr_dir() -> Option<PathBuf> {
    [OCR_DETECTOR, OCR_DETECTOR_V3]
        .into_iter()
        .find_map(|name| find(name).and_then(|p| p.parent().map(PathBuf::from)))
}

/// Where a model should be looked for or reported missing: its real path if it
/// exists, else the data directory's (which is where a user would put it).
pub fn expected(name: &str) -> PathBuf {
    find(name).unwrap_or_else(|| data_root().join("models").join(name))
}

/// ExifTool this install carries or has fetched: `resources/exiftool` beside the
/// executable, else `<data>/tools/exiftool` (else it is found on PATH).
pub fn exiftool() -> Option<PathBuf> {
    resources()
        .map(|r| r.join("exiftool"))
        .into_iter()
        .chain([data_root().join("tools").join("exiftool")])
        .flat_map(|dir| ["exiftool.exe", "exiftool(-k).exe"].map(|n| dir.join(n)))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_directory_wins_and_a_missing_model_is_reported_there() {
        let dir = std::env::temp_dir().join(format!("conrod-models-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("models")).unwrap();
        std::fs::write(dir.join("models").join(DETECTOR), b"x").unwrap();
        std::env::set_var("CONROD_HOME", &dir);
        assert_eq!(find(DETECTOR), Some(dir.join("models").join(DETECTOR)));
        assert_eq!(find("nope.onnx"), None);
        assert_eq!(expected("nope.onnx"), dir.join("models").join("nope.onnx"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
