# Byte order layers — SPEC

Provenance: written from three sources only. (1) The repository file
`RUSTY WASTELAND Engine Implementation Guide, Revision 2.md`, specifically its
section *Asset formats and loading*, its correction **C1**, its section *Data
compiled into the executable*, its section *Saves*, its table of byte order
exceptions, its engineering-philosophy list and its anti-deviation table.
(2) Bytes of the reference executable `falloutw.exe`, whose SHA-256 is
`444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272`, read on
2026-10-17 for the specific words named below. (3) Bytes of `MASTER.DAT` from
the same install, read for its root header. Nothing else was consulted. No
Community Edition source was opened, quoted or paraphrased. Byte values and
offsets below are stated as measurements; no disassembly is reproduced.

Companion ledger: `docs/spec/endian.constants.toml`.

## How to read the evidence tags here

The four tags are used exactly as the guide defines them. Two notes for the
readers of this file.

- A tag names **what a claim rests on**. Where the guide records a check
  against the executable or against real game data, and I did not re-read the
  byte myself, the claim says so in its own body. Where I read the byte, the
  claim names the address and the bytes.
- Claims whose first words are **Mandate, not a witness** are construction rules
  the guide issues. The original engine had no Rust error type and no unit test,
  so no byte in the shipped files can witness them. They are tagged UNVERIFIED
  because the tag records the missing witness, not doubt about the rule. The
  normative text is unconditional; the test author implements them.

## Claims

### The two layers

| ID | Claim | Evidence |
| --- | --- | --- |
| END-001 | Two byte order layers exist. The asset layer is big-endian. The executable layer is little-endian. They are two rules, not one rule with exceptions. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-002 | The DAT container is big-endian. Its 16-byte root header is four big-endian u32: directory count, a capacity to ignore, a size field that is 0 at the root, and a dead pointer. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-003 | The asset layer covers `.PRO`, `.FRM`, `.MAP`, `.GCD`, `.AAF` and `.INT`. All are big-endian. | CONFIRMED IN COMMUNITY EDITION ONLY |
| END-004 | The executable layer holds the hit location penalty, the critter and player critical hit tables, the stat limits, the player stat limits and the XP thresholds table. Every i32 in them is little-endian. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-005 | The XP thresholds table is 21 little-endian i32 values at virtual address `0x5082D0` of the reference build, reached through the PE section headers. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-006 | The XP value for level 1 is **1**. Its four bytes are `01 00 00 00`. Read little-endian that is 1. Read big-endian that is **16,777,216**. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-007 | The XP values are the unscaled triangular numbers L(L+1)/2 for L = 0 to 20, indexed by level. The caller scales by 1,000. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-008 | A wrong-order read never errors and returns a plausible number. Every entry of every executable table decodes in both orders without complaint; only the magnitude changes. No reader can detect its own misuse from the data it is reading. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-009 | No crate outside `fo1-bigendian` and `fo1-endian-host` may read an integer by any other route. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-033 | `.INT` files are big-endian. The older note that placed them in the little-endian layer is discarded and must not be implemented. | VERSION DIFFERENCE IDENTIFIED |
| END-032 | Revision 1's single rule, that every integer is big-endian, is superseded. Correction C1 replaces it with the two-layer rule above. | VERSION DIFFERENCE IDENTIFIED |

### Why the two layers must not share code

Every claim in this block is forced by END-008. If a wrong-order read can be
silent, nothing inside the read path can catch it, so the only place the rule
can live is the type.

| ID | Claim | Evidence |
| --- | --- | --- |
| END-010 | The two readers live in two crates because a wrong-endian read fails silently. `fo1-bigendian` serves the asset layer, `fo1-endian-host` serves the executable layer. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-011 | The two reader types are written out twice, not shared. No function, macro, generic or private helper is common to both crates. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-012 | The reader types are not genericised over an order parameter, and there is no conversion, cast or transcode from one to the other. A `BigEndian` value cannot produce a `LittleEndian` value or the reverse. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-013 | Neither type has a default order. No constructor argument, struct field, public constant, trait or type parameter selects one. The order is a property of the type the caller already chose. | CONFIRMED AGAINST ORIGINAL BINARY |

