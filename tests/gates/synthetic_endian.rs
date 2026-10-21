//! Synthetic gate tests for the two byte order layers (`fo1-bigendian` and
//! `fo1-endian-host`).
//!
//! Spec: `docs/spec/endian.md`. Every fixture here is built byte by byte in this
//! file and named `synthetic_*`, so the suite needs no game install and runs in
//! CI.
//!
//! A synthetic fixture cannot support a CONFIRMED evidence tag (guide: Fixtures
//! and golden files), so **no test below can be oracle class 1, 2 or 3**: none of
//! those classes can be produced out of bytes this file invented. Every test is
//! class 5, a hand computed vector the spec states, or class 6, an algebraic law
//! about the reader's own arithmetic. The two claims in this layer that a strong
//! oracle would settle, END-006 and END-035, are pinned here as class 5 vectors
//! because the spec states both results; the class 1 run that lifts the four bytes
//! out of the original executable belongs to `tests/gates/install_exe_tables.rs`.
//!
//! The oracle classes used below follow `docs/spec/trace.md`: oracle class 5 (hand
//! computed vectors from the spec) and oracle class 6 (algebraic laws).
//!
//! Two numbers govern the whole layer, and every fixture is shaped to make them
//! visible at once:
//!
//! ```text
//!   01 00 00 00   executable layer   ->          1   the XP value for level 1
//!   01 00 00 00   asset layer        ->  16,777,216   the very same four bytes
//! ```
//!
//! Neither read raises an error (END-008), so nothing inside the read path could
//! notice the mistake and the only place the rule can live is the type (END-010,
//! END-012, END-013). That is why these tests assert the fold, the widths, the
//! error's three labels and the absence of a cross-layer conversion, rather than
//! any single decoded value.

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
use fo1_bigendian::{BigEndian, BigEndianError};
use fo1_endian_host::{LittleEndian, LittleEndianError};

// --- ledger constants ---------------------------------------------------------

/// Width of one `u8` read, in bytes (`END-C001`).
const WIDTH_U8: usize = 1;

/// Width of one `u16` read and of one `i16` read, in bytes (`END-C002`).
const WIDTH_U16: usize = 2;

/// Width of one `u32` read and of one `i32` read, in bytes (`END-C003`).
const WIDTH_U32: usize = 4;

/// Field label the spec fixes for a failed `u8` read.
const FIELD_U8: &str = "u8";

/// Field label the spec fixes for a failed `u16` read.
const FIELD_U16: &str = "u16";

/// Field label the spec fixes for a failed `u32` read.
const FIELD_U32: &str = "u32";

/// Field label the spec fixes for a failed `i16` read.
const FIELD_I16: &str = "i16";

/// Field label the spec fixes for a failed `i32` read.
const FIELD_I32: &str = "i32";

/// Field label the spec fixes for a failed `bytes` read.
const FIELD_BYTES: &str = "bytes";

/// Field label the spec fixes for a failed `skip`.
const FIELD_SKIP: &str = "skip";

/// Field label the spec fixes for a rejected `seek`.
const FIELD_SEEK: &str = "seek";

/// Error variants per crate: `Truncated` and `SeekOutOfRange` (`END-C012`).
const ERROR_VARIANT_COUNT: usize = 2;

/// The layer name the asset reader reports for itself.
const ASSET_LAYER_LABEL: &str = "asset layer, big-endian";

/// The layer name the executable reader reports for itself.
const HOST_LAYER_LABEL: &str = "executable layer, little-endian";

/// Diagnostic label every asset-layer fixture is parsed under.
const SYNTHETIC_ASSET_ORIGIN: &str = "synthetic_asset_origin";

/// A second asset-layer label, so a test can prove the origin travels with the
/// reader instance rather than being baked into the error type (END-022).
const SYNTHETIC_ASSET_ALT_ORIGIN: &str = "synthetic_asset_alt_origin";

/// Diagnostic label every executable-layer fixture is parsed under.
const SYNTHETIC_HOST_ORIGIN: &str = "synthetic_host_origin";

/// A second executable-layer label, for the same reason as
/// [`SYNTHETIC_ASSET_ALT_ORIGIN`].
const SYNTHETIC_HOST_ALT_ORIGIN: &str = "synthetic_host_alt_origin";

// --- the four bytes every fold test reads -------------------------------------

/// The four bytes whose two folds differ by three orders of magnitude. Also the
/// four bytes at the executable's XP table, so one fixture carries both the fold
/// rule (END-015) and the anti-deviation witness (END-006).
const SYNTHETIC_FOLD_BYTES: [u8; WIDTH_U32] = [0x12, 0x34, 0x56, 0x78];

/// `SYNTHETIC_FOLD_BYTES` folded most significant byte first: `12 34 56 78`.
const SYNTHETIC_U32_ASSET: u32 = 0x1234_5678;

/// `SYNTHETIC_FOLD_BYTES` folded least significant byte first: `78 56 34 12`.
const SYNTHETIC_U32_HOST: u32 = 0x7856_3412;

/// The first two bytes folded most significant byte first: `12 34`.
const SYNTHETIC_U16_ASSET: u16 = 0x1234;

/// The first two bytes folded least significant byte first: `34 12`.
const SYNTHETIC_U16_HOST: u16 = 0x3412;

/// The first byte read by either layer: a one byte fold cannot differ.
const SYNTHETIC_U8_BOTH: u8 = 0x12;

/// `SYNTHETIC_ALL_ONES` as an unsigned value: the trap END-016 names.
const SYNTHETIC_ALL_ONES_UNSIGNED: u32 = 4_294_967_295;

/// The same four bytes as a two's complement value: minus one, not 4,294,967,295.
const SYNTHETIC_ALL_ONES_SIGNED: i32 = -1;

/// Minus one as an `i16`, for the two byte half of END-016.
const SYNTHETIC_TWO_ALL_ONES_SIGNED: i16 = -1;

/// The most negative `i16`, spelled out by hand rather than named `i16::MIN`, so
/// a reader that returned the unsigned reinterpret would fail this assertion.
const SYNTHETIC_I16_MIN: i16 = -32_768;

/// The same two bytes under the other fold: `80 00` reads as 128, not minus
/// 32,768, because the sign bit is the low one.
const SYNTHETIC_I16_MIN_AS_OTHER_ORDER: i16 = 128;

/// The most negative `i32`, spelled out by hand.
const SYNTHETIC_I32_MIN: i32 = -2_147_483_648;

/// The same four bytes under the other fold, where the high bit is a low bit.
const SYNTHETIC_I32_MIN_AS_OTHER_ORDER: i32 = 128;

/// Four all-ones bytes, the exact case END-016 names.
const SYNTHETIC_ALL_ONES: [u8; WIDTH_U32] = [0xFF, 0xFF, 0xFF, 0xFF];

/// Two all-ones bytes, which are minus one in two's complement at either width.
const SYNTHETIC_TWO_ALL_ONES: [u8; WIDTH_U16] = [0xFF, 0xFF];

/// The most negative `i16`, whose asset-layer bytes are `80 00`.
const SYNTHETIC_I16_MIN_ASSET_BYTES: [u8; WIDTH_U16] = [0x80, 0x00];

/// The most negative `i16`, whose executable-layer bytes are `00 80`.
const SYNTHETIC_I16_MIN_HOST_BYTES: [u8; WIDTH_U16] = [0x00, 0x80];

/// The most negative `i32`, whose asset-layer bytes are `80 00 00 00`: the
/// highest bit is the sign bit and it is read first.
const SYNTHETIC_I32_MIN_ASSET_BYTES: [u8; WIDTH_U32] = [0x80, 0x00, 0x00, 0x00];

/// The most negative `i32`, whose executable-layer bytes are `00 00 00 80`: the
/// lowest bit is the sign bit and it is read last.
const SYNTHETIC_I32_MIN_HOST_BYTES: [u8; WIDTH_U32] = [0x00, 0x00, 0x00, 0x80];

// --- the vectors the per-reader tests fold ------------------------------------

/// One byte, one short of what a two byte read asks for (END-017).
///
/// The length is written as `WIDTH_U16 - 1` and not as a literal, so the
/// fixture cannot drift away from the width it is short of.
const SYNTHETIC_ONE_BYTE_SHORT: [u8; WIDTH_U16 - 1] = [0x12];

/// Three bytes, one short of what a four byte read asks for (END-017).
const SYNTHETIC_THREE_BYTES_SHORT: [u8; WIDTH_U32 - 1] = [0x12, 0x34, 0x56];

/// Two bytes whose two's complement value is neither zero, one nor minus one:
/// `FF FE`.
///
/// Most significant byte first the pair is `0xFFFE`, which is -2 as an `i16`;
/// least significant byte first it is `0xFEFF`, which is -257. The two layers
/// therefore answer with two different numbers, and neither answer is a
/// constant a reader could return instead of folding (END-016).
const SYNTHETIC_SIGNED_PAIR: [u8; WIDTH_U16] = [0xFF, 0xFE];

/// `FF FE` read most significant byte first is -2 as an `i16`.
const SYNTHETIC_SIGNED_PAIR_ASSET: i16 = -2;

/// `FF FE` read least significant byte first is -257 as an `i16`.
const SYNTHETIC_SIGNED_PAIR_HOST: i16 = -257;

// --- the mixed-read buffer ----------------------------------------------------

/// Sixteen bytes, long enough for every read the width test performs and for the
/// monotonic `remaining` walk. The bytes are distinct so a wrong slice shows up in
/// the failure message instead of looking like another byte of the buffer.
const SYNTHETIC_MIXED_BYTES: [u8; 16] = *b"SYNTHETIC-ENDIAN";

/// Length of [`SYNTHETIC_MIXED_BYTES`].
const MIXED_LEN: usize = 16;

/// Cursor position after one `u8` out of [`SYNTHETIC_MIXED_BYTES`].
const MIXED_AFTER_U8: usize = WIDTH_U8;

/// Cursor position after the `u8` and the `u16`.
const MIXED_AFTER_U16: usize = MIXED_AFTER_U8 + WIDTH_U16;

/// Cursor position after the `u8`, the `u16` and the `u32`.
const MIXED_AFTER_U32: usize = MIXED_AFTER_U16 + WIDTH_U32;

/// Cursor position after the four reads above plus the `i16`.
const MIXED_AFTER_I16: usize = MIXED_AFTER_U32 + WIDTH_U16;

/// Cursor position after every integer read of the width test.
const MIXED_AFTER_I32: usize = MIXED_AFTER_I16 + WIDTH_U32;

/// Bytes still left once every integer read has run.
const MIXED_AFTER_I32_LEFT: usize = MIXED_LEN - MIXED_AFTER_I32;

/// The `remaining` sequence the mixed walk must produce: one sample before the
/// first read, then one after each of the six reads. Each step falls by exactly
/// the width of the read that produced it, and no step rises.
const MIXED_REMAINING: [usize; 7] = [
    MIXED_LEN,
    MIXED_LEN - MIXED_AFTER_U8,
    MIXED_LEN - MIXED_AFTER_U16,
    MIXED_LEN - MIXED_AFTER_U32,
    MIXED_LEN - MIXED_AFTER_I16,
    MIXED_LEN - MIXED_AFTER_I32,
    MIXED_AFTER_TAIL,
];

/// Reads in the mixed walk: one fewer than the count of [`MIXED_REMAINING`]
/// values, because the first of those is the state before any read ran.
const MIXED_STEPS: usize = MIXED_REMAINING.len() - 1;

/// Adjacent pairs in the mixed walk.
const MIXED_PAIRS: usize = MIXED_REMAINING.len() - 1;

/// Every adjacent pair of the mixed walk falls, because every width is positive.
const MIXED_DESCENTS: usize = MIXED_PAIRS;

/// One trailing `bytes` read consumes exactly the last byte and leaves nothing.
const MIXED_TRAILING_WIDTH: usize = MIXED_AFTER_I32_LEFT;

/// Bytes left once the trailing `bytes` read has run.
const MIXED_AFTER_TAIL: usize = 0;

/// The first byte of [`SYNTHETIC_MIXED_BYTES`], which a one byte read returns.
const SYNTHETIC_MIXED_FIRST_BYTE: u8 = b'S';

// --- the totality tables ------------------------------------------------------

/// Which read method a totality case exercises. A tag rather than a string, so
/// the dispatch in [`asset_read`] is exhaustive and a new method cannot join the
/// table without the compiler naming it.
#[derive(Clone, Copy)]
enum ReadMethod {
    /// One `u8` read.
    U8,
    /// One `u16` read.
    U16,
    /// One `u32` read.
    U32,
    /// One `i16` read.
    I16,
    /// One `i32` read.
    I32,
    /// One `bytes` read of the case's width.
    Bytes,
    /// One `skip` of the case's width.
    Skip,
}

/// One totality case: a name for the failure message, the method, the width that
/// method asks for, and the field label the spec fixes for it.
struct TotalityCase {
    /// Name printed with every assertion in this case.
    name: &'static str,
    /// Which read the case drives.
    method: ReadMethod,
    /// Bytes the read asks for, which is its own width.
    width: usize,
    /// Field label the spec fixes for that read.
    field: &'static str,
}

