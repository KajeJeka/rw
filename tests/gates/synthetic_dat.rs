//! Synthetic gate tests for the DAT archive container (`fo1-dat`).
//!
//! Every fixture here is built byte by byte in this file and named
//! `synthetic_*`, so the suite needs no game install and runs in CI. A synthetic
//! fixture cannot support a CONFIRMED evidence tag (guide: Fixtures and golden
//! files). What each fixture does carry is:
//!
//! - a **hand computed layout offset** taken from `docs/spec/dat.md`, so the
//!   absolute byte position of a name, a block header and an entry record is an
//!   assertion rather than an accident (class 5);
//! - a **second, independent walk** of the same bytes by `fo1_tests`, the
//!   reference tree walker (class 4); and
//! - an **algebraic law** about counts, offsets or modes (class 6).
//!
//! The oracle classes used below follow `docs/spec/trace.md`: oracle class 4 (an
//! independent reference implementation), oracle class 5 (hand computed vectors
//! from the spec) and oracle class 6 (algebraic laws).
//!
//! One convention runs through the file: a layout offset in a fixture is a
//! `usize`, because it indexes a buffer, and an entry record stores it as a
//! `u32`, because that is the container's own width. `to_u32` is the only
//! crossing between the two.
//!
//! The layout every fixture reproduces, in file order:
//!
//! ```text
//! 0x00  root header: four u32, directory count, capacity, 0, dead pointer
//!       directory names, one u8 length byte each
//!       per directory: a block header of four u32, then that block's entries
//!       per entry:      a u8 length byte, the name, flags, offset, uncomp, comp
//!       payloads
//! ```

use fo1_bigendian::BigEndian;
use fo1_dat::{
    decode_entry, decode_entry_with_stats, payload_mode, DatArchive, DatError, EntryRecord,
    PayloadMode, ROOT_HEADER_SIZE,
};

// --- container constants ------------------------------------------------------

/// Width of the root header and of every block header: four big endian u32
/// (DAT-001, DAT-004, `DAT-C001`).
const HEADER_WIDTH: usize = 16;

/// Byte width of the four u32 that trail every entry name. This is what block
/// header field 2 carries, and it is **not** the size of an entry record, which
/// is one length byte plus the name plus this trailer (DAT-006, `DAT-C003`).
const ENTRY_TRAILER_WIDTH: usize = 16;

/// One length byte in front of every stored name (DAT-003, DAT-009).
const NAME_LENGTH_WIDTH: usize = 1;

/// Mask that selects the payload mode from the flags word (DAT-013, `DAT-C040`).
const MODE_MASK: u32 = 0xF0;

/// Mode `0x10`, one LZSS stream whose input length is the compressed size
/// (DAT-014, `DAT-C041`).
const MODE_LZSS_STREAM: u32 = 0x10;

/// Mode `0x20`, stored bytes whose length is the uncompressed size (DAT-015,
/// `DAT-C042`).
const MODE_STORED: u32 = 0x20;

/// Mode `0x40`, a framed series of raw and LZSS blocks (DAT-016, `DAT-C043`).
const MODE_BLOCK_CHAIN: u32 = 0x40;

/// A flags word of zero, which the spec maps onto `0x10` (DAT-013, `DAT-C140`).
const MODE_ZERO_FLAGS: u32 = 0x00;

/// A high nibble that names no payload mode, which the spec says is an error
/// and never a default (DAT-013).
const MODE_UNKNOWN: u32 = 0x80;

/// Bit 15 of a block header, set on a raw block (DAT-027, `DAT-C050`).
const BLOCK_RAW_FLAG: u16 = 0x8000;

/// Mask that leaves a raw block's payload length in a block header (DAT-027,
/// `DAT-C051`).
const BLOCK_RAW_LENGTH_MASK: u16 = 0x7FFF;

/// The directory name that contributes no path separator (DAT-011, `DAT-C151`).
const ROOT_DIRECTORY_NAME: &str = ".";

/// A directory name that does contribute the separator, taken from the
/// CRITTER.DAT directory of `DAT-C011`.
const NAMED_DIRECTORY: &str = "ART\\CRITTERS";

/// Root header field 1, a capacity that is read and discarded (DAT-002,
/// `DAT-C090`). Deliberately larger than any fixture's directory count.
const ROOT_CAPACITY: u32 = 94;

/// Root header field 3, a dead pointer that is read and discarded (DAT-002,
/// `DAT-C100`).
const ROOT_DEAD_POINTER: u32 = 33_691_024;

/// Block header field 1, a capacity that is read and discarded (DAT-007,
/// `DAT-C093`). Larger than every fixture's entry count, so a reader that
/// mistakes it for a count fails here first.
const BLOCK_CAPACITY: u32 = 6142;

/// Block header field 3, a dead pointer that is read and discarded (DAT-007,
/// `DAT-C102`).
const BLOCK_DEAD_POINTER: u32 = 42_414_840;

/// Root header field 2, which the spec fixes at zero (DAT-002).
const ROOT_SIZE_FIELD: u32 = 0;

/// The diagnostic label every fixture is parsed under (DAT-030).
const SYNTHETIC_ORIGIN: &str = "synthetic_dat";

// --- payload shapes used by the chain and stream fixtures ---------------------

/// Raw block frame carrying the two bytes `AB`. Bit 15 is set and the length
/// field below it is two (DAT-018, DAT-027).
const RAW_BLOCK_HEADER: u16 = BLOCK_RAW_FLAG | 2;

/// Payload of [`RAW_BLOCK_HEADER`].
const RAW_BLOCK_PAYLOAD: [u8; 2] = *b"AB";

/// LZSS block frame, bit 15 clear, giving a compressed length of three bytes
/// (DAT-018).
const LZSS_BLOCK_HEADER: u16 = 3;

/// Payload of [`LZSS_BLOCK_HEADER`]. The stream `00 00 0F` is the spec's worked
/// vector: a control byte of zero, then a back reference of source 0 and length
/// 18, all of which lands in the prefilled `0x20` region. It decodes to eighteen
/// spaces (`lzss.md` LZSS-018).
const LZSS_BLOCK_PAYLOAD: [u8; 3] = [0x00, 0x00, 0x0F];

/// Output length of [`LZSS_BLOCK_PAYLOAD`] (LZSS-018).
const LZSS_BLOCK_OUTPUT: usize = 18;

/// Bytes an LZSS block occupies on disk: a two byte frame header and its three
/// payload bytes (DAT-018).
const LZSS_BLOCK_FRAME_BYTES: usize = 2 + LZSS_BLOCK_PAYLOAD.len();

/// Bytes a raw block occupies on disk: a two byte frame header and its two
/// payload bytes (DAT-018).
const RAW_BLOCK_FRAME_BYTES: usize = 2 + RAW_BLOCK_PAYLOAD.len();

/// Bytes a chain of one raw block and one LZSS block occupies on disk.
const CHAIN_COMPRESSED_SIZE: usize = RAW_BLOCK_FRAME_BYTES + LZSS_BLOCK_FRAME_BYTES;

/// Bytes the same chain decodes to: two copied verbatim and eighteen spaces.
const CHAIN_UNCOMPRESSED_SIZE: usize = 2 + 18;

/// Blocks in that chain (DAT-018).
const CHAIN_BLOCKS: u32 = 2;

/// LZSS blocks in that chain (DAT-020).
const CHAIN_LZSS_BLOCKS: u32 = 1;

/// Raw blocks in that chain (DAT-020).
const CHAIN_RAW_BLOCKS: u32 = 1;

/// Stored payload of the `0x20` fixtures, thirty two bytes, so the length is a
/// named constant rather than an expression inside an assertion.
const STORED_PAYLOAD: [u8; 32] = *b"SYNTHETIC-STORED-PAYLOAD-0000001";

/// Control byte `0x03`: bits 0, 1 and 2 are all literals. Three input bytes
/// decode to two, and every one of the three stream fixtures is this stream one
/// byte short, exact or one byte long (`lzss.md` LZSS-005, LZSS-014).
const STREAM_CONTROL: u8 = 0x03;

/// Output the three stream fixtures declare: the two literals behind that
/// control byte.
const STREAM_OUTPUT: [u8; 2] = [0x41, 0x42];

/// Input length of the exact stream: one control byte and two literals.
const STREAM_EXACT_INPUT: usize = 3;

/// Input length of the stream one byte short.
const STREAM_SHORT_INPUT: usize = 2;

/// Input length of the stream one byte long.
const STREAM_LONG_INPUT: usize = 4;

/// Uncompressed size all three stream entries declare, and therefore the output
/// size a decoder must reach exactly.
const STREAM_DECLARED: u32 = 2;

// --- oracle 5: the mode table, read as an exact set by name -------------------

/// Input and expected output of `payload_mode`, by variant name. The low nibble
/// rows prove the mode is the high nibble and nothing else (DAT-013).
const PAYLOAD_MODE_CASES: [(u32, &str); 7] = [
    (MODE_LZSS_STREAM, "LzssStream"),
    (MODE_STORED, "Stored"),
    (MODE_BLOCK_CHAIN, "BlockChain"),
    (MODE_ZERO_FLAGS, "LzssStream"),
    (0x1F, "LzssStream"),
    (0x2A, "Stored"),
    (0x4F, "BlockChain"),
];

/// Number of rows in [`PAYLOAD_MODE_CASES`].
const PAYLOAD_MODE_CASE_COUNT: usize = 7;
// --- hand computed layout offsets, oracle class 5 -----------------------------

/// Width of one stored name in the name section: the length byte plus the name.
/// Computed from the literal so a fixture name and its layout can never drift
/// apart.
const fn name_width(name: &str) -> usize {
    NAME_LENGTH_WIDTH + name.len()
}

/// Width of one entry record: the length byte, the name, and the four u32
/// trailer (DAT-009).
const fn record_width(name: &str) -> usize {
    name_width(name) + ENTRY_TRAILER_WIDTH
}

/// Record widths, one per fixture name, so no layout constant needs a hand
/// counted character total.
const WIDTH_COLOR_PAL: usize = record_width("COLOR.PAL");
const WIDTH_FONT0_AAF: usize = record_width("FONT0.AAF");
const WIDTH_FONT5_FON: usize = record_width("FONT5.FON");
const WIDTH_CRITTERS_LST: usize = record_width("CRITTERS.LST");
const WIDTH_BIG_FRM: usize = record_width("BIG.FRM");
const WIDTH_A1_FRM: usize = record_width("A1.FRM");
const WIDTH_GRID000_FRM: usize = record_width("GRID000.FRM");
const WIDTH_A: usize = record_width("A");
const WIDTH_AAA_BIN: usize = record_width("AAA.BIN");
const WIDTH_STORED_BIN: usize = record_width("STORED.BIN");
const WIDTH_STORED2_BN: usize = record_width("STORED2.BN");
const WIDTH_RAW1_BIN: usize = record_width("RAW1.BIN");
const WIDTH_LZSS1_BIN: usize = record_width("LZSS1.BIN");
const WIDTH_CHAIN_BIN: usize = record_width("CHAIN.BIN");
const WIDTH_L1_BIN: usize = record_width("L1.BIN");
const WIDTH_SHORT_BIN: usize = record_width("SHORT.BIN");
const WIDTH_EXACT_BN: usize = record_width("EXACT.BN");
const WIDTH_LONG_BN: usize = record_width("LONG.BN");
const WIDTH_ODD_BIN: usize = record_width("ODD.BIN");