END-011 and END-012 are the load-bearing pair. A single generic reader with an
`order` parameter would compile at every call site and would be correct at
whichever call sites the author was careful about. Two duplicated types cannot
express "the wrong one": `fo1-dat` simply has no `LittleEndian` in scope.

### Widths and sign

| ID | Claim | Evidence |
| --- | --- | --- |
| END-014 | Widths are fixed and named: `u8` consumes 1 byte, `u16` and `i16` consume 2, `u32` and `i32` consume 4. A successful read advances the cursor by exactly that width. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-015 | Big-endian folds the most significant byte first. Little-endian folds the least significant byte first. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-016 | `i16` and `i32` reinterpret the folded bits as two's complement rather than converting them. Four all-ones bytes are -1 in the executable layer, not 4,294,967,295. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-024 | The two error enums are distinct types. An asset-layer error cannot be returned by an executable-layer reader. | CONFIRMED AGAINST ORIGINAL BINARY |

### Bounds and totality

| ID | Claim | Evidence |
| --- | --- | --- |
| END-017 | Mandate, not a witness. Every read and every skip compares the width it wants against the bytes left before it consumes anything. | UNVERIFIED |
| END-018 | Mandate, not a witness. A read or skip that would pass the end is an error. There is no short read, no zero fill, no padding and no truncation. A failed read leaves the cursor where it was. | UNVERIFIED |
| END-019 | Mandate, not a witness. `seek` accepts any position from 0 through the buffer length inclusive, and rejects anything larger. `skip` only advances; `seek` is the only way back. | UNVERIFIED |
| END-020 | Mandate, not a witness. `bytes(n)` hands back a borrowed view of the caller's own buffer. It copies nothing and allocates nothing, so it cannot be a place where content is copied into an error. | UNVERIFIED |

### Errors

| ID | Claim | Evidence |
| --- | --- | --- |
| END-021 | Mandate, not a witness. One error enum per crate, a closed set, matched exhaustively with no wildcard arm so that adding a variant fails the build until every site decides. | UNVERIFIED |
| END-022 | Mandate, not a witness. Every variant carries three things: the byte offset at which the failed operation started, a static field label, and the origin label handed to `new`. | UNVERIFIED |
| END-023 | Mandate, not a witness. No variant carries file content. The error type has no lifetime parameter, so it cannot hold a borrow of the buffer, and it holds no `String`, no `Vec` and no owned byte slice. | UNVERIFIED |

### Byte order exceptions

The layer is the default for a format. Each row below is the exception to it.

| ID | Format | Claim | Evidence |
| --- | --- | --- | --- |
| END-025 | `.ACM` | The whole 14-byte header is little-endian: magic `97 28 03 01`, then sample count u32, channel count u16, sample rate u16, then one u16 packing rows in the high 12 bits and levels in the low 4. The guide records 22,050 as the sample rate in all 1,746 raw files and the packed words 263 and 264 giving levels 7 and 8 with 16 rows, so the field order is corroborated by real data; the order itself follows two public format references. Queue a byte check against one raw `.ACM` before the audio crate closes. | CONFIRMED IN COMMUNITY EDITION ONLY |
| END-026 | `.MVE` | The first 18 bytes are the text `Interplay MVE File` and every field after them is little-endian. The guide records that the format is publicly documented and that no Community Edition parser exists to follow. Queue a byte check against one of the 13 shipped movies. | CONFIRMED IN COMMUNITY EDITION ONLY |
| END-027 | `.LIP` | The trailing marker table is little-endian. The guide saw this in the bytes and did not confirm it, and separately records the `.LIP` layout as open with the community reader diverging at offset `0x14`. Isolate it; do not read a shipped `.LIP` until the layout is settled. | UNVERIFIED |
| END-028 | Save files | The 30,051-byte save header is big-endian. The guide records that the binary confirms the constants 18, 24, 30, 128 and 29,792 and that the remaining field offsets come from community material and sum to the same total. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-029 | Save block 20 | The world map block is 1,028 bytes copied raw and never routed through the integer reader: a 31 by 29 grid (899 bytes), a 15 by 7 grid (105 bytes) and six ints (24 bytes). 899 + 105 + 24 closes on 1,028 exactly. These are the raw x86 memory bytes of the original host. | CONFIRMED AGAINST ORIGINAL BINARY |
| END-030 | Save block 23 | The skill use block is 216 bytes and its element type is not established. The guide flags that an 18 by 3 byte array read as raw little-endian would need 216 bytes from a 54 byte array, and directs the reader to the function at `0x499520` in the executable before choosing an element width. Do not pick one from this spec. | UNVERIFIED |
| END-031 | Port save format | The port format is our own file, is little-endian, and therefore reads through `fo1-endian-host` with its own origin label so a port-format failure can never be mistaken for a failure inside the original executable. | CONFIRMED IN COMMUNITY EDITION ONLY |

