//! Total reader for the little-endian **executable** byte order layer.
//!
//! Spec: `docs/spec/endian.md`. Claims END-001, END-004 to END-008,
//! END-010 to END-024, END-031.
//! Guide: correction C1, section "Asset formats and loading", section
//! "Data compiled into the executable".
//!
//! This is the only route by which any crate may read a table compiled into
//! `falloutw.exe`. Reading the XP thresholds big-endian gives 16,777,216 for
//! level 1 instead of 1 (END-006), and no read raises an error when it does
//! (END-008). That is why this crate is a separate copy of the asset layer's
//! reader and not a shared one.

#![deny(clippy::arithmetic_side_effects)]

/// Width of one `u8` read, in bytes (`END-014`, `END-C001`).
pub const WIDTH_U8: usize = 1;
/// Width of one `u16` and one `i16` read, in bytes (`END-014`, `END-C002`).
pub const WIDTH_U16: usize = 2;
/// Width of one `u32` and one `i32` read, in bytes (`END-014`, `END-C003`).
pub const WIDTH_U32: usize = 4;

/// Field label carried by a failed `u8` read (`END-022`).
pub const FIELD_U8: &str = "u8";
/// Field label carried by a failed `u16` read (`END-022`).
pub const FIELD_U16: &str = "u16";
/// Field label carried by a failed `u32` read (`END-022`).
pub const FIELD_U32: &str = "u32";
/// Field label carried by a failed `i16` read (`END-022`).
pub const FIELD_I16: &str = "i16";
/// Field label carried by a failed `i32` read (`END-022`).
pub const FIELD_I32: &str = "i32";
/// Field label carried by a failed `bytes` read (`END-022`).
pub const FIELD_BYTES: &str = "bytes";
/// Field label carried by a failed `skip` (`END-022`).
pub const FIELD_SKIP: &str = "skip";
/// Field label carried by a failed `seek` (`END-022`).
pub const FIELD_SEEK: &str = "seek";

/// Why a little-endian read of the executable layer stopped.
///
/// Every variant carries the byte offset at which the operation started, the
/// static label of the operation, and the origin label given to `new`. No
/// variant carries any part of the executable, and the type has no lifetime,
/// so it cannot hold a borrow of the table either (END-023). The type is
/// distinct from `BigEndianError`, so an asset-layer failure cannot cross into
/// this layer (END-024).
///
/// The set is closed and every match on it is exhaustive with no wildcard arm,
/// so adding a variant fails the build until each site has decided about it
/// (END-021).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LittleEndianError {
    /// The operation wanted `needed` bytes at `offset` and only `available`
    /// were left. Nothing was consumed (END-017, END-018).
    Truncated {
        /// Cursor position where the failed operation started.
        offset: usize,
        /// Static label of the operation, such as `u32` or `bytes`.
        field: &'static str,
        /// Origin label handed to `LittleEndian::new`.
        origin: &'static str,
        /// Bytes the operation asked for.
        needed: usize,
        /// Bytes that were actually left in the buffer.
        available: usize,
    },
    /// `seek` named a position past the end of the buffer. Nothing moved
    /// (END-019).
    SeekOutOfRange {
        /// Cursor position before the rejected `seek`.
        offset: usize,
        /// Static label of the operation, always `seek`.
        field: &'static str,
        /// Origin label handed to `LittleEndian::new`.
        origin: &'static str,
        /// Position the caller asked for.
        requested: usize,
        /// Total length of the buffer.
        length: usize,
    },
}

impl std::fmt::Display for LittleEndianError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Exhaustive with no wildcard arm, so a third variant breaks this build
        // (END-021). The text quotes only the labels and the counts, never a
        // byte of the table (END-023).
        match *self {
            Self::Truncated {
                offset,
                field,
                origin,
                needed,
                available,
            } => write!(
                formatter,
                "{origin}: {field} at byte {offset} wanted {needed} bytes but only \
                 {available} remain"
            ),
            Self::SeekOutOfRange {
                offset,
                field,
                origin,
                requested,
                length,
            } => write!(
                formatter,
                "{origin}: {field} from byte {offset} asked for {requested}, past a \
                 buffer of {length} bytes"
            ),
        }
    }
}

impl std::error::Error for LittleEndianError {}

/// A little-endian cursor over one borrowed buffer.
///
/// END-013: the struct has no order field, and the order is not reachable from
/// any value. Holding a `LittleEndian` means little-endian.
pub struct LittleEndian<'a> {
    bytes: &'a [u8],
    offset: usize,
    origin: &'static str,
}

impl<'a> LittleEndian<'a> {
    /// Start a cursor at offset 0 over `bytes`. `origin` names where the buffer
    /// came from, such as `falloutw.exe:0x5082D0`, and is the label every later
    /// error carries (END-022). The port save format passes its own label here
    /// so its failures stay separable from the original executable's (END-031).
    pub fn new(bytes: &'a [u8], origin: &'static str) -> Self {
        Self {
            bytes,
            origin,
            offset: 0,
        }
    }

    /// Bytes left after the cursor. This is the quantity END-017 compares a
    /// requested width against before anything is consumed.
    pub fn remaining(&self) -> usize {
        // Exact, not a guess: the cursor never passes the end, so the
        // subtraction can never underflow and saturating is a safety net that
        // keeps the crate free of arithmetic that can panic.
        self.bytes.len().saturating_sub(self.offset)
    }