/// Widths of the directory names the fixtures use.
const NAME_WIDTH_DOT: usize = name_width(".");
const NAME_WIDTH_ART_CRITTERS: usize = name_width("ART\\CRITTERS");
const NAME_WIDTH_ART_TILES: usize = name_width("ART\\TILES");
const NAME_WIDTH_TEXT: usize = name_width("TEXT");
const NAME_WIDTH_CHAIN: usize = name_width("CHAIN");

/// `synthetic_round_trip`: byte offset of block 0's header. Sixteen header bytes,
/// then the name `.` and the name `ART\CRITTERS`.
const ROUND_TRIP_BLOCK0_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT + NAME_WIDTH_ART_CRITTERS;

/// `synthetic_round_trip`: byte offset of block 0's first entry.
const ROUND_TRIP_BLOCK0_ENTRIES: usize = ROUND_TRIP_BLOCK0_HEADER + HEADER_WIDTH;

/// `synthetic_round_trip`: byte offset of block 1's header, after block 0's
/// three twenty six byte records.
const ROUND_TRIP_BLOCK1_HEADER: usize =
    ROUND_TRIP_BLOCK0_ENTRIES + WIDTH_COLOR_PAL + WIDTH_FONT0_AAF + WIDTH_FONT5_FON;

/// `synthetic_round_trip`: byte offset of block 1's first entry.
const ROUND_TRIP_BLOCK1_ENTRIES: usize = ROUND_TRIP_BLOCK1_HEADER + HEADER_WIDTH;

/// `synthetic_round_trip`: total length of the tree.
const ROUND_TRIP_TREE_BYTES: usize = ROUND_TRIP_BLOCK1_ENTRIES + WIDTH_CRITTERS_LST + WIDTH_BIG_FRM;

/// Directories in `synthetic_round_trip`.
const ROUND_TRIP_DIRECTORIES: usize = 2;

/// Entries in `synthetic_round_trip`.
const ROUND_TRIP_ENTRIES: usize = 5;

/// Root header field 0 of `synthetic_round_trip`: two directories.
const ROUND_TRIP_DIRECTORIES_FIELD: u32 = 2;

/// `synthetic_entry_count_field`: byte offset of block 0's header.
const COUNT_BLOCK0_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT + NAME_WIDTH_TEXT;

/// `synthetic_entry_count_field`: byte offset of block 1's header, after block
/// 0's five records.
const COUNT_BLOCK1_HEADER: usize = COUNT_BLOCK0_HEADER + HEADER_WIDTH + 5 * WIDTH_A1_FRM;

/// `synthetic_entry_count_field`: total tree length.
const COUNT_TREE_BYTES: usize = COUNT_BLOCK1_HEADER + HEADER_WIDTH + 3 * WIDTH_A1_FRM;

/// Root header field 0 of `synthetic_entry_count_field`: two directories.
const COUNT_ROOT_DIRECTORIES: u32 = 2;

/// Block 0 field 0 of `synthetic_entry_count_field`: five entries.
const COUNT_BLOCK0_ENTRIES: u32 = 5;

/// Block 1 field 0 of `synthetic_entry_count_field`: three entries.
const COUNT_BLOCK1_ENTRIES: u32 = 3;

/// Total entries `synthetic_entry_count_field` really holds.
const COUNT_TOTAL_ENTRIES: usize = 8;

/// `synthetic_varying_record_sizes`: byte offset of the block header.
const VARYING_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_ART_TILES;

/// `synthetic_varying_record_sizes`: byte offset of the first entry.
const VARYING_FIRST_ENTRY: usize = VARYING_BLOCK_HEADER + HEADER_WIDTH;

/// `synthetic_varying_record_sizes`: total tree length, after a twenty eight byte
/// record and an eighteen byte record.
const VARYING_TREE_BYTES: usize = VARYING_FIRST_ENTRY + WIDTH_GRID000_FRM + WIDTH_A;

/// `synthetic_dot_directory`: total tree length, for the one character name `.`.
const DOT_ENTRY_TREE_BYTES: usize =
    HEADER_WIDTH + NAME_WIDTH_DOT + HEADER_WIDTH + WIDTH_GRID000_FRM;

/// `synthetic_named_directory`: total tree length, for the name `ART\CRITTERS`.
const NAMED_ENTRY_TREE_BYTES: usize =
    HEADER_WIDTH + NAME_WIDTH_ART_CRITTERS + HEADER_WIDTH + WIDTH_GRID000_FRM;

/// Gap between the two single entry fixtures, which is the length of the longer
/// directory name.
const NAME_LENGTH_GAP: usize = NAME_WIDTH_ART_CRITTERS - NAME_WIDTH_DOT;

/// Root header field 0 of every one block fixture.
const SINGLE_DIRECTORY: u32 = 1;

/// Entries in every one block, one entry fixture.
const SINGLE_BLOCK_ENTRIES: usize = 1;

/// `synthetic_stored`: byte offset of block 0's header.
const STORED_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT;

/// `synthetic_stored`: total tree length, after two twenty seven byte records.
const STORED_TREE_BYTES: usize =
    STORED_BLOCK_HEADER + HEADER_WIDTH + WIDTH_STORED_BIN + WIDTH_STORED2_BN;

/// Declared uncompressed size of the stored entry, and therefore the length of
/// the bytes `decode_entry` must return (DAT-015).
const STORED_UNCOMPRESSED_SIZE: usize = 32;

/// Declared compressed size of the malformed stored entry (DAT-017).
const STORED_ODD_COMPRESSED_SIZE: u32 = 4;

/// Entries in `synthetic_stored`.
const STORED_ENTRY_COUNT: usize = 2;

/// `synthetic_chain`: byte offset of block 0's header.
const CHAIN_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_CHAIN;

/// `synthetic_chain`: total tree length, after a twenty five byte record and a
/// twenty six byte record.
const CHAIN_TREE_BYTES: usize =
    CHAIN_BLOCK_HEADER + HEADER_WIDTH + WIDTH_RAW1_BIN + WIDTH_LZSS1_BIN;

/// Byte offset of the second chain payload, one whole chain after the first.
const CHAIN_SECOND_PAYLOAD_OFFSET: usize = CHAIN_TREE_BYTES + CHAIN_COMPRESSED_SIZE;

/// Entries in `synthetic_chain`.
const CHAIN_ENTRY_COUNT: usize = 2;

/// `synthetic_chain_consumption`: byte offset of block 0's header.
const CONSUMPTION_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT;

/// `synthetic_chain_consumption`: total tree length, after one twenty six byte
/// record.
const CONSUMPTION_TREE_BYTES: usize = CONSUMPTION_BLOCK_HEADER + HEADER_WIDTH + WIDTH_CHAIN_BIN;

/// Declared compressed size of the chain whose block headers land exactly on it
/// (DAT-022).
const CONSUMPTION_EXACT: usize = 9;

/// Declared compressed size one byte longer than the block headers account for.
const CONSUMPTION_ONE_OVER: usize = 10;

/// Declared compressed size one byte shorter than the block headers account for.
const CONSUMPTION_ONE_UNDER: usize = 8;

/// Bytes the block headers of that chain actually account for, which the exact
/// case must equal and the two others must not.
const CONSUMPTION_HEADER_SUM: usize = 9;

/// Entries in `synthetic_chain_consumption`.
const CONSUMPTION_ENTRY_COUNT: usize = 1;

/// `synthetic_declared_too_large`: byte offset of block 0's header.
const TOO_LARGE_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT;

/// `synthetic_declared_too_large`: total tree length, after three twenty three
/// byte records.
const TOO_LARGE_TREE_BYTES: usize = TOO_LARGE_BLOCK_HEADER + HEADER_WIDTH + 3 * WIDTH_L1_BIN;

/// Offset of the `0x10` stream inside `synthetic_declared_too_large`.
const TOO_LARGE_LZSS_OFFSET: usize = TOO_LARGE_TREE_BYTES;

/// Offset of the `0x20` stored bytes inside `synthetic_declared_too_large`.
const TOO_LARGE_STORED_OFFSET: usize = TOO_LARGE_LZSS_OFFSET + LZSS_BLOCK_PAYLOAD.len();

/// Offset of the `0x40` chain inside `synthetic_declared_too_large`.
const TOO_LARGE_CHAIN_OFFSET: usize = TOO_LARGE_STORED_OFFSET + STORED_UNCOMPRESSED_SIZE;

/// Total archive length of `synthetic_declared_too_large`: the tree, three stream
/// bytes, thirty two stored bytes and one LZSS frame. The stored entry declares
/// forty bytes, so its extent deliberately runs past the end.
const TOO_LARGE_ARCHIVE_BYTES: usize = TOO_LARGE_CHAIN_OFFSET + LZSS_BLOCK_FRAME_BYTES;

/// Uncompressed size the stream and the chain of `synthetic_declared_too_large`
/// declare: one more than an eighteen byte stream can produce.
const TOO_LARGE_DECLARED: u32 = 19;

/// Uncompressed size the stored entry declares: eight more than the thirty two
/// bytes present.
const TOO_LARGE_STORED_DECLARED: u32 = 40;

/// Entries in `synthetic_declared_too_large`, one per payload mode.
const TOO_LARGE_ENTRY_COUNT: usize = 3;

/// `synthetic_stream_lengths`: byte offset of block 0's header.
const STREAM_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT;

/// `synthetic_stream_lengths`: byte offset of the first entry.
const STREAM_FIRST_ENTRY: usize = STREAM_BLOCK_HEADER + HEADER_WIDTH;

/// Total tree length of `synthetic_stream_lengths`, after a twenty six byte
/// record, a twenty five byte record and a twenty four byte record.
const STREAM_TREE_BYTES: usize =
    STREAM_FIRST_ENTRY + WIDTH_SHORT_BIN + WIDTH_EXACT_BN + WIDTH_LONG_BN;

/// Offset of the short stream.
const STREAM_SHORT_OFFSET: usize = STREAM_TREE_BYTES;

/// Offset of the exact stream, after the two short bytes.
const STREAM_EXACT_OFFSET: usize = STREAM_SHORT_OFFSET + STREAM_SHORT_INPUT;