/// Every read method the spec's interface lists, each against a buffer one byte
/// shorter than it asks for. The buffer length is derived as `width - 1`, so a
/// case cannot pass by accident: the read is genuinely one byte short and not
/// empty.
const SHORT_CASES: [TotalityCase; 7] = [
    TotalityCase {
        name: "u8",
        method: ReadMethod::U8,
        width: WIDTH_U8,
        field: FIELD_U8,
    },
    TotalityCase {
        name: "u16",
        method: ReadMethod::U16,
        width: WIDTH_U16,
        field: FIELD_U16,
    },
    TotalityCase {
        name: "u32",
        method: ReadMethod::U32,
        width: WIDTH_U32,
        field: FIELD_U32,
    },
    TotalityCase {
        name: "i16",
        method: ReadMethod::I16,
        width: WIDTH_U16,
        field: FIELD_I16,
    },
    TotalityCase {
        name: "i32",
        method: ReadMethod::I32,
        width: WIDTH_U32,
        field: FIELD_I32,
    },
    TotalityCase {
        name: "bytes",
        method: ReadMethod::Bytes,
        width: WIDTH_U32,
        field: FIELD_BYTES,
    },
    TotalityCase {
        name: "skip",
        method: ReadMethod::Skip,
        width: WIDTH_U32,
        field: FIELD_SKIP,
    },
];

/// Cases in [`SHORT_CASES`].
const SHORT_CASE_COUNT: usize = SHORT_CASES.len();

/// Bytes short of the end for every case in [`SHORT_CASES`].
const SHORT_DEFICIT: usize = 1;

/// The cursor every case in [`SHORT_CASES`] fails at, because the reader is fresh
/// and the bounds check runs before anything is consumed.
const SHORT_CASE_OFFSET: usize = 0;

/// Every read method the spec's interface lists, against an empty buffer.
const EMPTY_CASES: [TotalityCase; 7] = [
    TotalityCase {
        name: "u8",
        method: ReadMethod::U8,
        width: WIDTH_U8,
        field: FIELD_U8,
    },
    TotalityCase {
        name: "u16",
        method: ReadMethod::U16,
        width: WIDTH_U16,
        field: FIELD_U16,
    },
    TotalityCase {
        name: "u32",
        method: ReadMethod::U32,
        width: WIDTH_U32,
        field: FIELD_U32,
    },
    TotalityCase {
        name: "i16",
        method: ReadMethod::I16,
        width: WIDTH_U16,
        field: FIELD_I16,
    },
    TotalityCase {
        name: "i32",
        method: ReadMethod::I32,
        width: WIDTH_U32,
        field: FIELD_I32,
    },
    TotalityCase {
        name: "bytes",
        method: ReadMethod::Bytes,
        width: WIDTH_U8,
        field: FIELD_BYTES,
    },
    TotalityCase {
        name: "skip",
        method: ReadMethod::Skip,
        width: WIDTH_U8,
        field: FIELD_SKIP,
    },
];

/// Cases in [`EMPTY_CASES`].
const EMPTY_CASE_COUNT: usize = EMPTY_CASES.len();

/// Length of the buffer every case in [`EMPTY_CASES`] is given.
const EMPTY_BUFFER_LEN: usize = 0;

/// Cases the empty buffer test drives: both tables, because on an empty buffer
/// every method fails and both tables have something to say about which one.
const EMPTY_DRIVEN_CASE_COUNT: usize = SHORT_CASE_COUNT + EMPTY_CASE_COUNT;

/// Independent proofs that a failed read is inert: every method of
/// [`SHORT_CASES`], driven once per layer.
const INERT_CASE_COUNT: usize = SHORT_CASE_COUNT * 2;

// --- seek, skip and borrow constants ------------------------------------------

/// Length of the buffer the cursor tests drive.
const CURSOR_LEN: usize = 8;

/// Positions `seek` must accept: every one from zero through the buffer length,
/// inclusive.
const SEEK_ACCEPTED: [usize; CURSOR_LEN + 1] = [0, 1, 2, 3, 4, 5, 6, 7, 8];

/// Cases in [`SEEK_ACCEPTED`].
const SEEK_ACCEPTED_COUNT: usize = SEEK_ACCEPTED.len();

/// The first position past the end, which `seek` must reject.
const SEEK_REJECTED_FIRST: usize = CURSOR_LEN + 1;

/// Positions past the end the rejection test drives, so the rejection is a range
/// check and not one hand-picked value.
const SEEK_REJECTED_POSITIONS: [usize; 3] = [CURSOR_LEN + 1, CURSOR_LEN + 2, CURSOR_LEN + 7];

/// Cases in [`SEEK_REJECTED_POSITIONS`].
const SEEK_REJECTED_COUNT: usize = SEEK_REJECTED_POSITIONS.len();

/// Bytes left after `seek` lands on the buffer length.
const SEEK_AT_END_REMAINING: usize = 0;

/// Bytes a read still wants after `seek` has landed on the end, which is why the
/// read there is an error and not a short answer.
const SEEK_AT_END_READ: usize = WIDTH_U8;

/// Two successive skips, each of this width, so the second starts where the first
/// stopped.
const SKIP_STEP: usize = 2;

/// Cursor after one skip of [`SKIP_STEP`].
const SKIP_AFTER_FIRST: usize = SKIP_STEP;

/// Cursor after two skips of [`SKIP_STEP`].
const SKIP_AFTER_SECOND: usize = 2 * SKIP_STEP;

/// Bytes still left after two skips of [`SKIP_STEP`].
const SKIP_AFTER_SECOND_LEFT: usize = CURSOR_LEN - SKIP_AFTER_SECOND;

/// Bytes the skip test drives to the end in one step.
const SKIP_TO_END: usize = CURSOR_LEN - SKIP_AFTER_SECOND;

/// Bytes left once the skip test has reached the end.
const SKIP_AT_END_REMAINING: usize = 0;

/// Width of the over-long skip the skip test attempts at the end of the buffer.
const SKIP_PAST_END: usize = WIDTH_U8;

/// Bytes available to that over-long skip.
const SKIP_PAST_END_AVAILABLE: usize = SKIP_AT_END_REMAINING;

/// The position `seek` returns to, which is the only way backwards.
const SEEK_BACK_TO: usize = 0;

/// Bytes the borrow test hands out in one call.
const BORROW_WIDTH: usize = WIDTH_U32;

/// Cursor the borrow test seeks a second reader to, so two readers look at the
/// same four bytes.
const BORROW_SEEK: usize = WIDTH_U8;

/// Bytes that follow the borrowed window in the cursor buffer, once the window
/// itself has been consumed.
const BORROW_TAIL: usize = CURSOR_LEN - BORROW_AFTER;

/// Cursor after the borrowed window has been consumed.
const BORROW_AFTER: usize = BORROW_SEEK + BORROW_WIDTH;

/// The first byte of the four byte window the borrow test hands out, taken
/// straight from the fixture so the borrow is compared against the caller's own
/// bytes and not against a copy of them.
const SYNTHETIC_WINDOW_FIRST_BYTE: u8 = 0x5B;

/// The cursor buffer: eight distinct bytes, so a borrow of the wrong window is
/// visible in the assertion rather than looking right. The borrow window starts
/// at [`BORROW_SEEK`], so its first byte is the second entry here.
const SYNTHETIC_CURSOR_BYTES: [u8; CURSOR_LEN] = [
    0x5A,
    SYNTHETIC_WINDOW_FIRST_BYTE,
    0x5C,
    0x5D,
    0x5E,
    0x5F,
    0x60,
    0x61,
];

// --- error label constants ----------------------------------------------------

/// Cursor the first truncation error test starts from, after a skip.
const ERROR_START_OFFSET: usize = 3;

/// Bytes still left at [`ERROR_START_OFFSET`] in a five byte buffer.
const ERROR_START_AVAILABLE: usize = 2;

/// Length of the buffer the error label tests drive.
const ERROR_BUFFER_LEN: usize = 5;

/// A second start offset, one byte from the end, so the reported offset is shown
/// to move with the cursor rather than being pinned at one value.
const ERROR_LATE_OFFSET: usize = ERROR_BUFFER_LEN - 1;

/// Bytes still left at [`ERROR_LATE_OFFSET`].
const ERROR_LATE_AVAILABLE: usize = 1;

/// Width of the read that fails from [`ERROR_LATE_OFFSET`].
const ERROR_LATE_READ: usize = WIDTH_U16;

/// Bytes a read asks for from [`ERROR_START_OFFSET`], more than the buffer holds
/// from there.
const ERROR_START_NEEDED: usize = WIDTH_U32;

/// The canary the error label test puts in its buffer. No error may quote it,
/// because END-023 forbids a variant from carrying file content.
const SYNTHETIC_CANARY: &[u8] = b"CANARYXX";

/// Length of [`SYNTHETIC_CANARY`]. It is longer than the error fixture on
/// purpose, so a cursor parked near its end still has a whole read left to run
/// out of.
const SYNTHETIC_CANARY_LEN: usize = 8;

/// Cursor the canary read fails from: one byte from the end of
/// [`SYNTHETIC_CANARY`], so a whole `u32` is short.
const CANARY_CURSOR: usize = SYNTHETIC_CANARY_LEN - WIDTH_U8;

/// Bytes still left at [`CANARY_CURSOR`], which is the count the canary's failed
/// `u32` reports as available.
const CANARY_AVAILABLE: usize = SYNTHETIC_CANARY_LEN - CANARY_CURSOR;

/// The cursor a fresh reader reports when a `seek` is refused: nothing has been
/// read yet, so the cursor is still at the start of the buffer.
const SEEK_ERROR_OFFSET: usize = 0;

/// The canary as the text an error message would have to contain to leak it.
const SYNTHETIC_CANARY_TEXT: &str = "CANARYXX";

// --- bit flip constants -------------------------------------------------------

/// Length of the buffer every bit is flipped in: one bit per byte.
const FLIP_LEN: usize = WIDTH_U32;

/// Bits in the flip buffer, all of which are flipped one at a time.
const FLIP_BITS: usize = FLIP_LEN * 8;

/// Bits per byte, so a bit index names its byte as `bit / BITS_PER_BYTE`.
const BITS_PER_BYTE: usize = 8;

/// Width the flip test asks for on a buffer one byte too short, so the error arm
/// of the test is real rather than hypothetical.
const FLIP_TOO_LONG: usize = WIDTH_U32 + 1;

/// Bytes left after the flip test's over-long read is refused.
const FLIP_TOO_LONG_AVAILABLE: usize = FLIP_LEN;

/// Reads that must succeed on every flipped buffer, because a bit flip never
/// changes a length.
const FLIP_OK_PER_BIT: usize = 3;

/// Reads that must fail on every flipped buffer, for the same reason.
const FLIP_ERR_PER_BIT: usize = 1;

/// Successful reads across the whole flip walk.
const FLIP_OK_TOTAL: usize = FLIP_BITS * FLIP_OK_PER_BIT;

/// Refused reads across the whole flip walk.
const FLIP_ERR_TOTAL: usize = FLIP_BITS * FLIP_ERR_PER_BIT;

// --- the anti-deviation fixtures ----------------------------------------------

/// The four bytes the spec measures at the executable's XP table for level 1.
const SYNTHETIC_XP_WORD: [u8; WIDTH_U32] = [0x01, 0x00, 0x00, 0x00];

/// The XP value for level 1, folded least significant byte first (`END-C005`).
const SYNTHETIC_XP_LEVEL_ONE_HOST: u32 = 1;

/// The same four bytes, folded most significant byte first (`END-C004`).
const SYNTHETIC_XP_LEVEL_ONE_ASSET: u32 = 16_777_216;

/// How far apart the two folds of [`SYNTHETIC_XP_WORD`] are. Neither read errors,
/// so the magnitude is the only witness that the two orders really differ
/// (END-008).
const SYNTHETIC_XP_SPREAD: u32 = SYNTHETIC_XP_LEVEL_ONE_ASSET - SYNTHETIC_XP_LEVEL_ONE_HOST;

/// Length of the DAT root header: four big endian u32 words (`END-C015`).
const SYNTHETIC_ROOT_HEADER_LEN: usize = 16;

/// The first sixteen bytes of a DAT archive root header as the spec measures them:
/// four big endian words, being the directory count, a capacity, the size field
/// and a dead pointer.
const SYNTHETIC_ROOT_HEADER: [u8; SYNTHETIC_ROOT_HEADER_LEN] = [
    0x00, 0x00, 0x00, 0x41, 0x00, 0x00, 0x00, 0x5E, 0x00, 0x00, 0x00, 0x00, 0x02, 0x02, 0x15, 0x90,
];

/// The four words [`SYNTHETIC_ROOT_HEADER`] carries, folded big endian.
const SYNTHETIC_ROOT_WORDS: [u32; 4] = [65, 94, 0, 33_691_024];

/// Words in [`SYNTHETIC_ROOT_HEADER`].
const SYNTHETIC_ROOT_WORD_COUNT: usize = SYNTHETIC_ROOT_WORDS.len();

/// The directory count the spec records for the archive, which the big endian
/// fold reproduces and the little endian fold cannot (`END-C006`).
const SYNTHETIC_DIRECTORY_COUNT: u32 = 65;

/// The same first four bytes folded the other way (`END-C007`).
const SYNTHETIC_DIRECTORY_COUNT_WRONG_ORDER: u32 = 1_090_519_040;

/// The root header's size field, which the spec fixes at zero at the root.
const SYNTHETIC_ROOT_SIZE_FIELD: u32 = 0;

// --- helpers ------------------------------------------------------------------

/// Which layer a reader belongs to, read off its own type.
///
/// END-013: the order is a property of the type the caller already chose, so the
/// layer is reachable from a `BigEndian` value and from a `LittleEndian` value
/// and there is no third answer. No implementation sits behind this trait; it
/// exists so that the type name is the only input that decides the answer.
trait ReaderLayer {
    /// The layer this reader type belongs to, in the spec's own words.
    const LAYER: &'static str;
}

/// The layer of a reader, named without its concrete type in the signature.
fn reader_layer<T: ReaderLayer>() -> &'static str {
    T::LAYER
}

/// END-023: an error that carries no borrow, no `String` and no owned bytes is
/// `'static` and `Copy`. A type holding a `&'a [u8]` would fail the first bound
/// and a type that merely cloned its payload would fail the second.
fn reader_error_is_static_and_copy<T: 'static + Copy>() {}

