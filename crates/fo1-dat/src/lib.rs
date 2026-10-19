//! DAT archive container.
//!
//! A two level tree over bytes. A 16 byte root header of four big endian u32, one
//! length-prefixed directory name per directory, then one 16 byte block header
//! per directory followed by that directory's entries, with nothing separating
//! the blocks (DAT-001, DAT-003, DAT-004, DAT-008).
//!
//! Four rules in this container are load bearing and are the ones a reader gets
//! wrong:
//!
//! - **Block field 0 is the entry count for that block**, not the directory
//!   count the same slot holds at the root (DAT-005). Summing it over the 66
//!   measured block headers gives 25,243, the corpus entry total.
//! - **An entry's stored region is `offset .. offset + compressed_size`** and
//!   never `offset + uncompressed_size`: seven real entries decode to more bytes
//!   than their archive has left (DAT-021).
//! - **Path composition keys on the directory being named `.`**, not on a block
//!   being first. CRITTER.DAT's only block is named `ART\CRITTERS` and all 5,459
//!   of its paths keep that prefix (DAT-011).
//! - **A block's header bit 15 is tested per block, not per chain.** One chain
//!   may mix raw and LZSS blocks, and MASTER.DAT holds 6,593 raw blocks that all
//!   live inside chains (DAT-020).
//!
//! Tree order is **not** physical order. In MASTER.DAT the first tree entry has
//! offset 211,060,669 while the smallest offset anywhere in it is 556,484, so an
//! offset is read from the record and from nowhere else (DAT-012).
//!
//! # No truncation, ever
//!
//! The declared uncompressed size reaches this crate for one purpose only: to
//! detect that the tokens would write past it, and to detect that a stream ended
//! before reaching it. Both events are errors (DAT-024). The output is never cut,
//! never padded, never substituted and never grown past the declaration, and a
//! chain's framed blocks must additionally consume exactly the declared
//! compressed size (DAT-022). A decoder that stopped at the declared size would
//! pass a size gate for any input at all, which is the failure mode the archive
//! gate exists to catch.
//!
//! # How this crate satisfies its own lint table
//!
//! `#![deny(clippy::arithmetic_side_effects)]` is in force, so there is no `+`,
//! `-` or `*` on any offset, length or counter anywhere below. The places
//! arithmetic would have been are `read_u32`'s callers, `region`, the three
//! extent functions, the chain driver's frame walk and `counted`; each one uses
//! `checked_*` or `saturating_*` and names its failure instead of swallowing it.
//!
//! Every byte of the archive is reached through `fo1_bigendian::BigEndian` for a
//! number and through `slice::get` for a run of bytes (END-013). There is no
//! slice index, no `unsafe`, no `unwrap`, no `expect`, no `panic!` and no cast
//! that changes width or sign.
//!
//! # The lookup index
//!
//! `find` compares the composed path of each entry against the caller's path.
//! It builds no index and stores no hash map: iteration order is the tree's own
//! order and the lookup has no ordering to depend on. `docs/spec/dat.md` leaves
//! the index "an internal choice the spec does not constrain beyond
//! correctness", and `tools/lint_rules.sh` bans hash map order from production
//! sources.
//!
//! See `docs/spec/dat.md`.

#![deny(clippy::arithmetic_side_effects)]

use fo1_bigendian::{BigEndian, BigEndianError};
use fo1_lzss::{decode_into, LzssDecoder, LzssError};

/// Width of the root header and of every block header: four big endian u32
/// (DAT-001, DAT-004, `DAT-C001`).
pub const ROOT_HEADER_SIZE: usize = 16;

/// Bytes in one framed block header inside a chain (DAT-018).
const BLOCK_HEADER_WIDTH: usize = 2;

/// Bit 15 of a chain block header. Set means the block is raw (DAT-027,
/// `DAT-C050`).
const BLOCK_RAW_FLAG: u16 = 0x8000;

/// Mask that leaves a block payload's length in a chain block header, for both
/// the raw and the LZSS case (DAT-027, `DAT-C051`).
const BLOCK_PAYLOAD_MASK: u16 = 0x7FFF;