### The asset layer anti-deviation witness

| ID | Claim | Evidence |
| --- | --- | --- |
| END-035 | The DAT root header read little-endian yields 1,090,519,040 for the directory count, against the 65 the file actually holds. The asset layer is established by that magnitude mismatch, not by an absence of error, and it is the counterpart of END-006. | CONFIRMED AGAINST ORIGINAL BINARY |

## The two layers are different facts, proved differently

END-006 is the executable-layer witness. I read the four bytes for level 1 at
virtual address `0x5082D4` in the reference build. They are `01 00 00 00`.
Folded least significant first the value is 1; folded most significant first it
is 16,777,216. Neither fold produces an error and both produce an integer, which
is precisely the hazard END-008 describes. A test that reads this table with the
asset-layer reader and asserts 16,777,216 fails the moment someone "fixes" the
reader to try both orders and return the smaller value.

Three further readings support the same layer, and the test author may use them
as independent oracles:

- The 21 words at `0x5082D0` fold little-endian to 0, 1, 3, 6, 10, 15, 21, 28,
  36, 45, 55, 66, 78, 91, 105, 120, 136, 153, 171, 190, 210, which are L(L+1)/2
  for L = 0 to 20. The guide's formula and the bytes agree.
- The critter critical record at `0x4FECB4` carries the bytes `89 13 00 00` and
  `88 13 00 00` in its two message id fields. Little-endian that is 5,001 and
  5,000. Big-endian the same bytes are 2,304,432,896 and 2,282,357,760, which no
  message id is.
- The stat limits record at `0x507EC8` folds little-endian to 0, 0, 0, 1, 10, 5.
  Field 0 is the stat index, fields 1 and 2 are not understood, and fields 3, 4
  and 5 are the minimum, maximum and default.

END-035 is the asset-layer witness, and it is the same argument pointing the
other way. The first 16 bytes of `MASTER.DAT` are
`00 00 00 41 00 00 00 5E 00 00 00 00 02 02 15 90`. Folded big-endian the four
words are 65, 94, 0 and 33,691,024: a directory count of 65, a capacity, the
size field at the root holding 0, and a dead pointer. The guide records 65
directories for MASTER.DAT, so the big-endian fold matches the data. Folded
little-endian the first word is 1,090,519,040, which no archive holds.

## Why two crates and two types

A wrong-order read is the one class of defect in this engine that no runtime
check can catch. The bytes are the same bytes; the reader returns a `u32`; the
caller adds it to another `u32`. Nothing raises. The failure surfaces much later
as a sprite that is the wrong height or an experience threshold that is never
crossed, far from the code that read it.

Three properties follow, and all three are claims above.

The layers get separate crates (END-010) so that `cargo metadata` states which
layer a crate reads from. `tools/check_layers.sh` already places both at band 0.

The types are duplicated rather than shared (END-011). A shared helper is a
shared code path, and a shared code path is where an order parameter would
eventually appear.

The types are not genericised over an order (END-012, END-013). A parameter is a
value, and a value can be computed, defaulted, passed down through a helper and
reached by the wrong caller. A type cannot. `fo1-dat` cannot read a table from
`falloutw.exe` even by accident, because `LittleEndian` is not a name it has.

The duplication is deliberate and it is the only place in this workspace where
the same twenty lines appear twice. END-008 is the reason.

## Constraints on the implementation

These are not claims and carry no ids. They come from the guide's engineering
philosophy and from the workspace lint table, and the build enforces them.

- No `unsafe`. The workspace forbids it and both crates opt in.
- No `unwrap`, no `expect`, no `panic`, no indexing slice expression. A reader
  that parses bytes returns `Result`.