/// The spec's constraint that both error types implement `Display` and
/// `std::error::Error`, so a caller can propagate one without unwrapping.
fn reader_error_is_display<T: std::fmt::Display + std::error::Error>() {}

/// Name of an asset-layer variant, matched exhaustively with no wildcard arm, so
/// a third variant is a compile error here rather than a value that slips past a
/// name check (the closed set of `END-C012`).
fn big_endian_error_name(error: &BigEndianError) -> &'static str {
    match error {
        BigEndianError::Truncated { .. } => "Truncated",
        BigEndianError::SeekOutOfRange { .. } => "SeekOutOfRange",
    }
}

/// Name of an executable-layer variant, exhaustively (the closed set of
/// `END-C012`).
fn little_endian_error_name(error: &LittleEndianError) -> &'static str {
    match error {
        LittleEndianError::Truncated { .. } => "Truncated",
        LittleEndianError::SeekOutOfRange { .. } => "SeekOutOfRange",
    }
}

/// The index an asset-layer variant has in its error type: one for `Truncated`
/// and two for `SeekOutOfRange`.
///
/// The match is exhaustive with no wildcard arm, so a third variant has no arm
/// and breaks the build, and each arm carries its own variant's index. The
/// highest index the type can produce is therefore the number of variants the
/// type has, so the ledger's `END-C012` count is compared against the crate's
/// own error type and not against a list this file made up.
fn big_endian_variant_index(error: &BigEndianError) -> usize {
    match error {
        BigEndianError::Truncated { .. } => 1,
        BigEndianError::SeekOutOfRange { .. } => 2,
    }
}

/// The index an executable-layer variant has in its error type (the closed set
/// of `END-C012`).
fn little_endian_variant_index(error: &LittleEndianError) -> usize {
    match error {
        LittleEndianError::Truncated { .. } => 1,
        LittleEndianError::SeekOutOfRange { .. } => 2,
    }
}

/// The five fields of an asset-layer `Truncated`, as the spec fixes them, or a
/// panic naming the case when the reader answered `Ok` or reported the other
/// variant. `offset` is where the failed operation started, `field` is the static
/// label of the operation, `origin` is the label handed to `new` (END-022), and
/// `needed` and `available` are the two counts the bounds check compared.
fn big_endian_truncated(
    result: Result<(), BigEndianError>,
    case: &str,
) -> (usize, &'static str, &'static str, usize, usize) {
    match result {
        Ok(()) => panic!("{case}: the read succeeded where a truncation was required"),
        Err(error) => match error {
            BigEndianError::Truncated {
                offset,
                field,
                origin,
                needed,
                available,
            } => (offset, field, origin, needed, available),
            BigEndianError::SeekOutOfRange {
                offset,
                field,
                origin,
                requested,
                length,
            } => {
                panic!(
                    "{case}: expected Truncated, got SeekOutOfRange at offset {offset} \
                     field {field} origin {origin} requested {requested} length {length}"
                )
            }
        },
    }
}

/// The five fields of an executable-layer `Truncated` (END-022).
fn little_endian_truncated(
    result: Result<(), LittleEndianError>,
    case: &str,
) -> (usize, &'static str, &'static str, usize, usize) {
    match result {
        Ok(()) => panic!("{case}: the read succeeded where a truncation was required"),
        Err(error) => match error {
            LittleEndianError::Truncated {
                offset,
                field,
                origin,
                needed,
                available,
            } => (offset, field, origin, needed, available),
            LittleEndianError::SeekOutOfRange {
                offset,
                field,
                origin,
                requested,
                length,
            } => {
                panic!(
                    "{case}: expected Truncated, got SeekOutOfRange at offset {offset} \
                     field {field} origin {origin} requested {requested} length {length}"
                )
            }
        },
    }
}

/// The four fields of an asset-layer `SeekOutOfRange` (END-022). `offset` is the
/// cursor before the rejected `seek`, `requested` is the position asked for and
/// `length` is the buffer's whole extent.
fn big_endian_seek_out_of_range(
    result: Result<(), BigEndianError>,
    case: &str,
) -> (usize, &'static str, &'static str, usize, usize) {
    match result {
        Ok(()) => panic!("{case}: the seek succeeded past the end of the buffer"),
        Err(error) => match error {
            BigEndianError::SeekOutOfRange {
                offset,
                field,
                origin,
                requested,
                length,
            } => (offset, field, origin, requested, length),
            BigEndianError::Truncated {
                offset,
                field,
                origin,
                needed,
                available,
            } => {
                panic!(
                    "{case}: expected SeekOutOfRange, got Truncated at offset {offset} \
                     field {field} origin {origin} needed {needed} available {available}"
                )
            }
        },
    }
}

/// The four fields of an executable-layer `SeekOutOfRange` (END-022).
fn little_endian_seek_out_of_range(
    result: Result<(), LittleEndianError>,
    case: &str,
) -> (usize, &'static str, &'static str, usize, usize) {
    match result {
        Ok(()) => panic!("{case}: the seek succeeded past the end of the buffer"),
        Err(error) => match error {
            LittleEndianError::SeekOutOfRange {
                offset,
                field,
                origin,
                requested,
                length,
            } => (offset, field, origin, requested, length),
            LittleEndianError::Truncated {
                offset,
                field,
                origin,
                needed,
                available,
            } => {
                panic!(
                    "{case}: expected SeekOutOfRange, got Truncated at offset {offset} \
                     field {field} origin {origin} needed {needed} available {available}"
                )
            }
        },
    }
}

/// Runs one asset-layer read and reports only whether it failed, so one table can
/// drive every method through one set of assertions. The value is dropped on
/// purpose: the totality tests are about the error, not the number.
fn asset_read(
    reader: &mut BigEndian<'_>,
    method: ReadMethod,
    width: usize,
) -> Result<(), BigEndianError> {
    match method {
        ReadMethod::U8 => reader.u8().map(|_| ()),
        ReadMethod::U16 => reader.u16().map(|_| ()),
        ReadMethod::U32 => reader.u32().map(|_| ()),
        ReadMethod::I16 => reader.i16().map(|_| ()),
        ReadMethod::I32 => reader.i32().map(|_| ()),
        ReadMethod::Bytes => reader.bytes(width).map(|_| ()),
        ReadMethod::Skip => reader.skip(width),
    }
}

/// The executable-layer twin of [`asset_read`], written out again on purpose.
/// END-011 forbids a helper common to both crates, and a test that reached for
/// one would quietly re-create the thing the claim rules out.
fn host_read(
    reader: &mut LittleEndian<'_>,
    method: ReadMethod,
    width: usize,
) -> Result<(), LittleEndianError> {
    match method {
        ReadMethod::U8 => reader.u8().map(|_| ()),
        ReadMethod::U16 => reader.u16().map(|_| ()),
        ReadMethod::U32 => reader.u32().map(|_| ()),
        ReadMethod::I16 => reader.i16().map(|_| ()),
        ReadMethod::I32 => reader.i32().map(|_| ()),
        ReadMethod::Bytes => reader.bytes(width).map(|_| ()),
        ReadMethod::Skip => reader.skip(width),
    }
}

/// A buffer of exactly `length` bytes whose i-th byte is `i`, so a reader that
/// answers with the wrong window names the byte in the failure message. Every
/// fixture in this file is synthetic and is built here or as a literal above it.
fn synthetic_filler(length: usize) -> Vec<u8> {
    (0..length)
        .map(|index| u8::try_from(index).expect("a fixture length stays inside one byte"))
        .collect()
}

/// Reads the four bytes at the cursor through the asset-layer reader and returns
/// the offset it started at.
///
/// The parameter type accepts `BigEndian` and nothing else, so no value of any
/// other type can enter this function (END-012). [`host_layer_word`] has the same
/// property with its own type, and because the two signatures share no type at
/// all, no value can be handed from one to the other.
fn asset_layer_word(reader: &mut BigEndian<'_>) -> Result<usize, BigEndianError> {
    let start = reader.offset();
    let _word = reader.u32()?;
    Ok(start)
}

/// The executable-layer twin of [`asset_layer_word`], carrying its own error type
/// so an asset-layer failure cannot cross into the executable layer either
/// (END-024).
fn host_layer_word(reader: &mut LittleEndian<'_>) -> Result<usize, LittleEndianError> {
    let start = reader.offset();
    let _word = reader.u32()?;
    Ok(start)
}

/// Compile-time assertion that the two reader types are two types.
///
/// Claims END-012 and END-013. The two readers are not generic over an order and
/// there is no conversion, cast or transcode between them, so nothing in Rust can
/// turn a `BigEndian` into a `LittleEndian` or the reverse: a caller must already
/// have named the right crate. This block is the strongest statement a caller can
/// make about that from outside, and the compiler is what checks it.
///
/// The witness is the pair of impls below. They are ordinary impls of one trait,
/// and coherence only accepts them while the two types differ. If either name ever
/// became an alias, a re-export or a `type` line over the other, the second impl
/// would collide with the first and this block would stop compiling with E0119.
/// The same collision fires if both types were folded onto one generic reader,
/// which is the shape END-012 forbids, so the block is also the standing proof
/// that END-011's deliberate duplication has not been quietly undone.
const _: () = {
    impl<'buffer> ReaderLayer for BigEndian<'buffer> {
        const LAYER: &'static str = ASSET_LAYER_LABEL;
    }

    impl<'buffer> ReaderLayer for LittleEndian<'buffer> {
        const LAYER: &'static str = HOST_LAYER_LABEL;
    }
};

// --- tests --------------------------------------------------------------------

/// claim: END-015
/// oracle: class 5 (hand computed vectors from the spec: `12 34 56 78` folded most significant byte first)
#[test]
fn synthetic_asset_layer_folds_the_most_significant_byte_first() {
    let mut word = BigEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_ASSET_ORIGIN);
    let first_u32 = word.u32().expect("the asset layer reads four bytes");
    assert_eq!(
        first_u32, SYNTHETIC_U32_ASSET,
        "the asset layer folds 12 34 56 78 as 0x12345678"
    );
    assert_eq!(
        first_u32, 305_419_896,
        "the same value written out, so a wrong literal in the constant fails"
    );
    assert_eq!(word.offset(), WIDTH_U32, "one u32 consumed four bytes");

    let mut pair = BigEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        pair.u16().expect("the asset layer reads two bytes"),
        SYNTHETIC_U16_ASSET,
        "the asset layer folds 12 34 as 0x1234"
    );
    assert_eq!(pair.offset(), WIDTH_U16, "one u16 consumed two bytes");

    let mut single = BigEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        single.u8().expect("the asset layer reads one byte"),
        SYNTHETIC_U8_BOTH,
        "one byte folds the same way in both layers"
    );

    // The two folds of one buffer are different numbers. A reader that returned
    // the same value either way would pass the assertions above and fail here,
    // which is the whole reason the two layers are separate types.
    assert_ne!(
        SYNTHETIC_U32_ASSET, SYNTHETIC_U32_HOST,
        "the two orders must not fold one buffer to the same value"
    );
}

/// claim: END-015
/// oracle: class 5 (hand computed vectors from the spec: `12 34 56 78` folded least significant byte first)
#[test]
fn synthetic_executable_layer_folds_the_least_significant_byte_first() {
    let mut word = LittleEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_HOST_ORIGIN);
    let first_u32 = word.u32().expect("the executable layer reads four bytes");
    assert_eq!(
        first_u32, SYNTHETIC_U32_HOST,
        "the executable layer folds 12 34 56 78 as 0x78563412"
    );
    assert_eq!(
        first_u32, 2_018_915_346,
        "the same value written out, so a wrong literal in the constant fails"
    );
    assert_eq!(word.offset(), WIDTH_U32, "one u32 consumed four bytes");

    let mut pair = LittleEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        pair.u16().expect("the executable layer reads two bytes"),
        SYNTHETIC_U16_HOST,
        "the executable layer folds 12 34 as 0x3412"
    );
    assert_eq!(pair.offset(), WIDTH_U16, "one u16 consumed two bytes");

    let mut single = LittleEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        single.u8().expect("the executable layer reads one byte"),
        SYNTHETIC_U8_BOTH,
        "one byte folds the same way in both layers"
    );
}