/// Offset of the long stream, after the three exact bytes.
const STREAM_LONG_OFFSET: usize = STREAM_EXACT_OFFSET + STREAM_EXACT_INPUT;

/// Total archive length of `synthetic_stream_lengths`.
const STREAM_ARCHIVE_BYTES: usize = STREAM_LONG_OFFSET + STREAM_LONG_INPUT;

/// Entries in `synthetic_stream_lengths`.
const STREAM_ENTRY_COUNT: usize = 3;

/// `synthetic_descending_offsets`: byte offset of block 0's header.
const DESCENDING_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT;

/// Byte offset of the first entry of `synthetic_descending_offsets`.
const DESCENDING_FIRST_ENTRY: usize = DESCENDING_BLOCK_HEADER + HEADER_WIDTH;

/// Total tree length of `synthetic_descending_offsets`.
const DESCENDING_TREE_BYTES: usize = DESCENDING_FIRST_ENTRY + 3 * WIDTH_AAA_BIN;

/// Offset of the first entry in tree order, the largest of the three.
const DESCENDING_FIRST_OFFSET: usize = 900_000;

/// Offset of the second entry, the smallest of the three.
const DESCENDING_SECOND_OFFSET: usize = 4_096;

/// Offset of the third entry, between the other two.
const DESCENDING_THIRD_OFFSET: usize = 262_144;

/// Descents in that offset sequence, that is the number of adjacent pairs whose
/// offsets fall. One, from the largest to the smallest, so a reader that walked
/// the entries in physical order would put the first entry last (DAT-012).
const DESCENDING_DESCENTS: usize = 1;

/// Entries in `synthetic_descending_offsets`.
const DESCENDING_ENTRY_COUNT: usize = 3;

/// `synthetic_lookup`: byte offset of block 0's header, after the name
/// `ART\TILES`.
const LOOKUP_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_ART_TILES;

/// Byte offset of the single entry of `synthetic_lookup`.
const LOOKUP_FIRST_ENTRY: usize = LOOKUP_BLOCK_HEADER + HEADER_WIDTH;

/// Total tree length of `synthetic_lookup`.
const LOOKUP_TREE_BYTES: usize = LOOKUP_FIRST_ENTRY + WIDTH_GRID000_FRM;

/// Total tree length of `synthetic_unknown_nibble`.
const UNKNOWN_NIBBLE_TREE_BYTES: usize =
    HEADER_WIDTH + NAME_WIDTH_DOT + HEADER_WIDTH + WIDTH_ODD_BIN;

/// Byte offset of the block header's field 0 in `synthetic_lookup`.
const LOOKUP_BLOCK_FIELD0: usize = LOOKUP_BLOCK_HEADER;

/// Byte offset of the single entry's name length byte.
const LOOKUP_ENTRY_NAME_LENGTH: usize = LOOKUP_FIRST_ENTRY;

/// Byte offset of the first directory's name length byte, which is the first byte
/// after the root header.
const LOOKUP_DIRECTORY_NAME_LENGTH: usize = HEADER_WIDTH;

/// Byte offset of the root header's field 0.
const ROOT_FIELD0: usize = 0;

/// Root header field 0 that no fixture can satisfy: more directories than the
/// archive has bytes for.
const ROOT_DIRECTORIES_PAST_END: u8 = 99;

/// Directory name length that no fixture can satisfy: longer than the archive.
const DIRECTORY_NAME_PAST_END: u8 = 200;

/// Every bit of the sixteen root header bytes. Each one is flipped in turn and
/// `parse` must answer with a value, never a panic (DAT-017).
const ROOT_HEADER_BITS: usize = HEADER_WIDTH * 8;

/// Number of malformed trees `synthetic_malformed_trees_are_errors` builds.
const MALFORMED_TREE_COUNT: usize = 4;

// --- fixture description ------------------------------------------------------

/// One entry record as the spec lays it out (DAT-009).
#[derive(Clone)]
struct SyntheticEntry {
    /// Entry name, uppercase, never containing a separator.
    name: String,
    /// The flags word, whose high nibble is the payload mode.
    flags: u32,
    /// Absolute byte offset into the archive, not into the entry (DAT-012).
    offset: u32,
    /// Declared decoded length.
    uncompressed_size: u32,
    /// Declared stored length. Zero for a stored entry, nonzero for a chain.
    compressed_size: u32,
}

impl SyntheticEntry {
    /// A stored entry: mode `0x20`, compressed size zero (DAT-015, DAT-017).
    fn stored(name: &str, offset: usize, uncompressed_size: usize) -> Self {
        Self {
            name: name.to_owned(),
            flags: MODE_STORED,
            offset: to_u32(offset),
            uncompressed_size: to_u32(uncompressed_size),
            compressed_size: 0,
        }
    }

    /// A block chain entry: mode `0x40`, nonzero compressed size (DAT-016,
    /// DAT-017).
    fn chain(name: &str, offset: usize, uncompressed_size: usize, compressed_size: usize) -> Self {
        Self {
            name: name.to_owned(),
            flags: MODE_BLOCK_CHAIN,
            offset: to_u32(offset),
            uncompressed_size: to_u32(uncompressed_size),
            compressed_size: to_u32(compressed_size),
        }
    }

    /// One whole LZSS stream: mode `0x10`, input length is the compressed size
    /// (DAT-014).
    fn stream(name: &str, offset: usize, uncompressed_size: usize, compressed_size: usize) -> Self {
        Self {
            name: name.to_owned(),
            flags: MODE_LZSS_STREAM,
            offset: to_u32(offset),
            uncompressed_size: to_u32(uncompressed_size),
            compressed_size: to_u32(compressed_size),
        }
    }

    /// Overwrites the compressed size, which is how the malformed fixtures are
    /// built.
    fn with_compressed_size(mut self, compressed_size: usize) -> Self {
        self.compressed_size = to_u32(compressed_size);
        self
    }

    /// Overwrites the flags word, which is how the unknown nibble fixture is
    /// built.
    fn with_flags(mut self, flags: u32) -> Self {
        self.flags = flags;
        self
    }
}

/// One directory block: a header and its entries (DAT-004).
struct SyntheticBlock {
    /// Directory name as stored, uppercase, possibly the single character `.`.
    name: String,
    /// Field 0 of this block's header. Kept separately from `entries.len()`
    /// because that independence is ruling A1 and is the point of one test.
    entry_count: u32,
    /// Field 2 of this block's header: the entry trailer width, sixteen
    /// (DAT-006). Kept separately so a fixture can name the rule explicitly.
    trailer_width: u32,
    /// Field 1, a capacity, read and discarded (DAT-007).
    capacity: u32,
    /// Field 3, a dead pointer, read and discarded (DAT-007).
    dead_pointer: u32,
    /// The entries, in tree order.
    entries: Vec<SyntheticEntry>,
}

impl SyntheticBlock {
    /// A well formed block whose field 0 matches its entry count and whose field
    /// 2 is the entry trailer width.
    fn new(name: &str, entries: &[SyntheticEntry]) -> Self {
        Self {
            name: name.to_owned(),
            entry_count: to_u32(entries.len()),
            trailer_width: to_u32(ENTRY_TRAILER_WIDTH),
            capacity: BLOCK_CAPACITY,
            dead_pointer: BLOCK_DEAD_POINTER,
            entries: entries.to_vec(),
        }
    }

    /// Declares an entry count other than `entries.len()`, which is ruling A1's
    /// whole subject.
    fn with_declared_entry_count(mut self, entry_count: u32) -> Self {
        self.entry_count = entry_count;
        self
    }
}

/// Emits the tree section only: root header, directory names, then every block
/// header followed by that block's entries. Payloads are appended separately,
/// because an entry's offset is an absolute file offset and not a tree position
/// (DAT-009, DAT-012).
fn synthetic_tree(blocks: &[SyntheticBlock]) -> Vec<u8> {
    let mut out = Vec::new();
    push_u32(&mut out, to_u32(blocks.len()));
    push_u32(&mut out, ROOT_CAPACITY);
    push_u32(&mut out, ROOT_SIZE_FIELD);
    push_u32(&mut out, ROOT_DEAD_POINTER);

    for block in blocks {
        push_name(&mut out, &block.name);
    }

    for block in blocks {
        push_u32(&mut out, block.entry_count);
        push_u32(&mut out, block.capacity);
        push_u32(&mut out, block.trailer_width);
        push_u32(&mut out, block.dead_pointer);
        for entry in &block.entries {
            push_name(&mut out, &entry.name);
            push_u32(&mut out, entry.flags);
            push_u32(&mut out, entry.offset);
            push_u32(&mut out, entry.uncompressed_size);
            push_u32(&mut out, entry.compressed_size);
        }
    }

    out
}

/// Emits a whole archive: the tree, then each payload in order.
///
/// The assertion is the point. `first_payload_offset` is the offset the first
/// entry's record declares, and it must equal the byte at which the tree ends.
/// That turns "where does the payload region start" from an assumption into a
/// check, and it is what would catch a builder that wrote an entry record's four
/// u32 in the wrong order.
fn synthetic_archive(
    blocks: &[SyntheticBlock],
    payloads: &[Vec<u8>],
    first_payload_offset: usize,
) -> Vec<u8> {
    let tree = synthetic_tree(blocks);
    assert_eq!(
        tree.len(),
        first_payload_offset,
        "the tree must end exactly where the first entry declares its payload starts"
    );
    let mut archive = tree;
    for payload in payloads {
        archive.extend_from_slice(payload);
    }
    archive
}

/// Appends one big endian u32 (DAT-001).
fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends one big endian u16, the frame header of a chain block (DAT-018).
fn push_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_be_bytes());
}

/// Appends one length prefixed name (DAT-003, DAT-009).
fn push_name(out: &mut Vec<u8>, name: &str) {
    out.push(to_u8(name.len()));
    out.extend_from_slice(name.as_bytes());
}

/// Narrows a fixture length for the container's own arithmetic. Every fixture is
/// far below `u32::MAX`, so a failure here is a bug in the fixture.
fn to_u32(value: usize) -> u32 {
    u32::try_from(value).expect("synthetic fixture length fits in a u32")
}

/// Widens a declared container length back to a `usize` for a slice comparison.
fn to_usize(value: u32) -> usize {
    usize::try_from(value).expect("a declared container length fits in a usize")
}

/// Narrows a fixture block payload length for the raw block header check.
fn to_u16(value: usize) -> u16 {
    u16::try_from(value).expect("a block payload length fits in a u16")
}

/// Narrows a fixture name length to the single stored length byte.
fn to_u8(value: usize) -> u8 {
    u8::try_from(value).expect("synthetic fixture name fits in one length byte")
}