- No `as` cast to change width or sign anywhere on a read path.
  END-016 makes the standard library sufficient: fold into a `u32`, then hand the
  same bits back through `to_*_bytes` and `from_*_bytes`.
- `arithmetic_side_effects` is denied in both crates. Cursor arithmetic uses
  `checked_add` and `saturating_sub`.
- Both error types implement `Display` and `std::error::Error` so a caller can
  propagate without unwrapping.
- Public items carry a doc comment that names the claim they implement.
- Both crates use the standard library only.

## Interface — `crates/fo1-bigendian`

```rust
//! Total reader for the big-endian **asset** byte order layer.
//!
//! Spec: `docs/spec/endian.md`. Claims END-001, END-010 to END-024.
//! Guide: section "Asset formats and loading", correction C1.
//!
//! This is the only route by which any crate may read an integer in the asset
//! layer. There is no default order and no way to select one at run time,
//! because a wrong-order read returns plausible numbers and raises no error
//! (END-008), so nothing inside this crate could catch the mistake (END-013).

/// Width of one `u8` read, in bytes.
pub const WIDTH_U8: usize = 1;
/// Width of one `u16` and one `i16` read, in bytes.
pub const WIDTH_U16: usize = 2;
/// Width of one `u32` and one `i32` read, in bytes.
pub const WIDTH_U32: usize = 4;

/// Field label carried by a failed `u8` read.
pub const FIELD_U8: &'static str = "u8";
/// Field label carried by a failed `u16` read.
pub const FIELD_U16: &'static str = "u16";
/// Field label carried by a failed `u32` read.
pub const FIELD_U32: &'static str = "u32";
/// Field label carried by a failed `i16` read.
pub const FIELD_I16: &'static str = "i16";
/// Field label carried by a failed `i32` read.
pub const FIELD_I32: &'static str = "i32";
/// Field label carried by a failed `bytes` read.
pub const FIELD_BYTES: &'static str = "bytes";
/// Field label carried by a failed `skip`.
pub const FIELD_SKIP: &'static str = "skip";
/// Field label carried by a failed `seek`.
pub const FIELD_SEEK: &'static str = "seek";

/// Why a big-endian read of the asset layer stopped.
///
/// Every variant carries the byte offset at which the operation started, the
/// static label of the operation, and the origin label given to `new`. No
/// variant carries any part of the file, and the type has no lifetime, so it
/// cannot hold a borrow of the buffer either (END-023).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BigEndianError {
    /// The operation wanted `needed` bytes at `offset` and only `available`
    /// were left. Nothing was consumed.
    Truncated {
        /// Cursor position where the failed operation started.
        offset: usize,
        /// Static label of the operation, such as `u32` or `bytes`.
        field: &'static str,
        /// Origin label handed to `BigEndian::new`.
        origin: &'static str,
        /// Bytes the operation asked for.
        needed: usize,
        /// Bytes that were actually left in the buffer.
        available: usize,
    },
    /// `seek` named a position past the end of the buffer. Nothing moved.
    SeekOutOfRange {
        /// Cursor position before the rejected `seek`.
        offset: usize,
        /// Static label of the operation, always `seek`.
        field: &'static str,
        /// Origin label handed to `BigEndian::new`.
        origin: &'static str,
        /// Position the caller asked for.
        requested: usize,
        /// Total length of the buffer.
        length: usize,
    },
}

/// A big-endian cursor over one borrowed buffer.
///
/// END-013: the struct has no order field, and the order is not reachable from
/// any value. Holding a `BigEndian` means big-endian.
pub struct BigEndian<'a> {
    bytes: &'a [u8],
    offset: usize,
    origin: &'static str,
}

impl<'a> BigEndian<'a> {
    /// Start a cursor at offset 0 over `bytes`. `origin` names where the buffer
    /// came from, such as `MASTER.DAT` or `falloutw.exe:0x5082D0`, and is the
    /// label every later error carries (END-022).
    pub fn new(bytes: &'a [u8], origin: &'static str) -> Self {
        Self { bytes, origin, offset: 0 }
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
    fn take(&mut self, n: usize, field: &'static str) -> Result<&'a [u8], BigEndianError> {
        let available = self.remaining();
        if n > available {
            return Err(BigEndianError::Truncated {
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
                return Err(BigEndianError::Truncated {
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
                return Err(BigEndianError::Truncated {
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

    /// Read one byte and advance.
    pub fn u8(&mut self) -> Result<u8, BigEndianError> {
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

    /// Read two bytes, most significant first, and advance.
    pub fn u16(&mut self) -> Result<u16, BigEndianError> {
        let taken = self.take(WIDTH_U16, FIELD_U16)?;
        let mut word = 0_u16;
        for byte in taken {
            // Each byte outranks every byte already folded in, which is what
            // big endian means (END-015).
            word = (word << 8) | u16::from(*byte);
        }
        Ok(word)
    }

    /// Read four bytes, most significant first, and advance.
    pub fn u32(&mut self) -> Result<u32, BigEndianError> {
        let taken = self.take(WIDTH_U32, FIELD_U32)?;
        let mut word = 0_u32;
        for byte in taken {
            word = (word << 8) | u32::from(*byte);
        }
        Ok(word)
    }

    /// Read two bytes, most significant first, reinterpret them as a two's
    /// complement value and advance.
    pub fn i16(&mut self) -> Result<i16, BigEndianError> {
        let taken = self.take(WIDTH_U16, FIELD_I16)?;
        let mut word = 0_u16;
        for byte in taken {
            word = (word << 8) | u16::from(*byte);
        }
        // Hand the same bits straight back at the same width so the value is
        // reinterpreted rather than converted. This is why no `as` appears
        // anywhere on the read path (END-016).
        Ok(i16::from_be_bytes(word.to_be_bytes()))
    }

    /// Read four bytes, most significant first, reinterpret them as a two's
    /// complement value and advance.
    pub fn i32(&mut self) -> Result<i32, BigEndianError> {
        let taken = self.take(WIDTH_U32, FIELD_I32)?;
        let mut word = 0_u32;
        for byte in taken {
            word = (word << 8) | u32::from(*byte);
        }
        Ok(i32::from_be_bytes(word.to_be_bytes()))
    }

    /// Borrow the next `n` bytes and advance past them. Nothing is copied, so
    /// this is how a caller reads a name, a magic or a raw block (END-020).
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], BigEndianError> {
        self.take(n, FIELD_BYTES)
    }

    /// Advance past `n` bytes without reading them. The bound is the same one
    /// `bytes` uses; a skip past the end is an error, not a clamped move.
    pub fn skip(&mut self, n: usize) -> Result<(), BigEndianError> {
        self.take(n, FIELD_SKIP).map(|_| ())
    }

    /// Move the cursor to `off`. Any position from 0 through the buffer length
    /// is accepted, including the very end, where `remaining` becomes 0.
    /// Anything larger is an error and leaves the cursor where it was (END-019).
    pub fn seek(&mut self, off: usize) -> Result<(), BigEndianError> {
        if off > self.bytes.len() {
            return Err(BigEndianError::SeekOutOfRange {
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
```