/// Mask that selects the payload mode from the flags word: the high nibble
/// (DAT-013, `DAT-C040`).
const MODE_MASK: u32 = 0xF0;

/// Mode `0x10`, one LZSS stream whose input length is the compressed size
/// (DAT-014, `DAT-C041`).
const MODE_LZSS_STREAM: u32 = 0x10;

/// Mode `0x20`, stored bytes whose length is the uncompressed size (DAT-015,
/// `DAT-C042`).
const MODE_STORED: u32 = 0x20;

/// Mode `0x40`, a framed series of raw and LZSS blocks (DAT-016, `DAT-C043`).
const MODE_BLOCK_CHAIN: u32 = 0x40;

/// The directory name that contributes no path separator (DAT-011, `DAT-C151`).
const ROOT_DIRECTORY_NAME: &str = ".";

/// The path separator every stored directory name and composed path uses
/// (DAT-010).
const PATH_SEPARATOR: char = '\\';

/// Everything the reader can fail at. There is no panic path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatError {
    /// The declared structure ran out before the reader did.
    Truncated {
        /// File-relative byte offset at which the reader ran out.
        at: u64,
    },
    /// A length or count field exceeded the bytes actually present, or a stored
    /// name was not the UTF-8 the path type requires.
    LengthOutOfRange {
        /// File-relative byte offset of the length byte or field.
        at: u64,
    },
    /// A block header or payload ran outside the archive.
    OutOfBounds {
        /// File-relative byte offset of the region that ran out.
        at: u64,
    },
    /// An offset plus a length did not fit in the address arithmetic.
    OffsetOverflow,
    /// A flags high nibble this build does not know.
    UnknownPayloadMode {
        /// The flags word exactly as stored, reported back whole.
        flags: u32,
    },
    /// The tokens would have written more than the declared output size.
    OutputOverrun {
        /// The declared uncompressed size that was exceeded.
        declared: u32,
    },
    /// The stream ended before reaching the declared output size.
    OutputShort {
        /// The declared uncompressed size that was not reached.
        declared: u32,
    },
    /// The payload did not consume exactly the declared compressed size.
    ConsumptionMismatch {
        /// The compressed size the entry declares.
        declared: u32,
        /// The bytes the payload actually consumed.
        actual: u32,
    },
    /// A block header declared a zero length payload.
    EmptyBlock {
        /// File-relative byte offset of the block header.
        at: u64,
    },
}

impl core::fmt::Display for DatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::Truncated { at } => {
                write!(f, "fo1-dat: the archive ran out at byte {at}")
            }
            Self::LengthOutOfRange { at } => {
                write!(
                    f,
                    "fo1-dat: a length field at byte {at} exceeded the bytes present"
                )
            }
            Self::OutOfBounds { at } => {
                write!(f, "fo1-dat: a region at byte {at} ran outside the archive")
            }
            Self::OffsetOverflow => write!(f, "fo1-dat: an offset plus a length overflowed"),
            Self::UnknownPayloadMode { flags } => {
                write!(
                    f,
                    "fo1-dat: the flags word {flags:#04X} names no known payload mode"
                )
            }
            Self::OutputOverrun { declared } => {
                write!(
                    f,
                    "fo1-dat: the tokens would write past {declared} declared bytes"
                )
            }
            Self::OutputShort { declared } => {
                write!(
                    f,
                    "fo1-dat: the stream ended short of {declared} declared bytes"
                )
            }
            Self::ConsumptionMismatch { declared, actual } => {
                write!(
                    f,
                    "fo1-dat: the payload consumed {actual} byte(s) against {declared} declared"
                )
            }
            Self::EmptyBlock { at } => {
                write!(
                    f,
                    "fo1-dat: the block header at byte {at} declares a zero length payload"
                )
            }
        }
    }
}

impl std::error::Error for DatError {}

/// One directory of the two level tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectoryRecord {
    /// Name exactly as stored, uppercase, possibly the single character `.`.
    pub name: String,
    /// Field 0 of this block's header: how many entries follow it.
    pub entry_count: u32,
}