/// claim: END-014
/// oracle: class 5 (hand computed vectors from the spec: `u8` consumes one byte, `u16` and `i16` two, `u32` and `i32` four)
#[test]
fn synthetic_each_read_consumes_exactly_its_own_width() {
    assert_eq!(
        MIXED_AFTER_I32_LEFT, 3,
        "three bytes survive the integer reads"
    );

    let mut reader = BigEndian::new(&SYNTHETIC_MIXED_BYTES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(reader.offset(), 0, "a fresh reader starts at zero");
    assert_eq!(reader.remaining(), MIXED_LEN, "and holds the whole buffer");

    assert_eq!(
        reader.u8().expect("u8 reads"),
        SYNTHETIC_MIXED_FIRST_BYTE,
        "u8 value"
    );
    assert_eq!(reader.offset(), MIXED_AFTER_U8, "u8 advances one byte");

    let _ = reader.u16().expect("u16 reads");
    assert_eq!(reader.offset(), MIXED_AFTER_U16, "u16 advances two bytes");

    let _ = reader.u32().expect("u32 reads");
    assert_eq!(reader.offset(), MIXED_AFTER_U32, "u32 advances four bytes");

    let _ = reader.i16().expect("i16 reads");
    assert_eq!(
        reader.offset(),
        MIXED_AFTER_I16,
        "i16 advances two bytes, the same width as u16"
    );

    let _ = reader.i32().expect("i32 reads");
    assert_eq!(
        reader.offset(),
        MIXED_AFTER_I32,
        "i32 advances four bytes, the same width as u32"
    );
    assert_eq!(
        reader.remaining(),
        MIXED_AFTER_I32_LEFT,
        "remaining is the length minus the cursor"
    );

    let tail = reader
        .bytes(MIXED_AFTER_I32_LEFT)
        .expect("the trailing bytes read");
    assert_eq!(tail.len(), MIXED_AFTER_I32_LEFT, "the tail is three bytes");
    assert_eq!(reader.offset(), MIXED_LEN, "and the cursor is at the end");
    assert_eq!(reader.remaining(), 0, "with nothing left");

    // The executable layer advances by the same widths. The two crates are
    // written out twice (END-011), so the widths are pinned separately rather
    // than assumed to follow from the asset layer.
    let mut host = LittleEndian::new(&SYNTHETIC_MIXED_BYTES, SYNTHETIC_HOST_ORIGIN);
    let _ = host.u8().expect("host u8 reads");
    assert_eq!(host.offset(), MIXED_AFTER_U8, "host u8 advances one byte");
    let _ = host.u16().expect("host u16 reads");
    assert_eq!(
        host.offset(),
        MIXED_AFTER_U16,
        "host u16 advances two bytes"
    );
    let _ = host.u32().expect("host u32 reads");
    assert_eq!(
        host.offset(),
        MIXED_AFTER_U32,
        "host u32 advances four bytes"
    );
    let _ = host.i16().expect("host i16 reads");
    assert_eq!(
        host.offset(),
        MIXED_AFTER_I16,
        "host i16 advances two bytes"
    );
    let _ = host.i32().expect("host i32 reads");
    assert_eq!(
        host.offset(),
        MIXED_AFTER_I32,
        "host i32 advances four bytes"
    );
}

/// claim: END-016
/// oracle: class 5 (hand computed vectors from the spec: signed reads reinterpret the folded bits rather than convert them)
#[test]
fn synthetic_signed_reads_reinterpret_the_folded_bits_rather_than_convert() {
    // Two all-ones bytes are minus one, not 65,535.
    let mut short_asset = BigEndian::new(&SYNTHETIC_TWO_ALL_ONES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        short_asset.i16().expect("the asset layer reads an i16"),
        SYNTHETIC_TWO_ALL_ONES_SIGNED,
        "FF FF is minus one as an i16"
    );

    let mut short_host = LittleEndian::new(&SYNTHETIC_TWO_ALL_ONES, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        short_host.i16().expect("the executable layer reads an i16"),
        SYNTHETIC_TWO_ALL_ONES_SIGNED,
        "FF FF is minus one as an i16 in the executable layer too"
    );

    // Four all-ones bytes are minus one, not 4,294,967,295. This is END-016's
    // own example, and it is the exact trap a numeric conversion would fall
    // into, so the unsigned read of the same four bytes is asserted beside it.
    // Two cursors, because one cursor cannot read the same four bytes twice.
    let mut signed_asset = BigEndian::new(&SYNTHETIC_ALL_ONES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        signed_asset.i32().expect("the asset layer reads an i32"),
        SYNTHETIC_ALL_ONES_SIGNED,
        "four all-ones bytes are minus one as an i32"
    );
    let mut unsigned_asset = BigEndian::new(&SYNTHETIC_ALL_ONES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        unsigned_asset.u32().expect("the asset layer reads a u32"),
        SYNTHETIC_ALL_ONES_UNSIGNED,
        "the very same four bytes are 4294967295 as a u32"
    );

    let mut word_host = LittleEndian::new(&SYNTHETIC_ALL_ONES, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        word_host.i32().expect("the executable layer reads an i32"),
        SYNTHETIC_ALL_ONES_SIGNED,
        "the executable layer agrees that four all-ones bytes are minus one"
    );

    // The most negative value is order specific, which pins the fold and the sign
    // rule together: the same two bytes mean two different numbers.
    let mut min_asset = BigEndian::new(&SYNTHETIC_I16_MIN_ASSET_BYTES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        min_asset.i16().expect("the asset layer reads the minimum"),
        SYNTHETIC_I16_MIN,
        "80 00 is minus 32768 when the most significant byte comes first"
    );

    let mut min_asset_other = BigEndian::new(&SYNTHETIC_I16_MIN_HOST_BYTES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        min_asset_other.i16().expect("the asset layer reads 00 80"),
        SYNTHETIC_I16_MIN_AS_OTHER_ORDER,
        "00 80 is 128 in the asset layer, because its high bit is a low bit"
    );

    let mut min_host = LittleEndian::new(&SYNTHETIC_I16_MIN_HOST_BYTES, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        min_host
            .i16()
            .expect("the executable layer reads the minimum"),
        SYNTHETIC_I16_MIN,
        "00 80 is minus 32768 when the least significant byte comes first"
    );

    let mut min_host_other =
        LittleEndian::new(&SYNTHETIC_I16_MIN_ASSET_BYTES, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        min_host_other
            .i16()
            .expect("the executable layer reads 80 00"),
        SYNTHETIC_I16_MIN_AS_OTHER_ORDER,
        "80 00 is 128 in the executable layer, because its sign bit is the low one"
    );

    // And the same at four bytes, in all four combinations, so the fold and the
    // sign rule are pinned together: the asset layer reads the minimum out of
    // `80 00 00 00` and 128 out of `00 00 00 80`, and the executable layer reads
    // them the other way round.
    let mut wide_asset = BigEndian::new(&SYNTHETIC_I32_MIN_ASSET_BYTES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        wide_asset.i32().expect("the asset layer reads 80 00 00 00"),
        SYNTHETIC_I32_MIN,
        "80 00 00 00 is the minimum when the most significant byte comes first"
    );

    let mut wide_asset_other =
        BigEndian::new(&SYNTHETIC_I32_MIN_HOST_BYTES, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        wide_asset_other
            .i32()
            .expect("the asset layer reads 00 00 00 80"),
        SYNTHETIC_I32_MIN_AS_OTHER_ORDER,
        "00 00 00 80 is 128 in the asset layer, because its high bit is a low bit"
    );

    let mut wide_host = LittleEndian::new(&SYNTHETIC_I32_MIN_HOST_BYTES, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        wide_host
            .i32()
            .expect("the executable layer reads 00 00 00 80"),
        SYNTHETIC_I32_MIN,
        "00 00 00 80 is the minimum when the least significant byte comes last"
    );

    let mut wide_host_other =
        LittleEndian::new(&SYNTHETIC_I32_MIN_ASSET_BYTES, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        wide_host_other
            .i32()
            .expect("the executable layer reads 80 00 00 00"),
        SYNTHETIC_I32_MIN_AS_OTHER_ORDER,
        "80 00 00 00 is 128 in the executable layer, because its sign bit is the low one"
    );
}

/// claim: END-014
/// oracle: class 6 (algebraic law: remaining falls by exactly the width of each read and never rises)
#[test]
fn synthetic_remaining_falls_by_the_width_of_every_mixed_read() {
    let mut reader = BigEndian::new(&SYNTHETIC_MIXED_BYTES, SYNTHETIC_ASSET_ORIGIN);
    let mut observed: Vec<usize> = vec![reader.remaining()];

    let _ = reader.u8().expect("mixed walk u8");
    observed.push(reader.remaining());
    let _ = reader.u16().expect("mixed walk u16");
    observed.push(reader.remaining());
    let _ = reader.u32().expect("mixed walk u32");
    observed.push(reader.remaining());
    let _ = reader.i16().expect("mixed walk i16");
    observed.push(reader.remaining());
    let _ = reader.i32().expect("mixed walk i32");
    observed.push(reader.remaining());
    let tail = reader
        .bytes(MIXED_TRAILING_WIDTH)
        .expect("mixed walk trailing bytes");
    observed.push(reader.remaining());

    assert_eq!(
        observed.len(),
        MIXED_STEPS + 1,
        "one sample per read, plus the start"
    );
    assert_eq!(
        observed.as_slice(),
        MIXED_REMAINING.as_slice(),
        "remaining falls by exactly the width of each read"
    );

    // The sequence is strictly falling, which is what monotonic means here. Every
    // width above is positive, so every adjacent pair falls.
    let descents = observed.windows(2).filter(|pair| pair[1] < pair[0]).count();
    assert_eq!(
        descents, MIXED_DESCENTS,
        "every adjacent pair of the walk falls, so remaining is monotone"
    );
    assert_eq!(
        observed.first().copied(),
        Some(MIXED_LEN),
        "the walk starts with the whole buffer"
    );
    assert_eq!(
        observed.last().copied(),
        Some(0),
        "and ends with nothing left"
    );
    assert_eq!(
        tail.len(),
        MIXED_TRAILING_WIDTH,
        "the trailing read took exactly the bytes that were left"
    );

    // The executable layer walks the same way over the same buffer.
    let mut host = LittleEndian::new(&SYNTHETIC_MIXED_BYTES, SYNTHETIC_HOST_ORIGIN);
    let mut host_observed: Vec<usize> = vec![host.remaining()];
    let _ = host.u8().expect("host mixed walk u8");
    host_observed.push(host.remaining());
    let _ = host.u16().expect("host mixed walk u16");
    host_observed.push(host.remaining());
    let _ = host.u32().expect("host mixed walk u32");
    host_observed.push(host.remaining());
    let _ = host.i16().expect("host mixed walk i16");
    host_observed.push(host.remaining());
    let _ = host.i32().expect("host mixed walk i32");
    host_observed.push(host.remaining());
    let _ = host
        .bytes(MIXED_TRAILING_WIDTH)
        .expect("host mixed walk bytes");
    host_observed.push(host.remaining());

    assert_eq!(
        host_observed.as_slice(),
        MIXED_REMAINING.as_slice(),
        "the executable layer consumes the same widths as the asset layer"
    );
}

/// claim: END-017, END-018
/// oracle: class 5 (hand computed vectors from the spec: every read against an empty buffer is an error, never a short answer)
#[test]
fn synthetic_every_read_method_refuses_an_empty_buffer() {
    let buffer = synthetic_filler(EMPTY_BUFFER_LEN);
    assert_eq!(buffer.len(), EMPTY_BUFFER_LEN, "the empty fixture is empty");

    let mut refused = 0usize;
    for case in SHORT_CASES.iter().chain(EMPTY_CASES.iter()) {
        let mut reader = BigEndian::new(&buffer, SYNTHETIC_ASSET_ORIGIN);
        let case_name = case.name;
        let (offset, field, origin, needed, available) =
            big_endian_truncated(asset_read(&mut reader, case.method, case.width), case_name);
        assert_eq!(offset, SHORT_CASE_OFFSET, "{case_name}: the failure offset");
        assert_eq!(field, case.field, "{case_name}: the field label");
        assert_eq!(
            origin, SYNTHETIC_ASSET_ORIGIN,
            "{case_name}: the origin label"
        );
        assert_eq!(needed, case.width, "{case_name}: the width asked for");
        assert_eq!(available, EMPTY_BUFFER_LEN, "{case_name}: bytes left");
        refused += 1;
    }
    assert_eq!(
        refused, EMPTY_DRIVEN_CASE_COUNT,
        "every read method refused the empty buffer"
    );

    let mut host_refused = 0usize;
    for case in SHORT_CASES.iter().chain(EMPTY_CASES.iter()) {
        let mut reader = LittleEndian::new(&buffer, SYNTHETIC_HOST_ORIGIN);
        let case_name = case.name;
        let (offset, field, origin, needed, available) =
            little_endian_truncated(host_read(&mut reader, case.method, case.width), case_name);
        assert_eq!(
            offset, SHORT_CASE_OFFSET,
            "{case_name}: the host failure offset"
        );
        assert_eq!(field, case.field, "{case_name}: the host field label");
        assert_eq!(
            origin, SYNTHETIC_HOST_ORIGIN,
            "{case_name}: the host origin label"
        );
        assert_eq!(needed, case.width, "{case_name}: the host width asked for");
        assert_eq!(available, EMPTY_BUFFER_LEN, "{case_name}: host bytes left");
        host_refused += 1;
    }
    assert_eq!(
        host_refused, EMPTY_DRIVEN_CASE_COUNT,
        "every read method refused the empty buffer in the executable layer too"
    );

    // The variant is matched, not merely counted, and the closed set of
    // `END-C012` has exactly two variants a piece.
    let mut reader = BigEndian::new(&buffer, SYNTHETIC_ASSET_ORIGIN);
    let error = match asset_read(&mut reader, ReadMethod::U8, WIDTH_U8) {
        Ok(()) => panic!("an empty buffer must not yield a byte"),
        Err(error) => error,
    };
    assert_eq!(
        big_endian_error_name(&error),
        "Truncated",
        "an empty buffer is a truncation, not a seek"
    );
}

/// claim: END-017, END-018
/// oracle: class 5 (hand computed vectors from the spec: a read one byte past the end is an error and never a short read)
#[test]
fn synthetic_every_read_method_refuses_a_buffer_one_byte_short() {
    let mut checked = 0usize;
    for case in SHORT_CASES.iter() {
        let buffer = synthetic_filler(case.width - SHORT_DEFICIT);
        let available = buffer.len();
        let case_name = case.name;

        let mut asset = BigEndian::new(&buffer, SYNTHETIC_ASSET_ORIGIN);
        let (offset, field, origin, needed, reported) =
            big_endian_truncated(asset_read(&mut asset, case.method, case.width), case_name);
        assert_eq!(
            offset, SHORT_CASE_OFFSET,
            "{case_name}: the failure offset on a short buffer"
        );
        assert_eq!(field, case.field, "{case_name}: the field label");
        assert_eq!(
            origin, SYNTHETIC_ASSET_ORIGIN,
            "{case_name}: the origin label"
        );
        assert_eq!(needed, case.width, "{case_name}: the width asked for");
        assert_eq!(
            reported, available,
            "{case_name}: the reader reports exactly the bytes that were left"
        );

        let mut host = LittleEndian::new(&buffer, SYNTHETIC_HOST_ORIGIN);
        let (host_offset, host_field, host_origin, host_needed, host_reported) =
            little_endian_truncated(host_read(&mut host, case.method, case.width), case_name);
        assert_eq!(
            host_offset, SHORT_CASE_OFFSET,
            "{case_name}: the host failure offset"
        );
        assert_eq!(host_field, case.field, "{case_name}: the host field label");
        assert_eq!(
            host_origin, SYNTHETIC_HOST_ORIGIN,
            "{case_name}: the host origin label"
        );
        assert_eq!(
            host_needed, case.width,
            "{case_name}: the host width asked for"
        );
        assert_eq!(
            host_reported, available,
            "{case_name}: the host reports exactly the bytes that were left"
        );

        checked += 1;
    }
    assert_eq!(
        checked, SHORT_CASE_COUNT,
        "every read method was tried one byte short of its own width"
    );

    // The deficit is exactly one byte in every case, so nothing here is passing
    // because the buffer was empty by accident.
    for case in SHORT_CASES.iter() {
        let buffer = synthetic_filler(case.width - SHORT_DEFICIT);
        assert_eq!(
            case.width - buffer.len(),
            SHORT_DEFICIT,
            "{} sits exactly one byte short",
            case.name
        );
    }
}

/// claim: END-018
/// oracle: class 6 (algebraic law: offset plus remaining is invariant, so a failed read is inert)
#[test]
fn synthetic_a_failed_read_leaves_the_cursor_exactly_where_it_was() {
    let mut checked = 0usize;
    for case in SHORT_CASES.iter() {
        let buffer = synthetic_filler(case.width - SHORT_DEFICIT);
        let case_name = case.name;

        let mut asset = BigEndian::new(&buffer, SYNTHETIC_ASSET_ORIGIN);
        let before_offset = asset.offset();
        let before_remaining = asset.remaining();
        let _ = big_endian_truncated(asset_read(&mut asset, case.method, case.width), case_name);
        assert_eq!(
            asset.offset(),
            before_offset,
            "{case_name}: a failed read does not move the asset cursor"
        );
        assert_eq!(
            asset.remaining(),
            before_remaining,
            "{case_name}: a failed read consumes nothing"
        );

        let mut host = LittleEndian::new(&buffer, SYNTHETIC_HOST_ORIGIN);
        let host_before_offset = host.offset();
        let host_before_remaining = host.remaining();
        let _ = little_endian_truncated(host_read(&mut host, case.method, case.width), case_name);
        assert_eq!(
            host.offset(),
            host_before_offset,
            "{case_name}: a failed read does not move the host cursor"
        );
        assert_eq!(
            host.remaining(),
            host_before_remaining,
            "{case_name}: a failed host read consumes nothing"
        );

        checked += 2;
    }
    assert_eq!(
        checked, INERT_CASE_COUNT,
        "every failed read was checked in both layers"
    );

    // The same holds part way into a buffer, where a reader that advanced before
    // it checked would leave the cursor somewhere the caller never asked for.
    let mid = synthetic_filler(ERROR_BUFFER_LEN);
    let mut mid_asset = BigEndian::new(&mid, SYNTHETIC_ASSET_ORIGIN);
    mid_asset
        .skip(ERROR_START_OFFSET)
        .expect("the mid skip succeeds");
    let mid_offset = mid_asset.offset();
    let mid_remaining = mid_asset.remaining();
    let _ = big_endian_truncated(
        asset_read(&mut mid_asset, ReadMethod::U32, WIDTH_U32),
        "mid",
    );
    assert_eq!(
        mid_asset.offset(),
        mid_offset,
        "a mid-buffer failure is inert"
    );
    assert_eq!(mid_asset.remaining(), mid_remaining, "and consumes nothing");

    let mut mid_host = LittleEndian::new(&mid, SYNTHETIC_HOST_ORIGIN);
    mid_host
        .skip(ERROR_START_OFFSET)
        .expect("the host mid skip succeeds");
    let host_mid_offset = mid_host.offset();
    let host_mid_remaining = mid_host.remaining();
    let _ = little_endian_truncated(host_read(&mut mid_host, ReadMethod::U32, WIDTH_U32), "mid");
    assert_eq!(
        mid_host.offset(),
        host_mid_offset,
        "the host mid-buffer failure is inert"
    );
    assert_eq!(
        mid_host.remaining(),
        host_mid_remaining,
        "and consumes nothing"
    );
}

/// claim: END-019
/// oracle: class 5 (hand computed vectors from the spec: seek accepts zero through the length and rejects anything larger)
#[test]
fn synthetic_seek_takes_any_position_up_to_the_length_and_no_further() {
    assert_eq!(
        SEEK_ACCEPTED_COUNT,
        CURSOR_LEN + 1,
        "every position, inclusive"
    );

    let mut accepted = 0usize;
    for target in SEEK_ACCEPTED.iter() {
        let mut reader = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
        reader
            .seek(*target)
            .expect("a position inside the buffer is accepted");
        assert_eq!(
            reader.offset(),
            *target,
            "seek lands exactly where it was told"
        );
        assert_eq!(
            reader.remaining(),
            CURSOR_LEN - target,
            "remaining is the length minus the new cursor"
        );
        accepted += 1;
    }
    assert_eq!(
        accepted, SEEK_ACCEPTED_COUNT,
        "every position from zero through the length was accepted"
    );

    // The very end is accepted, and leaves nothing rather than erroring.
    let mut at_end = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
    at_end
        .seek(CURSOR_LEN)
        .expect("the buffer length itself is accepted");
    assert_eq!(
        at_end.remaining(),
        SEEK_AT_END_REMAINING,
        "nothing is left at the end"
    );
    let (offset, field, origin, needed, available) = big_endian_truncated(
        asset_read(&mut at_end, ReadMethod::U8, SEEK_AT_END_READ),
        "read at the end",
    );
    assert_eq!(
        offset, CURSOR_LEN,
        "a read at the end reports the end as its offset"
    );
    assert_eq!(field, FIELD_U8, "and names the method it tried");
    assert_eq!(origin, SYNTHETIC_ASSET_ORIGIN, "and carries the origin");
    assert_eq!(needed, SEEK_AT_END_READ, "and asked for a whole byte");
    assert_eq!(available, SEEK_AT_END_REMAINING, "with none left to give");

    // Anything larger is rejected, by a different variant, and nothing moves.
    let mut rejected = 0usize;
    for target in SEEK_REJECTED_POSITIONS.iter() {
        let mut reader = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
        reader
            .seek(SKIP_AFTER_SECOND)
            .expect("the cursor is moved first");
        let before = reader.offset();
        let case_name = format!("seek to {}", target);
        let (offset, field, origin, requested, length) =
            big_endian_seek_out_of_range(reader.seek(*target), &case_name);
        assert_eq!(
            offset, before,
            "{case_name}: the cursor before the rejected seek"
        );
        assert_eq!(field, FIELD_SEEK, "{case_name}: the field label is seek");
        assert_eq!(
            origin, SYNTHETIC_ASSET_ORIGIN,
            "{case_name}: the origin label"
        );
        assert_eq!(requested, *target, "{case_name}: the position asked for");
        assert_eq!(length, CURSOR_LEN, "{case_name}: the length of the buffer");
        assert_eq!(reader.offset(), before, "{case_name}: and nothing moved");
        rejected += 1;
    }
    assert_eq!(
        rejected, SEEK_REJECTED_COUNT,
        "every position past the end was rejected"
    );

    // The rejection is its own variant, not the truncation a read would report
    // at the end of the very same buffer (END-024: two distinct error types,
    // two variants each).
    let mut reader = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
    reader
        .seek(CURSOR_LEN)
        .expect("the cursor is parked at the end first");
    let seek_error = match reader.seek(SEEK_REJECTED_FIRST) {
        Ok(()) => panic!("a seek past the end must not succeed"),
        Err(error) => error,
    };
    let read_error = match asset_read(&mut reader, ReadMethod::U32, WIDTH_U32) {
        Ok(()) => panic!("the cursor is at the end, so a u32 must not succeed"),
        Err(error) => error,
    };
    assert_eq!(
        big_endian_error_name(&seek_error),
        "SeekOutOfRange",
        "a seek past the end is its own variant"
    );
    assert_ne!(
        big_endian_error_name(&seek_error),
        big_endian_error_name(&read_error),
        "and it is not the variant a short read reports"
    );

    // The executable layer takes the same range and reports the same distinction.
    let mut host = LittleEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_HOST_ORIGIN);
    host.seek(CURSOR_LEN).expect("the host accepts the end");
    assert_eq!(
        host.remaining(),
        SEEK_AT_END_REMAINING,
        "nothing is left at the host end"
    );
    let (host_offset, host_field, host_origin, host_needed, host_available) =
        little_endian_truncated(
            host_read(&mut host, ReadMethod::U8, SEEK_AT_END_READ),
            "host read at the end",
        );
    assert_eq!(
        host_offset, CURSOR_LEN,
        "the host read at the end reports the end"
    );
    assert_eq!(host_field, FIELD_U8, "and names the method it tried");
    assert_eq!(
        host_origin, SYNTHETIC_HOST_ORIGIN,
        "and carries the host origin"
    );
    assert_eq!(host_needed, SEEK_AT_END_READ, "and asked for a whole byte");
    assert_eq!(
        host_available, SEEK_AT_END_REMAINING,
        "with none left to give"
    );

    let mut host_rejected = 0usize;
    for target in SEEK_REJECTED_POSITIONS.iter() {
        let mut reader = LittleEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_HOST_ORIGIN);
        let case_name = format!("host seek to {}", target);
        let (offset, field, origin, requested, length) =
            little_endian_seek_out_of_range(reader.seek(*target), &case_name);
        assert_eq!(
            offset, 0,
            "{case_name}: the cursor before the rejected seek"
        );
        assert_eq!(
            field, FIELD_SEEK,
            "{case_name}: the host field label is seek"
        );
        assert_eq!(
            origin, SYNTHETIC_HOST_ORIGIN,
            "{case_name}: the host origin label"
        );
        assert_eq!(requested, *target, "{case_name}: the position asked for");
        assert_eq!(length, CURSOR_LEN, "{case_name}: the length of the buffer");
        assert_eq!(reader.offset(), 0, "{case_name}: and nothing moved");
        host_rejected += 1;
    }
    assert_eq!(
        host_rejected, SEEK_REJECTED_COUNT,
        "every position past the end was rejected by the executable layer too"
    );
}

