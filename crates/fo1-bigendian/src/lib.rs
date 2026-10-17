//! Total reader for the big-endian **asset** byte order layer.
//!
//! The DAT container, `.PRO`, `.FRM`, `.MAP`, `.GCD`, `.AAF` and `.INT` are
//! big-endian. This crate is the only route by which any crate may read an
//! integer in that layer (guide: Asset formats and loading).
//!
//! A wrong endian read returns plausible numbers and raises no error, so the
//! reader type has **no default order** and no implicit conversion to or from
//! the little-endian layer. See `docs/spec/endian.md`.

#![deny(clippy::arithmetic_side_effects)]

/// Nothing is implemented yet. The spec and the gate tests come first.
pub const ORDER: &str = "big-endian";