/// One entry. `path` is already composed and already uppercase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryRecord {
    /// Directory name and entry name joined by one backslash, or the bare entry
    /// name when the directory is named `.` (DAT-011).
    pub path: String,
    /// The flags word. Its high nibble selects the payload mode (DAT-013).
    pub flags: u32,
    /// Absolute byte offset into the archive file, not into the entry (DAT-009,
    /// DAT-012).
    pub offset: u32,
    /// Declared decoded length. The output must reach it exactly (DAT-024).
    pub uncompressed_size: u32,
    /// Declared stored length, which is also the file bound on the entry's region
    /// for a chain (DAT-021). Zero for a stored entry (DAT-017).
    pub compressed_size: u32,
}

/// Which of the three payload shapes a flags value selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadMode {
    /// 0x10. One LZSS stream, input length is the compressed size (DAT-014).
    LzssStream,
    /// 0x20. Stored bytes, length is the uncompressed size (DAT-015).
    Stored,
    /// 0x40. A framed series of raw and LZSS blocks (DAT-016).
    BlockChain,
}

/// Per-chain block census, used by the gate rather than by callers (DAT-020).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChainStats {
    /// Blocks framed, of either kind.
    pub blocks: u32,
    /// Blocks whose header bit 15 was clear.
    pub lzss_blocks: u32,
    /// Blocks whose header bit 15 was set.
    pub raw_blocks: u32,
}

/// A parsed DAT tree. It owns every string it reports and borrows nothing, so
/// the archive bytes may be released as soon as the tree is built.
pub struct DatArchive {
    /// Diagnostic label the archive was parsed under (DAT-030). It names which
    /// archive a message came from and is never a path to open.
    origin: &'static str,
    /// Directories in file order, one per block header.
    directories: Vec<DirectoryRecord>,
    /// Entries in tree order: all of block 0, then all of block 1, and so on.
    entries: Vec<EntryRecord>,
}

impl DatArchive {
    /// Parses the root header, the directory names, and every block with its
    /// entries.
    ///
    /// Parsing never reads the payload region, so a truncated payload does not
    /// stop the directory from being listed. The payload mode is not inspected
    /// either: an entry whose flags high nibble names no known mode parses and is
    /// refused by [`decode_entry`] instead (DAT-013).
    ///
    /// `origin` is a diagnostic label such as `MASTER.DAT`, carried on the
    /// archive and reported by [`DatArchive::origin`]. It is never opened.
    ///
    /// # Errors
    ///
    /// [`DatError::Truncated`] when a header or a record ran out,
    /// [`DatError::LengthOutOfRange`] when a stored length exceeded the bytes
    /// present or a name was not UTF-8, and [`DatError::OutOfBounds`] when a
    /// region fell outside the archive.
    pub fn parse(bytes: &[u8], origin: &'static str) -> Result<Self, DatError> {
        let mut reader = BigEndian::new(bytes, origin);

        // DAT-001, DAT-002. Fields 1 and 3 are read so the cursor advances and
        // then discarded: a capacity is not a length and a dead pointer is not a
        // bound, and neither may influence a stride or an extent.
        let directory_count = read_u32(&mut reader)?;
        let _capacity = read_u32(&mut reader)?;
        let _size_field = read_u32(&mut reader)?;
        let _dead_pointer = read_u32(&mut reader)?;

        // DAT-003: exactly field 0 length-prefixed names, in file order.
        let mut names: Vec<String> = Vec::new();
        for _ in 0..directory_count {
            names.push(read_name(&mut reader)?);
        }

        // DAT-004, DAT-005, DAT-008: one block header per directory, matched to
        // name `i`, then that block's entries, then the next block header.
        let mut directories: Vec<DirectoryRecord> = Vec::new();
        let mut entries: Vec<EntryRecord> = Vec::new();
        for name in &names {
            let entry_count = read_u32(&mut reader)?;
            let _block_capacity = read_u32(&mut reader)?;
            // DAT-006: this is the entry trailer width, 16, and not the size of
            // an entry record. The record width comes from the name length, so
            // the field is read and discarded.
            let _trailer_width = read_u32(&mut reader)?;
            let _block_dead_pointer = read_u32(&mut reader)?;

            for _ in 0..entry_count {
                let entry_name = read_name(&mut reader)?;
                // DAT-009: the four u32 in order, flags first.
                let flags = read_u32(&mut reader)?;
                let offset = read_u32(&mut reader)?;
                let uncompressed_size = read_u32(&mut reader)?;
                let compressed_size = read_u32(&mut reader)?;
                entries.push(EntryRecord {
                    path: compose_path(name, &entry_name),
                    flags,
                    offset,
                    uncompressed_size,
                    compressed_size,
                });
            }

            directories.push(DirectoryRecord {
                name: name.clone(),
                entry_count,
            });
        }

        Ok(Self {
            origin,
            directories,
            entries,
        })
    }