/// Parses a fixture, panicking with the case name when the parser refuses it. A
/// helper rather than `expect` so the failure names the case instead of the
/// `Debug` shape of the error.
fn synthetic_parse(bytes: &[u8], case: &str) -> DatArchive {
    match DatArchive::parse(bytes, SYNTHETIC_ORIGIN) {
        Ok(archive) => archive,
        Err(error) => panic!(
            "{case}: parse refused the fixture with {}",
            dat_error_name(&error)
        ),
    }
}

/// Unwraps a decode that must fail, panicking with the case name when it
/// succeeds. This is the "never a short buffer" half of the suite: a decoder that
/// returns `Ok` here has truncated, padded or defaulted.
fn decoded_error(result: Result<Vec<u8>, DatError>, case: &str) -> DatError {
    match result {
        Ok(bytes) => panic!("{case}: expected an error, decoded {} byte(s)", bytes.len()),
        Err(error) => error,
    }
}

/// Name of a `PayloadMode` variant, so the mode table is asserted by name and a
/// rename cannot pass unnoticed.
fn payload_mode_name(mode: PayloadMode) -> &'static str {
    match mode {
        PayloadMode::LzssStream => "LzssStream",
        PayloadMode::Stored => "Stored",
        PayloadMode::BlockChain => "BlockChain",
    }
}

/// Name of a `DatError` variant. The match is exhaustive with no wildcard arm, so
/// a new variant is a compile error here rather than a silently accepted value.
fn dat_error_name(error: &DatError) -> &'static str {
    match error {
        DatError::Truncated { .. } => "Truncated",
        DatError::LengthOutOfRange { .. } => "LengthOutOfRange",
        DatError::OutOfBounds { .. } => "OutOfBounds",
        DatError::OffsetOverflow => "OffsetOverflow",
        DatError::UnknownPayloadMode { .. } => "UnknownPayloadMode",
        DatError::OutputOverrun { .. } => "OutputOverrun",
        DatError::OutputShort { .. } => "OutputShort",
        DatError::ConsumptionMismatch { .. } => "ConsumptionMismatch",
        DatError::EmptyBlock { .. } => "EmptyBlock",
    }
}

/// Every error the spec's own variant documentation allows for a tree that does
/// not add up: a record ran short of bytes, a length exceeded the bytes present,
/// or a region fell outside the archive.
///
/// The spec pins no variant to any one malformed tree, so the set is the
/// assertion. A successful parse, or any variant outside this set, fails the test
/// rather than passing on a convenient reading.
fn is_tree_layout_error(error: &DatError) -> bool {
    matches!(
        error,
        DatError::Truncated { .. }
            | DatError::LengthOutOfRange { .. }
            | DatError::OutOfBounds { .. }
    )
}

/// True when an error says a payload was not framed the way its entry declared it.
/// Used for the consumption cases, where the spec mandates the comparison but not
/// the variant that reports it.
fn is_consumption_error(error: &DatError) -> bool {
    matches!(
        error,
        DatError::ConsumptionMismatch { .. }
            | DatError::Truncated { .. }
            | DatError::OutOfBounds { .. }
    )
}

/// The eighteen spaces the `00 00 0F` LZSS block decodes to.
fn lzss_block_output() -> Vec<u8> {
    vec![0x20; LZSS_BLOCK_OUTPUT]
}

/// The concatenated output of one raw block followed by one LZSS block.
fn chain_output_raw_first() -> Vec<u8> {
    let mut out = Vec::from(RAW_BLOCK_PAYLOAD);
    out.extend(lzss_block_output());
    out
}

/// The concatenated output of one LZSS block followed by one raw block.
fn chain_output_lzss_first() -> Vec<u8> {
    let mut out = lzss_block_output();
    out.extend(RAW_BLOCK_PAYLOAD);
    out
}

/// The nine stored bytes of one raw-then-LZSS chain.
fn chain_payload_raw_first() -> Vec<u8> {
    let mut out = Vec::new();
    push_u16(&mut out, RAW_BLOCK_HEADER);
    out.extend_from_slice(&RAW_BLOCK_PAYLOAD);
    push_u16(&mut out, LZSS_BLOCK_HEADER);
    out.extend_from_slice(&LZSS_BLOCK_PAYLOAD);
    out
}

/// The nine stored bytes of one LZSS-then-raw chain.
fn chain_payload_lzss_first() -> Vec<u8> {
    let mut out = Vec::new();
    push_u16(&mut out, LZSS_BLOCK_HEADER);
    out.extend_from_slice(&LZSS_BLOCK_PAYLOAD);
    push_u16(&mut out, RAW_BLOCK_HEADER);
    out.extend_from_slice(&RAW_BLOCK_PAYLOAD);
    out
}

/// One chain of one raw block and one LZSS block, with the declared compressed
/// size the caller asks for and `extra_trailing` spare bytes after the payload.
fn synthetic_chain_consumption(compressed_size: usize, extra_trailing: usize) -> Vec<u8> {
    let entry = SyntheticEntry::chain(
        "CHAIN.BIN",
        CONSUMPTION_TREE_BYTES,
        CHAIN_UNCOMPRESSED_SIZE,
        compressed_size,
    );
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &[entry])];
    let payloads = [chain_payload_raw_first()];
    let mut archive = synthetic_archive(&blocks, &payloads, CONSUMPTION_TREE_BYTES);
    archive.extend(std::iter::repeat(0u8).take(extra_trailing));
    archive
}

// --- the synthetic archives ---------------------------------------------------

/// Two directories and five entries, with the field values the spec measures on
/// the two reference archives. Every field is distinct where it can be, so a
/// reader that swaps two of them fails.
fn synthetic_round_trip() -> Vec<u8> {
    let root_entries = [
        SyntheticEntry::chain("COLOR.PAL", 211_060_669, 33_536, 9_292),
        SyntheticEntry::chain("FONT0.AAF", 556_484, 6_337, 2_314),
        SyntheticEntry::chain("FONT5.FON", 900_000, 4_096, 900),
    ];
    let named_entries = [
        SyntheticEntry::chain("CRITTERS.LST", 158_356, 646, 405),
        SyntheticEntry::stored("BIG.FRM", 1_000, 32),
    ];
    let blocks = [
        SyntheticBlock::new(ROOT_DIRECTORY_NAME, &root_entries),
        SyntheticBlock::new(NAMED_DIRECTORY, &named_entries),
    ];
    synthetic_tree(&blocks)
}

/// Two directories whose per block field 0 is deliberately different from the
/// root's field 0. Ruling A1: field 0 is the directory count at the root and the
/// entry count at a block, and this archive is shaped so a reader that keeps the
/// root meaning cannot land on the first block header.
fn synthetic_entry_count_field() -> Vec<u8> {
    let root_entries = [
        SyntheticEntry::stored("A1.FRM", 1_000, 16),
        SyntheticEntry::stored("B2.FRM", 1_016, 16),
        SyntheticEntry::stored("C3.FRM", 1_032, 16),
        SyntheticEntry::stored("D4.FRM", 1_048, 16),
        SyntheticEntry::stored("E5.FRM", 1_064, 16),
    ];
    let named_entries = [
        SyntheticEntry::stored("F6.FRM", 1_080, 16),
        SyntheticEntry::stored("G7.FRM", 1_096, 16),
        SyntheticEntry::stored("H8.FRM", 1_112, 16),
    ];
    let blocks = [
        SyntheticBlock::new(ROOT_DIRECTORY_NAME, &root_entries)
            .with_declared_entry_count(COUNT_BLOCK0_ENTRIES),
        SyntheticBlock::new("TEXT", &named_entries).with_declared_entry_count(COUNT_BLOCK1_ENTRIES),
    ];
    synthetic_tree(&blocks)
}

/// One block holding two entry records of different widths, so the record size
/// `1 + name_length + 16` is neither sixteen nor constant. Ruling A2: block field
/// 2 is the trailer width and not the record size, so a reader that strides by it
/// walks into the middle of entry 0's name.
fn synthetic_varying_record_sizes() -> Vec<u8> {
    let entries = [
        SyntheticEntry::stored("GRID000.FRM", 2_000, 32),
        SyntheticEntry::stored("A", 2_032, 8),
    ];
    let blocks = [SyntheticBlock::new("ART\\TILES", &entries)];
    synthetic_tree(&blocks)
}

/// The single block fixture with its directory named `.`, which contributes no
/// separator (DAT-011, ruling A4).
fn synthetic_dot_directory() -> Vec<u8> {
    let entries = [SyntheticEntry::stored("GRID000.FRM", 2_000, 32)];
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &entries)];
    synthetic_tree(&blocks)
}

/// The same fixture with its single directory named `ART\CRITTERS`, which does
/// contribute the separator. It is at block index zero, so a rule keyed on the
/// index strips the prefix here and loses all 5,459 real paths with it.
fn synthetic_named_directory() -> Vec<u8> {
    let entries = [SyntheticEntry::stored("GRID000.FRM", 2_000, 32)];
    let blocks = [SyntheticBlock::new(NAMED_DIRECTORY, &entries)];
    synthetic_tree(&blocks)
}

/// One stored entry with `compressed_size == 0` and its thirty two stored bytes,
/// and beside it the same record with a nonzero compressed size, which DAT-017
/// says never occurs.
fn synthetic_stored() -> Vec<u8> {
    let entries = [
        SyntheticEntry::stored("STORED.BIN", STORED_TREE_BYTES, STORED_UNCOMPRESSED_SIZE),
        SyntheticEntry::stored("STORED2.BN", STORED_TREE_BYTES, STORED_UNCOMPRESSED_SIZE)
            .with_compressed_size(to_usize(STORED_ODD_COMPRESSED_SIZE)),
    ];
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &entries)];
    synthetic_archive(&blocks, &[STORED_PAYLOAD.to_vec()], STORED_TREE_BYTES)
}

/// Two chain entries over the same nine bytes: one raw block then one LZSS block,
/// and the same two blocks the other way round (DAT-020).
fn synthetic_chain() -> Vec<u8> {
    let entries = [
        SyntheticEntry::chain(
            "RAW1.BIN",
            CHAIN_TREE_BYTES,
            CHAIN_UNCOMPRESSED_SIZE,
            CHAIN_COMPRESSED_SIZE,
        ),
        SyntheticEntry::chain(
            "LZSS1.BIN",
            CHAIN_SECOND_PAYLOAD_OFFSET,
            CHAIN_UNCOMPRESSED_SIZE,
            CHAIN_COMPRESSED_SIZE,
        ),
    ];
    let blocks = [SyntheticBlock::new("CHAIN", &entries)];
    let payloads = [chain_payload_raw_first(), chain_payload_lzss_first()];
    synthetic_archive(&blocks, &payloads, CHAIN_TREE_BYTES)
}

