//! The data tables compiled into `falloutw.exe`, read through the PE section
//! headers and the little-endian executable layer.
//!
//! The stat limits, the critical hit tables and the XP table live in the
//! executable, not in the game data files. They are read from the player's own
//! copy at run time, or from a local cache under `target/`. They are never
//! copied out of the executable into this repository.
//!
//! Three adjacency identities catch a wrong address or a wrong record size.
//! See `docs/spec/exe-tables.md`.

#![deny(clippy::arithmetic_side_effects)]

/// Nothing is implemented yet. The spec and the gate tests come first.
///
/// The reader refuses any build whose SHA-256 differs from this one.
pub const REFERENCE_BUILD_SHA256: &str =
    "444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272";
