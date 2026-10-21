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
//!   reference tree walker, and the reference LZSS decoder beside it (class 4);
//! - an **algebraic law** about counts, offsets or modes (class 6).
//!
//! The oracle classes used below follow `docs/spec/trace.md`: oracle class 4 (an
//! independent reference implementation), oracle class 5 (hand computed vectors
//! from the spec) and oracle class 6 (algebraic laws).
//!
//! The class 4 oracle is real: the tests below import `fo1_tests::RefDatTree`
//! and `fo1_tests::RefLzss` and cross-walk or cross-decode the same bytes with
//! them, and the two answers are compared. A test that does not do that claims
//! class 5 or class 6 only, and says so in its own doc comment.
//!
//! Every constant a test compares against is read out of the constants ledger
//! at run time, so a change in `docs/spec/dat.constants.toml` or in
//! `docs/spec/lzss.constants.toml` fails a test here instead of leaving a stale
//! literal behind in a file that reads like evidence.
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

// Gate tests are test code. `clippy.toml` sets `allow-unwrap-in-tests` and its
// siblings, which cover a `#[cfg(test)]` module or a `#[test]` function body.
// An integration test file's top-level helper functions sit outside both, so the
// same exemptions are restated here, with reasons, for the gate files only. The
// workspace lint table stays fully denied for the six production crates, which
// are what the guide's "no panicking readers" guarantee is about.
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "gate-test helpers: the test author must be able to assert a panic and index fixed-width headers outside #[test] bodies"
)]
use fo1_bigendian::BigEndian;
use fo1_dat::{
    decode_entry, decode_entry_with_stats, payload_mode, DatArchive, DatError, EntryRecord,
    PayloadMode, ROOT_HEADER_SIZE,
};
use fo1_tests::{RefDatTree, RefEntry, RefLzss};

// --- the constants ledgers -----------------------------------------------------

/// The DAT constants ledger, embedded at compile time.
///
/// A constant that lives in a document is only evidence if a test reads it there.
/// The ledger is the source of truth for `docs/spec/dat.md`, so the values below
/// are compared against what this file holds.
const DAT_CONSTANTS_LEDGER: &str = include_str!("../../docs/spec/dat.constants.toml");

/// The LZSS constants ledger, embedded at compile time for the same reason.
const LZSS_CONSTANTS_LEDGER: &str = include_str!("../../docs/spec/lzss.constants.toml");

/// The `value` of one constant in a ledger, by its `id`.
///
/// A ledger is a list of `[[constant]]` blocks, each with an `id` line and a
/// `value` line. A missing id, a missing value or a value this reader cannot
/// parse is a panic and not a default: the ledger is the source of truth, and a
/// constant that has gone missing must fail loudly rather than quietly become
/// zero.
fn ledger_value(ledger: &str, id: &str) -> usize {
    let wanted = format!("id = \"{id}\"");
    let start = match ledger.lines().position(|line| line.trim() == wanted) {
        Some(at) => at + 1,
        None => panic!("the ledger has no constant {id}"),
    };
    let line = match ledger
        .lines()
        .skip(start)
        .find(|line| line.trim().starts_with("value = "))
    {
        Some(line) => line,
        None => panic!("the ledger constant {id} has no value"),
    };
    let raw = match line.trim().strip_prefix("value = ") {
        Some(raw) => raw.trim(),
        None => panic!("the ledger constant {id} has no value"),
    };
    // The ledger writes numbers in decimal, and its own header says the hex
    // form is the same number, so both are read.
    let parsed = match raw.strip_prefix("0x") {
        Some(hex) => usize::from_str_radix(hex, 16),
        None => raw.parse::<usize>(),
    };
    match parsed {
        Ok(value) => value,
        Err(_) => panic!("the ledger constant {id} has a value this reader cannot read"),
    }
}

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

/// A second diagnostic label, for the test that proves the archive reports the
/// label it was parsed under rather than a fixed string (DAT-030).
const SYNTHETIC_ALT_ORIGIN: &str = "synthetic_dat_alt";

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

/// LZSS block payload whose back reference is missing its second byte: a
/// control byte of zero, which asks for a back reference, and the first byte of
/// that pair, and nothing else (LZSS-007, LZSS-015).
const LZSS_CUT_PAIR_PAYLOAD: [u8; 2] = [0x00, 0x00];

/// LZSS block payload that fills the declaration exactly and then runs out in
/// the middle of a second back reference: the spec's worked vector `00 00 0F`
/// (LZSS-018), followed by the first byte of a second pair and by nothing else.
///
/// The control byte is zero, so all eight of its tokens are back references
/// (LZSS-007): the first pair is the worked vector and writes eighteen bytes,
/// and the second pair starts one byte before the input ends.
const LZSS_FULL_THEN_CUT_PAIR_PAYLOAD: [u8; 4] = [0x00, 0x00, 0x0F, 0x00];

// --- the offsets an error carries (DAT-030) ------------------------------------

/// A byte that is not valid UTF-8, so a stored name that carries it cannot be
/// decoded (DAT-009, DAT-030).
const NOT_UTF8_BYTE: u8 = 0xFF;

/// Frame header of a chain block that declares a zero length payload, which
/// the reader refuses rather than spin on (DAT-018).
const ZERO_PAYLOAD_BLOCK_HEADER: u16 = 0;

/// Frame header of a chain block that declares five payload bytes the chain
/// does not carry (DAT-018).
const OVERSIZED_BLOCK_HEADER: u16 = 5;

/// Declared output size of the chain that starts with a raw block: the raw
/// block produces two bytes and the entry owes one more, so the walk reaches
/// the second block.
const CHAIN_AFTER_RAW_DECLARED: usize = 3;

/// Position of the second block's frame header within the chain: the raw
/// block's four bytes on disk (DAT-018).
const CHAIN_SECOND_BLOCK_CURSOR: usize = RAW_BLOCK_FRAME_BYTES;

/// `synthetic_chain_after_raw`: total tree length, after one twenty six byte
/// record. The chain's stored bytes start there.
const CHAIN_AFTER_RAW_TREE_BYTES: usize =
    HEADER_WIDTH + NAME_WIDTH_DOT + HEADER_WIDTH + WIDTH_CHAIN_BIN;

/// Declared output size of the cut pair stream: one byte, so the stream still
/// owes output the truncated pair never produced (LZSS-015).
const CUT_PAIR_STREAM_DECLARED: usize = 1;

/// Position of the incomplete pair's first byte inside the LZSS stream: the
/// control byte is byte zero, so the pair starts at byte one (LZSS-007).
const CUT_PAIR_STREAM_POSITION: usize = 1;

/// `synthetic_cut_pair_stream`: total tree length, after one twenty four byte
/// record. The stream's input starts there.
const CUT_PAIR_STREAM_TREE_BYTES: usize =
    HEADER_WIDTH + NAME_WIDTH_DOT + HEADER_WIDTH + WIDTH_CUT_BIN;

// --- the two LZSS block chain (DAT-019) ---------------------------------------

/// Control byte `0x01` of block A: bit 0 is a literal and the seven bits above
/// it are a back reference with no input behind it, so the block ends after
/// one literal (`lzss.md` LZSS-005, LZSS-006, LZSS-013).
const TWO_BLOCK_A_CONTROL: u8 = 0x01;

/// The one byte block A writes. It is neither the prefill `0x20` nor zero, so a
/// byte that crossed a block boundary would be visible in the output.
const TWO_BLOCK_A_LITERAL: u8 = 0x41;

/// Payload of block A: a control byte and one literal.
const TWO_BLOCK_A_PAYLOAD: [u8; 2] = [TWO_BLOCK_A_CONTROL, TWO_BLOCK_A_LITERAL];

/// Frame header of block A: bit 15 clear, so an LZSS block, and two payload
/// bytes (DAT-018).
const TWO_BLOCK_A_HEADER: u16 = 2;

/// Bytes block A occupies on disk: its frame header and its payload.
const TWO_BLOCK_A_FRAME_BYTES: usize = 2 + TWO_BLOCK_A_PAYLOAD.len();

/// The back reference of block B, `b1` then `b2` (`lzss.md` LZSS-007).
///
/// `source = b1 | ((b2 & 0xF0) << 4)` must be 4078, the slot a fresh ring's
/// write head starts at (LZSS-003). 4078 is 0xFEE, so `b1` is 0xEE and the top
/// nibble of `b2` is 0xF. `length = (b2 & 0x0F) + 3` must be 18, the longest
/// run the format allows (LZSS-008), so the bottom nibble is 0xF.
const TWO_BLOCK_B_TOKEN: [u8; 2] = [0xEE, 0xFF];

/// Payload of block B: a control byte of zero, so every one of its eight bits
/// is a back reference, and the token.
const TWO_BLOCK_B_PAYLOAD: [u8; 3] = [0x00, TWO_BLOCK_B_TOKEN[0], TWO_BLOCK_B_TOKEN[1]];