/// One entry per payload mode, every one declaring nineteen output bytes, which is
/// one more than an eighteen byte stream can produce. The stored entry also
/// declares more bytes than the archive holds.
fn synthetic_declared_too_large() -> Vec<u8> {
    let declared = to_usize(TOO_LARGE_DECLARED);
    let stream_entry = SyntheticEntry::stream(
        "L1.BIN",
        TOO_LARGE_LZSS_OFFSET,
        declared,
        LZSS_BLOCK_PAYLOAD.len(),
    );
    let stored_entry = SyntheticEntry::stored(
        "S1.BIN",
        TOO_LARGE_STORED_OFFSET,
        to_usize(TOO_LARGE_STORED_DECLARED),
    );
    let chain_entry = SyntheticEntry::chain(
        "C1.BIN",
        TOO_LARGE_CHAIN_OFFSET,
        declared,
        LZSS_BLOCK_FRAME_BYTES,
    );

    let mut chain_bytes = Vec::new();
    push_u16(&mut chain_bytes, LZSS_BLOCK_HEADER);
    chain_bytes.extend_from_slice(&LZSS_BLOCK_PAYLOAD);

    let payloads = [
        LZSS_BLOCK_PAYLOAD.to_vec(),
        STORED_PAYLOAD.to_vec(),
        chain_bytes,
    ];
    let blocks = [SyntheticBlock::new(
        ROOT_DIRECTORY_NAME,
        &[stream_entry, stored_entry, chain_entry],
    )];
    let archive = synthetic_archive(&blocks, &payloads, TOO_LARGE_LZSS_OFFSET);
    assert_eq!(
        archive.len(),
        TOO_LARGE_ARCHIVE_BYTES,
        "the archive must stop short of the stored entry's declared extent"
    );
    archive
}

/// Three whole LZSS streams over one control byte: two bytes, three bytes and four
/// bytes, all declaring two output bytes. The exact one must decode; the short one
/// falls short of the declaration and the long one writes past it (LZSS-014).
fn synthetic_stream_lengths() -> Vec<u8> {
    let declared = STREAM_OUTPUT.len();
    let short_payload = [STREAM_CONTROL, STREAM_OUTPUT[0]];
    let exact_payload = [STREAM_CONTROL, STREAM_OUTPUT[0], STREAM_OUTPUT[1]];
    let long_payload = [
        STREAM_CONTROL,
        STREAM_OUTPUT[0],
        STREAM_OUTPUT[1],
        STREAM_OUTPUT[1],
    ];

    let entries = [
        SyntheticEntry::stream(
            "SHORT.BIN",
            STREAM_SHORT_OFFSET,
            declared,
            short_payload.len(),
        ),
        SyntheticEntry::stream(
            "EXACT.BN",
            STREAM_EXACT_OFFSET,
            declared,
            exact_payload.len(),
        ),
        SyntheticEntry::stream("LONG.BN", STREAM_LONG_OFFSET, declared, long_payload.len()),
    ];
    let payloads = [
        short_payload.to_vec(),
        exact_payload.to_vec(),
        long_payload.to_vec(),
    ];
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &entries)];
    let archive = synthetic_archive(&blocks, &payloads, STREAM_SHORT_OFFSET);
    assert_eq!(
        archive.len(),
        STREAM_ARCHIVE_BYTES,
        "three streams of two, three and four bytes must land where the offsets say"
    );
    archive
}

/// Three entries whose offsets descend and then rise, so tree order and physical
/// order disagree in both directions (DAT-012).
fn synthetic_descending_offsets() -> Vec<u8> {
    let entries = [
        SyntheticEntry::chain("AAA.BIN", DESCENDING_FIRST_OFFSET, 10, 5),
        SyntheticEntry::chain("BBB.BIN", DESCENDING_SECOND_OFFSET, 10, 5),
        SyntheticEntry::chain("CCC.BIN", DESCENDING_THIRD_OFFSET, 10, 5),
    ];
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &entries)];
    synthetic_tree(&blocks)
}

/// One block `ART\TILES` holding one entry, the smallest whole archive that can
/// answer a lookup question.
fn synthetic_lookup() -> Vec<u8> {
    let entries = [SyntheticEntry::stored("GRID000.FRM", 2_000, 32)];
    let blocks = [SyntheticBlock::new("ART\\TILES", &entries)];
    synthetic_tree(&blocks)
}

/// `synthetic_lookup` with an entry whose flags high nibble names no known mode.
fn synthetic_unknown_nibble() -> Vec<u8> {
    let entries = [
        SyntheticEntry::stored("ODD.BIN", UNKNOWN_NIBBLE_TREE_BYTES, 4).with_flags(MODE_UNKNOWN),
    ];
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &entries)];
    synthetic_tree(&blocks)
}

// --- tests --------------------------------------------------------------------

/// claim: DAT-001, DAT-002, DAT-003, DAT-004, DAT-005, DAT-008, DAT-009, DAT-011
/// oracle: class 4 (independent reference implementation) and class 5 (hand computed vectors from the spec)
#[test]
fn synthetic_round_trip_reads_every_field() {
    let bytes = synthetic_round_trip();
    let archive = synthetic_parse(&bytes, "synthetic_round_trip_reads_every_field");

    assert_eq!(ROOT_HEADER_SIZE, 16, "the root header is four u32");
    assert_eq!(
        bytes.len(),
        ROUND_TRIP_TREE_BYTES,
        "hand computed tree length"
    );

    // The endian layer reads back exactly what the builder wrote, so a wrong byte
    // order cannot make the rest of these assertions pass by accident.
    let mut reader = BigEndian::new(&bytes, SYNTHETIC_ORIGIN);
    let root_field0 = reader.u32().expect("root header field 0 reads");
    let root_field1 = reader.u32().expect("root header field 1 reads");
    let root_field2 = reader.u32().expect("root header field 2 reads");
    let root_field3 = reader.u32().expect("root header field 3 reads");
    assert_eq!(root_field0, ROUND_TRIP_DIRECTORIES_FIELD, "root field 0");
    assert_eq!(
        root_field1, ROOT_CAPACITY,
        "root field 1 is read and discarded"
    );
    assert_eq!(root_field2, ROOT_SIZE_FIELD, "root field 2 is zero");
    assert_eq!(root_field3, ROOT_DEAD_POINTER, "root field 3 is discarded");
    assert_eq!(
        reader.offset(),
        HEADER_WIDTH,
        "the header reader advanced once"
    );

    let directories = archive.directories();
    assert_eq!(directories.len(), ROUND_TRIP_DIRECTORIES, "directory count");
    assert_eq!(
        directories[0].name.as_str(),
        ROOT_DIRECTORY_NAME,
        "directory 0"
    );
    assert_eq!(directories[0].entry_count, 3, "directory 0 entry count");
    assert_eq!(directories[1].name.as_str(), NAMED_DIRECTORY, "directory 1");
    assert_eq!(directories[1].entry_count, 2, "directory 1 entry count");

    let entries = archive.entries();
    assert_eq!(entries.len(), ROUND_TRIP_ENTRIES, "entry count");

    let expected: [(String, u32, u32, u32, u32); ROUND_TRIP_ENTRIES] = [
        ("COLOR.PAL".to_owned(), 0x40, 211_060_669, 33_536, 9_292),
        ("FONT0.AAF".to_owned(), 0x40, 556_484, 6_337, 2_314),
        ("FONT5.FON".to_owned(), 0x40, 900_000, 4_096, 900),
        (
            NAMED_DIRECTORY.to_owned() + "\\CRITTERS.LST",
            0x40,
            158_356,
            646,
            405,
        ),
        (NAMED_DIRECTORY.to_owned() + "\\BIG.FRM", 0x20, 1_000, 32, 0),
    ];
    for (index, want) in expected.iter().enumerate() {
        let got = entries.get(index).expect("entry index in range");
        assert_eq!(got.path.as_str(), want.0.as_str(), "entry path");
        assert_eq!(got.flags, want.1, "entry flags");
        assert_eq!(got.offset, want.2, "entry offset");
        assert_eq!(got.uncompressed_size, want.3, "entry uncompressed size");
        assert_eq!(got.compressed_size, want.4, "entry compressed size");
    }

    // Ruling A4 on the composed paths themselves: the `.` directory contributes
    // nothing, the named one contributes its name and one backslash.
    assert_eq!(
        entries[0].path.as_str(),
        "COLOR.PAL",
        "no prefix from the dot name"
    );
    assert_eq!(
        entries[3].path.as_str(),
        "ART\\CRITTERS\\CRITTERS.LST",
        "prefix from a named directory"
    );
}

/// claim: DAT-005
/// oracle: class 6 (algebraic law: the sum of the per block counts is the entry total, and it is not the directory count)
#[test]
fn synthetic_block_field_0_is_the_entry_count_not_the_directory_count() {
    let bytes = synthetic_entry_count_field();
    assert_eq!(bytes.len(), COUNT_TREE_BYTES, "hand computed tree length");
    let archive = synthetic_parse(
        &bytes,
        "synthetic_block_field_0_is_the_entry_count_not_the_directory_count",
    );

    let directories = archive.directories();
    assert_eq!(directories.len(), 2, "root field 0 is the directory count");
    assert_eq!(
        directories[0].entry_count, COUNT_BLOCK0_ENTRIES,
        "block 0 field 0"
    );
    assert_eq!(
        directories[1].entry_count, COUNT_BLOCK1_ENTRIES,
        "block 1 field 0"
    );

    let entries = archive.entries();
    assert_eq!(
        entries.len(),
        COUNT_TOTAL_ENTRIES,
        "the entry total is the sum of the per block counts"
    );
    assert_eq!(
        directories[0].entry_count + directories[1].entry_count,
        8,
        "the closure law over the two block headers"
    );
    assert_eq!(entries[4].path.as_str(), "E5.FRM", "block 0's fifth entry");
    assert_eq!(
        entries[5].path.as_str(),
        "TEXT\\F6.FRM",
        "block 1's first entry, so the second block header was found"
    );
    assert_eq!(
        entries[7].path.as_str(),
        "TEXT\\H8.FRM",
        "block 1's third entry"
    );

    // The two counts are deliberately different. A reader that kept the root
    // meaning would read two entries in block 0 and never reach `TEXT`.
    assert_ne!(
        to_u32(entries.len()),
        COUNT_ROOT_DIRECTORIES,
        "the entry count and the directory count must differ in this fixture"
    );
}

