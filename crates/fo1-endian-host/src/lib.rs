//! Total reader for the little-endian **executable** byte order layer.
//!
//! The data tables inside `falloutw.exe` are little-endian while the asset
//! layer is big-endian. Reading the XP table big-endian returns 16,777,216 for
//! level 1 instead of 1, which is why the two layers have separate crates and
//! separate reader types (guide: correction C1, Asset formats and loading).
//!
//! See `docs/spec/endian.md`.

#![deny(clippy::arithmetic_side_effects)]

/// Nothing is implemented yet. The spec and the gate tests come first.
pub const ORDER: &str = "little-endian";
