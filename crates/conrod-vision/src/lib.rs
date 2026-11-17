//! Conrod's pixel work: Pillow- and OpenCV-exact image operations, the
//! detector, and the sharpness measure. Checked against recorded snapshots
//! in `tests/`.

pub mod colour;
pub mod detect;
pub mod edgeblur;
pub mod exposure;
pub mod expression;
pub mod faces;
pub mod framing;
pub mod imageops;
pub mod level;
pub mod look;
pub mod ocr;
pub mod plates;
pub mod quality;
pub mod sharpness;
pub mod similarity;
pub mod taste;
pub mod streaks;