/// Frame header of block B: bit 15 clear, so an LZSS block, and three payload
/// bytes (DAT-018).
const TWO_BLOCK_B_HEADER: u16 = 3;

/// Bytes block B occupies on disk: its frame header and its payload.
const TWO_BLOCK_B_FRAME_BYTES: usize = 2 + TWO_BLOCK_B_PAYLOAD.len();

/// Bytes the two block chain occupies on disk.
const TWO_BLOCK_COMPRESSED_SIZE: usize = TWO_BLOCK_A_FRAME_BYTES + TWO_BLOCK_B_FRAME_BYTES;

/// Ring slot block B's token reads: 4078, the write head of a fresh ring
/// (LZSS-003). A fresh ring holds zero there (LZSS-002) and block A has just
/// written its literal into it.
const TWO_BLOCK_B_SLOT: usize = 4078;

/// The eighteen bytes block B produces on a fresh ring.
///
/// A fresh ring holds zero in slots 4078..=4095, so a run of eighteen that
/// starts at slot 4078 reads eighteen zero bytes and writes eighteen zero
/// bytes. The run trails the write head by one slot, so on a ring carried over
/// from block A it would instead replicate block A's literal eighteen times
/// (LZSS-011). The two answers differ in every byte, which is what makes this
/// fixture able to see a ring that crossed a block boundary.
const TWO_BLOCK_B_OUTPUT: [u8; 18] = [0x00; 18];

/// The one byte block A produces.
const TWO_BLOCK_A_OUTPUT: [u8; 1] = [TWO_BLOCK_A_LITERAL];

/// Bytes the two block chain decodes to: block A's one byte and block B's
/// eighteen.
const TWO_BLOCK_UNCOMPRESSED_SIZE: usize = TWO_BLOCK_A_OUTPUT.len() + TWO_BLOCK_B_OUTPUT.len();

/// Blocks in the two block chain (DAT-018).
const TWO_BLOCK_BLOCKS: u32 = 2;

/// LZSS blocks in the two block chain (DAT-020).
const TWO_BLOCK_LZSS_BLOCKS: u32 = 2;

/// Raw blocks in the two block chain (DAT-020).
const TWO_BLOCK_RAW_BLOCKS: u32 = 0;

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

// --- oracle 5: the error type's rendered message -------------------------------

/// The nine variants of `DatError`, the quantity each one must report, the name
/// the variant is graded under, and the exact text the spec fixes for it.
///
/// `docs/spec/dat.md` DAT-030 fixes the variant set and what each variant
/// means, and the *Error text* section of the same document fixes the wording
/// of every message, the way `docs/spec/exe-tables.md` does for the executable
/// layer. The expected strings below are read from that section, so they are
/// the spec's words and not a copy of what the crate happens to print: the
/// assertion below fails the moment the wording moves.
///
/// The quantities are hand chosen and pairwise distinct, so a message that
/// names the wrong number fails instead of passing on a coincidence. The
/// variant with no quantity of its own is the one that reports an arithmetic
/// failure rather than a place.
const DAT_ERROR_CASES: [(&str, DatError, &[u64], &str); 9] = [
    (
        "Truncated",
        DatError::Truncated { at: 41 },
        &[41],
        "fo1-dat: the archive ran out at byte 41",
    ),
    (
        "LengthOutOfRange",
        DatError::LengthOutOfRange { at: 42 },
        &[42],
        "fo1-dat: a length field at byte 42 exceeded the bytes present",
    ),
    (
        "OutOfBounds",
        DatError::OutOfBounds { at: 43 },
        &[43],
        "fo1-dat: a region at byte 43 ran outside the archive",
    ),
    (
        "OffsetOverflow",
        DatError::OffsetOverflow,
        &[],
        "fo1-dat: an offset plus a length overflowed",
    ),
    (
        "UnknownPayloadMode",
        DatError::UnknownPayloadMode { flags: 0x5A },
        &[0x5A],
        "fo1-dat: the flags word 0x5A names no known payload mode",
    ),
    (
        "OutputOverrun",
        DatError::OutputOverrun { declared: 44 },
        &[44],
        "fo1-dat: the tokens would write past 44 declared bytes",
    ),
    (
        "OutputShort",
        DatError::OutputShort { declared: 45 },
        &[45],
        "fo1-dat: the stream ended short of 45 declared bytes",
    ),
    (
        "ConsumptionMismatch",
        DatError::ConsumptionMismatch {
            declared: 46,
            actual: 47,
        },
        &[46, 47],
        "fo1-dat: the payload consumed 47 byte(s) against 46 declared",
    ),
    (
        "EmptyBlock",
        DatError::EmptyBlock { at: 48 },
        &[48],
        "fo1-dat: the block header at byte 48 declares a zero length payload",
    ),
];

/// Number of rows in [`DAT_ERROR_CASES`], which is the number of variants the
/// error type has.
const DAT_ERROR_CASE_COUNT: usize = 9;

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
const WIDTH_SYNTH_PAL: usize = record_width("SYNTH.PAL");
const WIDTH_SYNTH_FON: usize = record_width("SYNTH.FON");
const WIDTH_CRITTERS_LST: usize = record_width("CRITTERS.LST");
const WIDTH_SYNTH_FRM: usize = record_width("SYNTH.FRM");
const WIDTH_A1_FRM: usize = record_width("A1.FRM");
const WIDTH_GRID000_FRM: usize = record_width("GRID000.FRM");
const WIDTH_A: usize = record_width("A");
const WIDTH_AAA_BIN: usize = record_width("AAA.BIN");
const WIDTH_STORED_BIN: usize = record_width("STORED.BIN");
const WIDTH_STORED2_BN: usize = record_width("STORED2.BN");
const WIDTH_RAW1_BIN: usize = record_width("RAW1.BIN");
const WIDTH_LZSS1_BIN: usize = record_width("LZSS1.BIN");
const WIDTH_TWO_BIN: usize = record_width("TWO.BIN");
const WIDTH_CHAIN_BIN: usize = record_width("CHAIN.BIN");
const WIDTH_L1_BIN: usize = record_width("L1.BIN");
const WIDTH_SHORT_BIN: usize = record_width("SHORT.BIN");
const WIDTH_EXACT_BN: usize = record_width("EXACT.BN");
const WIDTH_LONG_BN: usize = record_width("LONG.BN");
const WIDTH_ODD_BIN: usize = record_width("ODD.BIN");
const WIDTH_CUT_BIN: usize = record_width("CUT.BIN");

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
    ROUND_TRIP_BLOCK0_ENTRIES + WIDTH_COLOR_PAL + WIDTH_SYNTH_PAL + WIDTH_SYNTH_FON;

/// `synthetic_round_trip`: byte offset of block 1's first entry.
const ROUND_TRIP_BLOCK1_ENTRIES: usize = ROUND_TRIP_BLOCK1_HEADER + HEADER_WIDTH;

/// `synthetic_round_trip`: total length of the tree.
const ROUND_TRIP_TREE_BYTES: usize =
    ROUND_TRIP_BLOCK1_ENTRIES + WIDTH_CRITTERS_LST + WIDTH_SYNTH_FRM;

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

/// Root header field 0 of every one block fixture, as the slice length it is
/// compared against. `Archive::directories()` and the other `len()` accessors
/// return `usize`, so the fixture count is held in that width here.
const SINGLE_DIRECTORY_COUNT: usize = 1;

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

/// `synthetic_two_block_chain`: byte offset of block 0's header.
const TWO_BLOCK_BLOCK_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT;

/// `synthetic_two_block_chain`: total tree length, after one twenty four byte
/// record.
const TWO_BLOCK_TREE_BYTES: usize = TWO_BLOCK_BLOCK_HEADER + HEADER_WIDTH + WIDTH_TWO_BIN;

/// Entries in `synthetic_two_block_chain`.
const TWO_BLOCK_ENTRY_COUNT: usize = 1;

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

/// Bytes the chain's decoded form runs past the end of the archive: it decodes
/// to twenty bytes and stores nine (DAT-021).
const CONSUMPTION_DECODED_OVERRUN: usize = CHAIN_UNCOMPRESSED_SIZE - CHAIN_COMPRESSED_SIZE;

/// `synthetic_single_lzss_chain`: byte offset of block 0's header.
const SINGLE_BLOCK_CHAIN_HEADER: usize = HEADER_WIDTH + NAME_WIDTH_DOT;

/// `synthetic_single_lzss_chain`: total tree length, after one twenty six byte
/// record.
const SINGLE_BLOCK_CHAIN_TREE_BYTES: usize =
    SINGLE_BLOCK_CHAIN_HEADER + HEADER_WIDTH + WIDTH_CHAIN_BIN;

/// Entries in `synthetic_single_lzss_chain`.
const SINGLE_BLOCK_CHAIN_ENTRY_COUNT: usize = 1;

/// Bytes the chain of one complete LZSS block occupies on disk: the two byte
/// frame header and its three payload bytes (DAT-018).
const SINGLE_BLOCK_CHAIN_COMPLETE_COMPRESSED: usize = 2 + LZSS_BLOCK_PAYLOAD.len();