    /// The diagnostic label this archive was parsed under (DAT-030).
    #[must_use]
    pub fn origin(&self) -> &'static str {
        self.origin
    }

    /// Directory names in file order.
    #[must_use]
    pub fn directories(&self) -> &[DirectoryRecord] {
        &self.directories
    }

    /// Entries in file order: all of block 0, then all of block 1, and so on.
    /// This is tree order, which is not physical order (DAT-012).
    #[must_use]
    pub fn entries(&self) -> &[EntryRecord] {
        &self.entries
    }

    /// Looks up a path the caller has already uppercased and already spelled
    /// with backslashes.
    ///
    /// This performs no case folding of any kind: it does not call
    /// `to_uppercase`, it does not compare with `eq_ignore_ascii_case`, and it
    /// consults no locale (DAT-031, DAT-032). `None` means the uppercase form is
    /// not in the archive, which is an answer rather than an error: the archive
    /// stays usable and the same path can be looked up again.
    #[must_use]
    pub fn find(&self, upper_path: &str) -> Option<&EntryRecord> {
        self.entries.iter().find(|entry| entry.path == upper_path)
    }
}

/// The payload mode a flags word selects: its high nibble (DAT-013).
///
/// A `flags` value of 0 means `0x10`. The signature in DAT-030 returns a
/// [`PayloadMode`] and cannot carry an error, so this function is total: a high
/// nibble this build does not know yields [`PayloadMode::BlockChain`] as a
/// placeholder that names no real mode. DAT-013's requirement is met where it can
/// be observed — [`decode_entry`] and [`decode_entry_with_stats`] answer
/// [`DatError::UnknownPayloadMode`] for every nibble this function cannot map.
#[must_use]
pub fn payload_mode(flags: u32) -> PayloadMode {
    classify(flags).unwrap_or(PayloadMode::BlockChain)
}

/// Decodes one entry to exactly its declared uncompressed size, consuming
/// exactly its declared compressed size.
///
/// # Errors
///
/// [`DatError::UnknownPayloadMode`] for a flags high nibble this build does not
/// know (DAT-013), [`DatError::LengthOutOfRange`] or [`DatError::OutOfBounds`]
/// when an extent falls outside the archive (DAT-021),
/// [`DatError::OutputOverrun`] when the tokens would write past the declaration
/// and [`DatError::OutputShort`] when the payload ended before reaching it
/// (DAT-024), [`DatError::EmptyBlock`] for a zero length frame (DAT-018), and
/// [`DatError::ConsumptionMismatch`] when a chain's frames did not account for
/// its compressed size exactly (DAT-022).
pub fn decode_entry(entry: &EntryRecord, bytes: &[u8]) -> Result<Vec<u8>, DatError> {
    match decode_entry_with_stats(entry, bytes) {
        Ok((produced, _stats)) => Ok(produced),
        Err(error) => Err(error),
    }
}