/// claim: END-019
/// oracle: class 6 (algebraic law: skip only adds to the cursor, and seek is the only way to take it back)
#[test]
fn synthetic_skip_only_advances_and_seek_is_the_only_way_back() {
    let mut reader = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
    reader.skip(SKIP_STEP).expect("the first skip fits");
    assert_eq!(
        reader.offset(),
        SKIP_AFTER_FIRST,
        "one skip advances the cursor"
    );
    reader.skip(SKIP_STEP).expect("the second skip fits");
    assert_eq!(
        reader.offset(),
        SKIP_AFTER_SECOND,
        "a second skip starts where the first stopped"
    );
    assert_eq!(
        reader.remaining(),
        SKIP_AFTER_SECOND_LEFT,
        "remaining follows the cursor"
    );

    reader
        .skip(SKIP_TO_END)
        .expect("a skip of exactly what is left succeeds");
    assert_eq!(reader.offset(), CURSOR_LEN, "the cursor is at the end");
    assert_eq!(
        reader.remaining(),
        SKIP_AT_END_REMAINING,
        "with nothing left"
    );

    // A skip past the end is an error, and the bounds check is the same one a
    // read uses, so it carries the same variant and the same three labels.
    let (offset, field, origin, needed, available) =
        big_endian_truncated(reader.skip(SKIP_PAST_END).map(|_| ()), "skip past the end");
    assert_eq!(
        offset, CURSOR_LEN,
        "the skip failed at the end of the buffer"
    );
    assert_eq!(field, FIELD_SKIP, "and names itself as skip");
    assert_eq!(origin, SYNTHETIC_ASSET_ORIGIN, "and carries the origin");
    assert_eq!(needed, SKIP_PAST_END, "and asked for a whole byte");
    assert_eq!(available, SKIP_PAST_END_AVAILABLE, "with none left to give");
    assert_eq!(reader.offset(), CURSOR_LEN, "and the cursor did not move");

    // `seek` is the only way backwards. There is no rewind, no relative seek and
    // no negative skip in the interface, so this is the whole of it.
    reader
        .seek(SEEK_BACK_TO)
        .expect("seek takes the cursor back to the start");
    assert_eq!(reader.offset(), SEEK_BACK_TO, "the cursor is at zero");
    assert_eq!(
        reader.remaining(),
        CURSOR_LEN,
        "and the whole buffer is back"
    );

    // The same walk in the executable layer.
    let mut host = LittleEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_HOST_ORIGIN);
    host.skip(SKIP_STEP).expect("the host first skip fits");
    host.skip(SKIP_STEP).expect("the host second skip fits");
    assert_eq!(
        host.offset(),
        SKIP_AFTER_SECOND,
        "the host cursor matches the asset one"
    );
    assert_eq!(
        host.remaining(),
        SKIP_AFTER_SECOND_LEFT,
        "and so does what is left"
    );
    host.skip(SKIP_TO_END)
        .expect("the host skip to the end fits");
    assert_eq!(host.remaining(), SKIP_AT_END_REMAINING, "nothing is left");
    let (host_offset, host_field, host_origin, host_needed, host_available) =
        little_endian_truncated(
            host.skip(SKIP_PAST_END).map(|_| ()),
            "host skip past the end",
        );
    assert_eq!(host_offset, CURSOR_LEN, "the host skip failed at the end");
    assert_eq!(host_field, FIELD_SKIP, "and names itself as skip");
    assert_eq!(
        host_origin, SYNTHETIC_HOST_ORIGIN,
        "and carries the host origin"
    );
    assert_eq!(host_needed, SKIP_PAST_END, "and asked for a whole byte");
    assert_eq!(
        host_available, SKIP_PAST_END_AVAILABLE,
        "with none left to give"
    );
    host.seek(SEEK_BACK_TO)
        .expect("the host seek takes the cursor back");
    assert_eq!(host.remaining(), CURSOR_LEN, "and the whole buffer is back");
}