/// Bytes the chain of one cut LZSS block occupies on disk.
const SINGLE_BLOCK_CHAIN_CUT_COMPRESSED: usize = 2 + LZSS_CUT_PAIR_PAYLOAD.len();

/// Bytes the chain of one full-then-cut LZSS block occupies on disk.
const SINGLE_BLOCK_CHAIN_FULL_THEN_CUT_COMPRESSED: usize =
    2 + LZSS_FULL_THEN_CUT_PAIR_PAYLOAD.len();

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

/// Bytes of the root header that hold field 0, the directory count. The other
/// three fields are read and discarded (DAT-002), so a bit flip in them must
/// change nothing at all.
const ROOT_FIELD_0_WIDTH: usize = 4;

/// Byte offset of the root header's field 2, the third u32. Two fields of four
/// bytes each come before it (DAT-001).
const ROOT_FIELD_2_OFFSET: usize = 2 * ROOT_FIELD_0_WIDTH;

/// Length of the truncated fixture: the root header's third u32 starts at byte
/// 8 and needs four bytes, so ten bytes leaves two of them.
const TRUNCATED_TREE_BYTES: usize = ROOT_FIELD_2_OFFSET + 2;

/// The byte position the reader runs out at, in the width `DatError` reports a
/// position in. The root header's third u32 starts at byte 8 (DAT-001) and the
/// ten byte fixture stops two bytes into it, so the read that fails is the four
/// byte read that starts at byte 8 (DAT-030).
const TRUNCATED_AT: u64 = 8;

/// Byte offset of the root header's field 3, the fourth u32. Three fields of
/// four bytes each come before it (DAT-001).
const ROOT_FIELD_3_OFFSET: usize = 3 * ROOT_FIELD_0_WIDTH;

/// Length of the second truncated fixture: the root header's fourth u32 starts
/// at byte 12 and needs four bytes, so fifteen bytes leaves three of them.
const TRUNCATED_FIELD3_TREE_BYTES: usize = ROOT_FIELD_3_OFFSET + 3;

/// The byte position the second truncated fixture reports. The root header's
/// fourth u32 starts at byte 12, so the read that fails is the four byte read
/// that starts at byte 12 (DAT-030).
const TRUNCATED_FIELD3_AT: u64 = 12;

/// Byte offset of the first directory's name bytes: the root header is sixteen
/// bytes and the name length byte is one more (DAT-001, DAT-003).
const TRUNCATED_NAME_OFFSET: usize = HEADER_WIDTH + NAME_LENGTH_WIDTH;

/// Length of the third truncated fixture: the root header, a name length byte
/// that says two, and one byte of the name that follows it. The name field is
/// missing its second byte.
const TRUNCATED_NAME_TREE_BYTES: usize = TRUNCATED_NAME_OFFSET + 1;

/// The byte position the third truncated fixture reports. The name field starts
/// at byte 17 and the archive stops one byte into it, so the region that ran
/// out starts at 17 and not at the 18 the reader noticed it at (DAT-030).
const TRUNCATED_NAME_AT: u64 = 17;

/// The name length byte of the third truncated fixture: it says two, and only
/// one of the two bytes is in the archive.
const TRUNCATED_NAME_LENGTH: u8 = 2;

/// The one byte of the name that the third truncated fixture does carry.
const TRUNCATED_NAME_BYTE: u8 = b'A';

/// Root header field 0 of the third truncated fixture: one directory, so the
/// reader reaches the name that is cut short.
const TRUNCATED_NAME_DIRECTORIES: u32 = 1;

/// Root header field 0 that no fixture can satisfy: more directories than the
/// archive has bytes for.
const ROOT_DIRECTORIES_PAST_END: u8 = 99;

/// Directory name length that no fixture can satisfy: longer than the archive.
const DIRECTORY_NAME_PAST_END: u8 = 200;

/// Every bit of the sixteen root header bytes. Each one is flipped in turn and
/// `parse` must answer with a value, never a panic (DAT-017).
const ROOT_HEADER_BITS: usize = HEADER_WIDTH * 8;

/// Number of malformed trees `synthetic_malformed_trees_are_errors` builds. Every
/// one of these claims more bytes than the buffer holds.
const MALFORMED_TREE_COUNT: usize = 2;

/// Number of trees `synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted`
/// builds. The spec is silent on both, so they are accepted by ruling, not by rule.
const ACCEPTED_TREE_COUNT: usize = 2;

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

/// True when `text` reports `value` as a number of its own: one token equal to
/// the decimal spelling, or one token equal to the hexadecimal spelling with or
/// without its `0x` prefix.
///
/// The spec fixes WHICH quantity an error names, not how the number is spelled,
/// so both spellings are accepted. The token has to equal the whole number, so
/// a different number that merely contains these digits does not pass.
fn reports_number(text: &str, value: u64) -> bool {
    let decimal = value.to_string();
    let hexadecimal = format!("{value:X}");
    let prefixed = format!("{value:#X}");
    text.split(|character: char| !character.is_ascii_alphanumeric())
        .any(|token| token == decimal || token == hexadecimal || token == prefixed)
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
    archive.extend(std::iter::repeat_n(0u8, extra_trailing));
    archive
}

/// One archive holding one chain entry whose single LZSS block carries
/// `payload`, and whose entry declares `declared` output bytes.
///
/// The chain is one framed block and nothing else (DAT-018), so the payload the
/// caller hands over is the block's payload and nothing else: the frame header
/// is written here, and the compressed size the entry declares is the header
/// plus that payload.
fn synthetic_single_lzss_chain(payload: &[u8], declared: usize) -> Vec<u8> {
    let mut chain = Vec::new();
    push_u16(&mut chain, to_u16(payload.len()));
    chain.extend_from_slice(payload);
    let entry = SyntheticEntry::chain(
        "CHAIN.BIN",
        SINGLE_BLOCK_CHAIN_TREE_BYTES,
        declared,
        chain.len(),
    );
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &[entry])];
    synthetic_archive(&blocks, &[chain], SINGLE_BLOCK_CHAIN_TREE_BYTES)
}

/// One chain of a raw block followed by a framed block whose frame header the
/// caller supplies and whose payload the chain does not carry.
///
/// The raw block is four bytes on disk: a two byte frame header and its two
/// payload bytes. So the second block's frame header starts at cursor four,
/// and the second block is the one the walk reaches after the raw block's two
/// bytes (DAT-018).
fn synthetic_chain_after_raw(second_header: u16) -> Vec<u8> {
    let mut chain = Vec::new();
    push_u16(&mut chain, RAW_BLOCK_HEADER);
    chain.extend_from_slice(&RAW_BLOCK_PAYLOAD);
    push_u16(&mut chain, second_header);
    let entry = SyntheticEntry::chain(
        "CHAIN.BIN",
        CHAIN_AFTER_RAW_TREE_BYTES,
        CHAIN_AFTER_RAW_DECLARED,
        chain.len(),
    );
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &[entry])];
    synthetic_archive(&blocks, &[chain], CHAIN_AFTER_RAW_TREE_BYTES)
}

/// One archive holding one whole LZSS stream whose input is
/// [`LZSS_CUT_PAIR_PAYLOAD`], so the stream runs out in the middle of a back
/// reference (LZSS-015).
///
/// The stream is mode `0x10`, so the codec reads the whole stored region and
/// the position it reports is counted from the start of that region.
fn synthetic_cut_pair_stream() -> Vec<u8> {
    let entry = SyntheticEntry::stream(
        "CUT.BIN",
        CUT_PAIR_STREAM_TREE_BYTES,
        CUT_PAIR_STREAM_DECLARED,
        LZSS_CUT_PAIR_PAYLOAD.len(),
    );
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &[entry])];
    synthetic_archive(
        &blocks,
        &[LZSS_CUT_PAIR_PAYLOAD.to_vec()],
        CUT_PAIR_STREAM_TREE_BYTES,
    )
}

// --- the synthetic archives ---------------------------------------------------