## Interface — `crates/fo1-endian-host`

This block is deliberately a second copy rather than a shared implementation.
See END-011 and END-012 for why.

```rust
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

/// Width of one `u8` read, in bytes.
pub const WIDTH_U8: usize = 1;
/// Width of one `u16` and one `i16` read, in bytes.
pub const WIDTH_U16: usize = 2;
/// Width of one `u32` and one `i32` read, in bytes.
pub const WIDTH_U32: usize = 4;

/// Field label carried by a failed `u8` read.
pub const FIELD_U8: &'static str = "u8";
/// Field label carried by a failed `u16` read.
pub const FIELD_U16: &'static str = "u16";
/// Field label carried by a failed `u32` read.
pub const FIELD_U32: &'static str = "u32";
/// Field label carried by a failed `i16` read.
pub const FIELD_I16: &'static str = "i16";
/// Field label carried by a failed `i32` read.
pub const FIELD_I32: &'static str = "i32";
/// Field label carried by a failed `bytes` read.
pub const FIELD_BYTES: &'static str = "bytes";
/// Field label carried by a failed `skip`.
pub const FIELD_SKIP: &'static str = "skip";
/// Field label carried by a failed `seek`.
pub const FIELD_SEEK: &'static str = "seek";

/// Why a little-endian read of the executable layer stopped.
///
/// Every variant carries the byte offset at which the operation started, the
/// static label of the operation, and the origin label given to `new`. No
/// variant carries any part of the executable, and the type has no lifetime,
/// so it cannot hold a borrow of the table either (END-023). The type is
/// distinct from `BigEndianError`, so an asset-layer failure cannot cross into
/// this layer (END-024).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LittleEndianError {
    /// The operation wanted `needed` bytes at `offset` and only `available`
    /// were left. Nothing was consumed.
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
    /// `seek` named a position past the end of the buffer. Nothing moved.
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
        Self { bytes, origin, offset: 0 }
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

    /// Read one byte and advance.
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

    /// Read two bytes, least significant first, and advance.
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

    /// Read four bytes, least significant first, and advance.
    pub fn u32(&mut self) -> Result<u32, LittleEndianError> {
        let taken = self.take(WIDTH_U32, FIELD_U32)?;
        let mut word = 0_u32;
        for byte in taken.iter().rev() {
            word = (word << 8) | u32::from(*byte);
        }
        Ok(word)
    }

    /// Read two bytes, least significant first, reinterpret them as a two's
    /// complement value and advance.
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
    /// complement value and advance.
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
    /// `bytes` uses; a skip past the end is an error, not a clamped move.
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
```