/// As [`decode_entry`], and also returns the chain census so the gate can assert
/// the block totals without a second pass (DAT-020).
///
/// The census is all zeros for a stored entry and for a whole LZSS stream: only
/// a block chain frames blocks.
pub fn decode_entry_with_stats(
    entry: &EntryRecord,
    bytes: &[u8],
) -> Result<(Vec<u8>, ChainStats), DatError> {
    let mode = match classify(entry.flags) {
        Some(mode) => mode,
        // DAT-013: an unknown high nibble is an error, never a default.
        None => return Err(DatError::UnknownPayloadMode { flags: entry.flags }),
    };

    let (start, end) = payload_extent(entry, mode)?;
    let declared = match usize::try_from(entry.uncompressed_size) {
        Ok(declared) => declared,
        Err(_) => return Err(DatError::LengthOutOfRange { at: start }),
    };

    match mode {
        PayloadMode::Stored => {
            // DAT-017: a stored entry never carries a compressed size. Measured
            // across all 25,243 entries, so refusing it is exhaustive rather
            // than defensive.
            if entry.compressed_size != 0 {
                return Err(DatError::ConsumptionMismatch {
                    declared: 0,
                    actual: entry.compressed_size,
                });
            }
            let stored = region(bytes, start, end)?;
            Ok((stored.to_vec(), ChainStats::default()))
        }
        PayloadMode::LzssStream => {
            // DAT-014: one whole stream whose input length is the compressed
            // size. `decode_into` sees the end of the input, so it enforces both
            // the overrun check and the shortfall check.
            let input = region(bytes, start, end)?;
            match decode_into(input, declared) {
                Ok(produced) => Ok((produced, ChainStats::default())),
                Err(error) => Err(lzss_failure(error, entry, start)),
            }
        }
        PayloadMode::BlockChain => decode_chain(entry, region(bytes, start, end)?, start, declared),
    }
}

/// The payload mode of a flags word, or `None` when its high nibble names no
/// mode this build knows (DAT-013).
fn classify(flags: u32) -> Option<PayloadMode> {
    match flags & MODE_MASK {
        // DAT-013: zero is not a fourth mode, it is the way a flags word spells
        // `0x10`. No measured entry carries it (DAT-C140), so the substitution
        // never fires on the reference install.
        0 => Some(PayloadMode::LzssStream),
        MODE_LZSS_STREAM => Some(PayloadMode::LzssStream),
        MODE_STORED => Some(PayloadMode::Stored),
        MODE_BLOCK_CHAIN => Some(PayloadMode::BlockChain),
        _ => None,
    }
}

/// The stored region of an entry. This is bounded by the compressed size, never
/// by the uncompressed size: seven real entries decode to more bytes than the
/// file has left (DAT-021).
fn stored_extent(entry: &EntryRecord) -> Result<(u64, u64), DatError> {
    let start = u64::from(entry.offset);
    let end = match start.checked_add(u64::from(entry.compressed_size)) {
        Some(end) => end,
        None => return Err(DatError::OffsetOverflow),
    };
    Ok((start, end))
}

/// For a stored entry the bytes are already decoded, so the stored region is the
/// uncompressed size instead. Chain and LZSS entries keep the compressed bound.
fn payload_extent(entry: &EntryRecord, mode: PayloadMode) -> Result<(u64, u64), DatError> {
    if matches!(mode, PayloadMode::Stored) {
        let start = u64::from(entry.offset);
        let end = match start.checked_add(u64::from(entry.uncompressed_size)) {
            Some(end) => end,
            None => return Err(DatError::OffsetOverflow),
        };
        Ok((start, end))
    } else {
        stored_extent(entry)
    }
}

/// `start..end` of the whole archive as a slice, refused rather than clamped
/// (DAT-021).
fn region(bytes: &[u8], start: u64, end: u64) -> Result<&[u8], DatError> {
    let from = match usize::try_from(start) {
        Ok(from) => from,
        Err(_) => return Err(DatError::OffsetOverflow),
    };
    let to = match usize::try_from(end) {
        Ok(to) => to,
        Err(_) => return Err(DatError::OffsetOverflow),
    };
    if to > bytes.len() {
        return Err(DatError::OutOfBounds { at: end });
    }
    match bytes.get(from..to) {
        Some(slice) => Ok(slice),
        None => Err(DatError::OutOfBounds { at: start }),
    }
}

/// Joins a directory name and an entry name. The root directory of MASTER.DAT is
/// named `.` and contributes no separator, which is why this is not a plain
/// `format!`: the rule keys on that **name**, not on a block being first
/// (DAT-011).
fn compose_path(directory: &str, name: &str) -> String {
    if directory == ROOT_DIRECTORY_NAME {
        return String::from(name);
    }
    // No capacity arithmetic: a length hint is not worth a checked add.
    let mut joined = String::new();
    joined.push_str(directory);
    joined.push(PATH_SEPARATOR);
    joined.push_str(name);
    joined
}

