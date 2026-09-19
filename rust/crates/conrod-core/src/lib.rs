//! Conrod's pure logic.
//!
//! Everything here is deterministic and free of IO, so it can be checked
//! against the Python implementation's recorded behaviour (see
//! `rust/fixtures` and `tools/gen_golden.py`) rather than against a reading of
//! its source.

pub mod framing;
pub mod ridge;