/// Two directories and five entries.
///
/// Two of the rows are measured: `COLOR.PAL` is the first entry of MASTER.DAT
/// and `CRITTERS.LST` the first entry of CRITTER.DAT, with the field values
/// `docs/spec/dat.md` records for them (sections 1.1 and 1.2, and the ledger
/// constants `DAT-C070`, `DAT-C080` and `DAT-C081`).
///
/// The other three rows are named `SYNTH.*` and carry arbitrary fixture values,
/// distinct from each other and from the measured rows so a reader that swaps
/// two fields cannot pass. They are not corpus rows. A fixture that borrows a
/// corpus constant for an unrelated entry is how a reader ends up believing
/// that entry lives at that offset, so nothing here carries one.
fn synthetic_round_trip() -> Vec<u8> {
    let root_entries = [
        SyntheticEntry::chain("COLOR.PAL", 211_060_669, 33_536, 9_292),
        SyntheticEntry::chain("SYNTH.PAL", 700_000, 4_100, 900),
        SyntheticEntry::chain("SYNTH.FON", 800_000, 6_400, 2_400),
    ];
    let named_entries = [
        SyntheticEntry::chain("CRITTERS.LST", 158_356, 646, 405),
        SyntheticEntry::stored("SYNTH.FRM", 1_000, 32),
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

/// One chain of two LZSS blocks, the second of which reads the ring slot the
/// first one wrote (DAT-019).
///
/// A chain of one LZSS block cannot tell a fresh ring from a carried one, so
/// this fixture has two: block A writes one literal into slot 4078 and block B
/// reads an eighteen byte run out of that slot.
fn synthetic_two_block_chain() -> Vec<u8> {
    let entry = SyntheticEntry::chain(
        "TWO.BIN",
        TWO_BLOCK_TREE_BYTES,
        TWO_BLOCK_UNCOMPRESSED_SIZE,
        TWO_BLOCK_COMPRESSED_SIZE,
    );
    let blocks = [SyntheticBlock::new(ROOT_DIRECTORY_NAME, &[entry])];
    synthetic_archive(&blocks, &[two_block_chain_payload()], TWO_BLOCK_TREE_BYTES)
}

/// The stored bytes of the two block chain: block A's frame, then block B's.
fn two_block_chain_payload() -> Vec<u8> {
    let mut out = Vec::new();
    push_u16(&mut out, TWO_BLOCK_A_HEADER);
    out.extend_from_slice(&TWO_BLOCK_A_PAYLOAD);
    push_u16(&mut out, TWO_BLOCK_B_HEADER);
    out.extend_from_slice(&TWO_BLOCK_B_PAYLOAD);
    out
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

    assert_eq!(
        ROOT_HEADER_SIZE,
        ledger_value(DAT_CONSTANTS_LEDGER, "DAT-C001"),
        "the root header is four u32"
    );
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
        ("SYNTH.PAL".to_owned(), 0x40, 700_000, 4_100, 900),
        ("SYNTH.FON".to_owned(), 0x40, 800_000, 6_400, 2_400),
        (
            NAMED_DIRECTORY.to_owned() + "\\CRITTERS.LST",
            0x40,
            158_356,
            646,
            405,
        ),
        (
            NAMED_DIRECTORY.to_owned() + "\\SYNTH.FRM",
            0x20,
            1_000,
            32,
            0,
        ),
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

    // The class 4 cross-check. The reference tree walker was written from
    // `docs/spec/dat.md` alone and is never imported by production code, so it
    // is an independent second walk of the same bytes. It must find the same
    // tree, the same paths and the same four fields of every entry.
    let reference = RefDatTree::walk(&bytes).expect("the reference walker accepts the fixture");
    assert_eq!(
        reference.declared_directory_count(),
        ROUND_TRIP_DIRECTORIES_FIELD,
        "the reference walker reads the root's field 0 as the directory count"
    );
    assert_eq!(
        reference.directory_count(),
        ROUND_TRIP_DIRECTORIES,
        "the reference walker's directory count"
    );
    assert_eq!(
        reference.entries_total(),
        ROUND_TRIP_ENTRIES,
        "the reference walker's entry count"
    );
    assert_eq!(
        reference.duplicate_paths().len(),
        0,
        "the reference walker found no duplicate composed path"
    );
    assert_eq!(
        reference.directory_names(),
        vec![ROOT_DIRECTORY_NAME, NAMED_DIRECTORY],
        "the reference walker's directory names"
    );
    for (index, directory) in reference.directories().iter().enumerate() {
        let mine = archive
            .directories()
            .get(index)
            .expect("directory index in range");
        assert_eq!(
            mine.name.as_str(),
            directory.name.as_str(),
            "directory name"
        );
        assert_eq!(
            mine.entry_count,
            to_u32(directory.entries.len()),
            "directory entry count"
        );
    }
    let reference_entries: Vec<&RefEntry> = reference
        .directories()
        .iter()
        .flat_map(|directory| directory.entries.iter())
        .collect();
    assert_eq!(
        reference_entries.len(),
        entries.len(),
        "the two walks found the same number of entries"
    );
    for (mine, theirs) in entries.iter().zip(reference_entries.iter()) {
        let directory = &reference.directories()[theirs.directory_index].name;
        assert_eq!(
            mine.path.as_str(),
            theirs.path(directory).as_str(),
            "entry path"
        );
        assert_eq!(mine.flags, theirs.flags, "entry flags");
        assert_eq!(mine.offset, theirs.offset, "entry offset");
        assert_eq!(
            mine.uncompressed_size, theirs.uncompressed_size,
            "entry uncompressed size"
        );
        assert_eq!(
            mine.compressed_size, theirs.compressed_size,
            "entry compressed size"
        );
    }
}

/// claim: DAT-002, DAT-003
/// oracle: class 4 (independent reference implementation) and class 5 (hand computed vectors from the spec)
#[test]
fn synthetic_the_directory_list_is_the_root_header_names_in_file_order() {
    // The spec's own CRITTER.DAT directory list: one name, `ART\CRITTERS`
    // (DAT-003). The accessor has to report that list and nothing else, so an
    // accessor that returned an empty list, or a list of the wrong length, or
    // the names in the wrong order, fails here and not somewhere else.
    let named = synthetic_named_directory();
    let named_archive = synthetic_parse(
        &named,
        "synthetic_the_directory_list_is_the_root_header_names_in_file_order",
    );
    let named_directories = named_archive.directories();
    assert_eq!(
        named_directories.len(),
        SINGLE_DIRECTORY_COUNT,
        "the archive whose only directory is the spec's CRITTER.DAT name reports one directory"
    );
    assert_eq!(
        named_directories[0].name.as_str(),
        NAMED_DIRECTORY,
        "the only directory is the name the spec gives CRITTER.DAT"
    );

    // The round trip archive: two directories, the `.` root first and the named
    // one second, in file order (DAT-003, with the count from DAT-002's field 0).
    let bytes = synthetic_round_trip();
    let archive = synthetic_parse(
        &bytes,
        "synthetic_the_directory_list_is_the_root_header_names_in_file_order",
    );
    let directories = archive.directories();
    assert_eq!(
        directories.len(),
        ROUND_TRIP_DIRECTORIES,
        "the directory count is the root header's field 0"
    );
    assert_eq!(
        directories[0].name.as_str(),
        ROOT_DIRECTORY_NAME,
        "directory 0 is the root directory"
    );
    assert_eq!(
        directories[1].name.as_str(),
        NAMED_DIRECTORY,
        "directory 1 is the named directory"
    );

    // The class 4 cross-check. The reference tree walker was written from
    // `docs/spec/dat.md` alone and is never imported by production code, so it
    // is an independent second walk of the same bytes. It has to find the same
    // directory list, in the same order.
    let named_reference =
        RefDatTree::walk(&named).expect("the reference walker accepts the fixture");
    assert_eq!(
        named_reference.directory_names(),
        vec![NAMED_DIRECTORY],
        "the reference walker's directory names"
    );
    let reference = RefDatTree::walk(&bytes).expect("the reference walker accepts the fixture");
    assert_eq!(
        reference.directory_names(),
        vec![ROOT_DIRECTORY_NAME, NAMED_DIRECTORY],
        "the reference walker's directory names"
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
        SINGLE_DIRECTORY_COUNT,
        "directory count"
    );
    let entries = archive.entries();
    assert_eq!(entries.len(), SINGLE_BLOCK_ENTRIES * 2, "entry count");

    // The two records are twenty eight and eighteen bytes wide. Neither is
    // sixteen, and a reader that strided by field 2 would start entry 1 four bytes
    // into entry 0's name and read `000.FRM` instead of `GRID000.FRM`.
    assert_eq!(
        WIDTH_GRID000_FRM,
        ledger_value(DAT_CONSTANTS_LEDGER, "DAT-C004") + "GRID000.FRM".len(),
        "the ledger's minimum record plus the name's eleven bytes"
    );
    assert_ne!(
        WIDTH_GRID000_FRM,
        ledger_value(DAT_CONSTANTS_LEDGER, "DAT-C003"),
        "the first record is not sixteen bytes wide"
    );
    assert_ne!(
        WIDTH_A,
        ledger_value(DAT_CONSTANTS_LEDGER, "DAT-C003"),
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
        NAME_LENGTH_GAP,
        NAMED_DIRECTORY.len() - ledger_value(DAT_CONSTANTS_LEDGER, "DAT-C151"),
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
        SINGLE_DIRECTORY_COUNT,
        "the dot archive has one directory"
    );
    assert_eq!(
        named_archive.directories().len(),
        SINGLE_DIRECTORY_COUNT,
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
    assert_eq!(
        MODE_MASK,
        to_u32(ledger_value(DAT_CONSTANTS_LEDGER, "DAT-C040")),
        "the mode mask is the high nibble"
    );

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

/// claim: DAT-016, DAT-018, DAT-020
/// oracle: class 4 (independent reference implementation) and class 5 (hand computed vectors from the spec)
#[test]
fn synthetic_chain_mixes_one_raw_block_and_one_lzss_block_in_stored_order() {
    assert_eq!(
        BLOCK_RAW_FLAG,
        to_u16(ledger_value(DAT_CONSTANTS_LEDGER, "DAT-C050")),
        "bit 15 of a block header marks a raw block"
    );
    assert_eq!(
        BLOCK_RAW_LENGTH_MASK,
        to_u16(ledger_value(DAT_CONSTANTS_LEDGER, "DAT-C051")),
        "the raw length mask"
    );
    assert_eq!(
        RAW_BLOCK_HEADER & BLOCK_RAW_LENGTH_MASK,
        to_u16(RAW_BLOCK_PAYLOAD.len()),
        "the raw frame header declares its own payload length"
    );
    assert_eq!(
        LZSS_BLOCK_HEADER,
        to_u16(LZSS_BLOCK_PAYLOAD.len()),
        "the lzss frame header declares its own payload length"
    );
    assert_eq!(
        to_u32(LZSS_BLOCK_OUTPUT),
        to_u32(ledger_value(LZSS_CONSTANTS_LEDGER, "LZSS-C036")),
        "the spec's worked vector decodes to the ledger's eighteen bytes"
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

    // The class 4 cross-check of the LZSS block. The reference decoder was
    // written from `docs/spec/lzss.md` alone; given the spec's worked vector
    // `00 00 0F` it must produce the eighteen spaces the production decoder
    // produced (LZSS-018).
    let reference_spaces = RefLzss::canonical()
        .decode(&LZSS_BLOCK_PAYLOAD, LZSS_BLOCK_OUTPUT)
        .expect("the reference decoder decodes the spec's worked vector");
    assert_eq!(
        reference_spaces.len(),
        ledger_value(LZSS_CONSTANTS_LEDGER, "LZSS-C036"),
        "the reference decoder's output length is the ledger's"
    );
    assert!(
        reference_spaces
            .iter()
            .all(|byte| *byte == to_u8(ledger_value(LZSS_CONSTANTS_LEDGER, "LZSS-C002"))),
        "every byte of the vector is the ledger's prefill"
    );
    assert_eq!(
        reference_spaces.as_slice(),
        lzss_block_output().as_slice(),
        "the reference decoder and the production decoder agree on the vector"
    );

    // The class 4 cross-check of the tree. The reference walker must find the
    // same two chains in the same bytes.
    let reference = RefDatTree::walk(&bytes).expect("the reference walker accepts the fixture");
    assert_eq!(
        reference.entries_total(),
        CHAIN_ENTRY_COUNT,
        "the reference walker's entry count"
    );
    for (mine, theirs) in entries
        .iter()
        .zip(reference.directories()[0].entries.iter())
    {
        assert_eq!(
            mine.path.as_str(),
            theirs.path(&reference.directories()[0].name).as_str(),
            "entry path"
        );
        assert_eq!(mine.flags, theirs.flags, "entry flags");
        assert_eq!(mine.offset, theirs.offset, "entry offset");
        assert_eq!(
            mine.uncompressed_size, theirs.uncompressed_size,
            "entry uncompressed size"
        );
        assert_eq!(
            mine.compressed_size, theirs.compressed_size,
            "entry compressed size"
        );
    }
}

/// claim: DAT-019
/// oracle: class 4 (independent reference implementation) and class 5 (hand computed vectors from the spec)
#[test]
fn synthetic_a_chain_of_two_lzss_blocks_gives_each_block_a_fresh_ring() {
    // The token of block B names ring slot 4078, the write head of a fresh
    // ring, so a carried ring and a fresh ring cannot read the same byte.
    assert_eq!(
        u32::from(TWO_BLOCK_B_TOKEN[0]) | (u32::from(TWO_BLOCK_B_TOKEN[1] & 0xF0) << 4),
        to_u32(TWO_BLOCK_B_SLOT),
        "the token's source is the write head of a fresh ring"
    );

    let bytes = synthetic_two_block_chain();
    assert_eq!(
        bytes.len(),
        TWO_BLOCK_TREE_BYTES + TWO_BLOCK_COMPRESSED_SIZE,
        "archive length"
    );
    let archive = synthetic_parse(
        &bytes,
        "synthetic_a_chain_of_two_lzss_blocks_gives_each_block_a_fresh_ring",
    );
    let entries = archive.entries();
    assert_eq!(entries.len(), TWO_BLOCK_ENTRY_COUNT, "entry count");
    let entry = entries.first().expect("the chain entry");
    assert_eq!(
        payload_mode_name(payload_mode(entry.flags)),
        "BlockChain",
        "chain mode"
    );
    assert_eq!(
        entry.compressed_size,
        to_u32(TWO_BLOCK_COMPRESSED_SIZE),
        "the chain's stored length"
    );
    assert_eq!(
        entry.uncompressed_size,
        to_u32(TWO_BLOCK_UNCOMPRESSED_SIZE),
        "the chain's decoded length"
    );

    // The chain decodes to block A's literal followed by block B's eighteen
    // bytes. A decoder that kept one ring across the chain's two blocks would
    // give block B's run block A's literal instead of zero, so every byte of
    // the second half would be wrong (DAT-019).
    let mut expected = TWO_BLOCK_A_OUTPUT.to_vec();
    expected.extend_from_slice(&TWO_BLOCK_B_OUTPUT);
    let (out, stats) = decode_entry_with_stats(entry, &bytes).expect("the two block chain decodes");
    assert_eq!(
        out.as_slice(),
        expected.as_slice(),
        "block A's literal, then block B's eighteen bytes out of a fresh ring"
    );
    assert_eq!(
        out.len(),
        TWO_BLOCK_UNCOMPRESSED_SIZE,
        "produced byte count"
    );
    assert_eq!(stats.blocks, TWO_BLOCK_BLOCKS, "block census");
    assert_eq!(
        stats.lzss_blocks, TWO_BLOCK_LZSS_BLOCKS,
        "lzss block census"
    );
    assert_eq!(stats.raw_blocks, TWO_BLOCK_RAW_BLOCKS, "raw block census");

    // The class 4 cross-check. The reference decoder was written from
    // `docs/spec/lzss.md` alone and is never imported by production code.
    // Given each block's own input and its own hand computed output length it
    // must produce the same bytes the chain decoder produced.
    let reference_a = RefLzss::canonical()
        .decode(&TWO_BLOCK_A_PAYLOAD, TWO_BLOCK_A_OUTPUT.len())
        .expect("the reference decoder decodes block A");
    assert_eq!(
        reference_a.as_slice(),
        TWO_BLOCK_A_OUTPUT.as_slice(),
        "block A's output"
    );
    let reference_b = RefLzss::canonical()
        .decode(&TWO_BLOCK_B_PAYLOAD, TWO_BLOCK_B_OUTPUT.len())
        .expect("the reference decoder decodes block B");
    assert_eq!(
        reference_b.as_slice(),
        TWO_BLOCK_B_OUTPUT.as_slice(),
        "block B's output"
    );
    let mut reference_chain = reference_a.clone();
    reference_chain.extend_from_slice(&reference_b);
    assert_eq!(
        reference_chain.as_slice(),
        out.as_slice(),
        "the reference decoder and the chain decoder agree"
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

/// claim: DAT-021
/// oracle: class 5 (hand computed vectors from the spec: the stored region is bounded by the compressed size, and the decoded form may be longer than the file)
#[test]
fn synthetic_a_stored_region_is_bounded_by_the_compressed_size_not_the_uncompressed_one() {
    // The good case. The chain stores nine bytes and decodes to twenty, so the
    // decoded extent runs eleven bytes past the end of the archive. Seven real
    // entries are in exactly this state, so a reader that bounds the stored
    // region by the uncompressed size rejects valid entries (DAT-021).
    let bytes = synthetic_chain_consumption(CONSUMPTION_EXACT, 0);
    assert_eq!(
        bytes.len(),
        CONSUMPTION_TREE_BYTES + CHAIN_COMPRESSED_SIZE,
        "archive length"
    );
    let archive = synthetic_parse(
        &bytes,
        "synthetic_a_stored_region_is_bounded_by_the_compressed_size_not_the_uncompressed_one",
    );
    let entries = archive.entries();
    assert_eq!(entries.len(), CONSUMPTION_ENTRY_COUNT, "entry count");
    let entry = entries.first().expect("one chain entry");
    assert_eq!(
        entry.compressed_size,
        to_u32(CHAIN_COMPRESSED_SIZE),
        "the chain's stored length"
    );
    assert_eq!(
        entry.uncompressed_size,
        to_u32(CHAIN_UNCOMPRESSED_SIZE),
        "the chain's decoded length"
    );
    assert_eq!(
        to_u32(to_usize(entry.offset) + CHAIN_COMPRESSED_SIZE),
        to_u32(bytes.len()),
        "the stored region lands exactly on the end of the archive"
    );
    assert_eq!(
        to_u32(to_usize(entry.offset) + CHAIN_UNCOMPRESSED_SIZE),
        to_u32(bytes.len() + CONSUMPTION_DECODED_OVERRUN),
        "the decoded region runs eleven bytes past the end of the archive"
    );
    let decoded = decode_entry(entry, &bytes).expect("the chain still decodes");
    assert_eq!(
        decoded.as_slice(),
        chain_output_raw_first().as_slice(),
        "the chain's twenty decoded bytes"
    );
    assert_eq!(
        decoded.len(),
        CHAIN_UNCOMPRESSED_SIZE,
        "produced byte count"
    );

    // The refusal case, and the twin of the case above: the same chain, one
    // byte longer in its stored form than the archive holds. The stored region
    // is bounded by the compressed size, so this one is refused (DAT-021).
    let over_bytes = synthetic_chain_consumption(CONSUMPTION_ONE_OVER, 0);
    assert_eq!(
        over_bytes.len(),
        CONSUMPTION_TREE_BYTES + CHAIN_COMPRESSED_SIZE,
        "the archive holds nine payload bytes"
    );
    let over_archive = synthetic_parse(
        &over_bytes,
        "synthetic_a_stored_region_is_bounded_by_the_compressed_size_not_the_uncompressed_one",
    );
    let over_entry = over_archive.entries().first().expect("one chain entry");
    assert_eq!(
        over_entry.compressed_size,
        to_u32(CONSUMPTION_ONE_OVER),
        "this chain declares one byte more than the archive holds"
    );
    match decoded_error(
        decode_entry(over_entry, &over_bytes),
        "synthetic_a_stored_region_is_bounded_by_the_compressed_size_not_the_uncompressed_one: one byte over",
    ) {
        DatError::OutOfBounds { at } => {
            assert_eq!(
                at,
                u64::from(over_entry.offset) + u64::from(to_u32(CONSUMPTION_ONE_OVER)),
                "the bound is the end of the stored region"
            );
        }
        other => panic!("expected OutOfBounds, got {}", dat_error_name(&other)),
    }
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
    assert_eq!(exact_out.len(), STREAM_OUTPUT.len(), "produced byte count");

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
/// oracle: class 6 (algebraic law: the key that finds the record is the stored uppercase path; a key that is not the stored path, and an absent path, are both `None` rather than errors)
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
    assert_eq!(
        upper_entry.path.as_str(),
        "ART\\TILES\\GRID000.FRM",
        "the key that finds a record is the record's own stored uppercase path"
    );

    // RULING: `find` takes an already-uppercased path and does not fold case
    // internally. Uppercasing is the caller's job. Evidence: the implementation guide
    // says "Paths use backslashes. Uppercase a path before you look it up." The
    // parameter is named `upper_path` for the same reason, and the independent oracle
    // `RefDatTree::find` is case-exact, which agrees.
    //
    // So a lowercase path is not the same entry: it misses, and a miss is `None`.
    // Do NOT "fix" this back into a case-folding lookup.
    assert!(
        archive.find("art\\tiles\\grid000.frm").is_none(),
        "a lowercase path is not the uppercased key, so it finds nothing"
    );
    assert!(
        archive.find("Art\\Tiles\\Grid000.Frm").is_none(),
        "a partially uppercased path is not the uppercased key either"
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

/// claim: DAT-002
/// oracle: class 5 (hand computed vectors from the spec: the three discarded root fields change nothing, and the directory count is the only field that changes the answer) and class 6 (algebraic law: a parse that succeeds reports exactly the number of directories the header declares)
#[test]
fn synthetic_every_bit_flip_of_the_root_header_answers_without_panicking() {
    let base = synthetic_lookup();
    assert_eq!(base.len(), LOOKUP_TREE_BYTES, "hand computed tree length");
    let archive = synthetic_parse(
        &base,
        "synthetic_every_bit_flip_of_the_root_header_answers_without_panicking",
    );
    let directories = archive.directories().len();
    let entries = archive.entries().len();
    let entry = archive.entries().first().expect("the one entry");

    let mut answered = 0usize;
    for bit in 0..ROOT_HEADER_BITS {
        let mut mutated = base.clone();
        let byte_index = bit / 8;
        let mask = 1u8 << (bit % 8);
        mutated[byte_index] ^= mask;
        let outcome = DatArchive::parse(&mutated, SYNTHETIC_ORIGIN);
        // The root header is four u32 (DAT-001). Field 0 is the directory
        // count; fields 1, 2 and 3 are read and discarded (DAT-002). So a flip
        // in field 0 is the only one that can change the answer, and a flip in
        // the other three must change nothing at all. Every one of them must
        // answer with a value and never with a panic.
        if byte_index < ROOT_FIELD_0_WIDTH {
            // The mutated count. The base holds one directory, so a flip either
            // clears the count to zero or grows it past what the archive holds.
            let count = u32::from_be_bytes([mutated[0], mutated[1], mutated[2], mutated[3]]);
            if count == 0 {
                // No directories means no names and no blocks, and the tree
                // still parses.
                let parsed = match outcome {
                    Ok(parsed) => parsed,
                    Err(error) => panic!(
                        "a cleared directory count must parse, got {}",
                        dat_error_name(&error)
                    ),
                };
                assert_eq!(parsed.directories().len(), 0, "no directories");
                assert_eq!(parsed.entries().len(), 0, "no entries");
            } else {
                // A count the bytes cannot satisfy is refused, and refused by a
                // layout error rather than by a panic.
                match outcome {
                    Ok(_) => panic!("a directory count the archive cannot satisfy must be refused"),
                    Err(error) => assert!(
                        is_tree_layout_error(&error),
                        "a refused tree is a layout error, got {}",
                        dat_error_name(&error)
                    ),
                }
            }
        } else {
            // A discarded field. The parse answers with exactly the archive it
            // answered with before the flip.
            let parsed = match outcome {
                Ok(parsed) => parsed,
                Err(error) => panic!(
                    "a discarded field never refuses the archive, got {}",
                    dat_error_name(&error)
                ),
            };
            assert_eq!(
                parsed.directories().len(),
                directories,
                "a discarded field changes nothing"
            );
            assert_eq!(
                parsed.entries().len(),
                entries,
                "a discarded field changes nothing"
            );
            let parsed_entry = parsed.entries().first().expect("the one entry");
            assert_eq!(
                parsed_entry.path.as_str(),
                entry.path.as_str(),
                "a discarded field changes nothing"
            );
            assert_eq!(
                parsed_entry.flags, entry.flags,
                "a discarded field changes nothing"
            );
            assert_eq!(
                parsed_entry.offset, entry.offset,
                "a discarded field changes nothing"
            );
            assert_eq!(
                parsed_entry.uncompressed_size, entry.uncompressed_size,
                "a discarded field changes nothing"
            );
            assert_eq!(
                parsed_entry.compressed_size, entry.compressed_size,
                "a discarded field changes nothing"
            );
        }
        answered += 1;
    }
    assert_eq!(
        answered, ROOT_HEADER_BITS,
        "every single bit flip of the root header answered with a value and not a panic"
    );
}

/// claim: DAT-002, DAT-003
/// oracle: class 5 (hand computed vectors from the spec: a tree that claims more bytes than the buffer holds is an error)
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

    // The root header claims more directories than the archive has bytes for.
    let mut too_many_directories = base.clone();
    too_many_directories[ROOT_FIELD0] = 0;
    too_many_directories[ROOT_FIELD0 + 1] = 0;
    too_many_directories[ROOT_FIELD0 + 2] = 0;
    too_many_directories[ROOT_FIELD0 + 3] = ROOT_DIRECTORIES_PAST_END;

    // A zero-entry block and a zero-length entry name are deliberately absent from this
    // list: both are accepted, by ruling. See
    // `synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted`.
    let cases = [
        ("directory name length past the end", over_long_name),
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

/// claim: DAT-033, DAT-034
/// oracle: class 5 (hand-computed / specified behaviour, NOT class 4: the guide and the spec are
/// silent on both cases, so nothing here is established by an independent reference
/// implementation. The oracle happens to agree; agreement without a rule is not a rule.)
#[test]
fn synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted() {
    let base = synthetic_lookup();
    synthetic_parse(
        &base,
        "synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted",
    );
    assert_eq!(base.len(), LOOKUP_TREE_BYTES, "hand computed tree length");

    // RULING: `DatArchive::parse` accepts both of these. The guide and the spec are
    // silent on both, no byte evidence establishes that either is malformed, and the
    // independent oracle returns `Ok` for both. Inventing a rejection rule would be an
    // unmeasured claim, which the guide forbids: "Take no step on an unmeasured claim.
    // Isolate it and list it as open." So the parser accepts them and the behaviour is
    // recorded as an open item rather than encoded as a rule.
    //
    // OPEN ITEM: whether a zero-entry block and a zero-length entry name are intended
    // to be accepted is UNRESOLVED. The spec does not say. This test pins the ruled
    // behaviour so the ruling is visible and cannot drift silently.
    //
    // Do NOT "fix" these two cases back into `synthetic_malformed_trees_are_errors`.

    // The block header's field 0 is the entry count, and zero is a legal count: a block
    // with no entries describes nothing, which is not the same as describing something
    // that runs off the end.
    let mut zero_block_count = base.clone();
    zero_block_count[LOOKUP_BLOCK_FIELD0] = 0;
    zero_block_count[LOOKUP_BLOCK_FIELD0 + 1] = 0;
    zero_block_count[LOOKUP_BLOCK_FIELD0 + 2] = 0;
    zero_block_count[LOOKUP_BLOCK_FIELD0 + 3] = 0;

    // An entry whose name length is zero. The seventeen byte minimum record still
    // applies, so the record is well formed; only the name is empty.
    let mut zero_entry_name = base.clone();
    zero_entry_name[LOOKUP_ENTRY_NAME_LENGTH] = 0;

    let cases = [
        ("block header entry count of zero", zero_block_count),
        ("entry name length of zero", zero_entry_name),
    ];
    assert_eq!(
        cases.len(),
        ACCEPTED_TREE_COUNT,
        "every ruled-accepted case built"
    );

    let mut accepted = 0usize;
    for (case, bytes) in &cases {
        match DatArchive::parse(bytes, SYNTHETIC_ORIGIN) {
            Ok(_) => accepted += 1,
            Err(error) => panic!(
                "{case}: a ruled-accepted tree parsed, got {}",
                dat_error_name(&error)
            ),
        }
    }
    assert_eq!(
        accepted, ACCEPTED_TREE_COUNT,
        "every ruled-accepted case accepted"
    );
}

/// claim: DAT-030, DAT-001
/// oracle: class 5 (hand computed vectors from the spec: the root header is four big endian u32, so its third field starts at byte 8 and a ten byte tree runs out there, and an error carries the offset the failed field started at)
///
/// The name of this test carries the old wording, "the byte position it ran
/// out at", and the trace map names it, so it stays. What the test pins is
/// the rule the spec now states: the offset is where the failed operation
/// **started**, and the offsets each variant carries are the ones the spec's
/// *byte offset an error carries* section gives, not one rule for all of them.
#[test]
fn synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at() {
    // The good input twin. A complete synthetic archive goes through the same
    // reader and is accepted, so the refusal below is about the ten bytes and
    // not about the fixture or about the reader refusing everything.
    let complete = synthetic_lookup();
    let twin = synthetic_parse(
        &complete,
        "synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at: the complete archive",
    );
    assert_eq!(
        twin.directories().len(),
        SINGLE_DIRECTORY_COUNT,
        "the complete archive has its one directory"
    );
    assert_eq!(
        twin.entries().len(),
        SINGLE_BLOCK_ENTRIES,
        "the complete archive has its one entry"
    );
    assert_eq!(
        complete.len(),
        LOOKUP_TREE_BYTES,
        "hand computed tree length"
    );

    // The refusal. Ten bytes of zeroes: a root header that stops two bytes into
    // its third field.
    let truncated = [0u8; TRUNCATED_TREE_BYTES];
    assert_eq!(truncated.len(), 10, "hand computed truncation length");
    assert_eq!(
        ROOT_FIELD_2_OFFSET, 8,
        "the root header's third u32 starts at byte 8"
    );

    // The root header is four big endian u32 (DAT-001): field 0 covers bytes
    // 0..4, field 1 covers 4..8 and field 2 covers 8..12. A ten byte buffer
    // leaves two of the four bytes field 2 needs, so the read that runs out is
    // the one that starts at byte 8. The error carries that position (DAT-030),
    // which is the whole claim: the refusal says where it ran out and not only
    // that it ran out.
    //
    // The rule the spec fixes is that the offset is the one the failed
    // operation started at, so the error carries 8 and not the 10 the reader
    // noticed it had run out at. The two readings are one byte apart for a
    // field that is missing its second byte, which is the third refusal below.
    let error = match DatArchive::parse(&truncated, SYNTHETIC_ORIGIN) {
        Ok(_) => panic!("a ten byte tree must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        error,
        DatError::Truncated { at: TRUNCATED_AT },
        "a ten byte tree runs out at byte 8, where the root header's third u32 starts"
    );

    // The second refusal, one field further on. The root header's fourth u32
    // starts at byte 12 and a fifteen byte tree leaves three of its four bytes,
    // so the read that fails is the one that starts at byte 12. The error
    // carries 12 and not the 15 the reader noticed it at.
    let truncated_field3 = [0u8; TRUNCATED_FIELD3_TREE_BYTES];
    assert_eq!(
        truncated_field3.len(),
        15,
        "hand computed truncation length"
    );
    let field3_error = match DatArchive::parse(&truncated_field3, SYNTHETIC_ORIGIN) {
        Ok(_) => panic!("a fifteen byte tree must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        field3_error,
        DatError::Truncated {
            at: TRUNCATED_FIELD3_AT
        },
        "a fifteen byte tree runs out at byte 12, where the root header's fourth u32 starts"
    );

    // The third refusal is the one the two readings are one byte apart on. A
    // directory name says two bytes and only one of them is in the archive, so
    // the name field is missing its second byte. The error carries 17, where
    // the name field starts, and not the 18 the reader noticed it at.
    let mut truncated_name = Vec::new();
    push_u32(&mut truncated_name, TRUNCATED_NAME_DIRECTORIES);
    push_u32(&mut truncated_name, 0);
    push_u32(&mut truncated_name, 0);
    push_u32(&mut truncated_name, 0);
    truncated_name.push(TRUNCATED_NAME_LENGTH);
    truncated_name.push(TRUNCATED_NAME_BYTE);
    assert_eq!(
        truncated_name.len(),
        TRUNCATED_NAME_TREE_BYTES,
        "hand computed truncation length"
    );
    let name_error = match DatArchive::parse(&truncated_name, SYNTHETIC_ORIGIN) {
        Ok(_) => panic!("a tree with a half written directory name must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        name_error,
        DatError::LengthOutOfRange { at: TRUNCATED_NAME_AT },
        "a name field that is missing its second byte reports the byte the field starts at, not the byte the reader noticed it at"
    );

    // The fourth refusal names a length byte rather than a name field. The error
    // set is closed (DAT-030) and has no malformed-name variant, so a stored
    // name that is not UTF-8 is reported as the unusable length it came from,
    // and the offset it carries is that length byte's own offset.
    let mut bad_name = synthetic_lookup();
    bad_name[LOOKUP_ENTRY_NAME_LENGTH + 1] = NOT_UTF8_BYTE;
    let bad_name_error = match DatArchive::parse(&bad_name, SYNTHETIC_ORIGIN) {
        Ok(_) => panic!("a tree whose entry name is not UTF-8 must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        bad_name_error,
        DatError::LengthOutOfRange {
            at: u64::from(to_u32(LOOKUP_ENTRY_NAME_LENGTH))
        },
        "a name that is not UTF-8 reports the length byte that asked for it"
    );

    // The fifth refusal is the LZSS path, and the only path that adds a base to
    // a position instead of copying a field's first byte. A whole LZSS stream
    // whose input runs out in the middle of a back reference reports the
    // entry's payload base plus the codec's position inside the stream, and
    // that position is the first byte of the incomplete pair (LZSS-007).
    let cut_stream_bytes = synthetic_cut_pair_stream();
    let cut_stream_archive = synthetic_parse(
        &cut_stream_bytes,
        "synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at: the cut pair stream",
    );
    let cut_stream_entry = cut_stream_archive
        .entries()
        .first()
        .expect("one stream entry");
    assert_eq!(
        cut_stream_entry.offset,
        to_u32(CUT_PAIR_STREAM_TREE_BYTES),
        "the stream's input starts where the tree ends"
    );
    let cut_stream_error = decoded_error(
        decode_entry(cut_stream_entry, &cut_stream_bytes),
        "synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at: the cut pair stream",
    );
    assert_eq!(
        cut_stream_error,
        DatError::Truncated {
            at: u64::from(to_u32(
                CUT_PAIR_STREAM_TREE_BYTES + CUT_PAIR_STREAM_POSITION
            ))
        },
        "an LZSS stream that runs out mid pair reports the payload base plus the pair's first byte"
    );

    // The sixth refusal names a framed block inside a chain, and the offset it
    // carries is the block's header start: the chain's payload base plus the
    // block's position within the chain (DAT-018).
    let empty_block_bytes = synthetic_chain_after_raw(ZERO_PAYLOAD_BLOCK_HEADER);
    let empty_block_archive = synthetic_parse(
        &empty_block_bytes,
        "synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at: the zero length frame",
    );
    let empty_block_entry = empty_block_archive
        .entries()
        .first()
        .expect("one chain entry");
    assert_eq!(
        empty_block_entry.offset,
        to_u32(CHAIN_AFTER_RAW_TREE_BYTES),
        "the chain's stored bytes start where the tree ends"
    );
    let empty_block_error = decoded_error(
        decode_entry(empty_block_entry, &empty_block_bytes),
        "synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at: the zero length frame",
    );
    assert_eq!(
        empty_block_error,
        DatError::EmptyBlock {
            at: u64::from(to_u32(
                CHAIN_AFTER_RAW_TREE_BYTES + CHAIN_SECOND_BLOCK_CURSOR
            ))
        },
        "a zero length frame reports the start of the block header that declares it"
    );

    // The seventh refusal is the same block with a payload the chain does not
    // carry, and it names the same place: the block's header start.
    let oversized_bytes = synthetic_chain_after_raw(OVERSIZED_BLOCK_HEADER);
    let oversized_archive = synthetic_parse(
        &oversized_bytes,
        "synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at: the oversized frame",
    );
    let oversized_entry = oversized_archive
        .entries()
        .first()
        .expect("one chain entry");
    let oversized_error = decoded_error(
        decode_entry(oversized_entry, &oversized_bytes),
        "synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at: the oversized frame",
    );
    assert_eq!(
        oversized_error,
        DatError::OutOfBounds {
            at: u64::from(to_u32(CHAIN_AFTER_RAW_TREE_BYTES + CHAIN_SECOND_BLOCK_CURSOR))
        },
        "a frame whose payload runs past the chain reports the start of the block header that declares it"
    );
}

/// claim: DAT-030
/// oracle: class 5 (hand computed vectors from the spec: the origin label handed to `parse` is the label the archive carries)
#[test]
fn synthetic_the_archive_reports_the_label_it_was_parsed_under() {
    // The good input twin, and the label every other fixture in this file is
    // parsed under.
    let bytes = synthetic_lookup();
    let archive = synthetic_parse(
        &bytes,
        "synthetic_the_archive_reports_the_label_it_was_parsed_under",
    );
    assert_eq!(
        archive.entries().len(),
        SINGLE_BLOCK_ENTRIES,
        "the archive has its one entry"
    );
    assert_eq!(
        archive.origin(),
        SYNTHETIC_ORIGIN,
        "the archive reports the label it was parsed under"
    );

    // The same bytes under a second label. The label travels with the parse
    // and not with the archive bytes, so the archive reports the label it was
    // parsed under this time.
    let relabelled = match DatArchive::parse(&bytes, SYNTHETIC_ALT_ORIGIN) {
        Ok(parsed) => parsed,
        Err(error) => panic!(
            "the same complete archive must parse under a second label, got {}",
            dat_error_name(&error)
        ),
    };
    assert_eq!(
        relabelled.origin(),
        SYNTHETIC_ALT_ORIGIN,
        "the label follows the parse, not the bytes"
    );
    assert_eq!(
        relabelled.entries().len(),
        archive.entries().len(),
        "the same archive under a second label"
    );
}

/// claim: DAT-030, DAT-024
/// oracle: class 5 (hand computed vectors from the spec: every error variant names the quantity it is about, and the *Error text* section of `docs/spec/dat.md` fixes the wording of every message word for word)
#[test]
fn synthetic_every_error_variant_names_what_went_wrong() {
    assert_eq!(
        DAT_ERROR_CASES.len(),
        DAT_ERROR_CASE_COUNT,
        "every variant of the error type is graded"
    );

    let mut rendered: Vec<String> = Vec::new();
    for (name, error, quantities, expected) in DAT_ERROR_CASES {
        let text = error.to_string();
        assert_eq!(
            dat_error_name(&error),
            name,
            "the row names the variant it holds"
        );
        assert!(!text.is_empty(), "{name}: the message must not be empty");
        for quantity in quantities {
            assert!(
                reports_number(&text, *quantity),
                "{name}: the message must name {quantity}"
            );
        }
        // The spec fixes the wording of every message, so the message is
        // asserted in full and not only for the quantity it names. This is what
        // makes the assertion able to fail: a message that names the right
        // number in the wrong words is still a message the spec does not fix.
        assert_eq!(
            text, *expected,
            "{name}: the message is the spec's, word for word"
        );
        rendered.push(text);
    }
    assert_eq!(
        rendered.len(),
        DAT_ERROR_CASE_COUNT,
        "every variant rendered a message"
    );

    // Two variants that rendered the same text would not be saying what went
    // wrong, so the nine messages have to differ from one another.
    let mut sorted = rendered.clone();
    sorted.sort();
    assert!(
        sorted.windows(2).all(|pair| pair[0] != pair[1]),
        "every variant must render a message of its own"
    );
}

/// claim: DAT-018, DAT-024, LZSS-014, LZSS-015
/// oracle: class 5 (hand computed vectors from the spec: a back reference that runs out of input is an overrun when the declaration was already met and a truncation when it was not)
#[test]
fn synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met() {
    // The positive control, and the twin of both refusals below. The block is
    // the spec's worked vector `00 00 0F` (LZSS-018): a control byte of zero,
    // then a back reference of source 0 and length 18, which lands in the
    // prefilled `0x20` region and decodes to eighteen spaces. The pair is
    // complete, the entry declares eighteen bytes, so the declaration is met
    // exactly and the chain decodes.
    let complete_bytes = synthetic_single_lzss_chain(&LZSS_BLOCK_PAYLOAD, LZSS_BLOCK_OUTPUT);
    let complete_archive = synthetic_parse(
        &complete_bytes,
        "synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met",
    );
    let complete_entries = complete_archive.entries();
    assert_eq!(
        complete_entries.len(),
        SINGLE_BLOCK_CHAIN_ENTRY_COUNT,
        "one chain entry"
    );
    let complete_entry = complete_entries.first().expect("one chain entry");
    assert_eq!(
        complete_entry.uncompressed_size,
        to_u32(LZSS_BLOCK_OUTPUT),
        "the declared output size"
    );
    assert_eq!(
        complete_entry.compressed_size,
        to_u32(SINGLE_BLOCK_CHAIN_COMPLETE_COMPRESSED),
        "the frame header plus its three payload bytes"
    );
    let complete_out =
        decode_entry(complete_entry, &complete_bytes).expect("the complete pair decodes");
    assert_eq!(
        complete_out.as_slice(),
        lzss_block_output().as_slice(),
        "the complete pair decodes to eighteen spaces"
    );
    assert_eq!(complete_out.len(), LZSS_BLOCK_OUTPUT, "produced byte count");

    // One byte shorter: the control byte and the first byte of the pair, and
    // nothing else. A back reference needs two input bytes and one is left
    // (LZSS-015), so the block cannot finish. Nothing had been decoded when
    // the pair started, so the entry was still owed all eighteen bytes: the
    // declaration was NOT met, and the container reports the truncation rather
    // than an overrun (DAT-024: the declared size exists to detect tokens
    // writing past it, and no token could have written more than the eighteen
    // bytes that were left, since a match is 3 to 18 bytes, LZSS-008).
    let cut_bytes = synthetic_single_lzss_chain(&LZSS_CUT_PAIR_PAYLOAD, LZSS_BLOCK_OUTPUT);
    let cut_archive = synthetic_parse(
        &cut_bytes,
        "synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met",
    );
    let cut_entry = cut_archive.entries().first().expect("one chain entry");
    assert_eq!(
        cut_entry.compressed_size,
        to_u32(SINGLE_BLOCK_CHAIN_CUT_COMPRESSED),
        "the frame header plus its two payload bytes"
    );
    let cut_error = decoded_error(
        decode_entry(cut_entry, &cut_bytes),
        "synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met: the pair is cut",
    );
    // The offset is not asserted: the spec fixes that the input ran out, not
    // which byte of the incomplete pair the container names.
    assert_eq!(
        dat_error_name(&cut_error),
        "Truncated",
        "the declaration was not met when the input ran out, so this is a truncation and not an overrun"
    );

    // The boundary. The block fills the declaration exactly and then runs out
    // in the middle of a second back reference. The declaration WAS met when
    // the input ran out, so the tokens still pending behind the control byte
    // would have written past it (DAT-024), and the container reports the
    // overrun.
    let full_bytes =
        synthetic_single_lzss_chain(&LZSS_FULL_THEN_CUT_PAIR_PAYLOAD, LZSS_BLOCK_OUTPUT);
    let full_archive = synthetic_parse(
        &full_bytes,
        "synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met",
    );
    let full_entry = full_archive.entries().first().expect("one chain entry");
    assert_eq!(
        full_entry.compressed_size,
        to_u32(SINGLE_BLOCK_CHAIN_FULL_THEN_CUT_COMPRESSED),
        "the frame header plus its four payload bytes"
    );
    match decoded_error(
        decode_entry(full_entry, &full_bytes),
        "synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met: the declaration was met",
    ) {
        DatError::OutputOverrun { declared } => {
            assert_eq!(
                declared,
                to_u32(LZSS_BLOCK_OUTPUT),
                "the declared output size the entry promised"
            );
        }
        other => panic!(
            "expected OutputOverrun, got {}",
            dat_error_name(&other)
        ),
    }
}