/// Reads one big endian u32 through the asset endian layer, which is the only
/// route by which an integer in this layer may be read (END-013, DAT-001).
fn read_u32(reader: &mut BigEndian<'_>) -> Result<u32, DatError> {
    reader.u32().map_err(truncated)
}

/// One length byte then exactly that many name bytes (DAT-003, DAT-009).
///
/// A zero length is a zero length name and is accepted (DAT-034): the four u32
/// trailer still follows at the computed offset and nothing is truncated.
fn read_name(reader: &mut BigEndian<'_>) -> Result<String, DatError> {
    let at = reader.offset();
    let length = reader.u8().map_err(truncated)?;
    let raw = reader
        .bytes(usize::from(length))
        .map_err(length_out_of_range)?;
    // DAT-009: no stored name in either archive carries a non-ASCII byte, so a
    // lossy decode would hide a layout change rather than report it. The error
    // set is closed (DAT-030) and has no malformed-name variant, so an unusable
    // name is reported as the unusable length it came from.
    match String::from_utf8(raw.to_vec()) {
        Ok(name) => Ok(name),
        Err(_) => Err(DatError::LengthOutOfRange { at: reported(at) }),
    }
}

/// Walks one block chain: framed blocks in stored order until the decoded output
/// reaches the declared uncompressed size, and no longer (DAT-018, DAT-024).
///
/// Every frame's header bit is tested on its own, so one chain may mix raw and
/// LZSS blocks (DAT-020). Every LZSS block starts with a fresh ring, because
/// `fo1-lzss` refills it on every call and the budget handed to that call is
/// what is still missing from the declaration (DAT-019). The frames must also
/// account for the compressed size exactly, which is the cross-check that makes
/// a decoder which merely stopped at the declared size impossible (DAT-022).
fn decode_chain(
    entry: &EntryRecord,
    window: &[u8],
    start: u64,
    declared: usize,
) -> Result<(Vec<u8>, ChainStats), DatError> {
    let mut stats = ChainStats::default();
    let mut produced: Vec<u8> = Vec::new();
    let mut decoder = LzssDecoder::new();
    let mut cursor = 0usize;

    while produced.len() < declared {
        let header_end = match cursor.checked_add(BLOCK_HEADER_WIDTH) {
            Some(header_end) => header_end,
            None => return Err(DatError::OffsetOverflow),
        };
        let header = match window.get(cursor..header_end) {
            Some(header) => header,
            // The declared extent ran out before the output reached the declared
            // size. DAT-024 calls that an error and never a padded buffer.
            None => {
                return Err(DatError::OutputShort {
                    declared: entry.uncompressed_size,
                })
            }
        };
        let mut frame = [0_u8; BLOCK_HEADER_WIDTH];
        for (slot, byte) in frame.iter_mut().zip(header) {
            *slot = *byte;
        }
        let value = u16::from_be_bytes(frame);

        // DAT-018: a zero length payload is an error, so the walk cannot spin.
        let payload_length = usize::from(value & BLOCK_PAYLOAD_MASK);
        if payload_length == 0 {
            return Err(DatError::EmptyBlock {
                at: start.saturating_add(reported(cursor)),
            });
        }
        let payload_end = match header_end.checked_add(payload_length) {
            Some(payload_end) => payload_end,
            None => return Err(DatError::OffsetOverflow),
        };
        let payload = match window.get(header_end..payload_end) {
            Some(payload) => payload,
            None => {
                return Err(DatError::OutOfBounds {
                    at: start.saturating_add(reported(cursor)),
                })
            }
        };

        if value & BLOCK_RAW_FLAG != 0 {
            // Raw: copied verbatim, and refused if it would pass the declaration.
            let wanted = match produced.len().checked_add(payload_length) {
                Some(wanted) => wanted,
                None => return Err(DatError::OffsetOverflow),
            };
            if wanted > declared {
                return Err(DatError::OutputOverrun {
                    declared: entry.uncompressed_size,
                });
            }
            produced.extend_from_slice(payload);
            stats.raw_blocks = counted(stats.raw_blocks)?;
        } else {
            // LZSS: the block's budget is what the whole entry still owes.
            let remaining = declared.saturating_sub(produced.len());
            decoder
                .decode_block(payload, remaining)
                .map_err(|error| lzss_failure(error, entry, start))?;
            produced.extend_from_slice(&decoder.take_output());
            stats.lzss_blocks = counted(stats.lzss_blocks)?;
        }

        cursor = payload_end;
        stats.blocks = counted(stats.blocks)?;
    }

    // DAT-022, DAT-024: the frames must account for the compressed size exactly,
    // with no trailing bytes and no overrun. `window` is exactly the declared
    // compressed size long, so this is the whole cross-check.
    if cursor != window.len() {
        let actual = match u32::try_from(cursor) {
            Ok(actual) => actual,
            Err(_) => return Err(DatError::OffsetOverflow),
        };
        return Err(DatError::ConsumptionMismatch {
            declared: entry.compressed_size,
            actual,
        });
    }

    Ok((produced, stats))
}