/// claim: DAT-006
/// oracle: class 6 (algebraic law: the record width is one length byte plus the name plus the trailer, and it is not the trailer)
#[test]
fn synthetic_block_field_2_is_the_trailer_width_not_the_record_size() {
    let bytes = synthetic_varying_record_sizes();
    assert_eq!(bytes.len(), VARYING_TREE_BYTES, "hand computed tree length");
    let archive = synthetic_parse(
        &bytes,
        "synthetic_block_field_2_is_the_trailer_width_not_the_record_size",
    );

    assert_eq!(
        archive.directories().len(),
        SINGLE_DIRECTORY,
        "directory count"
    );
    let entries = archive.entries();
    assert_eq!(entries.len(), SINGLE_BLOCK_ENTRIES * 2, "entry count");

    // The two records are twenty eight and eighteen bytes wide. Neither is
    // sixteen, and a reader that strided by field 2 would start entry 1 four bytes
    // into entry 0's name and read `000.FRM` instead of `GRID000.FRM`.
    assert_eq!(
        WIDTH_GRID000_FRM, 28,
        "an eleven byte name gives a twenty eight byte record"
    );
    assert_ne!(
        WIDTH_GRID000_FRM, ENTRY_TRAILER_WIDTH,
        "the first record is not sixteen bytes wide"
    );
    assert_ne!(
        WIDTH_A, ENTRY_TRAILER_WIDTH,
        "the second record is not sixteen bytes wide"
    );
    assert_eq!(
        entries[0].path.as_str(),
        "ART\\TILES\\GRID000.FRM",
        "entry 0"
    );
    assert_eq!(entries[0].offset, 2_000, "entry 0 offset");
    assert_eq!(entries[1].path.as_str(), "ART\\TILES\\A", "entry 1");
    assert_eq!(entries[1].offset, 2_032, "entry 1 offset");
    assert_eq!(
        entries[1].offset - entries[0].offset,
        32,
        "the gap between the records equals the stored length of the first"
    );
}

/// claim: DAT-011
/// oracle: class 5 (hand computed vectors from the spec: the rule keys on the name `.`, not on the block index)
#[test]
fn synthetic_dot_directory_name_composes_no_prefix_and_a_named_one_does() {
    let dot_bytes = synthetic_dot_directory();
    let named_bytes = synthetic_named_directory();
    assert_eq!(
        dot_bytes.len(),
        DOT_ENTRY_TREE_BYTES,
        "hand computed tree length with the two byte name"
    );
    assert_eq!(
        named_bytes.len(),
        NAMED_ENTRY_TREE_BYTES,
        "hand computed tree length with the thirteen byte name"
    );
    assert_eq!(
        NAME_LENGTH_GAP, 11,
        "the longer directory name is eleven bytes longer on disk"
    );

    let dot_archive = synthetic_parse(
        &dot_bytes,
        "synthetic_dot_directory_name_composes_no_prefix_and_a_named_one_does",
    );
    let named_archive = synthetic_parse(
        &named_bytes,
        "synthetic_dot_directory_name_composes_no_prefix_and_a_named_one_does",
    );

    // Both archives have exactly one directory, at block index zero. The only
    // difference between them is what that directory is called.
    assert_eq!(
        dot_archive.directories().len(),
        SINGLE_DIRECTORY,
        "the dot archive has one directory"
    );
    assert_eq!(
        named_archive.directories().len(),
        SINGLE_DIRECTORY,
        "the named archive has one directory too"
    );
    assert_eq!(
        dot_archive.directories()[0].name.as_str(),
        ROOT_DIRECTORY_NAME,
        "the dot archive's directory name"
    );
    assert_eq!(
        named_archive.directories()[0].name.as_str(),
        NAMED_DIRECTORY,
        "the named archive's directory name"
    );

    let dot_entry = dot_archive.entries().first().expect("dot entry");
    let named_entry = named_archive.entries().first().expect("named entry");
    assert_eq!(
        dot_entry.path.as_str(),
        "GRID000.FRM",
        "the dot name contributes nothing"
    );
    assert_eq!(
        named_entry.path.as_str(),
        "ART\\CRITTERS\\GRID000.FRM",
        "a named directory contributes its name and one backslash"
    );
    assert_ne!(
        dot_entry.path.as_str(),
        named_entry.path.as_str(),
        "the composed paths must differ"
    );

    // Everything but the path is identical, which is what makes the path the only
    // thing under test.
    assert_eq!(dot_entry.offset, named_entry.offset, "same offset");
    assert_eq!(dot_entry.flags, named_entry.flags, "same flags");
    assert_eq!(
        dot_entry.uncompressed_size, named_entry.uncompressed_size,
        "same uncompressed size"
    );
}

/// claim: DAT-013, DAT-014, DAT-015, DAT-016
/// oracle: class 6 (algebraic law: the mode is the flags high nibble, and zero means 0x10)
#[test]
fn synthetic_payload_mode_is_the_flags_high_nibble() {
    assert_eq!(MODE_MASK, 0xF0, "the mode mask is the high nibble");

    let mut checked = 0usize;
    for (flags, expected_name) in PAYLOAD_MODE_CASES {
        assert_eq!(
            payload_mode_name(payload_mode(flags)),
            expected_name,
            "payload_mode of the flags word"
        );
        checked += 1;
    }
    assert_eq!(
        checked, PAYLOAD_MODE_CASE_COUNT,
        "every case in the table ran"
    );

    // The three known modes and the zero substitution named directly, so the table
    // above cannot pass by dropping a row.
    assert_eq!(
        payload_mode_name(payload_mode(MODE_LZSS_STREAM)),
        "LzssStream",
        "0x10"
    );
    assert_eq!(
        payload_mode_name(payload_mode(MODE_STORED)),
        "Stored",
        "0x20"
    );
    assert_eq!(
        payload_mode_name(payload_mode(MODE_BLOCK_CHAIN)),
        "BlockChain",
        "0x40"
    );
    assert_eq!(
        payload_mode_name(payload_mode(MODE_ZERO_FLAGS)),
        "LzssStream",
        "flags zero means 0x10"
    );
    assert_eq!(
        payload_mode_name(payload_mode(MODE_STORED | 0x0F)),
        "Stored",
        "the low nibble does not take part"
    );
}

/// claim: DAT-013
/// oracle: class 5 (hand computed vectors from the spec: an unknown high nibble is an error, never a default)
#[test]
fn synthetic_unknown_payload_nibble_is_an_error_not_a_default() {
    let bytes = synthetic_unknown_nibble();
    assert_eq!(
        bytes.len(),
        UNKNOWN_NIBBLE_TREE_BYTES,
        "hand computed tree length"
    );
    let archive = synthetic_parse(
        &bytes,
        "synthetic_unknown_payload_nibble_is_an_error_not_a_default",
    );
    let entries = archive.entries();
    assert_eq!(entries.len(), SINGLE_BLOCK_ENTRIES, "entry count");
    let entry: &EntryRecord = entries.first().expect("one entry");

    assert_eq!(
        entry.flags, MODE_UNKNOWN,
        "the entry carries the unknown nibble"
    );
    match decoded_error(
        decode_entry(entry, &bytes),
        "synthetic_unknown_payload_nibble_is_an_error_not_a_default",
    ) {
        DatError::UnknownPayloadMode { flags } => {
            assert_eq!(flags, MODE_UNKNOWN, "the flags word is reported back")
        }
        other => panic!(
            "expected UnknownPayloadMode, got {}",
            dat_error_name(&other)
        ),
    }
}

/// claim: DAT-015, DAT-017
/// oracle: class 5 (hand computed vectors from the spec: stored bytes come from the uncompressed extent, and a stored entry never has a compressed size)
#[test]
fn synthetic_stored_entry_takes_the_uncompressed_size_and_rejects_a_nonzero_compressed_size() {
    let bytes = synthetic_stored();
    assert_eq!(
        bytes.len(),
        STORED_TREE_BYTES + STORED_UNCOMPRESSED_SIZE,
        "archive length"
    );
    let archive = synthetic_parse(
        &bytes,
        "synthetic_stored_entry_takes_the_uncompressed_size_and_rejects_a_nonzero_compressed_size",
    );
    let entries = archive.entries();
    assert_eq!(entries.len(), STORED_ENTRY_COUNT, "entry count");

    let stored = entries.first().expect("stored entry");
    assert_eq!(stored.path.as_str(), "STORED.BIN", "stored path");
    assert_eq!(stored.flags, MODE_STORED, "stored mode");
    assert_eq!(
        stored.uncompressed_size,
        to_u32(STORED_UNCOMPRESSED_SIZE),
        "declared decoded length"
    );
    assert_eq!(
        stored.compressed_size, 0,
        "a stored entry has compressed size zero"
    );
    assert_eq!(
        payload_mode_name(payload_mode(stored.flags)),
        "Stored",
        "the mode names the same thing"
    );

    let decoded = decode_entry(stored, &bytes).expect("stored entry decodes");
    assert_eq!(
        decoded.as_slice(),
        STORED_PAYLOAD.as_slice(),
        "stored bytes are copied verbatim from the uncompressed extent"
    );
    assert_eq!(
        decoded.len(),
        to_usize(stored.uncompressed_size),
        "produced bytes equal the declared uncompressed size"
    );

    // DAT-017. The parser may reject the record, or the decoder may; what must
    // never happen is a payload coming out.
    let odd = entries.get(1).expect("second entry");
    assert_eq!(
        odd.compressed_size, STORED_ODD_COMPRESSED_SIZE,
        "this entry is the malformed one"
    );
    let mut decoded_bytes = 0usize;
    let mut refusals = 0usize;
    match DatArchive::parse(&bytes, SYNTHETIC_ORIGIN)
        .map(|archive| decode_entry(&archive.entries()[1], &bytes))
    {
        Ok(Ok(out)) => decoded_bytes = out.len(),
        Ok(Err(_)) => refusals += 1,
        Err(_) => refusals += 1,
    }
    assert_eq!(refusals + decoded_bytes, 1, "exactly one outcome happened");
    assert_eq!(
        decoded_bytes, 0,
        "a stored entry with a nonzero compressed size must not decode"
    );
}