/// claim: END-020
/// oracle: class 6 (algebraic law: the borrowed window is the caller's own storage, because two cursors on one offset agree)
#[test]
fn synthetic_bytes_hands_back_the_callers_own_storage() {
    let mut reader = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
    reader
        .skip(BORROW_SEEK)
        .expect("the borrow test skips to its window");
    let borrowed = reader
        .bytes(BORROW_WIDTH)
        .expect("four bytes are available to borrow");
    assert_eq!(
        borrowed.len(),
        BORROW_WIDTH,
        "the borrow is exactly n bytes long"
    );
    assert_eq!(
        reader.offset(),
        BORROW_AFTER,
        "and the cursor advanced past it"
    );
    assert_eq!(reader.remaining(), BORROW_TAIL, "leaving the tail behind");

    // The contents are the caller's own bytes, taken straight from the fixture.
    let window = SYNTHETIC_CURSOR_BYTES
        .get(BORROW_SEEK..BORROW_AFTER)
        .expect("the window is inside the fixture");
    assert_eq!(borrowed, window, "the borrow holds the fixture bytes");
    assert_eq!(
        borrowed.first().copied(),
        Some(SYNTHETIC_WINDOW_FIRST_BYTE),
        "and starts where the fixture starts"
    );

    // The strong half of END-020: the borrow is not a copy. A copy would compare
    // equal here, so the assertion compares the addresses themselves.
    assert!(
        std::ptr::eq(borrowed, window),
        "bytes() must hand back the caller's own storage and not a copy of it"
    );

    // A second cursor at the same offset sees the same bytes, which is what a
    // borrow makes true and a copy would only make true by accident.
    let mut second = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
    second
        .seek(BORROW_SEEK)
        .expect("the second cursor seeks to the window");
    let again = second
        .bytes(BORROW_WIDTH)
        .expect("the window is available again");
    assert_eq!(
        again, borrowed,
        "two cursors at one offset see the same bytes"
    );
    assert!(
        std::ptr::eq(again, window),
        "and the second borrow is the caller's storage too"
    );

    // A borrow of zero bytes is legal and borrows nothing, so a caller cannot be
    // handed a non-empty window by asking for none.
    let mut empty = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
    let nothing = empty.bytes(0).expect("a zero byte borrow is legal");
    assert_eq!(nothing.len(), 0, "a zero byte borrow is empty");
    assert_eq!(empty.offset(), 0, "and moves the cursor nowhere");

    // The executable layer borrows from its own caller's buffer too.
    let mut host = LittleEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_HOST_ORIGIN);
    host.skip(BORROW_SEEK)
        .expect("the host skips to its window");
    let host_borrow = host
        .bytes(BORROW_WIDTH)
        .expect("the host borrows four bytes");
    assert_eq!(
        host_borrow, window,
        "the host borrow holds the fixture bytes"
    );
    assert!(
        std::ptr::eq(host_borrow, window),
        "the host borrow is the caller's own storage and not a copy"
    );
}

/// claim: END-010, END-011, END-012, END-013, END-024
/// oracle: class 6 (algebraic law: two layer types, two error types, two names, and a compile-time witness in the file's `const _` block)
#[test]
fn synthetic_no_default_order_and_no_conversion_between_the_layers() {
    // Each function takes one reader type and nothing else, so no value of the
    // other type can be handed to it in any form (END-012). Both are called here,
    // which is what stops the signatures from being decorative.
    let mut asset = BigEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_ASSET_ORIGIN);
    let mut host = LittleEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_HOST_ORIGIN);
    let asset_start = asset_layer_word(&mut asset).expect("the asset layer reads a word");
    let host_start = host_layer_word(&mut host).expect("the executable layer reads a word");
    assert_eq!(asset_start, 0, "the asset layer word starts at the cursor");
    assert_eq!(
        host_start, 0,
        "the executable layer word starts at the cursor"
    );
    assert_eq!(
        asset.offset(),
        WIDTH_U32,
        "the asset layer consumed four bytes"
    );
    assert_eq!(
        host.offset(),
        WIDTH_U32,
        "the executable layer consumed four bytes"
    );

    // The layer is a property of the type, and the type is the only input that
    // decides it (END-013). There is no third answer and no value to pass.
    let asset_layer = reader_layer::<BigEndian<'_>>();
    let host_layer = reader_layer::<LittleEndian<'_>>();
    assert_eq!(
        asset_layer, ASSET_LAYER_LABEL,
        "the asset reader names its layer"
    );
    assert_eq!(
        host_layer, HOST_LAYER_LABEL,
        "the executable reader names its layer"
    );
    assert_ne!(
        asset_layer, host_layer,
        "the two layers are named separately, so no default order is in play"
    );

    // The `const _` block in this file holds the compile-time half of the claim:
    // two trait impls that could not coexist unless the two type names are two
    // types. It is checked when this file compiles, not when this test runs.
    //
    // The two error types are distinct for the same reason, and an asset-layer
    // failure cannot be an executable-layer failure (END-024).
    let asset_error_type = std::any::TypeId::of::<BigEndianError>();
    let host_error_type = std::any::TypeId::of::<LittleEndianError>();
    let asset_reader_type = std::any::TypeId::of::<BigEndian<'static>>();
    let host_reader_type = std::any::TypeId::of::<LittleEndian<'static>>();
    assert_ne!(
        asset_reader_type, host_reader_type,
        "the two reader types are two types, with no alias between them"
    );
    assert_ne!(
        asset_error_type, host_error_type,
        "the two error types are two types, so an asset failure cannot cross layers"
    );
    reader_error_is_static_and_copy::<BigEndianError>();
    reader_error_is_static_and_copy::<LittleEndianError>();
    reader_error_is_display::<BigEndianError>();
    reader_error_is_display::<LittleEndianError>();

    // The two variants of each layer's error type, in the order the spec lists
    // them, named by the exhaustive matches in the helpers this file calls
    // (END-024: the two error types are two types, and each carries its own two
    // variants). The closed set of `END-C012` is proved when the file compiles:
    // a third variant has no arm in either helper and breaks the build, which
    // is the rule `docs/spec/trace.md` records as a claim no test maps. Both
    // readers are pushed past their end so both variants of both types are seen
    // here.
    let mut asset_past_end = BigEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_ASSET_ORIGIN);
    asset_past_end
        .seek(WIDTH_U32)
        .expect("the cursor is parked at the end first");
    let asset_truncation = match asset_past_end.u32() {
        Ok(_) => panic!("a four byte buffer must not yield a u32 at its own end"),
        Err(error) => error,
    };
    let mut asset_seeker = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
    let asset_seek = match asset_seeker.seek(SEEK_REJECTED_FIRST) {
        Ok(()) => panic!("a seek past the end of the buffer must be refused"),
        Err(error) => error,
    };
    let mut host_past_end = LittleEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_HOST_ORIGIN);
    host_past_end
        .seek(WIDTH_U32)
        .expect("the host cursor is parked at the end first");
    let host_truncation = match host_past_end.u32() {
        Ok(_) => panic!("a four byte buffer must not yield a host u32 at its own end"),
        Err(error) => error,
    };
    let mut host_seeker = LittleEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_HOST_ORIGIN);
    let host_seek = match host_seeker.seek(SEEK_REJECTED_FIRST) {
        Ok(()) => panic!("a host seek past the end of the buffer must be refused"),
        Err(error) => error,
    };

    // The names are compared as a set, so a variant that was renamed or a
    // failure that answered with the other variant fails here.
    let asset_names = [
        big_endian_error_name(&asset_truncation),
        big_endian_error_name(&asset_seek),
    ];
    let host_names = [
        little_endian_error_name(&host_truncation),
        little_endian_error_name(&host_seek),
    ];
    assert_eq!(
        asset_names,
        ["Truncated", "SeekOutOfRange"],
        "the asset layer's two refusals are the truncation and the refused seek"
    );
    assert_eq!(
        host_names,
        ["Truncated", "SeekOutOfRange"],
        "the executable layer's two refusals are the truncation and the refused seek"
    );

    // Both layers name the same two variants and no third, and the ledger's
    // count of the variants in a crate's closed set is that number.
    let mut distinct: Vec<&str> = asset_names.to_vec();
    distinct.extend_from_slice(&host_names);
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct,
        ["SeekOutOfRange", "Truncated"],
        "the two layers name the same two variants and no third"
    );
    // The ledger's count is compared against the crates' own error types. The
    // count is the highest index the exhaustive matches above can produce, and
    // a variant that the type gained would have no arm and would break the
    // build, so the number cannot drift away from the code in silence.
    assert_eq!(
        big_endian_variant_index(&asset_seek).max(big_endian_variant_index(&asset_truncation)),
        ERROR_VARIANT_COUNT,
        "the ledger's variant count is the number of variants the asset layer's \
         error type has"
    );
    assert_eq!(
        little_endian_variant_index(&host_seek).max(little_endian_variant_index(&host_truncation)),
        ERROR_VARIANT_COUNT,
        "the ledger's variant count is the number of variants the executable \
         layer's error type has"
    );
}

/// claim: END-018
/// oracle: class 6 (algebraic law: a total reader has no panic path, so every single bit flip answers)
#[test]
fn synthetic_every_bit_flip_of_the_asset_layer_answers() {
    let mut answered = 0usize;
    let mut succeeded = 0usize;
    let mut refused = 0usize;

    for bit in 0..FLIP_BITS {
        let case_name = format!("asset bit {bit}");
        let mut flipped = SYNTHETIC_FOLD_BYTES;
        let byte_index = bit / BITS_PER_BYTE;
        let mask = 1u8 << (bit % BITS_PER_BYTE);
        let target = flipped
            .get_mut(byte_index)
            .expect("the flip index is inside the four byte buffer");
        *target ^= mask;

        // Each read gets its own cursor. Four bytes cannot be read twice from one
        // cursor, and a reader that consumed two bytes before the `u32` would be
        // answering a different question than the one this test asks.
        //
        // Every one of the three reads that fit is asserted to answer `Ok`
        // before it is counted, so the counter counts answers rather than
        // attempts. A reader that refused every flipped buffer would fail here
        // instead of passing on a tally.
        let mut word_reader = BigEndian::new(&flipped, SYNTHETIC_ASSET_ORIGIN);
        assert!(
            word_reader.u16().is_ok(),
            "{case_name}: the u16 read still answers Ok on a four byte buffer"
        );
        succeeded += 1;
        let mut long_reader = BigEndian::new(&flipped, SYNTHETIC_ASSET_ORIGIN);
        assert!(
            long_reader.u32().is_ok(),
            "{case_name}: the u32 read still answers Ok on a four byte buffer"
        );
        succeeded += 1;
        let mut block_reader = BigEndian::new(&flipped, SYNTHETIC_ASSET_ORIGIN);
        assert!(
            block_reader.bytes(WIDTH_U32).is_ok(),
            "{case_name}: the four byte borrow still answers Ok"
        );
        succeeded += 1;
        let mut over_reader = BigEndian::new(&flipped, SYNTHETIC_ASSET_ORIGIN);
        let over_long = over_reader.bytes(FLIP_TOO_LONG);
        let (offset, field, origin, needed, available) =
            big_endian_truncated(over_long.map(|_| ()), &case_name);
        assert_eq!(
            offset, SHORT_CASE_OFFSET,
            "{case_name}: the refusal offset on a fresh cursor"
        );
        assert_eq!(field, FIELD_BYTES, "{case_name}: the refusal names bytes");
        assert_eq!(
            origin, SYNTHETIC_ASSET_ORIGIN,
            "{case_name}: the refusal origin"
        );
        assert_eq!(needed, FLIP_TOO_LONG, "{case_name}: the width asked for");
        assert_eq!(
            available, FLIP_TOO_LONG_AVAILABLE,
            "{case_name}: a bit flip never changes the bytes left"
        );
        refused += 1;
        answered += 1;
    }

    assert_eq!(answered, FLIP_BITS, "every bit of the buffer was flipped");
    assert_eq!(
        succeeded, FLIP_OK_TOTAL,
        "every flipped buffer answered Ok for the three reads that fit"
    );
    assert_eq!(
        refused, FLIP_ERR_TOTAL,
        "every flipped buffer answered Err for the read one byte too long"
    );
}