## How the test author should separate the two layers from outside

A test that can only see the public surface cannot tell the crates apart by
type name alone, because it names the crate it imported from. Two tests are
enough to pin both layers, and they are independent of each other:

- In `fo1-endian-host`, read the four bytes `01 00 00 00` at offset 0 and
  assert `u32()` is 1. In `fo1-bigendian`, read the same four bytes at offset 0
  and assert `u32()` is 16,777,216. Nothing about the input changed.
- In `fo1-bigendian`, read the first four bytes of a `MASTER.DAT` root header
  and assert 65. Little-endian would be 1,090,519,040 (END-035).

The first pair has two acceptable oracles and the test author should prefer
the stronger one. Class 1 is the right oracle: the test extracts the four bytes
at virtual address `0x5082D4` with its own reader, independent of the code under
test, folds them both ways itself, and asserts against the results. Class 5 is
the fallback for the run that has no install, because the guide states both
results. The second pair is class 2, closure on real data, because the value
read closes against the 65 directories the guide records for `MASTER.DAT`
independently of this reader. Both classes are in the set `tools/check_trace.sh`
accepts for a claim tagged CONFIRMED AGAINST ORIGINAL BINARY.

## What this spec does NOT settle

- **Which byte order each individual field of a format uses.** This spec gives
  the layer for a format and the exception list above. The per-field map belongs
  to that format's spec.
- **The `.LIP` layout.** END-027 records an unconfirmed byte order and nothing
  more. Where the marker table starts, what it holds and how many entries it has
  are all open, and the community reader is known to diverge at offset `0x14`.
- **The element type of save block 23.** END-030 fixes the 216 byte size and
  nothing else. The reader must be settled from the function at `0x499520`.
- **Floating point.** Neither reader has an `f32` or `f64` method, because the
  required interface has none and because no Fallout 1 format the guide
  describes puts a float in a layer this spec owns except save block 18, whose
  byte order is not settled anywhere. When a float is needed, that is a new
  claim and a new method, not an extension of these two.
- **The caller's field name.** `field` on an error names the operation, such as
  `u32` or `bytes`. The interface has no setter for a caller's own field name,
  so a format that wants `xp_threshold_7` in its own error carries it in its own
  error type and wraps this one.
- **Which reader a format with mixed orders picks per field.** END-025,
  END-026, END-027, END-029 and END-030 name the exceptions. They do not say how
  a caller should express "this field from here on is the other order", and the
  answer is that it opens a second reader over the remaining bytes.
- **Whether `fo1-dat` hands out a cursor or already-read values.** That is the
  DAT spec's decision. This spec only requires that whichever it is, every
  integer came through one of these two crates.
- **The DAT capacity word.** The guide says to ignore it and this spec does too.
  Its value in `MASTER.DAT` is 94 and this spec deliberately fixes no constant
  for it.
- **The 18 byte `.MVE` magic itself.** It is text, so it has no byte order.
  END-026 concerns only what follows it.
- **Alignment and padding on the original x86 host.** Nothing here states whether
  a table field is naturally aligned, because no reader in this workspace is
  allowed to depend on it: every read is bounds-checked byte by byte.