/// Translates a codec failure into the container's vocabulary, keeping the
/// entry's declared size rather than the block's remaining budget so the caller
/// is told what the record promised (DAT-024).
fn lzss_failure(error: LzssError, entry: &EntryRecord, base: u64) -> DatError {
    match error {
        LzssError::OutputOverrun { .. } => DatError::OutputOverrun {
            declared: entry.uncompressed_size,
        },
        LzssError::OutputShort { .. } => DatError::OutputShort {
            declared: entry.uncompressed_size,
        },
        // The codec ran out of input while reading a token's second byte. Two
        // different things are true at once and the produced count says which
        // one DAT-024 calls it.
        //
        // At or above the declared size there were still tokens pending behind
        // the control byte, so those tokens would have written past the
        // declaration: the overrun half, and the output is not extended. Below
        // the declared size the input simply ended first, which is the
        // structure running out before the reader did.
        LzssError::TruncatedMatch {
            produced,
            declared_out,
            ..
        } if produced >= declared_out => DatError::OutputOverrun {
            declared: entry.uncompressed_size,
        },
        LzssError::TruncatedMatch { at, .. } => DatError::Truncated {
            at: base.saturating_add(reported(at)),
        },
        // The codec's ring arithmetic did not fit, which is the same failure
        // this crate reports as `OffsetOverflow`.
        LzssError::IndexOverflow { .. } => DatError::OffsetOverflow,
    }
}

/// One more block in a census.
///
/// `None` would need more than `u32::MAX` blocks in one entry, and every framed
/// block occupies at least three bytes of a `u32` compressed size, so the arm is
/// unreachable. It is named rather than assumed, and rather than saturating,
/// which would turn an impossible count into a quiet lie.
fn counted(current: u32) -> Result<u32, DatError> {
    match current.checked_add(1) {
        Some(next) => Ok(next),
        None => Err(DatError::OffsetOverflow),
    }
}

/// A byte position in the width the error type reports it in.
///
/// `usize` is at most 64 bits on every target Rust supports, so this never loses
/// a position and never changes a sign. The conversion is written out rather than
/// asserted so the crate needs no `allow` to compile.
fn reported(position: usize) -> u64 {
    u64::try_from(position).unwrap_or(u64::MAX)
}

/// A read that ran out of archive is a truncation wherever it happens.
fn truncated(error: BigEndianError) -> DatError {
    match error {
        BigEndianError::Truncated { offset, .. } => DatError::Truncated {
            at: reported(offset),
        },
        BigEndianError::SeekOutOfRange { offset, .. } => DatError::OutOfBounds {
            at: reported(offset),
        },
    }
}

/// A read a stored length asked for is a length that could not be used.
fn length_out_of_range(error: BigEndianError) -> DatError {
    match error {
        BigEndianError::Truncated { offset, .. } => DatError::LengthOutOfRange {
            at: reported(offset),
        },
        BigEndianError::SeekOutOfRange { offset, .. } => DatError::OutOfBounds {
            at: reported(offset),
        },
    }
}