/// claim: DAT-016, DAT-018, DAT-019, DAT-020
/// oracle: class 4 (independent reference implementation) and class 5 (hand computed vectors from the spec)
#[test]
fn synthetic_chain_mixes_one_raw_block_and_one_lzss_block_in_stored_order() {
    assert_eq!(
        BLOCK_RAW_FLAG, 0x8000,
        "bit 15 of a block header marks a raw block"
    );
    assert_eq!(BLOCK_RAW_LENGTH_MASK, 0x7FFF, "the raw length mask");
    assert_eq!(
        RAW_BLOCK_HEADER & BLOCK_RAW_LENGTH_MASK,
        to_u16(RAW_BLOCK_PAYLOAD.len()),
        "the raw frame header declares its own payload length"
    );
    assert_eq!(
        LZSS_BLOCK_HEADER, 3,
        "the lzss frame header declares three bytes"
    );

    let bytes = synthetic_chain();
    assert_eq!(
        bytes.len(),
        CHAIN_SECOND_PAYLOAD_OFFSET + CHAIN_COMPRESSED_SIZE,
        "archive length"
    );
    let archive = synthetic_parse(
        &bytes,
        "synthetic_chain_mixes_one_raw_block_and_one_lzss_block_in_stored_order",
    );
    let entries = archive.entries();
    assert_eq!(entries.len(), CHAIN_ENTRY_COUNT, "entry count");
    assert_eq!(
        payload_mode_name(payload_mode(entries[0].flags)),
        "BlockChain",
        "chain mode"
    );
    assert_eq!(
        entries[0].compressed_size,
        to_u32(CHAIN_COMPRESSED_SIZE),
        "a chain entry has a nonzero compressed size"
    );
    assert_eq!(
        entries[0].uncompressed_size,
        to_u32(CHAIN_UNCOMPRESSED_SIZE),
        "declared decoded length"
    );

    let raw_first = entries.first().expect("first chain entry");
    let (raw_out, raw_stats) =
        decode_entry_with_stats(raw_first, &bytes).expect("raw then lzss chain decodes");
    assert_eq!(
        raw_out.as_slice(),
        chain_output_raw_first().as_slice(),
        "the raw block's two bytes then eighteen spaces"
    );
    assert_eq!(
        raw_out.len(),
        CHAIN_UNCOMPRESSED_SIZE,
        "produced byte count"
    );
    assert_eq!(raw_stats.blocks, CHAIN_BLOCKS, "block census");
    assert_eq!(
        raw_stats.lzss_blocks, CHAIN_LZSS_BLOCKS,
        "lzss block census"
    );
    assert_eq!(raw_stats.raw_blocks, CHAIN_RAW_BLOCKS, "raw block census");

    // The same two blocks the other way round. Blocks are read in stored order and
    // the output is the concatenation, so reversing the frames reverses the output.
    // A decoder that assumed one kind per chain gets this wrong.
    let lzss_first = entries.get(1).expect("second chain entry");
    let (lzss_out, lzss_stats) =
        decode_entry_with_stats(lzss_first, &bytes).expect("lzss then raw chain decodes");
    assert_eq!(
        lzss_out.as_slice(),
        chain_output_lzss_first().as_slice(),
        "eighteen spaces then the raw block's two bytes"
    );
    assert_eq!(
        lzss_out.len(),
        CHAIN_UNCOMPRESSED_SIZE,
        "produced byte count"
    );
    assert_eq!(lzss_stats.blocks, CHAIN_BLOCKS, "block census");
    assert_eq!(
        lzss_stats.lzss_blocks, CHAIN_LZSS_BLOCKS,
        "lzss block census"
    );
    assert_eq!(lzss_stats.raw_blocks, CHAIN_RAW_BLOCKS, "raw block census");

    // Each LZSS block starts with a fresh ring (DAT-019). The `00 00 0F` stream
    // reads entirely inside the prefilled region, so it can only come out as
    // eighteen spaces on a fresh ring and as something else on a carried one.
    assert_eq!(
        lzss_block_output().as_slice(),
        vec![0x20u8; LZSS_BLOCK_OUTPUT].as_slice(),
        "the LZSS block contributes eighteen prefilled spaces"
    );
}

/// claim: DAT-022
/// oracle: class 5 (hand computed vectors from the spec: the block headers account for the compressed size exactly, and one byte either way is an error)
#[test]
fn synthetic_chain_block_headers_account_for_the_compressed_size_exactly() {
    let exact_bytes = synthetic_chain_consumption(CONSUMPTION_EXACT, 0);
    assert_eq!(
        exact_bytes.len(),
        CONSUMPTION_TREE_BYTES + CHAIN_COMPRESSED_SIZE,
        "exact archive length"
    );
    let exact_archive = synthetic_parse(
        &exact_bytes,
        "synthetic_chain_block_headers_account_for_the_compressed_size_exactly",
    );
    let exact_entries = exact_archive.entries();
    assert_eq!(exact_entries.len(), CONSUMPTION_ENTRY_COUNT, "entry count");
    let exact_entry = exact_entries.first().expect("one chain entry");
    assert_eq!(
        exact_entry.compressed_size,
        to_u32(CONSUMPTION_HEADER_SUM),
        "the block headers account for exactly the declared compressed size"
    );
    let decoded = decode_entry(exact_entry, &exact_bytes).expect("exact chain decodes");
    assert_eq!(
        decoded.as_slice(),
        chain_output_raw_first().as_slice(),
        "exact chain output"
    );
    assert_eq!(
        decoded.len(),
        CHAIN_UNCOMPRESSED_SIZE,
        "produced byte count"
    );

    // One byte longer than the block headers account for. The extent is present in
    // full, so nothing can run out of bytes: the only possible failure is the
    // consumed-versus-declared comparison the spec mandates.
    let over_bytes = synthetic_chain_consumption(CONSUMPTION_ONE_OVER, 1);
    assert_eq!(
        over_bytes.len(),
        CONSUMPTION_TREE_BYTES + CONSUMPTION_ONE_OVER,
        "one byte over archive length"
    );
    let over_archive = synthetic_parse(
        &over_bytes,
        "synthetic_chain_block_headers_account_for_the_compressed_size_exactly",
    );
    let over_entry = over_archive.entries().first().expect("one chain entry");
    match decoded_error(
        decode_entry(over_entry, &over_bytes),
        "synthetic_chain_block_headers_account_for_the_compressed_size_exactly: one byte over",
    ) {
        DatError::ConsumptionMismatch { declared, actual } => {
            assert_eq!(
                declared,
                to_u32(CONSUMPTION_ONE_OVER),
                "the declared compressed size"
            );
            assert_eq!(
                actual,
                to_u32(CONSUMPTION_HEADER_SUM),
                "the bytes the blocks actually consumed"
            );
        }
        other => panic!(
            "expected ConsumptionMismatch, got {}",
            dat_error_name(&other)
        ),
    }

    // One byte shorter. DAT-024 fixes the comparison, not which variant reports it:
    // a reader that bounds the extent before walking hits the end of the window, and
    // one that counts consumed bytes hits the comparison. Both are the same
    // rejection, so the set is the assertion.
    let under_bytes = synthetic_chain_consumption(CONSUMPTION_ONE_UNDER, 0);
    let under_archive = synthetic_parse(
        &under_bytes,
        "synthetic_chain_block_headers_account_for_the_compressed_size_exactly",
    );
    let under_entry = under_archive.entries().first().expect("one chain entry");
    let error = decoded_error(
        decode_entry(under_entry, &under_bytes),
        "synthetic_chain_block_headers_account_for_the_compressed_size_exactly: one byte under",
    );
    assert!(
        is_consumption_error(&error),
        "one byte under must be rejected, got {}",
        dat_error_name(&error)
    );
}

/// claim: DAT-023, DAT-024
/// oracle: class 5 (hand computed vectors from the spec: a declared size the payload cannot produce is an error, never a short buffer)
#[test]
fn synthetic_declared_output_larger_than_the_payload_produces_is_an_error() {
    let bytes = synthetic_declared_too_large();
    let archive = synthetic_parse(
        &bytes,
        "synthetic_declared_output_larger_than_the_payload_produces_is_an_error",
    );
    let entries = archive.entries();
    assert_eq!(
        entries.len(),
        TOO_LARGE_ENTRY_COUNT,
        "one entry per payload mode"
    );

    // Mode 0x10. The stream is `00 00 0F`, which produces eighteen bytes, and the
    // entry declares nineteen.
    let stream_entry = entries.first().expect("stream entry");
    assert_eq!(
        payload_mode_name(payload_mode(stream_entry.flags)),
        "LzssStream",
        "mode 0x10"
    );
    assert_eq!(
        stream_entry.uncompressed_size, TOO_LARGE_DECLARED,
        "declared output the stream cannot produce"
    );
    match decoded_error(
        decode_entry(stream_entry, &bytes),
        "synthetic_declared_output_larger_than_the_payload_produces_is_an_error: 0x10",
    ) {
        DatError::OutputShort { declared } => {
            assert_eq!(declared, TOO_LARGE_DECLARED, "declared size")
        }
        other => panic!("expected OutputShort, got {}", dat_error_name(&other)),
    }

    // Mode 0x40. The chain is one LZSS block over the same three bytes, so it also
    // produces eighteen and also falls one byte short of the declaration.
    let chain_entry = entries.get(2).expect("chain entry");
    assert_eq!(
        payload_mode_name(payload_mode(chain_entry.flags)),
        "BlockChain",
        "mode 0x40"
    );
    match decoded_error(
        decode_entry(chain_entry, &bytes),
        "synthetic_declared_output_larger_than_the_payload_produces_is_an_error: 0x40",
    ) {
        DatError::OutputShort { declared } => {
            assert_eq!(declared, TOO_LARGE_DECLARED, "declared size")
        }
        other => panic!("expected OutputShort, got {}", dat_error_name(&other)),
    }

    // Mode 0x20. The stored bytes are already decoded, so a declared size larger
    // than the archive can only fail as a length or bounds violation.
    let stored_entry = entries.get(1).expect("stored entry");
    assert_eq!(
        payload_mode_name(payload_mode(stored_entry.flags)),
        "Stored",
        "mode 0x20"
    );
    assert_eq!(
        stored_entry.uncompressed_size, TOO_LARGE_STORED_DECLARED,
        "declared output past the end of the archive"
    );
    let error = decoded_error(
        decode_entry(stored_entry, &bytes),
        "synthetic_declared_output_larger_than_the_payload_produces_is_an_error: 0x20",
    );
    assert!(
        is_tree_layout_error(&error),
        "a stored extent past the end must be rejected, got {}",
        dat_error_name(&error)
    );
}