/// claim: END-018
/// oracle: class 6 (algebraic law: a total reader has no panic path, so every single bit flip answers)
#[test]
fn synthetic_every_bit_flip_of_the_executable_layer_answers() {
    let mut answered = 0usize;
    let mut succeeded = 0usize;
    let mut refused = 0usize;

    for bit in 0..FLIP_BITS {
        let case_name = format!("host bit {bit}");
        let mut flipped = SYNTHETIC_FOLD_BYTES;
        let byte_index = bit / BITS_PER_BYTE;
        let mask = 1u8 << (bit % BITS_PER_BYTE);
        let target = flipped
            .get_mut(byte_index)
            .expect("the flip index is inside the four byte buffer");
        *target ^= mask;

        // Each read gets its own cursor, for the same reason as in the asset layer.
        //
        // The three reads that fit are asserted to answer `Ok` before they are
        // counted, so the counter counts answers rather than attempts.
        let mut word_reader = LittleEndian::new(&flipped, SYNTHETIC_HOST_ORIGIN);
        assert!(
            word_reader.u16().is_ok(),
            "{case_name}: the u16 read still answers Ok on a four byte buffer"
        );
        succeeded += 1;
        let mut long_reader = LittleEndian::new(&flipped, SYNTHETIC_HOST_ORIGIN);
        assert!(
            long_reader.u32().is_ok(),
            "{case_name}: the u32 read still answers Ok on a four byte buffer"
        );
        succeeded += 1;
        let mut block_reader = LittleEndian::new(&flipped, SYNTHETIC_HOST_ORIGIN);
        assert!(
            block_reader.bytes(WIDTH_U32).is_ok(),
            "{case_name}: the four byte borrow still answers Ok"
        );
        succeeded += 1;
        let mut over_reader = LittleEndian::new(&flipped, SYNTHETIC_HOST_ORIGIN);
        let over_long = over_reader.bytes(FLIP_TOO_LONG);
        let (offset, field, origin, needed, available) =
            little_endian_truncated(over_long.map(|_| ()), &case_name);
        assert_eq!(
            offset, SHORT_CASE_OFFSET,
            "{case_name}: the refusal offset on a fresh cursor"
        );
        assert_eq!(field, FIELD_BYTES, "{case_name}: the refusal names bytes");
        assert_eq!(
            origin, SYNTHETIC_HOST_ORIGIN,
            "{case_name}: the refusal origin"
        );
        assert_eq!(needed, FLIP_TOO_LONG, "{case_name}: the width asked for");
        assert_eq!(
            available, FLIP_TOO_LONG_AVAILABLE,
            "{case_name}: a bit flip never changes the bytes left"
        );
        refused += 1;
        answered += 1;
    }

    assert_eq!(answered, FLIP_BITS, "every bit of the buffer was flipped");
    assert_eq!(
        succeeded, FLIP_OK_TOTAL,
        "every flipped buffer answered Ok for the three reads that fit"
    );
    assert_eq!(
        refused, FLIP_ERR_TOTAL,
        "every flipped buffer answered Err for the read one byte too long"
    );
}

/// claim: END-022, END-023
/// oracle: class 5 (hand computed vectors from the spec: a failed read reports where it began, which operation, and whose buffer; the rendered message is the wording docs/spec/endian.md "## Error text" fixes)
#[test]
fn synthetic_every_error_names_its_offset_its_field_its_origin_and_no_content() {
    let buffer = synthetic_filler(ERROR_BUFFER_LEN);
    assert_eq!(buffer.len(), ERROR_BUFFER_LEN, "the error fixture length");

    // The reported offset is where the failed operation began, not where it ended
    // up and not where the buffer began. The cursor has been moved first, so a
    // reader that reported zero would be caught here.
    let mut reader = BigEndian::new(&buffer, SYNTHETIC_ASSET_ORIGIN);
    reader
        .skip(ERROR_START_OFFSET)
        .expect("the first skip fits");
    assert_eq!(reader.offset(), ERROR_START_OFFSET, "the cursor is moved");
    assert_eq!(
        reader.remaining(),
        ERROR_START_AVAILABLE,
        "with only a few bytes left"
    );
    let (offset, field, origin, needed, available) =
        big_endian_truncated(reader.u32().map(|_| ()), "u32 from the middle");
    assert_eq!(
        offset, ERROR_START_OFFSET,
        "the error reports where the read began"
    );
    assert_eq!(field, FIELD_U32, "and names the operation that failed");
    assert_eq!(
        origin, SYNTHETIC_ASSET_ORIGIN,
        "and carries the origin label"
    );
    assert_eq!(needed, ERROR_START_NEEDED, "and the width it wanted");
    assert_eq!(
        available, ERROR_START_AVAILABLE,
        "and the bytes that were left"
    );

    // The same, from a different position, so the offset is shown to move with the
    // cursor rather than being pinned at one value.
    let mut late = BigEndian::new(&buffer, SYNTHETIC_ASSET_ORIGIN);
    late.seek(ERROR_LATE_OFFSET)
        .expect("the late seek is inside the buffer");
    let (late_offset, late_field, late_origin, late_needed, late_available) =
        big_endian_truncated(late.u16().map(|_| ()), "u16 from the last byte");
    assert_eq!(
        late_offset, ERROR_LATE_OFFSET,
        "the reported offset follows the cursor"
    );
    assert_eq!(late_field, FIELD_U16, "the field label follows the method");
    assert_eq!(
        late_origin, SYNTHETIC_ASSET_ORIGIN,
        "and the origin does not move"
    );
    assert_eq!(
        late_needed, ERROR_LATE_READ,
        "with the width that method wants"
    );
    assert_eq!(
        late_available, ERROR_LATE_AVAILABLE,
        "and the bytes that were left"
    );

    // The origin is the label this reader instance was given, not a constant
    // inside the error type. Two instances, two labels, one error type.
    let mut labelled = BigEndian::new(&buffer, SYNTHETIC_ASSET_ALT_ORIGIN);
    labelled
        .seek(ERROR_LATE_OFFSET)
        .expect("the alt label cursor is moved first");
    let (_, _, alt_origin, _, _) = big_endian_truncated(labelled.u32().map(|_| ()), "alt origin");
    assert_eq!(
        alt_origin, SYNTHETIC_ASSET_ALT_ORIGIN,
        "the origin is the label handed to new"
    );
    assert_ne!(
        alt_origin, SYNTHETIC_ASSET_ORIGIN,
        "and two readers over one type report two different origins"
    );

    let mut host = LittleEndian::new(&buffer, SYNTHETIC_HOST_ORIGIN);
    host.seek(ERROR_LATE_OFFSET)
        .expect("the host seek is inside the buffer");
    let (host_offset, host_field, host_origin, host_needed, host_available) =
        little_endian_truncated(host.u16().map(|_| ()), "host u16 from the last byte");
    assert_eq!(
        host_offset, ERROR_LATE_OFFSET,
        "the host reports where its read began"
    );
    assert_eq!(host_field, FIELD_U16, "and names the operation that failed");
    assert_eq!(
        host_origin, SYNTHETIC_HOST_ORIGIN,
        "and carries the host origin label"
    );
    assert_eq!(host_needed, ERROR_LATE_READ, "with the width it wanted");
    assert_eq!(
        host_available, ERROR_LATE_AVAILABLE,
        "and the bytes that were left"
    );

    let mut host_labelled = LittleEndian::new(&buffer, SYNTHETIC_HOST_ALT_ORIGIN);
    host_labelled
        .seek(ERROR_LATE_OFFSET)
        .expect("the host alt label cursor is moved first");
    let (_, _, host_alt_origin, _, _) =
        little_endian_truncated(host_labelled.u32().map(|_| ()), "host alt origin");
    assert_eq!(
        host_alt_origin, SYNTHETIC_HOST_ALT_ORIGIN,
        "the host origin is the label handed to new"
    );

    // END-023: no variant carries content. The buffer here is a word the failure
    // message would have to quote to leak, and it does not. The bound checks in
    // `reader_error_is_static_and_copy` cover the rest: no borrow, no `String`,
    // no owned slice, and `Copy` so nothing is cloned into the error either.
    let canary_buffer = SYNTHETIC_CANARY;
    assert_eq!(
        canary_buffer.len(),
        SYNTHETIC_CANARY_LEN,
        "the canary fills the fixture"
    );
    let mut canary_reader = BigEndian::new(canary_buffer, SYNTHETIC_ASSET_ORIGIN);
    canary_reader
        .seek(CANARY_CURSOR)
        .expect("the canary cursor is inside the buffer");
    let canary_error = match canary_reader.u32() {
        Ok(_) => panic!("an eight byte buffer must not yield a u32 from its last byte"),
        Err(error) => error,
    };
    let rendered = format!("{}", canary_error);
    assert!(
        !rendered.contains(SYNTHETIC_CANARY_TEXT),
        "the error text must not carry file content"
    );
    assert!(
        !rendered.is_empty(),
        "but it is not empty either: it has to say something"
    );

    // END-022: the message carries the offset the operation started at, the
    // label of the operation, the origin the reader was given and the two counts
    // the bounds check compared. `docs/spec/endian.md` "## Error text" fixes
    // the exact wording of that refusal, so the literal below is that wording:
    // a message that printed nothing, dropped a count or permuted the fields
    // fails here, and the numbers in it are the ones the failure reported.
    assert_eq!(
        rendered,
        format!(
            "{SYNTHETIC_ASSET_ORIGIN}: {FIELD_U32} at byte {CANARY_CURSOR} wanted \
             {WIDTH_U32} bytes but only {CANARY_AVAILABLE} remain"
        ),
        "END-022: the asset layer's refusal names where it stopped, what it wanted \
         and what was left"
    );

    // The executable layer's own error, rendered the same way. The two crates
    // are deliberate duplicates (END-011), so a `Display` that printed nothing
    // in one of them is a real defect, and the asset layer's message is not the
    // executable layer's message.
    let mut host_canary_reader = LittleEndian::new(canary_buffer, SYNTHETIC_HOST_ORIGIN);
    host_canary_reader
        .seek(CANARY_CURSOR)
        .expect("the host canary cursor is inside the buffer");
    let host_canary_error = match host_canary_reader.u32() {
        Ok(_) => panic!("an eight byte buffer must not yield a host u32 from its last byte"),
        Err(error) => error,
    };
    let host_rendered = format!("{}", host_canary_error);
    assert!(
        !host_rendered.contains(SYNTHETIC_CANARY_TEXT),
        "the executable layer's error text must not carry file content either"
    );
    assert_eq!(
        host_rendered,
        format!(
            "{SYNTHETIC_HOST_ORIGIN}: {FIELD_U32} at byte {CANARY_CURSOR} wanted \
             {WIDTH_U32} bytes but only {CANARY_AVAILABLE} remain"
        ),
        "END-022: the executable layer's refusal names where it stopped, what it \
         wanted and what was left"
    );

    // The other variant of the closed set, in both layers: a `seek` past the end
    // of the buffer. It carries the cursor it was at, the position it asked for
    // and the length of the buffer it was refused against.
    let mut asset_seeker = BigEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_ASSET_ORIGIN);
    let asset_seek_error = match asset_seeker.seek(SEEK_REJECTED_FIRST) {
        Ok(()) => panic!("a seek past the end of the buffer must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        format!("{}", asset_seek_error),
        format!(
            "{SYNTHETIC_ASSET_ORIGIN}: {FIELD_SEEK} from byte {SEEK_ERROR_OFFSET} asked \
             for {SEEK_REJECTED_FIRST}, past a buffer of {CURSOR_LEN} bytes"
        ),
        "END-022: the asset layer's refused seek names where it was, what it asked \
         for and how long the buffer is"
    );

    let mut host_seeker = LittleEndian::new(&SYNTHETIC_CURSOR_BYTES, SYNTHETIC_HOST_ORIGIN);
    let host_seek_error = match host_seeker.seek(SEEK_REJECTED_FIRST) {
        Ok(()) => panic!("a host seek past the end of the buffer must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        format!("{}", host_seek_error),
        format!(
            "{SYNTHETIC_HOST_ORIGIN}: {FIELD_SEEK} from byte {SEEK_ERROR_OFFSET} asked \
             for {SEEK_REJECTED_FIRST}, past a buffer of {CURSOR_LEN} bytes"
        ),
        "END-022: the executable layer's refused seek names where it was, what it \
         asked for and how long the buffer is"
    );

    reader_error_is_static_and_copy::<BigEndianError>();
    reader_error_is_static_and_copy::<LittleEndianError>();
}

/// claim: END-006, END-008, END-035
/// oracle: class 5 (hand computed vectors from the spec: the same bytes fold to 1 and to 16,777,216, and neither read errors)
#[test]
fn synthetic_the_same_bytes_read_one_and_sixteen_million_by_the_two_layers() {
    // The four bytes at the executable's XP table for level 1. Nothing about the
    // input changes between the two reads below.
    let word = SYNTHETIC_XP_WORD;
    assert_eq!(
        word.len(),
        WIDTH_U32,
        "the anti-deviation fixture is one u32"
    );

    let mut host = LittleEndian::new(&word, SYNTHETIC_HOST_ORIGIN);
    let host_value = host.u32().expect("the executable layer reads the word");
    assert_eq!(
        host_value, SYNTHETIC_XP_LEVEL_ONE_HOST,
        "folded least significant byte first the four bytes are 1"
    );
    assert_eq!(host_value, 1, "the same value written out for the ledger");
    assert_eq!(host.offset(), WIDTH_U32, "and it consumed the whole word");

    let mut asset = BigEndian::new(&word, SYNTHETIC_ASSET_ORIGIN);
    let asset_value = asset.u32().expect("the asset layer reads the same word");
    assert_eq!(
        asset_value, SYNTHETIC_XP_LEVEL_ONE_ASSET,
        "folded most significant byte first the very same four bytes are 16,777,216"
    );
    assert_eq!(
        asset_value, 16_777_216,
        "the same value written out for the ledger"
    );
    assert_eq!(
        asset.offset(),
        WIDTH_U32,
        "and it consumed the same four bytes"
    );

    // END-008: neither read raised an error, so nothing downstream can notice. The
    // magnitude is the only witness, and it is enormous.
    assert_eq!(
        asset_value - host_value,
        SYNTHETIC_XP_SPREAD,
        "the wrong order returns a plausible number with no error raised"
    );
    assert_eq!(
        SYNTHETIC_XP_SPREAD, 16_777_215,
        "the spread is written out so a wrong constant fails"
    );

    // END-035 is the same argument pointing the other way: the archive root
    // header folds to a directory count that matches what the spec records, and
    // the other order produces a number no archive holds.
    let header = SYNTHETIC_ROOT_HEADER;
    assert_eq!(
        header.len(),
        SYNTHETIC_ROOT_HEADER_LEN,
        "the archive root header is four u32"
    );

    let mut asset_header = BigEndian::new(&header, SYNTHETIC_ASSET_ORIGIN);
    let mut words: Vec<u32> = Vec::new();
    for index in 0..SYNTHETIC_ROOT_WORD_COUNT {
        words.push(
            asset_header
                .u32()
                .expect("the asset layer reads a root header word"),
        );
        let expected = SYNTHETIC_ROOT_WORDS[index];
        assert_eq!(
            words[index], expected,
            "root header word {index} folded most significant byte first"
        );
    }
    assert_eq!(
        words.len(),
        SYNTHETIC_ROOT_WORD_COUNT,
        "four words were read"
    );
    assert_eq!(
        words[0], SYNTHETIC_DIRECTORY_COUNT,
        "the first word is the directory count the spec records"
    );
    assert_eq!(
        words[2], SYNTHETIC_ROOT_SIZE_FIELD,
        "the third word is zero at the root"
    );
    assert_eq!(
        asset_header.remaining(),
        0,
        "and the header is exactly consumed"
    );

    let mut host_header = LittleEndian::new(&header, SYNTHETIC_HOST_ORIGIN);
    let wrong_order = host_header
        .u32()
        .expect("the executable layer reads the word");
    assert_eq!(
        wrong_order, SYNTHETIC_DIRECTORY_COUNT_WRONG_ORDER,
        "the same four bytes folded the other way are no archive's directory count"
    );
    assert_ne!(
        wrong_order, SYNTHETIC_DIRECTORY_COUNT,
        "so the magnitude is what establishes the order, not an absence of error"
    );
}

// --- one test per inherent integer reader ------------------------------------
//
// The two tests above prove the fold of a whole buffer and the two below it
// prove that a signed read reinterprets. Between them, one reader is proved by
// one assertion in a test that also proves four other things, so a failure
// names a fold rather than a reader, and the refusal of each reader is proved
// by the table driven tests, which discard the value on purpose.
//
// The six tests below close that: each names one reader, pins its exact value
// against a literal the spec states, and pairs it with the same reader's
// refusal of a buffer one byte short. `BigEndian::u16`, `BigEndian::i16`,
// `LittleEndian::u16`, `LittleEndian::u32` and `LittleEndian::i16` have no
// production caller anywhere in the workspace, so these six tests are the whole
// proof for them and not a second line of defence.

/// claim: END-015, END-014, END-017, END-018, END-022
/// oracle: class 5 (hand computed vectors from the spec: `12 34` folds most significant byte first to 0x1234; a one byte buffer is refused)
#[test]
fn synthetic_the_asset_layer_u16_folds_two_bytes_most_significant_first() {
    let mut reader = BigEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_ASSET_ORIGIN);
    let value = reader.u16().expect("the asset layer reads two bytes");
    assert_eq!(
        value, SYNTHETIC_U16_ASSET,
        "the asset layer folds 12 34 as 0x1234"
    );
    assert_eq!(
        value, 4_660,
        "the same value written out, so a wrong constant in the ledger fails"
    );
    assert_eq!(reader.offset(), WIDTH_U16, "one u16 consumed two bytes");

    // The good-input twin: the same reader, one byte short of the width it
    // wants. The refusal names the operation, the origin and the two counts,
    // and it leaves the cursor where it was.
    let mut short = BigEndian::new(&SYNTHETIC_ONE_BYTE_SHORT, SYNTHETIC_ASSET_ORIGIN);
    let (offset, field, origin, needed, available) =
        big_endian_truncated(short.u16().map(|_| ()), "asset u16 from one byte");
    assert_eq!(offset, 0, "the refused read started at byte 0");
    assert_eq!(field, FIELD_U16, "the refusal names the u16 field");
    assert_eq!(
        origin, SYNTHETIC_ASSET_ORIGIN,
        "the refusal names the origin"
    );
    assert_eq!(needed, WIDTH_U16, "the refusal says two bytes were wanted");
    assert_eq!(available, WIDTH_U8, "the refusal says one byte was left");
    assert_eq!(
        short.offset(),
        0,
        "and a refused read leaves the cursor where it was"
    );
}

