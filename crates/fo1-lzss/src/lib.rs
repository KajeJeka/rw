//! Total LZSS decoder.
//!
//! One 4096 byte ring, write index starting at 4078, one control byte per
//! eight tokens read from bit 0 upwards, back references of two bytes with the
//! length in the low nibble of the second byte (3 to 18).
//!
//! The decoder receives the declared output size **only** to detect an
//! overrun. It never cuts, pads or defaults to reach a length, because a
//! decoder that stops at the declared size passes a size gate for any input
//! (guide: No truncation, and a consumption check).
//!
//! See `docs/spec/lzss.md`.

#![deny(clippy::arithmetic_side_effects)]

/// Nothing is implemented yet. The spec and the gate tests come first.
///
/// Every LZSS block inside a DAT block chain starts with a fresh ring buffer.
pub const RING_WRITE_START: usize = 4078;
