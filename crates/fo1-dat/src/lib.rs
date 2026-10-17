//! DAT archive container.
//!
//! A two level tree. A 16 byte root header, then one length-prefixed directory
//! name per directory, then one 16 byte block header per directory followed by
//! that directory's entries. Nothing separates the blocks. Paths use
//! backslashes and are uppercased before lookup.
//!
//! Tree order is **not** physical order. In MASTER.DAT the first tree entry has
//! offset 211,060,669 while the smallest offset is 556,484, so an offset must
//! never be inferred from an entry's position in the tree.
//!
//! See `docs/spec/dat.md`.

#![deny(clippy::arithmetic_side_effects)]

/// Nothing is implemented yet. The spec and the gate tests come first.
///
/// Every raw entry has a compressed size of 0. Every block-chain entry has a
/// nonzero one. The split is asserted by the archive gate.
pub const ROOT_HEADER_SIZE: usize = 16;
