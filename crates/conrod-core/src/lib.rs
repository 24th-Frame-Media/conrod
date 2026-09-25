//! Conrod's pure logic.
//!
//! Everything here is deterministic and free of IO, so it can be checked
//! against recorded snapshots (see `fixtures` and the `regression` test)
//! rather than against a reading of its source.

pub mod analysis;
pub mod bursts;
pub mod culling;
pub mod framing;
pub mod grouping;
pub mod keywords;
pub mod mapping;
pub mod marques;
pub mod models;
pub mod normalise;
pub mod profile;
pub mod registry;
pub mod ridge;
pub mod settings;
pub mod tasks;
pub mod text;