/// claim: END-015, END-014, END-017, END-018, END-022
/// oracle: class 5 (hand computed vectors from the spec: `12 34` folds least significant byte first to 0x3412; a one byte buffer is refused)
#[test]
fn synthetic_the_executable_layer_u16_folds_two_bytes_least_significant_first() {
    let mut reader = LittleEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_HOST_ORIGIN);
    let value = reader.u16().expect("the executable layer reads two bytes");
    assert_eq!(
        value, SYNTHETIC_U16_HOST,
        "the executable layer folds 12 34 as 0x3412"
    );
    assert_eq!(
        value, 13_330,
        "the same value written out, so a wrong constant in the ledger fails"
    );
    assert_eq!(reader.offset(), WIDTH_U16, "one u16 consumed two bytes");

    let mut short = LittleEndian::new(&SYNTHETIC_ONE_BYTE_SHORT, SYNTHETIC_HOST_ORIGIN);
    let (offset, field, origin, needed, available) =
        little_endian_truncated(short.u16().map(|_| ()), "host u16 from one byte");
    assert_eq!(offset, 0, "the refused read started at byte 0");
    assert_eq!(field, FIELD_U16, "the refusal names the u16 field");
    assert_eq!(
        origin, SYNTHETIC_HOST_ORIGIN,
        "the refusal names the origin"
    );
    assert_eq!(needed, WIDTH_U16, "the refusal says two bytes were wanted");
    assert_eq!(available, WIDTH_U8, "the refusal says one byte was left");
    assert_eq!(
        short.offset(),
        0,
        "and a refused read leaves the cursor where it was"
    );
}

/// claim: END-015, END-014, END-017, END-018, END-022
/// oracle: class 5 (hand computed vectors from the spec: `12 34 56 78` folds most significant byte first to 0x12345678; a three byte buffer is refused)
#[test]
fn synthetic_the_asset_layer_u32_folds_four_bytes_most_significant_first() {
    let mut reader = BigEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_ASSET_ORIGIN);
    let value = reader.u32().expect("the asset layer reads four bytes");
    assert_eq!(
        value, SYNTHETIC_U32_ASSET,
        "the asset layer folds 12 34 56 78 as 0x12345678"
    );
    assert_eq!(
        value, 305_419_896,
        "the same value written out, so a wrong constant in the ledger fails"
    );
    assert_eq!(reader.offset(), WIDTH_U32, "one u32 consumed four bytes");

    let mut short = BigEndian::new(&SYNTHETIC_THREE_BYTES_SHORT, SYNTHETIC_ASSET_ORIGIN);
    let (offset, field, origin, needed, available) =
        big_endian_truncated(short.u32().map(|_| ()), "asset u32 from three bytes");
    assert_eq!(offset, 0, "the refused read started at byte 0");
    assert_eq!(field, FIELD_U32, "the refusal names the u32 field");
    assert_eq!(
        origin, SYNTHETIC_ASSET_ORIGIN,
        "the refusal names the origin"
    );
    assert_eq!(needed, WIDTH_U32, "the refusal says four bytes were wanted");
    assert_eq!(
        available,
        WIDTH_U32 - 1,
        "the refusal says three bytes were left"
    );
    assert_eq!(
        short.offset(),
        0,
        "and a refused read leaves the cursor where it was"
    );
}

/// claim: END-015, END-014, END-017, END-018, END-022
/// oracle: class 5 (hand computed vectors from the spec: `12 34 56 78` folds least significant byte first to 0x78563412; a three byte buffer is refused)
#[test]
fn synthetic_the_executable_layer_u32_folds_four_bytes_least_significant_first() {
    let mut reader = LittleEndian::new(&SYNTHETIC_FOLD_BYTES, SYNTHETIC_HOST_ORIGIN);
    let value = reader.u32().expect("the executable layer reads four bytes");
    assert_eq!(
        value, SYNTHETIC_U32_HOST,
        "the executable layer folds 12 34 56 78 as 0x78563412"
    );
    assert_eq!(
        value, 2_018_915_346,
        "the same value written out, so a wrong constant in the ledger fails"
    );
    assert_eq!(reader.offset(), WIDTH_U32, "one u32 consumed four bytes");

    let mut short = LittleEndian::new(&SYNTHETIC_THREE_BYTES_SHORT, SYNTHETIC_HOST_ORIGIN);
    let (offset, field, origin, needed, available) =
        little_endian_truncated(short.u32().map(|_| ()), "host u32 from three bytes");
    assert_eq!(offset, 0, "the refused read started at byte 0");
    assert_eq!(field, FIELD_U32, "the refusal names the u32 field");
    assert_eq!(
        origin, SYNTHETIC_HOST_ORIGIN,
        "the refusal names the origin"
    );
    assert_eq!(needed, WIDTH_U32, "the refusal says four bytes were wanted");
    assert_eq!(
        available,
        WIDTH_U32 - 1,
        "the refusal says three bytes were left"
    );
    assert_eq!(
        short.offset(),
        0,
        "and a refused read leaves the cursor where it was"
    );
}

/// claim: END-016, END-015, END-014, END-017, END-018, END-022
/// oracle: class 5 (hand computed vectors from the spec: `FF FE` read most significant byte first is 0xFFFE, which is -2 as an i16; a one byte buffer is refused)
#[test]
fn synthetic_the_asset_layer_i16_reinterprets_two_bytes_as_two_complement() {
    let mut reader = BigEndian::new(&SYNTHETIC_SIGNED_PAIR, SYNTHETIC_ASSET_ORIGIN);
    let value = reader.i16().expect("the asset layer reads an i16");
    assert_eq!(
        value, SYNTHETIC_SIGNED_PAIR_ASSET,
        "the asset layer reads FF FE as -2, not as 65534"
    );
    assert_eq!(
        value, -2,
        "the same value written out, so a wrong constant in the ledger fails"
    );
    assert_eq!(reader.offset(), WIDTH_U16, "one i16 consumed two bytes");

    // The same two bytes read the other way round are a different number, so
    // the sign did not come from a conversion that ignores the order.
    let mut host = LittleEndian::new(&SYNTHETIC_SIGNED_PAIR, SYNTHETIC_HOST_ORIGIN);
    assert_eq!(
        host.i16().expect("the executable layer reads an i16"),
        SYNTHETIC_SIGNED_PAIR_HOST,
        "the executable layer reads the same FF FE as -257"
    );

    let mut short = BigEndian::new(&SYNTHETIC_ONE_BYTE_SHORT, SYNTHETIC_ASSET_ORIGIN);
    let (offset, field, origin, needed, available) =
        big_endian_truncated(short.i16().map(|_| ()), "asset i16 from one byte");
    assert_eq!(offset, 0, "the refused read started at byte 0");
    assert_eq!(field, FIELD_I16, "the refusal names the i16 field");
    assert_eq!(
        origin, SYNTHETIC_ASSET_ORIGIN,
        "the refusal names the origin"
    );
    assert_eq!(needed, WIDTH_U16, "the refusal says two bytes were wanted");
    assert_eq!(available, WIDTH_U8, "the refusal says one byte was left");
    assert_eq!(
        short.offset(),
        0,
        "and a refused read leaves the cursor where it was"
    );
}

/// claim: END-016, END-015, END-014, END-017, END-018, END-022
/// oracle: class 5 (hand computed vectors from the spec: `FF FE` read least significant byte first is 0xFEFF, which is -257 as an i16; a one byte buffer is refused)
#[test]
fn synthetic_the_executable_layer_i16_reinterprets_two_bytes_as_two_complement() {
    let mut reader = LittleEndian::new(&SYNTHETIC_SIGNED_PAIR, SYNTHETIC_HOST_ORIGIN);
    let value = reader.i16().expect("the executable layer reads an i16");
    assert_eq!(
        value, SYNTHETIC_SIGNED_PAIR_HOST,
        "the executable layer reads FF FE as -257, not as 65279"
    );
    assert_eq!(
        value, -257,
        "the same value written out, so a wrong constant in the ledger fails"
    );
    assert_eq!(reader.offset(), WIDTH_U16, "one i16 consumed two bytes");

    // The same two bytes read the other way round are a different number, so
    // the sign did not come from a conversion that ignores the order.
    let mut asset = BigEndian::new(&SYNTHETIC_SIGNED_PAIR, SYNTHETIC_ASSET_ORIGIN);
    assert_eq!(
        asset.i16().expect("the asset layer reads an i16"),
        SYNTHETIC_SIGNED_PAIR_ASSET,
        "the asset layer reads the same FF FE as -2"
    );

    let mut short = LittleEndian::new(&SYNTHETIC_ONE_BYTE_SHORT, SYNTHETIC_HOST_ORIGIN);
    let (offset, field, origin, needed, available) =
        little_endian_truncated(short.i16().map(|_| ()), "host i16 from one byte");
    assert_eq!(offset, 0, "the refused read started at byte 0");
    assert_eq!(field, FIELD_I16, "the refusal names the i16 field");
    assert_eq!(
        origin, SYNTHETIC_HOST_ORIGIN,
        "the refusal names the origin"
    );
    assert_eq!(needed, WIDTH_U16, "the refusal says two bytes were wanted");
    assert_eq!(available, WIDTH_U8, "the refusal says one byte was left");
    assert_eq!(
        short.offset(),
        0,
        "and a refused read leaves the cursor where it was"
    );
}
