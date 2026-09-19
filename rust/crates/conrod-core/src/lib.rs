//! Conrod's pure logic.
//!
//! Everything here is deterministic and free of IO, so it can be checked
//! against the Python implementation's recorded behaviour (see
//! `rust/fixtures` and `tools/gen_golden.py`) rather than against a reading of
//! its source.

pub mod analysis;
pub mod bursts;
pub mod culling;
pub mod framing;
pub mod keywords;
pub mod mapping;
pub mod marques;
pub mod normalise;
pub mod py;
pub mod registry;
pub mod ridge;