    /// The cursor's byte position.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Borrow `n` bytes and advance past them. Every read and every `skip` goes
    /// through here, so this is the only place the end of the input can be
    /// reached. The length check happens before anything is consumed and the
    /// cursor only moves on success, which is what makes a short read
    /// impossible and a failed read inert (END-018).
    fn take(&mut self, n: usize, field: &'static str) -> Result<&'a [u8], LittleEndianError> {
        let available = self.remaining();
        if n > available {
            return Err(LittleEndianError::Truncated {
                offset: self.offset,
                field,
                origin: self.origin,
                needed: n,
                available,
            });
        }
        let start = self.offset;
        // Checked rather than `+`: the guard above already proves this cannot
        // overflow, and the workspace denies unchecked arithmetic.
        let end = match start.checked_add(n) {
            Some(end) => end,
            None => {
                return Err(LittleEndianError::Truncated {
                    offset: start,
                    field,
                    origin: self.origin,
                    needed: n,
                    available,
                })
            }
        };
        // `get` rather than an index expression, so the crate holds no slicing
        // index even though the bounds are already proven.
        let taken = match self.bytes.get(start..end) {
            Some(slice) => slice,
            None => {
                return Err(LittleEndianError::Truncated {
                    offset: start,
                    field,
                    origin: self.origin,
                    needed: n,
                    available,
                })
            }
        };
        self.offset = end;
        Ok(taken)
    }

    /// Read one byte and advance. END-014: one byte, and one byte of the cursor.
    pub fn u8(&mut self) -> Result<u8, LittleEndianError> {
        let taken = self.take(WIDTH_U8, FIELD_U8)?;
        // Folding one byte needs no index and no unwrap, and `take` already
        // refused any width it could not satisfy, so no second error arm is
        // needed here.
        let mut byte = 0_u8;
        for value in taken {
            byte = *value;
        }
        Ok(byte)
    }

    /// Read two bytes, least significant first, and advance. END-015: the fold.
    pub fn u16(&mut self) -> Result<u16, LittleEndianError> {
        let taken = self.take(WIDTH_U16, FIELD_U16)?;
        let mut word = 0_u16;
        // Walking the borrowed slice backwards makes the last byte read the most
        // significant, which is what little endian means (END-015). The shape
        // matches the asset layer's reader on purpose: the two must stay easy
        // to read side by side.
        for byte in taken.iter().rev() {
            word = (word << 8) | u16::from(*byte);
        }
        Ok(word)
    }

    /// Read four bytes, least significant first, and advance. END-015: the fold.
    pub fn u32(&mut self) -> Result<u32, LittleEndianError> {
        let taken = self.take(WIDTH_U32, FIELD_U32)?;
        let mut word = 0_u32;
        for byte in taken.iter().rev() {
            word = (word << 8) | u32::from(*byte);
        }
        Ok(word)
    }

    /// Read two bytes, least significant first, reinterpret them as a two's
    /// complement value and advance (END-016).
    pub fn i16(&mut self) -> Result<i16, LittleEndianError> {
        let taken = self.take(WIDTH_U16, FIELD_I16)?;
        let mut word = 0_u16;
        for byte in taken.iter().rev() {
            word = (word << 8) | u16::from(*byte);
        }
        // Hand the same bits straight back at the same width so the value is
        // reinterpreted rather than converted. Four all-ones bytes are -1
        // here, not 4,294,967,295 (END-016).
        Ok(i16::from_le_bytes(word.to_le_bytes()))
    }

    /// Read four bytes, least significant first, reinterpret them as a two's
    /// complement value and advance (END-016).
    pub fn i32(&mut self) -> Result<i32, LittleEndianError> {
        let taken = self.take(WIDTH_U32, FIELD_I32)?;
        let mut word = 0_u32;
        for byte in taken.iter().rev() {
            word = (word << 8) | u32::from(*byte);
        }
        Ok(i32::from_le_bytes(word.to_le_bytes()))
    }

    /// Borrow the next `n` bytes and advance past them. Nothing is copied, so
    /// this is how a caller reads a table's raw bytes without copying them
    /// into an error (END-020).
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], LittleEndianError> {
        self.take(n, FIELD_BYTES)
    }

    /// Advance past `n` bytes without reading them. The bound is the same one
    /// `bytes` uses; a skip past the end is an error, not a clamped move
    /// (END-018). `skip` only advances; `seek` is the only way back (END-019).
    pub fn skip(&mut self, n: usize) -> Result<(), LittleEndianError> {
        self.take(n, FIELD_SKIP).map(|_| ())
    }

    /// Move the cursor to `off`. Any position from 0 through the buffer length
    /// is accepted, including the very end, where `remaining` becomes 0.
    /// Anything larger is an error and leaves the cursor where it was (END-019).
    pub fn seek(&mut self, off: usize) -> Result<(), LittleEndianError> {
        if off > self.bytes.len() {
            return Err(LittleEndianError::SeekOutOfRange {
                offset: self.offset,
                field: FIELD_SEEK,
                origin: self.origin,
                requested: off,
                length: self.bytes.len(),
            });
        }
        self.offset = off;
        Ok(())
    }
}