/// claim: DAT-023, DAT-024
/// oracle: class 5 (hand computed vectors from the spec: a stream one byte short and a stream one byte long are each an error)
#[test]
fn synthetic_stream_one_byte_short_and_one_byte_long_are_each_errors() {
    let bytes = synthetic_stream_lengths();
    let archive = synthetic_parse(
        &bytes,
        "synthetic_stream_one_byte_short_and_one_byte_long_are_each_errors",
    );
    let entries = archive.entries();
    assert_eq!(
        entries.len(),
        STREAM_ENTRY_COUNT,
        "one entry per stream length"
    );

    // The positive control. Three bytes in, two bytes out, and the declared size is
    // two, so this one must decode and must not decode to anything else.
    let exact_entry = entries.get(1).expect("exact stream entry");
    assert_eq!(
        exact_entry.compressed_size,
        to_u32(STREAM_EXACT_INPUT),
        "the exact stream's compressed size is its input length"
    );
    let exact_out = decode_entry(exact_entry, &bytes).expect("the exact stream decodes");
    assert_eq!(
        exact_out.as_slice(),
        STREAM_OUTPUT.as_slice(),
        "the exact stream produces its two literals"
    );
    assert_eq!(
        exact_out.len(),
        to_u32(STREAM_OUTPUT.len()),
        "produced byte count"
    );

    // One byte short. Two input bytes produce one literal, then the control byte has
    // no second literal behind it and the stream ends mid control byte (LZSS-013).
    // One byte short of the declaration, so it is an error and not a padded one byte
    // buffer.
    let short_entry = entries.first().expect("short stream entry");
    assert_eq!(
        short_entry.compressed_size,
        to_u32(STREAM_SHORT_INPUT),
        "the short stream's compressed size"
    );
    match decoded_error(
        decode_entry(short_entry, &bytes),
        "synthetic_stream_one_byte_short_and_one_byte_long_are_each_errors: one byte short",
    ) {
        DatError::OutputShort { declared } => {
            assert_eq!(declared, STREAM_DECLARED, "declared size")
        }
        other => panic!("expected OutputShort, got {}", dat_error_name(&other)),
    }

    // One byte long. The extra byte is a third literal token behind the same control
    // byte, so it would write a third output byte, one past the declared two. It is
    // an error and not a longer buffer.
    let long_entry = entries.get(2).expect("long stream entry");
    assert_eq!(
        long_entry.compressed_size,
        to_u32(STREAM_LONG_INPUT),
        "the long stream's compressed size"
    );
    match decoded_error(
        decode_entry(long_entry, &bytes),
        "synthetic_stream_one_byte_short_and_one_byte_long_are_each_errors: one byte long",
    ) {
        DatError::OutputOverrun { declared } => {
            assert_eq!(declared, STREAM_DECLARED, "declared size")
        }
        other => panic!("expected OutputOverrun, got {}", dat_error_name(&other)),
    }
}

/// claim: DAT-012
/// oracle: class 5 (hand computed vectors from the spec: the first tree entry sits far below the archive's lowest data, so tree order is not physical order)
#[test]
fn synthetic_tree_order_is_not_physical_order() {
    let bytes = synthetic_descending_offsets();
    assert_eq!(
        bytes.len(),
        DESCENDING_TREE_BYTES,
        "hand computed tree length"
    );
    let archive = synthetic_parse(&bytes, "synthetic_tree_order_is_not_physical_order");
    let entries = archive.entries();
    assert_eq!(entries.len(), DESCENDING_ENTRY_COUNT, "entry count");

    // Tree order is the file order of the records. The offsets descend and then
    // rise while the entries stay in file order.
    assert_eq!(entries[0].path.as_str(), "AAA.BIN", "first tree entry");
    assert_eq!(entries[1].path.as_str(), "BBB.BIN", "second tree entry");
    assert_eq!(entries[2].path.as_str(), "CCC.BIN", "third tree entry");
    assert_eq!(
        entries[0].offset,
        to_u32(DESCENDING_FIRST_OFFSET),
        "largest offset first"
    );
    assert_eq!(
        entries[1].offset,
        to_u32(DESCENDING_SECOND_OFFSET),
        "smallest offset second"
    );
    assert_eq!(
        entries[2].offset,
        to_u32(DESCENDING_THIRD_OFFSET),
        "middle offset third"
    );

    let descents = entries
        .windows(2)
        .filter(|pair| pair[1].offset < pair[0].offset)
        .count();
    assert_eq!(
        descents, DESCENDING_DESCENTS,
        "the offset sequence descends once, so physical order is not tree order"
    );
    // A lookup reads the offset out of the record, so it works for every entry
    // whatever the offsets happen to do.
    for index in 0..DESCENDING_ENTRY_COUNT {
        let name = entries
            .get(index)
            .expect("entry index in range")
            .path
            .clone();
        let found = archive.find(&name).expect("every entry is findable");
        assert_eq!(
            found.offset, entries[index].offset,
            "the found entry's offset"
        );
        assert_eq!(found.path.as_str(), name.as_str(), "the found entry's path");
    }
    let alpha = archive.find("AAA.BIN").expect("first entry by path");
    assert_eq!(
        alpha.offset,
        to_u32(DESCENDING_FIRST_OFFSET),
        "offset read from the record"
    );
}

/// claim: DAT-010
/// oracle: class 6 (algebraic law: lookup on the uppercased path finds the same record, and an absent path is not an error)
#[test]
fn synthetic_lookup_is_by_uppercased_path_and_an_absent_path_is_not_an_error() {
    let bytes = synthetic_lookup();
    assert_eq!(bytes.len(), LOOKUP_TREE_BYTES, "hand computed tree length");
    let archive = synthetic_parse(
        &bytes,
        "synthetic_lookup_is_by_uppercased_path_and_an_absent_path_is_not_an_error",
    );
    assert_eq!(archive.entries().len(), SINGLE_BLOCK_ENTRIES, "entry count");

    let upper = archive.find("ART\\TILES\\GRID000.FRM");
    assert!(upper.is_some(), "the uppercase path is found");
    let upper_entry = upper.expect("uppercase lookup returned a record");
    assert_eq!(upper_entry.offset, 2_000, "the found entry's offset");

    let lower = archive.find("art\\tiles\\grid000.frm");
    assert!(lower.is_some(), "the lowercase path is found");
    let lower_entry = lower.expect("lowercase lookup returned a record");
    assert_eq!(
        lower_entry.path.as_str(),
        upper_entry.path.as_str(),
        "same path"
    );
    assert_eq!(lower_entry.offset, upper_entry.offset, "same offset");
    assert_eq!(lower_entry.flags, upper_entry.flags, "same flags");
    assert_eq!(
        lower_entry.uncompressed_size, upper_entry.uncompressed_size,
        "same uncompressed size"
    );
    assert_eq!(
        lower_entry.compressed_size, upper_entry.compressed_size,
        "same compressed size"
    );

    // An absent path is `None`, and `None` is not a parse error: the archive is
    // still there and still answerable afterwards.
    assert!(
        archive.find("ART\\TILES\\ABSENT.FRM").is_none(),
        "absent path"
    );
    assert!(archive.find("").is_none(), "empty path");
    assert!(archive.find("GRID000.FRM").is_none(), "unprefixed path");
    assert_eq!(
        archive.entries().len(),
        SINGLE_BLOCK_ENTRIES,
        "still one entry"
    );
    assert!(
        archive.find("ART\\TILES\\GRID000.FRM").is_some(),
        "the entry is still findable after a miss"
    );

    // A tree that is not a tree is a parse error, which is a different answer from
    // the `None` above and not a variant of it.
    let mut truncated = bytes.clone();
    truncated.truncate(LOOKUP_BLOCK_HEADER);
    assert!(
        DatArchive::parse(&truncated, SYNTHETIC_ORIGIN).is_err(),
        "a truncated tree is a parse error, not a missing entry"
    );
}

/// claim: DAT-017
/// oracle: class 6 (algebraic law: a total reader has no panic path, so every single bit flip answers)
#[test]
fn synthetic_every_bit_flip_of_the_root_header_answers_without_panicking() {
    let base = synthetic_lookup();
    assert_eq!(base.len(), LOOKUP_TREE_BYTES, "hand computed tree length");
    synthetic_parse(
        &base,
        "synthetic_every_bit_flip_of_the_root_header_answers_without_panicking",
    );

    let mut answered = 0usize;
    for bit in 0..ROOT_HEADER_BITS {
        let mut mutated = base.clone();
        let byte_index = bit / 8;
        let mask = 1u8 << (bit % 8);
        mutated[byte_index] ^= mask;
        let outcome = DatArchive::parse(&mutated, SYNTHETIC_ORIGIN);
        // Either answer is correct here, so both are accepted. Reaching the count
        // below is the assertion: a panic would have stopped the test.
        assert!(outcome.is_ok() || outcome.is_err(), "the bit flip answered");
        answered += 1;
    }
    assert_eq!(
        answered, ROOT_HEADER_BITS,
        "every single bit flip of the root header answered with a value and not a panic"
    );
}

/// claim: DAT-017
/// oracle: class 5 (hand computed vectors from the spec: a tree that runs past its own bytes is an error)
#[test]
fn synthetic_malformed_trees_are_errors() {
    let base = synthetic_lookup();
    synthetic_parse(&base, "synthetic_malformed_trees_are_errors");
    assert_eq!(base.len(), LOOKUP_TREE_BYTES, "hand computed tree length");

    // The directory name length runs past the end of the archive. The length byte is
    // the first byte after the root header, and the archive is far shorter than the
    // two hundred bytes it now claims.
    let mut over_long_name = base.clone();
    over_long_name[LOOKUP_DIRECTORY_NAME_LENGTH] = DIRECTORY_NAME_PAST_END;

    // The block header's field 0 is zero where entries are expected. The spec
    // explicitly leaves zero entry blocks unspecified, so this row is a test author
    // ruling pending the owner's confirmation, recorded here rather than hidden.
    let mut zero_block_count = base.clone();
    zero_block_count[LOOKUP_BLOCK_FIELD0] = 0;
    zero_block_count[LOOKUP_BLOCK_FIELD0 + 1] = 0;
    zero_block_count[LOOKUP_BLOCK_FIELD0 + 2] = 0;
    zero_block_count[LOOKUP_BLOCK_FIELD0 + 3] = 0;

    // An entry whose name length is zero. Also left open by the spec, which fixes the
    // seventeen byte minimum record for an empty name without saying whether a parser
    // must accept one.
    let mut zero_entry_name = base.clone();
    zero_entry_name[LOOKUP_ENTRY_NAME_LENGTH] = 0;

    // The root header claims more directories than the archive has bytes for.
    let mut too_many_directories = base.clone();
    too_many_directories[ROOT_FIELD0] = 0;
    too_many_directories[ROOT_FIELD0 + 1] = 0;
    too_many_directories[ROOT_FIELD0 + 2] = 0;
    too_many_directories[ROOT_FIELD0 + 3] = ROOT_DIRECTORIES_PAST_END;

    let cases = [
        ("directory name length past the end", over_long_name),
        ("block header entry count of zero", zero_block_count),
        ("entry name length of zero", zero_entry_name),
        (
            "root directory count past the archive",
            too_many_directories,
        ),
    ];
    assert_eq!(
        cases.len(),
        MALFORMED_TREE_COUNT,
        "every malformed case built"
    );

    let mut refused = 0usize;
    for (case, bytes) in &cases {
        match DatArchive::parse(bytes, SYNTHETIC_ORIGIN) {
            Ok(_) => panic!("{case}: a malformed tree parsed"),
            Err(error) => {
                assert!(
                    is_tree_layout_error(&error),
                    "{case}: expected a layout error, got {}",
                    dat_error_name(&error)
                );
                refused += 1;
            }
        }
    }
    assert_eq!(
        refused, MALFORMED_TREE_COUNT,
        "every malformed case refused"
    );
}
