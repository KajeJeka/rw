//! Synthetic gate tests for the LZSS decoder (`fo1-lzss`).
//!
//! Every fixture here is built byte by byte in this file and named
//! `synthetic_*`, so the suite needs no game install and runs in CI. A synthetic
//! fixture cannot support a CONFIRMED evidence tag (guide: Fixtures and golden
//! files). What each fixture does carry is:
//!
//! - a **hand computed** token, ring slot or output byte taken from
//!   `docs/spec/lzss.md`, so the expected value is arithmetic and not a decoder's
//!   opinion (oracle class 5);
//! - the **same fixture run through `fo1_tests::RefLzss`**, an independent
//!   reference decoder whose ring parameters are arguments rather than constants
//!   (oracle class 4), for the canary, where disagreement between ring parameter
//!   sets is the one thing a reference decoder can be the oracle for; and
//! - an **algebraic law** about the ring mapping or the token encoding (class 6).
//!
//! The expected bytes of a decode are **never** taken from the code under test
//! and never from `RefLzss` (guide: Independent oracles; Hold out vectors and
//! canaries).
//!
//! Two traps this file is written against, both of which produce a false pass
//! rather than a failure:
//!
//! 1. **A uniform literal fixture cannot detect a wrong write index.** The
//!    guide's vector `00 00 0F` decodes to eighteen `0x20` bytes and the ring
//!    prefill is `0x20`, so write starts 0, 4078 and 4095 produce byte identical
//!    output for it. Every literal run here is therefore **non-uniform**:
//!    `fill ^ (position % 251)`.
//!    `synthetic_canary_rejects_write_index_4095_zero_and_prefill_00` exists to
//!    keep that property from rotting, and it checks bytes, because LZSS-C062
//!    records that every wrong ring parameter set kept the length.
//! 2. **The mid pair case never occurs in the real corpus.** 0 of 61,070 blocks
//!    end mid back reference (LZSS-C024), so
//!    `synthetic_mid_pair_back_reference_is_an_error_not_a_loop` builds the
//!    stream itself. A decoder that loops there passes the entire corpus.
//!
//! A third trap is subtler and is recorded in the builder that runs into it: a
//! literal count that is not a whole number of control bytes leaves a partial
//! trailing control byte whose unused bits are **clear**, and the decoder keeps
//! reading its eight tokens, so those bits consume whatever input follows as a
//! back reference. Every fixture here uses a multiple of eight unless the
//! partial byte is the thing under test.
//!
//! # Interface
//!
//! `docs/spec/lzss.md` LZSS-024 states `decode_block` twice: as a method on
//! `LzssDecoder` in the interface block, and as a free function taking the
//! decoder as a parameter under "the required signatures from the plan". The
//! spec asserts both, and interface decision 1 defines `decode_into` as "one
//! `decode_block`", so this suite exercises both spellings.
//!
//! This file is EXPECTED NOT TO COMPILE until `fo1-lzss` exists. That is the
//! role: a gate test written before the implementation is what makes "fail
//! before, pass after" meaningful (guide: Fail before, pass after; a compile
//! error counts as a failure).

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
use fo1_lzss::{
    decode_block, decode_into, LzssDecoder, LzssError, MAX_MATCH, MIN_MATCH, RING_FILL, RING_SIZE,
    RING_WRITE_START,
};
use fo1_tests::RefLzss;

// --- the ring, from the constants ledger ---------------------------------------

/// Ring slots. The write head wraps here and nowhere else (LZSS-C001, LZSS-001).
const RING_SLOT_COUNT: usize = 4096;

/// Byte a fresh ring holds in slots `0..=4077` (LZSS-C002, LZSS-002).
const PREFILL_BYTE: u8 = 0x20;

/// Last slot of the prefilled region. Slots `4078..=4095` start as zero
/// (LZSS-C003, LZSS-002).
const PREFILLED_LAST_SLOT: usize = 4077;

/// Byte a fresh ring holds in slots `4078..=4095` (LZSS-002).
const ZERO_BYTE: u8 = 0x00;

/// Where the write head starts in every fresh ring (LZSS-C004, LZSS-003).
const WRITE_START: usize = 4078;

/// `4096 - 4078`: the distance from the write head's start to the end of the
/// ring, equal to `MAX_MATCH` (LZSS-C005).
const LOOKAHEAD_SLOTS: usize = 18;

/// Shortest run a back reference can encode (LZSS-C006, LZSS-008).
const SHORTEST_RUN: usize = 3;

/// Longest run a back reference can encode (LZSS-C007, LZSS-008).
const LONGEST_RUN: usize = 18;

/// Low nibble of the second byte: `length = (b2 & 0x0F) + 3` (LZSS-C008).
const LENGTH_BIAS: usize = 0x0F;

/// High nibble of the second byte: the top of the source position (LZSS-C009).
const POSITION_HIGH_MASK: usize = 0xF0;

/// Largest addressable source index. `b1` gives at most `0xFF` and the high
/// nibble at most `0xF00` (LZSS-C010, LZSS-012).
const LARGEST_SOURCE: usize = 4095;

/// Tokens one control byte carries, bit 0 first (LZSS-C011, LZSS-005).
const TOKENS_PER_CONTROL: usize = 8;

/// Every possible value of either byte of a token pair.
const BYTE_VALUE_COUNT: usize = 256;

/// `256 * 256` token pairs: every `(b1, b2)` a stream can ever present.
const TOKEN_PAIR_COUNT: usize = BYTE_VALUE_COUNT * BYTE_VALUE_COUNT;

/// Output positions walked when proving the ring mapping is a bijection.
const RING_TURN: usize = RING_SLOT_COUNT;

/// The output position a ring slot holds: the shift is `WRITE_START`, so the
/// slot the write head starts at holds the position the head wrote a whole turn
/// ago (LZSS-010).
const RING_SHIFT_BACK: usize = RING_SLOT_COUNT - WRITE_START;

/// The run the one turn sweep reads out of every slot: the shortest run a token
/// can encode (LZSS-008).
const TURN_PROBE_RUN: usize = SHORTEST_RUN;

/// Ring size the original binary declares, per the guide and not measured
/// (LZSS-C012, LZSS-004).
const DECLARED_ORIGINAL_RING: usize = 4116;

/// Slots of that declaration no token can address (LZSS-C013, LZSS-004).
const UNREACHABLE_SLOTS: usize = 20;

// --- the three worked vectors --------------------------------------------------

/// `docs/spec/lzss.md` LZSS-018: control byte `0x00`, then the pair `00 0F`.
const VECTOR_00000F_INPUT: [u8; 3] = [0x00, 0x00, 0x0F];

/// Output the spec gives `VECTOR_00000F_INPUT`: eighteen spaces (LZSS-C036).
const VECTOR_00000F_OUTPUT: usize = 18;

/// The eighteen spaces, as bytes, computed rather than read from the prefill of
/// the code under test.
const VECTOR_00000F_EXPECTED: [u8; VECTOR_00000F_OUTPUT] = [PREFILL_BYTE; VECTOR_00000F_OUTPUT];

/// Bytes the worked vector `00 00 0F` occupies on disk.
const VECTOR_00000F_WIDTH: usize = 3;

/// LZSS-C030 and LZSS-C031: the on-disk token of LZSS-017.
const TOKEN_4E36: [u8; 2] = [0x4E, 0x36];

/// LZSS-C040 and LZSS-C041: the token of LZSS-019.
const TOKEN_753F: [u8; 2] = [0x75, 0x3F];

/// Literals in front of the LZSS-017 and LZSS-019 tokens. 5000 is a whole number
/// of control bytes, so no partial control byte trails them.
const VECTOR_LONG_LITERALS: usize = 5000;

/// Output position where the LZSS-017 token is written (LZSS-C033).
const LZSS017_TARGET_POSITION: usize = 5000;

/// Output position the LZSS-017 run copies from (LZSS-C034).
const LZSS017_SOURCE_POSITION: usize = 4960;

/// Ring slot the LZSS-017 write head has reached (LZSS-C033).
const LZSS017_WRITE_SLOT: usize = 886;

/// Ring slot the LZSS-017 run reads (LZSS-C032).
const LZSS017_SOURCE_SLOT: usize = 846;

/// Distance between the LZSS-017 source position and its target.
const LZSS017_DISTANCE: usize = 40;

/// Bytes the LZSS-017 run writes (LZSS-C035).
const LZSS017_RUN: usize = 9;

/// Declared output of the LZSS-017 stream: the literals plus the run.
const LZSS017_DECLARED: usize = VECTOR_LONG_LITERALS + LZSS017_RUN;

/// Ring slot the LZSS-019 source names (LZSS-C042).
const LZSS019_SOURCE_SLOT: usize = 885;

/// Bytes the LZSS-019 run writes.
const LZSS019_RUN: usize = 18;

/// Distance from the LZSS-019 source to its write head.
const LZSS019_DISTANCE: usize = 1;

/// Declared output of the LZSS-019 stream.
const LZSS019_DECLARED: usize = VECTOR_LONG_LITERALS + LZSS019_RUN;

/// Output position of the byte the LZSS-019 run repeats.
const LZSS019_HEAD_POSITION: usize = VECTOR_LONG_LITERALS - 1;

// --- ring mapping anchors ------------------------------------------------------

/// Output position where the second, independent ring-mapping anchor sits.
const ANCHOR_TARGET_POSITION: usize = 1000;

/// Output position its run copies from, forty back.
const ANCHOR_SOURCE_POSITION: usize = ANCHOR_TARGET_POSITION - LZSS017_DISTANCE;

/// Ring slot the anchor run reads. `(960 + 4078) mod 4096 = 942`.
const ANCHOR_SOURCE_SLOT: usize = 942;

/// Ring slot the anchor write head has reached. `(1000 + 4078) mod 4096 = 982`.
const ANCHOR_WRITE_SLOT: usize = 982;

/// The anchor token's first byte: 942 is `0x3AE`.
const ANCHOR_TOKEN_B1: u8 = 0xAE;

/// The anchor token's second byte: high nibble 3, bias `9 - 3`.
const ANCHOR_TOKEN_B2: u8 = 0x36;

/// Declared output of the second anchor stream.
const ANCHOR_DECLARED: usize = ANCHOR_TARGET_POSITION + LZSS017_RUN;

/// The wrap anchors of `ring_index(p) = (p + 4078) mod 4096`, as
/// `(output position, ring slot)`.
const RING_WRAP_ANCHORS: [(usize, usize); 5] =
    [(0, 4078), (17, 4095), (18, 0), (4094, 4076), (4096, 4078)];

/// Number of rows in [`RING_WRAP_ANCHORS`].
const RING_WRAP_ANCHOR_COUNT: usize = 5;

// --- the prefill boundary ------------------------------------------------------

/// Ring slot on the last prefilled index. 4077 is `0xFED`.
const BOUNDARY_PREFILLED_SLOT: usize = PREFILLED_LAST_SLOT;

/// Ring slot on the first zero index. 4078 is `0xFEE`.
const BOUNDARY_ZERO_SLOT: usize = PREFILLED_LAST_SLOT + 1;

/// Ring slot at the very top of the addressable range.
const BOUNDARY_TOP_SLOT: usize = LARGEST_SOURCE;

/// Ring slot inside the prefilled region, used by the boundary fixtures.
const BOUNDARY_INSIDE_SLOT: usize = 0;

/// Run length the boundary fixtures ask for.
const BOUNDARY_RUN: usize = SHORTEST_RUN;

/// Declared output of a boundary fixture.
const BOUNDARY_DECLARED: usize = BOUNDARY_RUN;

// --- the match length range ----------------------------------------------------

/// Run the shortest-match fixture asks for.
const MIN_RUN_FIXTURE: usize = SHORTEST_RUN;

/// Run the longest-match fixture asks for.
const MAX_RUN_FIXTURE: usize = LONGEST_RUN;

/// Ring slot both length fixtures read, inside the prefilled region.
const LENGTH_FIXTURE_SLOT: usize = 6;

// --- the overlap fixture -------------------------------------------------------

/// Literals in front of the overlap token. 96 is a whole number of control bytes
/// and puts the write head at ring slot 78.
const OVERLAP_LITERALS: usize = 96;

/// Ring slot the overlap write head has reached. `(96 + 4078) mod 4096`.
const OVERLAP_WRITE_SLOT: usize = 78;

/// Distance from the overlap run's source to its write head. Smaller than
/// `OVERLAP_RUN`, so the run reads bytes it has not written yet.
const OVERLAP_DISTANCE: usize = 3;

/// Ring slot the overlap run reads: `75 - 3`.
const OVERLAP_SOURCE_SLOT: usize = 75;

/// Run length of the overlap fixture.
const OVERLAP_RUN: usize = LONGEST_RUN;

/// The overlap token's first byte: 75 is `0x04B`.
const OVERLAP_TOKEN_B1: u8 = 0x4B;

/// The overlap token's second byte: high nibble 0, bias `18 - 3`.
const OVERLAP_TOKEN_B2: u8 = 0x0F;

/// Output position the overlap run reads at its first three steps.
const OVERLAP_SEED_POSITION: usize = OVERLAP_LITERALS - OVERLAP_DISTANCE;

/// Whole three byte repetitions the overlap run emits.
const OVERLAP_REPEATS: usize = OVERLAP_RUN / OVERLAP_DISTANCE;

/// Declared output of the overlap stream.
const OVERLAP_DECLARED: usize = OVERLAP_LITERALS + OVERLAP_RUN;

/// Input bytes one control byte and eight literals occupy.
const CONTROL_BYTE_WIDTH: usize = TOKENS_PER_CONTROL + 1;

// --- the absolute index fixtures -----------------------------------------------

/// Literals in front of the first absolute-index token.
const ABSOLUTE_SHORT_LITERALS: usize = 96;

/// Literals in front of the second absolute-index token.
const ABSOLUTE_LONG_LITERALS: usize = 192;

/// Literals in front of the first same-distance token.
const DISTANCE_FIRST_LITERALS: usize = 288;

/// Literals in front of the second same-distance token.
const DISTANCE_SECOND_LITERALS: usize = 392;

/// Ring slot both absolute-index tokens name. 72 is `0x048`.
const ABSOLUTE_FIXTURE_SLOT: usize = 72;

/// Distance from the first absolute-index token to its write head.
const ABSOLUTE_FIRST_DISTANCE: usize = 6;

/// Distance from the second absolute-index token to its write head.
const ABSOLUTE_SECOND_DISTANCE: usize = 102;

/// Output position the first absolute-index token copies from.
const ABSOLUTE_FIRST_SOURCE_POSITION: usize = ABSOLUTE_SHORT_LITERALS - ABSOLUTE_FIRST_DISTANCE;

/// Output position the second absolute-index token copies from.
const ABSOLUTE_SECOND_SOURCE_POSITION: usize = ABSOLUTE_LONG_LITERALS - ABSOLUTE_SECOND_DISTANCE;

/// Distance from source to write head in both same-distance fixtures.
const FIXTURE_DISTANCE: usize = 100;

/// Ring slot the first same-distance token names. 170 is `0x0AA`.
const DISTANCE_FIRST_SLOT: usize = 170;

/// Ring slot the second same-distance token names. 274 is `0x112`.
const DISTANCE_SECOND_SLOT: usize = 274;

/// Run length of every absolute-index fixture.
const ABSOLUTE_RUN: usize = SHORTEST_RUN;

/// Declared output of the first absolute-index stream.
const ABSOLUTE_FIRST_DECLARED: usize = ABSOLUTE_SHORT_LITERALS + ABSOLUTE_RUN;

/// Declared output of the second absolute-index stream.
const ABSOLUTE_SECOND_DECLARED: usize = ABSOLUTE_LONG_LITERALS + ABSOLUTE_RUN;

/// Declared output of the first same-distance stream.
const DISTANCE_FIRST_DECLARED: usize = DISTANCE_FIRST_LITERALS + ABSOLUTE_RUN;

/// Declared output of the second same-distance stream.
const DISTANCE_SECOND_DECLARED: usize = DISTANCE_SECOND_LITERALS + ABSOLUTE_RUN;

// --- termination and length ----------------------------------------------------

/// Literals in the mid control byte fixtures. 40 is a whole number of control
/// bytes, so the trailing byte under test is the only partial one.
const MID_CONTROL_LITERALS: usize = 40;

/// Declared output of the mid control byte fixtures.
const MID_CONTROL_OUTPUT: usize = MID_CONTROL_LITERALS;

/// Input bytes the mid control byte fixtures carry: forty literals behind five
/// control bytes, and nothing else. It is the offset the input ran out at and
/// the number of input bytes consumed, both of them counted here rather than
/// read off the error.
const MID_CONTROL_INPUT_BYTES: usize =
    MID_CONTROL_LITERALS + MID_CONTROL_LITERALS / TOKENS_PER_CONTROL;

/// Declared size one more than the literals the mid control byte stream carries.
const SHORT_DECLARED: usize = MID_CONTROL_OUTPUT + 1;

/// Declared size one less, so the last literal would write past it.
const OVERRUN_ONE_DECLARED: usize = MID_CONTROL_OUTPUT - 1;

/// Fill byte of the shortfall and overrun fixtures.
const LENGTH_FIXTURE_FILL: u8 = 0x22;

/// Literals in front of the overrun token.
const OVERRUN_TOKEN_LITERALS: usize = 96;

/// Declared size the overrun token cannot fit into: exactly what the literals
/// already produced, so the token has nowhere to go.
const OVERRUN_TOKEN_DECLARED: usize = OVERRUN_TOKEN_LITERALS;

/// Run length the overrun token asks for, larger than the zero room it has.
const OVERRUN_TOKEN_RUN: usize = LZSS017_RUN;

/// Input offset the overrun token starts at: the ninety six literals take
/// ninety six bytes and twelve control bytes, the token's own control byte is
/// the next one, and the token's first byte follows it.
const OVERRUN_TOKEN_AT_OFFSET: usize =
    OVERRUN_TOKEN_LITERALS + OVERRUN_TOKEN_LITERALS / TOKENS_PER_CONTROL + 1;

/// Fill byte of the mid control byte fixtures.
const MID_CONTROL_FILL: u8 = 0x33;

/// The trailing control byte whose eight tokens all have no input behind them.
const MID_CONTROL_TRAILING: [u8; 1] = [0x00];

/// Literals in front of `ONE_TOKEN_CONTROL`.
const ONE_TOKEN_LITERALS: usize = TOKENS_PER_CONTROL;

/// A control byte with only bit 0 set, so seven of its eight tokens have no
/// input behind them.
const ONE_TOKEN_CONTROL: [u8; 1] = [0x01];

/// The one literal behind `ONE_TOKEN_CONTROL`.
const ONE_TOKEN_LITERAL: [u8; 1] = [0x66];

/// Fill byte of the one-token fixture.
const ONE_TOKEN_FILL: u8 = 0x44;

/// Declared output of the one-token stream.
const ONE_TOKEN_OUTPUT: usize = ONE_TOKEN_LITERALS + 1;

/// A stream that leaves a back reference with one input byte to read.
const MID_PAIR_AT_START: [u8; 2] = [0x00, 0x00];

/// The single pair byte behind the truncated reference.
const MID_PAIR_TAIL: [u8; 1] = [0x00];

/// Fill byte of the mid pair fixture.
const MID_PAIR_FILL: u8 = 0x77;

/// Input bytes left when the truncated pair starts.
const MID_PAIR_REMAINING: usize = 1;

/// Input offset the truncated pair starts at: the control byte is the first byte
/// of the stream and the pair's own first byte is the second.
const MID_PAIR_AT_OFFSET: usize = 1;

/// Input offset the truncated pair starts at in the mid stream fixture: the
/// forty literals take forty bytes and five control bytes, the trailing control
/// byte is the next one, and the pair's first byte follows it.
const MID_STREAM_PAIR_AT_OFFSET: usize =
    MID_CONTROL_LITERALS + MID_CONTROL_LITERALS / TOKENS_PER_CONTROL + 1;

// --- the canary ----------------------------------------------------------------

/// Fill byte of the canary literal run. Any value works; this one is not the
/// prefill, so a decoder that returned the prefill for a written byte is caught.
const CANARY_FILL: u8 = 0xA5;

/// Literals in front of the canary token. 24 leaves ring slots 6 through 4077
/// unwritten, so the token reads the prefilled region, and the literals vary with
/// their output position, so which slot an output occupies is visible.
const CANARY_LITERALS: usize = 24;

/// Ring slot the canary token reads. Under `RING_WRITE_START` it is unwritten;
/// under every other write start it is not, which is the whole canary.
const CANARY_SLOT: usize = 6;

/// The canary token's first byte.
const CANARY_TOKEN_B1: u8 = 0x06;

/// The canary token's second byte: high nibble 0, bias zero.
const CANARY_TOKEN_B2: u8 = 0x00;

/// Declared output of the canary stream.
const CANARY_DECLARED: usize = CANARY_LITERALS + SHORTEST_RUN;

/// The canary tail under the canonical ruling, as bytes.
const CANARY_CANONICAL_TAIL: [u8; SHORTEST_RUN] = [PREFILL_BYTE; SHORTEST_RUN];

/// The write index starts the ruling rejects.
const REJECTED_WRITE_STARTS: [(usize, &str); 2] =
    [(4095, "write_start 4095"), (0, "write_start 0")];

/// Wrong ring parameter sets checked against the ruling: the two write starts
/// above plus a zero prefill.
const REJECTED_VARIANT_COUNT: usize = 3;

// --- the reset fixtures --------------------------------------------------------

/// Literals in block A of the reset fixture: the overlap stream, whose 96 literals
/// write ring slots 4078..=4095 and 0..=77 and whose 18 byte run then writes
/// 78..=95. Block B reads slot 20 out of that written region.
const RESET_BLOCK_A_LITERALS: usize = OVERLAP_LITERALS;

/// Declared output of block A.
const RESET_BLOCK_A_DECLARED: usize = OVERLAP_DECLARED;

/// Literals in block B.
const RESET_BLOCK_B_LITERALS: usize = 32;

/// Fill byte of block A.
const RESET_BLOCK_A_FILL: u8 = CANARY_FILL;

/// Fill byte of block B.
const RESET_BLOCK_B_FILL: u8 = 0x5A;

/// Ring slot block B's token reads.
///
/// **This must be a slot block B does not write.** Block B's 32 literals start the
/// head at 4078 and cover slots 4078..=4095 and 0..=13, so a token naming slot 10
/// reads a slot block B has already overwritten with its own literal and can never
/// see block A. Slot 20 is inside block A's written region (its 96 literals plus
/// its 18 byte run cover 4078..=4095 and 0..=95) and outside block B's, so a fresh
/// ring reads the prefill there and a carried ring reads block A's byte. That
/// difference is the only thing the carried-ring assertion below can see.
const RESET_BLOCK_B_SLOT: usize = 20;

/// Run length block B's token asks for.
const RESET_BLOCK_B_RUN: usize = SHORTEST_RUN;

/// Distance from block B's write head to the slot its token reads, forward.
/// `(32 + 4078) mod 4096` is 14, and `20 - 14` is 6.
const RESET_BLOCK_B_DISTANCE: usize = 6;

/// Ring slots block B's 32 literals write, and block A's 96 literals plus its 18
/// byte run write. Slots 14..=95 belong to block A alone.
const RESET_BLOCK_A_ONLY_SLOT: usize = RESET_BLOCK_B_SLOT;

/// Declared output of block B.
const RESET_BLOCK_B_DECLARED: usize = RESET_BLOCK_B_LITERALS + RESET_BLOCK_B_RUN;

// --- the bit flip corpus -------------------------------------------------------

/// Bytes in the bit flip stream: the overlap stream.
const BIT_FLIP_STREAM_BYTES: usize = 111;

/// Bits in the bit flip stream.
const BIT_FLIP_BITS: usize = BIT_FLIP_STREAM_BYTES * 8;

/// Declared output of the bit flip stream.
const BIT_FLIP_DECLARED: usize = OVERLAP_DECLARED;

/// Input bytes of the bit flip stream that carry a literal value: the ninety
/// six literals and the twelve control bytes in front of them.
const BIT_FLIP_LITERAL_BYTES: usize = OVERLAP_LITERALS + OVERLAP_LITERALS / TOKENS_PER_CONTROL;

/// Bits of the bit flip stream that fall on a literal byte. A literal carries
/// no structure, so a flip in one changes an output byte and nothing else.
const BIT_FLIP_LITERAL_BITS: usize = OVERLAP_LITERALS * 8;

/// Ring slot the write head reaches after the declared output of the bit flip
/// stream: `(114 + 4078) mod 4096` is 96 (LZSS-010).
const BIT_FLIP_HEAD_SLOT: usize = 96;

// --- the error wording ---------------------------------------------------------

/// The input offset the unreachable `IndexOverflow` message names.
///
/// The three numbers below are this file's own: what is under test is the
/// wording, because the public API cannot produce the variant at all
/// (LZSS-024). The crate's `Display` renders what `docs/spec/lzss.md`
/// "## Error text" fixes, and a message that printed nothing, dropped the
/// offset or permuted the counts fails on the literal below.
const INDEX_OVERFLOW_AT: usize = 7;

/// The declared output size that message names.
const INDEX_OVERFLOW_DECLARED_OUT: usize = 12;

/// The produced count that message names.
const INDEX_OVERFLOW_PRODUCED: usize = 5;

// --- fixture builders ----------------------------------------------------------

/// Folds `position` into `byte` so a literal run is **not** uniform.
///
/// This is the property the whole file rests on. A uniform run decodes
/// identically no matter how the ring maps output positions onto slots, because
/// every slot that matters holds the same byte, so a canary built on one passes
/// on `write_start = 0` and on `4095` and proves nothing. `position % 251` is
/// below 251 by construction, so the narrowing cannot fail; `expect` is the loud
/// form, because a silent fallback to zero would make the run uniform and
/// destroy exactly the property this function exists to provide.
fn mix(byte: u8, position: usize) -> u8 {
    byte ^ u8::try_from(position % 251).expect("synthetic: position % 251 is below 251")
}

/// Builds a stream of `count` non-uniform literals: eight literals behind every
/// `0xFF` control byte, then a partial control byte carrying the remainder.
///
/// **`count` must be a multiple of eight.** A partial trailing control byte has
/// clear bits above the literals it carries, and the decoder keeps reading all
/// eight of its tokens, so those bits become back references over whatever
/// input follows. `literal_run(20)` therefore does not produce twenty literals
/// followed by a token: it produces twenty literals followed by a control byte
/// whose bits 4 through 7 consume that token as a pair. Every fixture here uses
/// a multiple of eight except where the partial byte is the thing under test.
fn literal_run(count: usize, byte: u8) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut position = 0usize;
    let mut left = count;
    while left >= TOKENS_PER_CONTROL {
        out.push(0xFF);
        for _ in 0..TOKENS_PER_CONTROL {
            out.push(mix(byte, position));
            position += 1;
        }
        left -= TOKENS_PER_CONTROL;
    }
    if left != 0 {
        let mut control = 0u8;
        for bit in 0..left {
            control |= 1u8 << bit;
        }
        out.push(control);
        for _ in 0..left {
            out.push(mix(byte, position));
            position += 1;
        }
    }
    out
}

/// The output `literal_run` must produce: the same fold, computed from the
/// position rather than read back from a stream. Nothing here calls a decoder.
fn expected_literals(count: usize, byte: u8) -> Vec<u8> {
    (0..count).map(|p| mix(byte, p)).collect()
}

/// The output a back reference must produce: `count` bytes starting at
/// `source_position` of a literal run built by `literal_run`.
fn expected_from(count: usize, byte: u8, source_position: usize) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    for step in 0..count {
        out.push(mix(byte, source_position + step));
    }
    out
}

/// The bytes a back reference of `length` bytes from ring slot `source` must
/// produce, when the ring holds `ring_byte` in every slot and the write head
/// starts at [`WRITE_START`].
///
/// LZSS-003 fixes the write head at `RING_WRITE_START`, and every output byte
/// moves it on by one slot. LZSS-011 fixes the read-before-write order: a run
/// whose source trails the write head by less than its own length reads the
/// bytes it has just written and repeats them, which is what the distance below
/// decides.
///
/// A run whose source is the write head's own slot reads every slot before it
/// is written, so it sees the ring's own byte in each of them.
fn expected_ring_run<F>(source: usize, length: usize, ring_byte: F) -> Vec<u8>
where
    F: Fn(usize) -> u8,
{
    let distance = (WRITE_START + RING_SLOT_COUNT - source) % RING_SLOT_COUNT;
    let mut out: Vec<u8> = Vec::new();
    for step in 0..length {
        let byte = if distance == 0 || step < distance {
            // The slot still holds what the ring held before the run.
            let slot = (source + step) % RING_SLOT_COUNT;
            ring_byte(slot)
        } else {
            // The slot was written by this run, `distance` steps ago.
            out[step - distance]
        };
        out.push(byte);
    }
    out
}

/// Appends one back reference token as `0x00 b1 b2`.
fn with_token(mut stream: Vec<u8>, b1: u8, b2: u8) -> Vec<u8> {
    stream.push(0x00);
    stream.push(b1);
    stream.push(b2);
    stream
}

/// Encodes a back reference for an absolute ring slot and a run length, reading
/// the result back the way LZSS-008 states so the fixture is proved to encode
/// what it claims.
///
/// The bias is the literal `3`, not `MIN_MATCH`. A fixture is an input, and an
/// input computed from the constant under test would be graded with the code
/// under test; `MIN_MATCH` is asserted against `SHORTEST_RUN` in its own test
/// instead. The caller must pass a length in `3..=18`.
fn token_for(source: usize, length: usize) -> [u8; 3] {
    let b1 = to_u8(source & 0xFF);
    let b2 = to_u8(((source >> 8) & 0x0F) << 4 | (length - SHORTEST_RUN));
    assert_eq!(
        usize::from(b1) | (usize::from(b2 & 0xF0) << 4),
        source,
        "synthetic: the token decodes back to the absolute source slot"
    );
    assert_eq!(
        usize::from(b2 & 0x0F) + SHORTEST_RUN,
        length,
        "synthetic: the token decodes back to the run length"
    );
    [0x00, b1, b2]
}

/// The ring slot an output position lives in: `(position + 4078) mod 4096`.
///
/// This is the spec's mapping written out (LZSS-010). It is a helper and not a
/// claim: the self-check that used to live here walked the mapping back to the
/// position it came from, which says nothing about the decoder and can
/// therefore not fail. The callers grade the helper against hand computed
/// constants instead, and `synthetic_output_position_maps_to_ring_slot_position_plus_4078`
/// grades the mapping itself against the decoder, one slot at a time.
fn ring_index_of(position: usize) -> usize {
    (position + WRITE_START) % RING_SLOT_COUNT
}

/// The output position the literal byte at a stream index names, or `None` when
/// the byte at that index carries structure instead of a literal.
///
/// `literal_run` lays the stream out as one control byte followed by eight
/// literals, twelve times over, and the three byte token follows. So below the
/// token a byte is a literal exactly when its index is not a multiple of nine,
/// and its output position is the index less the control bytes in front of it.
fn literal_position_of(index: usize) -> Option<usize> {
    let is_control = index.is_multiple_of(CONTROL_BYTE_WIDTH);
    if index >= BIT_FLIP_LITERAL_BYTES || is_control {
        None
    } else {
        Some(index - (index / CONTROL_BYTE_WIDTH) - 1)
    }
}

/// Narrows a fixture value for a byte field.
fn to_u8(value: usize) -> u8 {
    u8::try_from(value).expect("synthetic fixture value fits in one byte")
}

/// Unwraps a decode that must succeed, panicking with the case name when it does
/// not. A helper rather than `expect` so the failure names the case instead of
/// the `Debug` shape of the error.
fn decoded_bytes(result: Result<Vec<u8>, LzssError>, case: &str) -> Vec<u8> {
    match result {
        Ok(bytes) => bytes,
        Err(error) => panic!("{case}: expected a decode, got {}", lzss_error_name(&error)),
    }
}

/// Unwraps a decode that must fail, panicking with the case name when it
/// succeeds. This is the "never a short buffer" half of the suite: a decoder
/// that returned `Ok` here has truncated, padded or defaulted.
fn decode_error(result: Result<Vec<u8>, LzssError>, case: &str) -> LzssError {
    match result {
        Ok(bytes) => panic!("{case}: expected an error, decoded {} byte(s)", bytes.len()),
        Err(error) => error,
    }
}

/// Name of an `LzssError` variant. The match is exhaustive with no wildcard arm,
/// so a new variant is a compile error here rather than a silently accepted
/// value.
fn lzss_error_name(error: &LzssError) -> &'static str {
    match error {
        LzssError::OutputOverrun { .. } => "OutputOverrun",
        LzssError::OutputShort { .. } => "OutputShort",
        LzssError::TruncatedMatch { .. } => "TruncatedMatch",
        LzssError::IndexOverflow { .. } => "IndexOverflow",
    }
}

/// True for the three failures the spec prescribes. `IndexOverflow` is excluded
/// on purpose: it is unreachable with the constants above, so a decode that
/// reports it has taken a path the spec says cannot exist.
fn is_prescribed_failure(error: &LzssError) -> bool {
    matches!(
        error,
        LzssError::OutputOverrun { .. }
            | LzssError::OutputShort { .. }
            | LzssError::TruncatedMatch { .. }
    )
}

// --- the worked vectors --------------------------------------------------------

/// claim: LZSS-018, LZSS-013, LZSS-005, LZSS-002
/// oracle: class 5 (hand-computed vector from the spec)
#[test]
fn synthetic_vector_00000f_decodes_to_eighteen_spaces() {
    // LZSS-018, recomputed here so the expected value does not come from a
    // decoder:
    //   control = 0x00        all eight bits clear
    //   b1 = 0x00, b2 = 0x0F
    //   source = 0x00 | ((0x0F & 0xF0) << 4) = 0x00 | 0x00 = 0
    //   length = (0x0F & 0x0F) + 3             = 15 + 3  = 18
    let b1 = VECTOR_00000F_INPUT[1];
    let b2 = VECTOR_00000F_INPUT[2];
    let source = usize::from(b1) | (usize::from(b2 & 0xF0) << 4);
    let length = usize::from(b2 & 0x0F) + SHORTEST_RUN;
    assert_eq!(
        source, BOUNDARY_INSIDE_SLOT,
        "LZSS-018: the source is inside the prefilled region"
    );
    assert_eq!(length, VECTOR_00000F_OUTPUT, "LZSS-018: the run length");
    assert_eq!(
        length, LONGEST_RUN,
        "LZSS-018: this vector happens to ask for a maximum length run"
    );
    assert_eq!(
        VECTOR_00000F_INPUT.len(),
        VECTOR_00000F_WIDTH,
        "LZSS-018: the stream is three bytes, one control byte and one pair"
    );

    let decoded = decoded_bytes(
        decode_into(&VECTOR_00000F_INPUT, VECTOR_00000F_OUTPUT),
        "synthetic_vector_00000f_decodes_to_eighteen_spaces",
    );
    assert_eq!(
        decoded.len(),
        VECTOR_00000F_OUTPUT,
        "LZSS-018: the decoded length"
    );
    assert_eq!(
        decoded,
        VECTOR_00000F_EXPECTED.to_vec(),
        "LZSS-018: eighteen spaces, because slot 0 is prefilled and never written"
    );

    // LZSS-013 in the same vector: the input ran out after three bytes, four
    // tokens short of the control byte's eight, and the block finished anyway.
    // That is how 52,229 of 61,070 real blocks end (LZSS-C022).
    let mut decoder = LzssDecoder::new();
    decode_block(&VECTOR_00000F_INPUT, &mut decoder, VECTOR_00000F_OUTPUT)
        .expect("synthetic: the mid control byte stop is not an error");
    assert_eq!(
        decoder.produced(),
        VECTOR_00000F_OUTPUT,
        "LZSS-013: the block ended on input exhaustion, not on the declared size"
    );
    assert_eq!(
        decoder.take_output(),
        VECTOR_00000F_EXPECTED.to_vec(),
        "LZSS-013: the same eighteen bytes through the block entry point"
    );
    assert_eq!(
        decoder.produced(),
        0,
        "LZSS-024: take_output() leaves the decoder empty"
    );
}

/// claim: LZSS-017, LZSS-008, LZSS-009, LZSS-010
/// oracle: class 5 (hand-computed vector from the spec)
#[test]
fn synthetic_vector_token_4e_36_at_output_5000_copies_from_output_4960() {
    // The token's arithmetic, recomputed by hand so the expected bytes do not
    // come from a decoder:
    //   write index at output 5000 = (5000 + 4078) mod 4096 = 9078 mod 4096
    //                               = 886
    //   the run copies 9 bytes, that is output position 4960, and 886 - 846
    //   equals 40 while 5000 - 4960 equals 40, which is the check that the source
    //   index is an absolute ring position and not a distance
    //   ring index of the source      = (4960 + 4078) mod 4096 = 9038 mod 4096
    //                               = 846 = 0x34E
    //   b1 = 846 & 0xFF                          = 0x4E
    //   b2 = ((846 >> 8) & 0x0F) << 4 | (9 - 3)  = (3 << 4) | 6 = 0x36
    let write_slot = ring_index_of(LZSS017_TARGET_POSITION);
    let source_slot = ring_index_of(LZSS017_SOURCE_POSITION);
    assert_eq!(write_slot, LZSS017_WRITE_SLOT, "LZSS-017: the write slot");
    assert_eq!(
        source_slot, LZSS017_SOURCE_SLOT,
        "LZSS-017: the source slot"
    );
    assert_eq!(source_slot, 0x34E, "LZSS-017: the source slot in hex");
    assert_eq!(
        write_slot - source_slot,
        LZSS017_DISTANCE,
        "LZSS-009: a ring distance and an output distance agree, because the \
         position to slot map is a shift"
    );

    let encoded = token_for(source_slot, LZSS017_RUN);
    assert_eq!(
        encoded[1], TOKEN_4E36[0],
        "LZSS-017: the first token byte is 0x4E"
    );
    assert_eq!(
        encoded[2], TOKEN_4E36[1],
        "LZSS-017: the second token byte is 0x36"
    );
    assert_eq!(
        usize::from(TOKEN_4E36[0]) | (usize::from(TOKEN_4E36[1] & 0xF0) << 4),
        LZSS017_SOURCE_SLOT,
        "LZSS-008: reading the disk token back gives the absolute source"
    );
    assert_eq!(
        usize::from(TOKEN_4E36[1] & 0x0F) + SHORTEST_RUN,
        LZSS017_RUN,
        "LZSS-008: reading the disk token back gives the run length"
    );

    let stream = with_token(
        literal_run(VECTOR_LONG_LITERALS, 0xABu8),
        TOKEN_4E36[0],
        TOKEN_4E36[1],
    );
    let decoded = decoded_bytes(
        decode_into(&stream, LZSS017_DECLARED),
        "synthetic_vector_token_4e_36_at_output_5000_copies_from_output_4960",
    );
    assert_eq!(
        decoded.len(),
        LZSS017_DECLARED,
        "LZSS-017: the produced length"
    );

    // The claim: the nine bytes at 5000 are the nine bytes at 4960.
    let written = decoded[LZSS017_TARGET_POSITION..LZSS017_DECLARED].to_vec();
    let named = decoded[LZSS017_SOURCE_POSITION..LZSS017_SOURCE_POSITION + LZSS017_RUN].to_vec();
    assert_eq!(written.len(), LZSS017_RUN, "LZSS-017: the run length");
    assert_eq!(
        written, named,
        "LZSS-017: the run copied the nine bytes forty back"
    );

    // And both, against the hand arithmetic rather than against each other.
    let mut want = expected_literals(VECTOR_LONG_LITERALS, 0xABu8);
    want.extend_from_slice(&expected_from(LZSS017_RUN, 0xABu8, LZSS017_SOURCE_POSITION));
    assert_eq!(
        decoded, want,
        "LZSS-017: every literal and the nine byte run, computed from the spec"
    );
}

/// claim: LZSS-019, LZSS-011, LZSS-008, LZSS-009
/// oracle: class 5 (hand-computed vector from the spec)
#[test]
fn synthetic_vector_token_75_3f_at_write_index_886_repeats_the_previous_byte() {
    // LZSS-019 arithmetic:
    //   source = 0x75 | ((0x3F & 0xF0) << 4) = 0x75 | 0x300 = 885
    //   length = (0x3F & 0x0F) + 3             = 15 + 3    = 18
    //   886 - 885 = 1, so the source is exactly one byte behind the write head
    let write_slot = ring_index_of(LZSS017_TARGET_POSITION);
    let source = usize::from(TOKEN_753F[0]) | (usize::from(TOKEN_753F[1] & 0xF0) << 4);
    let length = usize::from(TOKEN_753F[1] & 0x0F) + SHORTEST_RUN;
    assert_eq!(write_slot, LZSS017_WRITE_SLOT, "LZSS-019: the write slot");
    assert_eq!(source, LZSS019_SOURCE_SLOT, "LZSS-019: the absolute source");
    assert_eq!(length, LZSS019_RUN, "LZSS-019: the run length");
    assert_eq!(
        write_slot - source,
        LZSS019_DISTANCE,
        "LZSS-019: the source is exactly one byte behind the head"
    );
    assert_eq!(
        length, LONGEST_RUN,
        "LZSS-019: this vector also asks for a maximum length run"
    );

    let fill = 0x5Au8;
    let stream = with_token(
        literal_run(VECTOR_LONG_LITERALS, fill),
        TOKEN_753F[0],
        TOKEN_753F[1],
    );
    let decoded = decoded_bytes(
        decode_into(&stream, LZSS019_DECLARED),
        "synthetic_vector_token_75_3f_at_write_index_886_repeats_the_previous_byte",
    );
    assert_eq!(
        decoded.len(),
        LZSS019_DECLARED,
        "LZSS-019: the produced length"
    );

    let head = decoded[LZSS019_HEAD_POSITION];
    assert_eq!(
        head,
        mix(fill, LZSS019_HEAD_POSITION),
        "LZSS-019: the byte about to be repeated is the literal before the token"
    );

    // LZSS-011 read before write: every step reads the byte the previous step
    // wrote, so all eighteen bytes are the one just behind the head.
    let tail = decoded[LZSS017_TARGET_POSITION..LZSS019_DECLARED].to_vec();
    assert_eq!(
        tail,
        vec![head; LZSS019_RUN],
        "LZSS-019: eighteen repeats of the byte just written. A decoder that \
         snapshots the run first emits that byte once and then whatever the ring \
         held before, which is the right length and the wrong bytes."
    );
    let mut want = expected_literals(VECTOR_LONG_LITERALS, fill);
    for _ in 0..LZSS019_RUN {
        want.push(mix(fill, LZSS019_HEAD_POSITION));
    }
    assert_eq!(
        decoded, want,
        "LZSS-019: every literal and the repeated run, computed from the spec"
    );
}

// --- the ring ------------------------------------------------------------------

/// claim: LZSS-001, LZSS-002, LZSS-003, LZSS-008
/// oracle: class 6 (algebraic law: the exported constants satisfy the identities the spec states) and class 5 (hand-computed values from the spec)
#[test]
fn synthetic_ring_constants_match_the_specified_values() {
    assert_eq!(
        RING_SIZE, RING_SLOT_COUNT,
        "LZSS-001: the ring is 4096 bytes"
    );
    assert_eq!(
        RING_FILL, PREFILL_BYTE,
        "LZSS-002: the prefill byte is a space"
    );
    assert_eq!(
        RING_WRITE_START, WRITE_START,
        "LZSS-003: the write head starts at 4078"
    );
    assert_eq!(MIN_MATCH, SHORTEST_RUN, "LZSS-008: the shortest run is 3");
    assert_eq!(MAX_MATCH, LONGEST_RUN, "LZSS-008: the longest run is 18");
    assert_eq!(
        RING_SIZE - RING_WRITE_START,
        LOOKAHEAD_SLOTS,
        "LZSS-C005: 4096 - 4078 is 18, the standard LZSS lookahead"
    );
    assert_eq!(
        RING_SIZE - RING_WRITE_START,
        MAX_MATCH,
        "LZSS-C005: the lookahead equals the longest run, which is why the head \
         starts there rather than at 0 or 4095"
    );
    assert_eq!(
        PREFILLED_LAST_SLOT + 1,
        RING_WRITE_START,
        "LZSS-002 and LZSS-003: the prefill boundary and the write start are the \
         same index"
    );
    assert_eq!(
        LONGEST_RUN - SHORTEST_RUN,
        LENGTH_BIAS,
        "LZSS-008: a four bit length bias spans exactly the 3 to 18 range"
    );
    assert_eq!(
        POSITION_HIGH_MASK, 0xF0,
        "LZSS-009: the source position's top nibble"
    );
    assert_eq!(LENGTH_BIAS, 0x0F, "LZSS-008: the run length's bias nibble");
    assert_eq!(
        RING_SIZE,
        LARGEST_SOURCE + 1,
        "LZSS-012: the largest addressable source is one below the ring"
    );

    // Observable behaviour, not only the constants. A fresh ring reads the
    // prefill back at an addressable slot inside the prefilled region.
    let inside = token_for(BOUNDARY_INSIDE_SLOT, BOUNDARY_RUN);
    let decoded = decoded_bytes(
        decode_into(&inside, BOUNDARY_DECLARED),
        "synthetic_ring_constants_match_the_specified_values: prefilled region",
    );
    assert_eq!(
        decoded,
        vec![PREFILL_BYTE; BOUNDARY_RUN],
        "LZSS-002: an unwritten slot in the prefilled region reads back the prefill"
    );

    // And the run lengths the two exported constants name are the two lengths the
    // decoder produces, which is what makes them the same numbers.
    let shortest = token_for(LENGTH_FIXTURE_SLOT, MIN_RUN_FIXTURE);
    let longest = token_for(LENGTH_FIXTURE_SLOT, MAX_RUN_FIXTURE);
    assert_eq!(
        shortest[2] & 0x0F,
        0x00,
        "LZSS-008: the shortest run encodes a bias of zero"
    );
    assert_eq!(
        longest[2] & 0x0F,
        u8::try_from(LENGTH_BIAS).expect("synthetic: 0x0F fits a byte"),
        "LZSS-008: the longest run encodes the full bias"
    );
}

/// claim: LZSS-002, LZSS-012
/// oracle: class 5 (hand-computed vector from the spec: slot 4077 reads 0x20 and slot 4078 reads 0x00)
#[test]
fn synthetic_fresh_ring_prefills_0_to_4077_and_zeroes_4078_to_4095() {
    // Both halves of LZSS-002 are load bearing. A decoder that prefills the whole
    // ring with 0x20, or the whole ring with 0x00, decodes the corpus to
    // plausible bytes that are wrong (LZSS-C070 through LZSS-C073).
    assert_eq!(
        BOUNDARY_PREFILLED_SLOT, 4077,
        "LZSS-C003: the last prefilled slot"
    );
    assert_eq!(BOUNDARY_ZERO_SLOT, 4078, "LZSS-002: the first zero slot");

    let prefilled = token_for(BOUNDARY_PREFILLED_SLOT, BOUNDARY_RUN);
    assert_eq!(
        prefilled[1], 0xED,
        "synthetic: 4077 is 0xFED, so the first token byte is 0xED"
    );
    assert_eq!(
        prefilled[2], 0xF0,
        "synthetic: the high nibble of 4077 is 0xF and the bias is zero"
    );
    let decoded = decoded_bytes(
        decode_into(&prefilled, BOUNDARY_DECLARED),
        "synthetic_fresh_ring_prefills_0_to_4077_and_zeroes_4078_to_4095: 4077",
    );
    assert_eq!(
        decoded,
        vec![PREFILL_BYTE; BOUNDARY_RUN],
        "LZSS-002: the last prefilled slot holds 0x20"
    );

    let zeroed = token_for(BOUNDARY_ZERO_SLOT, BOUNDARY_RUN);
    assert_eq!(
        zeroed[1], 0xEE,
        "synthetic: 4078 is 0xFEE, so the first token byte is 0xEE"
    );
    assert_eq!(
        zeroed[2], 0xF0,
        "synthetic: the high nibble of 4078 is 0xF and the bias is zero"
    );
    let decoded = decoded_bytes(
        decode_into(&zeroed, BOUNDARY_DECLARED),
        "synthetic_fresh_ring_prefills_0_to_4077_and_zeroes_4078_to_4095: 4078",
    );
    assert_eq!(
        decoded,
        vec![ZERO_BYTE; BOUNDARY_RUN],
        "LZSS-002: the first zero slot holds 0x00, not the prefill. A decoder that \
         prefills all 4096 bytes with 0x20 gets this wrong, and so does one that \
         prefills them all with 0x00 at slot 0."
    );

    let top = token_for(BOUNDARY_TOP_SLOT, BOUNDARY_RUN);
    assert_eq!(
        top[1], 0xFF,
        "synthetic: 4095 is 0xFFF, so the first token byte is 0xFF"
    );
    assert_eq!(top[2], 0xF0, "synthetic: the high nibble of 4095 is 0xF");
    let decoded = decoded_bytes(
        decode_into(&top, BOUNDARY_DECLARED),
        "synthetic_fresh_ring_prefills_0_to_4077_and_zeroes_4078_to_4095: 4095",
    );
    // The source cursor advances and wraps at RING_SIZE (LZSS-011), so a run of
    // three from slot 4095 reads slots 4095, 0, 1. Slot 4095 is in the zero
    // region, so it is 0x00; slots 0 and 1 are in the prefilled region and the
    // write head starts at 4078, so nothing has written them and they are 0x20.
    //
    // **This is not `[0x00, 0x00, 0x00]`, and it must not be "fixed" back to
    // that.** The three-zero reading only follows if the source cursor runs past
    // 4095 into slots 4096..=4115, which requires the 4116 byte ring the original
    // binary declares (LZSS-C012) and never reaches. The two models differ
    // whenever `source + length - 1 >= 4096`, which 222,965 of the corpus's
    // match tokens do. Measured over the 31 entries that exist both in the
    // archive and loose on disk: a 4096 slot ring whose source wraps at 4096 is
    // byte-exact for 28 of 31, and an 4116 slot ring whose source runs into the
    // never-written tail is byte-exact for 0 of 31. (The one miss under the
    // correct model is `ART\INTRFACE\MAINMENU.FRM`, a High Resolution Patch
    // override whose loose copy legitimately differs from the archive.)
    assert_eq!(
        decoded,
        vec![ZERO_BYTE, PREFILL_BYTE, PREFILL_BYTE],
        "LZSS-002 and LZSS-011: slot 4095 holds 0x00 in the zero region, and the \
         source cursor wraps to slots 0 and 1, which are still prefilled"
    );
}

/// claim: LZSS-010, LZSS-003, LZSS-009, LZSS-012
/// oracle: class 6 (algebraic law: the position to slot map is a shift by exactly 4078, and it is a bijection over one turn)
#[test]
fn synthetic_output_position_maps_to_ring_slot_position_plus_4078() {
    // A second, independent anchor. A token names an absolute ring slot, so the
    // run copies the bytes forty back only when the shift is exactly 4078: this
    // token names slot 942, which is the ring slot of output 960 for no other
    // shift.
    assert_eq!(
        (ANCHOR_TARGET_POSITION + WRITE_START) % RING_SLOT_COUNT,
        ANCHOR_WRITE_SLOT,
        "LZSS-010: the write head at output 1000"
    );
    assert_eq!(
        (ANCHOR_SOURCE_POSITION + WRITE_START) % RING_SLOT_COUNT,
        ANCHOR_SOURCE_SLOT,
        "LZSS-010: output 960 lives at ring slot 942"
    );
    assert_eq!(
        ANCHOR_TARGET_POSITION - ANCHOR_SOURCE_POSITION,
        LZSS017_DISTANCE,
        "LZSS-010: the anchor run copies forty bytes back"
    );

    let encoded = token_for(ANCHOR_SOURCE_SLOT, LZSS017_RUN);
    assert_eq!(
        encoded[1], ANCHOR_TOKEN_B1,
        "synthetic: 942 is 0x3AE, so the first token byte is 0xAE"
    );
    assert_eq!(
        encoded[2], ANCHOR_TOKEN_B2,
        "synthetic: the high nibble of 942 is 3 and the bias is 6"
    );

    let fill = 0x3Cu8;
    let stream = with_token(
        literal_run(ANCHOR_TARGET_POSITION, fill),
        ANCHOR_TOKEN_B1,
        ANCHOR_TOKEN_B2,
    );
    let decoded = decoded_bytes(
        decode_into(&stream, ANCHOR_DECLARED),
        "synthetic_output_position_maps_to_ring_slot_position_plus_4078",
    );
    assert_eq!(
        decoded.len(),
        ANCHOR_DECLARED,
        "LZSS-010: the produced length"
    );
    assert_eq!(
        decoded[ANCHOR_TARGET_POSITION..ANCHOR_DECLARED],
        decoded[ANCHOR_SOURCE_POSITION..ANCHOR_SOURCE_POSITION + LZSS017_RUN],
        "LZSS-010: the run copied the bytes forty back, so the shift is 4078"
    );
    let mut want = expected_literals(ANCHOR_TARGET_POSITION, fill);
    want.extend_from_slice(&expected_from(LZSS017_RUN, fill, ANCHOR_SOURCE_POSITION));
    assert_eq!(
        decoded, want,
        "LZSS-010: the whole anchor stream, computed from the shift"
    );

    // The wrap anchors, and the bijection over one whole turn.
    let mut anchor_positions: Vec<usize> = Vec::with_capacity(RING_WRAP_ANCHOR_COUNT);
    for (position, slot) in RING_WRAP_ANCHORS {
        assert_eq!(
            ring_index_of(position),
            slot,
            "LZSS-010: the ring slot of this output position"
        );
        anchor_positions.push(position);
    }
    // The five anchors are five different positions, so the wrap is covered at
    // five points and not at one of them five times. The count is of the
    // positions, so two anchors naming the same one would fail here.
    anchor_positions.sort_unstable();
    anchor_positions.dedup();
    assert_eq!(
        anchor_positions.len(),
        RING_WRAP_ANCHOR_COUNT,
        "the five wrap anchors are five different output positions"
    );

    // The bijection over one whole turn, read back through the decoder. Every
    // slot of the ring is named by a back reference of its own, in a stream that
    // fills the ring with a whole turn of non-uniform literals first, so the
    // bytes the decoder reads out of a slot say which output position that
    // slot holds. A decoder that mapped the output positions onto the ring
    // slots by any other shift answers with different bytes, and one that
    // visited a slot twice answers with different bytes as well.
    //
    // One decode per slot, because a run writes into the ring as it reads out
    // of it: a single stream could only read the slots its own runs had not
    // reached yet.
    let fill = 0x3Cu8;
    let literals = literal_run(RING_TURN, fill);
    // The byte each slot gave back, collected so the coverage claim is a
    // comparison and not a tally of loop iterations: the sweep read the byte
    // of every one of the 4096 positions of the turn, and the multiset of
    // those bytes is the multiset the spec's fold gives.
    let mut read_back: Vec<u8> = Vec::with_capacity(RING_TURN);
    for slot in 0..RING_SLOT_COUNT {
        let token = token_for(slot, TURN_PROBE_RUN);
        let mut stream = literals.clone();
        stream.push(token[0]);
        stream.push(token[1]);
        stream.push(token[2]);
        let decoded = decoded_bytes(
            decode_into(&stream, RING_TURN + TURN_PROBE_RUN),
            "synthetic_output_position_maps_to_ring_slot_position_plus_4078: the slot",
        );
        assert_eq!(
            decoded.len(),
            RING_TURN + TURN_PROBE_RUN,
            "LZSS-010: the probe produced the turn and the run it read"
        );
        assert_eq!(
            decoded[RING_TURN..],
            expected_ring_run(slot, TURN_PROBE_RUN, |read_slot| {
                mix(fill, (read_slot + RING_SHIFT_BACK) % RING_SLOT_COUNT)
            }),
            "LZSS-010: the slot holds the output position the shift gives it"
        );
        read_back.push(decoded[RING_TURN]);
    }
    let mut observed = read_back.clone();
    observed.sort_unstable();
    let mut expected: Vec<u8> = (0..RING_TURN).map(|position| mix(fill, position)).collect();
    expected.sort_unstable();
    assert_eq!(
        observed, expected,
        "LZSS-010: every slot of the ring was read back, and the bytes read are \
         the bytes of the whole turn"
    );
}

/// claim: LZSS-008, LZSS-005
/// oracle: class 5 (hand-computed vector from the spec: a bias of 0 gives 3 bytes and a bias of 0x0F gives 18)
#[test]
fn synthetic_match_length_range_is_exactly_three_to_eighteen() {
    // Every one of the sixteen biases the second byte of a back reference can
    // carry, decoded on its own. LZSS-008 says the corpus requests lengths
    // across the whole range and both ends are reachable, so the decoder's
    // bias mapping is exercised at all sixteen points and not only at the two
    // ends of the range.
    let mut runs = 0usize;
    for bias in 0..=LENGTH_BIAS {
        let length = bias + SHORTEST_RUN;
        let token = token_for(LENGTH_FIXTURE_SLOT, length);
        let decoded = decoded_bytes(
            decode_into(&token, length),
            "synthetic_match_length_range_is_exactly_three_to_eighteen",
        );
        assert_eq!(
            decoded.len(),
            length,
            "LZSS-008: the run length this bias encodes"
        );
        assert_eq!(
            decoded,
            vec![PREFILL_BYTE; length],
            "LZSS-002: the slots the run reads are all in the prefilled region, \
             so every byte is the prefill"
        );
        runs += 1;
    }
    assert_eq!(
        runs,
        LENGTH_BIAS + 1,
        "every bias the second byte can carry was decoded"
    );

    assert_eq!(
        MIN_MATCH, MIN_RUN_FIXTURE,
        "LZSS-008: the exported minimum is the length actually produced"
    );
    assert_eq!(
        MAX_MATCH, MAX_RUN_FIXTURE,
        "LZSS-008: the exported maximum is the length actually produced"
    );
    assert_eq!(
        MAX_RUN_FIXTURE - MIN_RUN_FIXTURE,
        LENGTH_BIAS,
        "LZSS-008: the range spans the whole four bit bias and no more"
    );
    assert_eq!(
        TOKENS_PER_CONTROL, 8,
        "LZSS-005: one control byte carries eight tokens, bit 0 first"
    );
}

/// claim: LZSS-011
/// oracle: class 5 (hand-computed vector from the spec: a run whose distance is smaller than its own length replicates itself)
#[test]
fn synthetic_overlapping_run_replicates_its_own_output_byte_for_byte() {
    // The write head after 96 literals is ring slot 78. A run of 18 starting at
    // ring slot 75 is three behind it, so from its fourth step on it reads bytes
    // it wrote itself. Read before write turns that into a period three
    // repetition of the three bytes in front of the head; a decoder that reads
    // the whole run into a buffer first produces the eighteen bytes from slot 75
    // upward, which is a different answer with the same length.
    let write_slot = ring_index_of(OVERLAP_LITERALS);
    assert_eq!(
        write_slot, OVERLAP_WRITE_SLOT,
        "synthetic: the write head after 96 literals"
    );
    assert_eq!(
        (write_slot + RING_SLOT_COUNT - OVERLAP_SOURCE_SLOT) % RING_SLOT_COUNT,
        OVERLAP_DISTANCE,
        "synthetic: the run's source is three behind the head"
    );
    assert_ne!(
        OVERLAP_RUN, OVERLAP_DISTANCE,
        "LZSS-011: the run is longer than its distance, so it reads its own output"
    );

    let encoded = token_for(OVERLAP_SOURCE_SLOT, OVERLAP_RUN);
    assert_eq!(
        encoded[1], OVERLAP_TOKEN_B1,
        "synthetic: 75 is 0x04B, so the first token byte is 0x4B"
    );
    assert_eq!(
        encoded[2], OVERLAP_TOKEN_B2,
        "synthetic: the high nibble of 75 is 0 and the bias is 15"
    );

    let fill = CANARY_FILL;
    let stream = with_token(literal_run(OVERLAP_LITERALS, fill), encoded[1], encoded[2]);
    let decoded = decoded_bytes(
        decode_into(&stream, OVERLAP_DECLARED),
        "synthetic_overlapping_run_replicates_its_own_output_byte_for_byte",
    );
    assert_eq!(
        decoded.len(),
        OVERLAP_DECLARED,
        "LZSS-011: the produced length"
    );

    let head = decoded[OVERLAP_SEED_POSITION..OVERLAP_LITERALS].to_vec();
    assert_eq!(
        head.len(),
        OVERLAP_DISTANCE,
        "LZSS-011: the three bytes in front of the head"
    );
    assert_eq!(
        head,
        expected_from(OVERLAP_DISTANCE, fill, OVERLAP_SEED_POSITION),
        "LZSS-011: the three bytes in front of the head, computed from the fold"
    );

    let mut want = Vec::new();
    for _ in 0..OVERLAP_REPEATS {
        want.extend_from_slice(&head);
    }
    assert_eq!(
        want.len(),
        OVERLAP_RUN,
        "LZSS-011: the repetition covers the whole run"
    );
    assert_eq!(
        decoded[OVERLAP_LITERALS..OVERLAP_DECLARED].to_vec(),
        want,
        "LZSS-011: read before write makes the run repeat itself byte for byte"
    );

    // What a snapshotting decoder produces instead, recorded rather than assumed:
    // the eighteen bytes from the source slot upward, without the replication.
    let snapshot = expected_from(OVERLAP_RUN, fill, OVERLAP_SEED_POSITION);
    assert_ne!(
        decoded[OVERLAP_LITERALS..OVERLAP_DECLARED].to_vec(),
        snapshot,
        "LZSS-011: a decoder that reads the run before writing it gives different bytes"
    );
    assert_eq!(
        snapshot.len(),
        want.len(),
        "LZSS-011: both answers are the same length, so only the bytes tell them apart"
    );
}

/// claim: LZSS-009, LZSS-008
/// oracle: class 5 (hand-computed vector from the spec: 0x75 0x3F at write index 886 is one byte back, not 885)
#[test]
fn synthetic_source_index_is_an_absolute_position_not_a_distance() {
    let fill = 0x11u8;

    // Two tokens naming the same absolute ring slot 72, at two different write
    // heads. A distance reading would copy 6 bytes back in one and 102 in the
    // other; an absolute reading copies the same three bytes in both.
    let encoded = token_for(ABSOLUTE_FIXTURE_SLOT, ABSOLUTE_RUN);
    assert_eq!(
        encoded[1], 0x48,
        "synthetic: 72 is 0x048, so the first token byte is 0x48"
    );
    assert_eq!(
        encoded[2], 0x00,
        "synthetic: the high nibble of 72 is 0 and the bias is 0"
    );
    assert_eq!(
        (ring_index_of(ABSOLUTE_SHORT_LITERALS) + RING_SLOT_COUNT - ABSOLUTE_FIXTURE_SLOT)
            % RING_SLOT_COUNT,
        ABSOLUTE_FIRST_DISTANCE,
        "synthetic: the first token is six behind its write head"
    );
    assert_eq!(
        (ring_index_of(ABSOLUTE_LONG_LITERALS) + RING_SLOT_COUNT - ABSOLUTE_FIXTURE_SLOT)
            % RING_SLOT_COUNT,
        ABSOLUTE_SECOND_DISTANCE,
        "synthetic: the second token is 102 behind its write head"
    );

    let short_stream = with_token(
        literal_run(ABSOLUTE_SHORT_LITERALS, fill),
        encoded[1],
        encoded[2],
    );
    let long_stream = with_token(
        literal_run(ABSOLUTE_LONG_LITERALS, fill),
        encoded[1],
        encoded[2],
    );
    let short_decoded = decoded_bytes(
        decode_into(&short_stream, ABSOLUTE_FIRST_DECLARED),
        "synthetic_source_index_is_an_absolute_position_not_a_distance: 96",
    );
    let long_decoded = decoded_bytes(
        decode_into(&long_stream, ABSOLUTE_SECOND_DECLARED),
        "synthetic_source_index_is_an_absolute_position_not_a_distance: 192",
    );

    let first = short_decoded[ABSOLUTE_SHORT_LITERALS..ABSOLUTE_FIRST_DECLARED].to_vec();
    let second = long_decoded[ABSOLUTE_LONG_LITERALS..ABSOLUTE_SECOND_DECLARED].to_vec();
    assert_eq!(
        first.len(),
        ABSOLUTE_RUN,
        "LZSS-009: the first run's length"
    );
    assert_eq!(
        second.len(),
        ABSOLUTE_RUN,
        "LZSS-009: the second run's length"
    );
    assert_eq!(
        first, second,
        "LZSS-009: the same absolute ring index copies the same bytes whatever the \
         write head is"
    );
    assert_eq!(
        first,
        expected_from(ABSOLUTE_RUN, fill, ABSOLUTE_FIRST_SOURCE_POSITION),
        "LZSS-009: the first run copied the bytes it named"
    );
    assert_eq!(
        second,
        expected_from(ABSOLUTE_RUN, fill, ABSOLUTE_SECOND_SOURCE_POSITION),
        "LZSS-009: the second run copied the same bytes from its own position"
    );

    // Two tokens encoding the same distance of 100 from two different absolute
    // positions. A distance reading would make these identical; an absolute
    // reading makes them different.
    let first_encoded = token_for(DISTANCE_FIRST_SLOT, ABSOLUTE_RUN);
    let second_encoded = token_for(DISTANCE_SECOND_SLOT, ABSOLUTE_RUN);
    assert_eq!(first_encoded[1], 0xAA, "synthetic: 170 is 0x0AA");
    assert_eq!(
        first_encoded[2], 0x00,
        "synthetic: the high nibble of 170 is 0"
    );
    assert_eq!(second_encoded[1], 0x12, "synthetic: 274 is 0x112");
    assert_eq!(
        second_encoded[2], 0x10,
        "synthetic: the high nibble of 274 is 1"
    );
    assert_eq!(
        (ring_index_of(DISTANCE_FIRST_LITERALS) + RING_SLOT_COUNT - DISTANCE_FIRST_SLOT)
            % RING_SLOT_COUNT,
        FIXTURE_DISTANCE,
        "synthetic: the third token is 100 behind its write head"
    );
    assert_eq!(
        (ring_index_of(DISTANCE_SECOND_LITERALS) + RING_SLOT_COUNT - DISTANCE_SECOND_SLOT)
            % RING_SLOT_COUNT,
        FIXTURE_DISTANCE,
        "synthetic: the fourth token is also 100 behind its write head"
    );

    let first_stream = with_token(
        literal_run(DISTANCE_FIRST_LITERALS, fill),
        first_encoded[1],
        first_encoded[2],
    );
    let second_stream = with_token(
        literal_run(DISTANCE_SECOND_LITERALS, fill),
        second_encoded[1],
        second_encoded[2],
    );
    let first_decoded = decoded_bytes(
        decode_into(&first_stream, DISTANCE_FIRST_DECLARED),
        "synthetic_source_index_is_an_absolute_position_not_a_distance: 288",
    );
    let second_decoded = decoded_bytes(
        decode_into(&second_stream, DISTANCE_SECOND_DECLARED),
        "synthetic_source_index_is_an_absolute_position_not_a_distance: 392",
    );
    let same_distance_first =
        first_decoded[DISTANCE_FIRST_LITERALS..DISTANCE_FIRST_DECLARED].to_vec();
    let same_distance_second =
        second_decoded[DISTANCE_SECOND_LITERALS..DISTANCE_SECOND_DECLARED].to_vec();
    assert_eq!(
        same_distance_first.len(),
        ABSOLUTE_RUN,
        "LZSS-009: the third run's length"
    );
    assert_eq!(
        same_distance_second.len(),
        ABSOLUTE_RUN,
        "LZSS-009: the fourth run's length"
    );
    assert_ne!(
        same_distance_first, same_distance_second,
        "LZSS-009: the same distance from different absolute positions copies \
         different bytes, so the source index is not a distance"
    );
    assert_eq!(
        same_distance_first,
        expected_from(
            ABSOLUTE_RUN,
            fill,
            DISTANCE_FIRST_LITERALS - FIXTURE_DISTANCE
        ),
        "LZSS-009: the third run copied 100 bytes back"
    );
    assert_eq!(
        same_distance_second,
        expected_from(
            ABSOLUTE_RUN,
            fill,
            DISTANCE_SECOND_LITERALS - FIXTURE_DISTANCE
        ),
        "LZSS-009: the fourth run copied 100 bytes back"
    );
}

// --- termination and length ----------------------------------------------------

/// claim: LZSS-013, LZSS-005, LZSS-014
/// oracle: class 5 (hand-computed vector from the spec: a stream that ends mid control byte decodes what it has)
#[test]
fn synthetic_input_exhaustion_mid_control_byte_is_not_an_error() {
    // A stream whose last byte is a control byte with nothing behind it. The
    // input cursor ends the block, never the produced length, so all eight of
    // its tokens are discarded. This is how 52,229 of 61,070 real blocks end
    // (LZSS-C022), and a decoder that insists on eight tokens per control byte
    // errors on every one of them.
    let mut stream = literal_run(MID_CONTROL_LITERALS, MID_CONTROL_FILL);
    stream.extend_from_slice(&MID_CONTROL_TRAILING);
    assert_eq!(
        stream.len(),
        (MID_CONTROL_LITERALS / TOKENS_PER_CONTROL) * CONTROL_BYTE_WIDTH + 1,
        "synthetic: forty literals plus one trailing control byte"
    );
    let expected = expected_literals(MID_CONTROL_LITERALS, MID_CONTROL_FILL);
    assert_eq!(
        expected.len(),
        MID_CONTROL_OUTPUT,
        "synthetic: forty literals"
    );

    let decoded = decoded_bytes(
        decode_into(&stream, MID_CONTROL_OUTPUT),
        "synthetic_input_exhaustion_mid_control_byte_is_not_an_error: eight missing",
    );
    assert_eq!(
        decoded.len(),
        MID_CONTROL_OUTPUT,
        "LZSS-013: the trailing control byte has no tokens behind it and is discarded"
    );
    assert_eq!(
        decoded, expected,
        "LZSS-013: the forty literals decoded, and nothing was padded to reach the size"
    );

    // The same shape with seven of the eight tokens missing.
    let mut one_token = literal_run(ONE_TOKEN_LITERALS, ONE_TOKEN_FILL);
    one_token.extend_from_slice(&ONE_TOKEN_CONTROL);
    one_token.extend_from_slice(&ONE_TOKEN_LITERAL);
    let mut want = expected_literals(ONE_TOKEN_LITERALS, ONE_TOKEN_FILL);
    want.push(ONE_TOKEN_LITERAL[0]);
    let decoded = decoded_bytes(
        decode_into(&one_token, ONE_TOKEN_OUTPUT),
        "synthetic_input_exhaustion_mid_control_byte_is_not_an_error: seven missing",
    );
    assert_eq!(
        decoded.len(),
        ONE_TOKEN_OUTPUT,
        "LZSS-013: the one token that had input behind it decoded"
    );
    assert_eq!(
        decoded, want,
        "LZSS-013: the other seven tokens of the final control byte were discarded"
    );

    // The input cursor, not the output length, is what ended the block: one more
    // byte turns the same prefix into a back reference with a single byte to
    // read. A decoder that stopped on the produced length would not have noticed
    // the extra byte at all, and would have returned the same forty bytes.
    let mut one_more = stream.clone();
    one_more.extend_from_slice(&MID_PAIR_TAIL);
    match decode_error(
        decode_into(&one_more, MID_CONTROL_OUTPUT),
        "synthetic_input_exhaustion_mid_control_byte_is_not_an_error: one byte more",
    ) {
        LzssError::TruncatedMatch { produced, .. } => assert_eq!(
            produced, MID_CONTROL_OUTPUT,
            "LZSS-013: the extra byte became the first byte of a pair, so the \
             block did read past its forty bytes and refused"
        ),
        other => panic!(
            "synthetic: expected TruncatedMatch, got {}",
            lzss_error_name(&other)
        ),
    }
}

/// claim: LZSS-014
/// oracle: class 5 (hand-computed vector from the spec: a token that would write past the declared size is an error, not a truncated output)
#[test]
fn synthetic_output_would_exceed_the_declared_size_is_an_error() {
    let literals = literal_run(OVERRUN_TOKEN_LITERALS, 0x5Au8);
    let stream = with_token(literals, TOKEN_4E36[0], TOKEN_4E36[1]);

    // The literals alone fill the declared size exactly, so the nine byte token
    // has nowhere to go. Whatever granularity the check uses, the failure is
    // detected at the declared size with nothing written past it.
    let overrun = decode_error(
        decode_into(&stream, OVERRUN_TOKEN_DECLARED),
        "synthetic_output_would_exceed_the_declared_size_is_an_error",
    );
    match overrun {
        LzssError::OutputOverrun {
            declared_out,
            produced,
            token_len,
            at,
        } => {
            assert_eq!(
                declared_out, OVERRUN_TOKEN_DECLARED,
                "LZSS-014: the declared size the overrun was measured against"
            );
            assert_eq!(
                produced, OVERRUN_TOKEN_DECLARED,
                "LZSS-014: the literals filled the declared size and the token \
                 wrote none of its nine bytes. LZSS-014 says do not write the \
                 token, not do not write its ninth byte."
            );
            assert_eq!(
                at, OVERRUN_TOKEN_AT_OFFSET,
                "LZSS-014: the error names the input offset the offending token \
                 started at"
            );
            assert_eq!(
                token_len, OVERRUN_TOKEN_RUN,
                "LZSS-014: the error names the length the token would have written"
            );
            // `docs/spec/lzss.md` "## Error text" fixes the exact wording of
            // this refusal, so the literal below is that wording: the message
            // names the offset, the declared size and the produced count.
            assert_eq!(
                format!("{}", overrun),
                format!(
                    "fo1-lzss: the token at input offset {OVERRUN_TOKEN_AT_OFFSET} would \
                     write {token_len} byte(s) past the declared output size \
                     {declared_out} ({produced} produced)"
                ),
                "LZSS-014: the refusal names the offset, the declared size and the \
                 produced count"
            );
        }
        other => panic!(
            "synthetic: expected OutputOverrun, got {}",
            lzss_error_name(&other)
        ),
    }

    // One run more of room and the very same bytes decode, which is the control:
    // the error came from the declared size and not from the input.
    let decoded = decoded_bytes(
        decode_into(&stream, OVERRUN_TOKEN_DECLARED + OVERRUN_TOKEN_RUN),
        "synthetic_output_would_exceed_the_declared_size_is_an_error: one run more",
    );
    assert_eq!(
        decoded.len(),
        OVERRUN_TOKEN_DECLARED + OVERRUN_TOKEN_RUN,
        "LZSS-014: with the declared size one run larger the same bytes decode"
    );
    // The control case is a real decode and not an empty buffer. Ninety six
    // literals write ring slots 4078 through 4095 and 0 through 77, so the nine
    // slots the token names, 846 through 854, are all inside the prefilled region
    // and none of them has been written.
    assert_eq!(
        decoded[OVERRUN_TOKEN_LITERALS..].to_vec(),
        vec![PREFILL_BYTE; OVERRUN_TOKEN_RUN],
        "LZSS-014: with room to write, the same token copies nine prefilled bytes, \
         so the earlier error was the declared size and nothing else"
    );
}

/// claim: LZSS-014
/// oracle: class 5 (hand-computed vector from the spec: input that ends before the declared size is an error, never a padded buffer)
#[test]
fn synthetic_input_ending_before_the_declared_size_is_an_error() {
    let stream = literal_run(MID_CONTROL_LITERALS, LENGTH_FIXTURE_FILL);

    // The exact stream decodes, which is the control for the case below.
    let decoded = decoded_bytes(
        decode_into(&stream, MID_CONTROL_OUTPUT),
        "synthetic_input_ending_before_the_declared_size_is_an_error: exact",
    );
    assert_eq!(
        decoded,
        expected_literals(MID_CONTROL_LITERALS, LENGTH_FIXTURE_FILL),
        "LZSS-014: the exact stream decodes to its forty literals"
    );

    let short = decode_error(
        decode_into(&stream, SHORT_DECLARED),
        "synthetic_input_ending_before_the_declared_size_is_an_error",
    );
    match short {
        LzssError::OutputShort {
            declared_out,
            produced,
            at,
            consumed,
        } => {
            assert_eq!(
                declared_out, SHORT_DECLARED,
                "LZSS-014: the declared size the shortfall was measured against"
            );
            assert_eq!(
                produced, MID_CONTROL_OUTPUT,
                "LZSS-014: forty bytes existed when the input ran out, and no \
                 byte was padded to reach the declared size"
            );
            assert_eq!(
                at, MID_CONTROL_INPUT_BYTES,
                "LZSS-014: the error names the input offset the input ran out at, \
                 which is the end of the forty five input bytes"
            );
            assert_eq!(
                consumed, MID_CONTROL_INPUT_BYTES,
                "LZSS-014: the error names how many input bytes were consumed, and \
                 the whole input was consumed"
            );
            // `docs/spec/lzss.md` "## Error text" fixes the exact wording of
            // this refusal, so the literal below is that wording and not a
            // description of it: a message that printed nothing, dropped the
            // offset or permuted the counts fails here.
            assert_eq!(
                format!("{}", short),
                "fo1-lzss: the input ran out at offset 45 with 40 of 41 declared \
                 output bytes produced (45 input byte(s) consumed)",
                "LZSS-014: the refusal renders the wording docs/spec/lzss.md fixes"
            );
        }
        other => panic!(
            "synthetic: expected OutputShort, got {}",
            lzss_error_name(&other)
        ),
    }
}

/// claim: LZSS-014
/// oracle: class 5 (hand-computed vector from the spec: an overrun by exactly one byte is an error)
#[test]
fn synthetic_overrun_by_exactly_one_byte_is_an_error() {
    let stream = literal_run(MID_CONTROL_LITERALS, LENGTH_FIXTURE_FILL);

    // The fortieth literal would be the forty first byte. Every reading of the
    // overrun check agrees on this one, because the offending token is a single
    // literal byte: declared 39, produced 39, token length 1.
    match decode_error(
        decode_into(&stream, OVERRUN_ONE_DECLARED),
        "synthetic_overrun_by_exactly_one_byte_is_an_error",
    ) {
        LzssError::OutputOverrun {
            declared_out,
            produced,
            token_len,
            ..
        } => {
            assert_eq!(
                declared_out, OVERRUN_ONE_DECLARED,
                "LZSS-014: the declared size"
            );
            assert_eq!(
                produced, OVERRUN_ONE_DECLARED,
                "LZSS-014: the last byte inside the declared size is the one that \
                 stopped the decoder"
            );
            assert_eq!(
                token_len, 1,
                "LZSS-014: the offending token was one literal byte"
            );
        }
        other => panic!(
            "synthetic: expected OutputOverrun, got {}",
            lzss_error_name(&other)
        ),
    }

    // The control: one byte more of room and the very same input decodes. A
    // decoder that stopped at the declared size would have returned 39 bytes
    // here instead of an error, and 40 bytes for every other input as well.
    let decoded = decoded_bytes(
        decode_into(&stream, MID_CONTROL_OUTPUT),
        "synthetic_overrun_by_exactly_one_byte_is_an_error: one byte more of room",
    );
    assert_eq!(
        decoded.len(),
        MID_CONTROL_OUTPUT,
        "LZSS-014: the same input with the declared size it really needs"
    );
}

/// claim: LZSS-015
/// oracle: class 5 (hand-computed vector from the spec: a back reference with one input byte left is an error, not a loop)
#[test]
fn synthetic_mid_pair_back_reference_is_an_error_not_a_loop() {
    // Zero of 61,070 real blocks end mid pair (LZSS-C024), so this stream is
    // built here and exists nowhere on disk. The original binary underflows its
    // length counter and loops in this state; the spec requires an error, and a
    // decoder that loops passes the entire corpus.
    let truncated = decode_error(
        decode_into(&MID_PAIR_AT_START, BOUNDARY_DECLARED),
        "synthetic_mid_pair_back_reference_is_an_error_not_a_loop: at the start",
    );
    match truncated {
        LzssError::TruncatedMatch {
            remaining_input,
            produced,
            declared_out,
            at,
        } => {
            assert_eq!(
                remaining_input, MID_PAIR_REMAINING,
                "LZSS-015: one input byte was left where a pair needs two"
            );
            assert_eq!(
                produced, 0,
                "LZSS-015: nothing had been decoded when the pair started"
            );
            assert_eq!(
                declared_out, BOUNDARY_DECLARED,
                "LZSS-015: the declared size travels with the error"
            );
            assert_eq!(
                at, MID_PAIR_AT_OFFSET,
                "LZSS-015: the error names the input offset the pair started at"
            );
            // The crate's promise is that a failure is an `LzssError` carrying the
            // byte offset and the lengths involved, never the bytes themselves.
            // `docs/spec/lzss.md` "## Error text" fixes the exact wording of
            // that refusal, so the literal below is that wording: a message
            // that printed nothing, dropped the offset or the counts, or
            // permuted them fails here.
            assert_eq!(
                format!("{}", truncated),
                format!(
                    "fo1-lzss: the back reference at input offset {MID_PAIR_AT_OFFSET} \
                     started with {MID_PAIR_REMAINING} input byte(s) left and needs two \
                     ({produced} of {declared_out} produced)"
                ),
                "LZSS-015: the refusal names the offset, the bytes left and the \
                 counts the caller declared"
            );
        }
        other => panic!(
            "synthetic: expected TruncatedMatch, got {}",
            lzss_error_name(&other)
        ),
    }

    // The same shape after forty literals have already been written, so the
    // guard is proven to fire mid stream and not only at offset zero.
    let mut stream = literal_run(MID_CONTROL_LITERALS, MID_PAIR_FILL);
    stream.push(MID_PAIR_TAIL[0]);
    stream.push(MID_PAIR_AT_START[1]);
    let truncated_mid_stream = decode_error(
        decode_into(&stream, MID_CONTROL_OUTPUT),
        "synthetic_mid_pair_back_reference_is_an_error_not_a_loop: mid stream",
    );
    match truncated_mid_stream {
        LzssError::TruncatedMatch {
            remaining_input,
            produced,
            declared_out,
            at,
        } => {
            assert_eq!(
                remaining_input, MID_PAIR_REMAINING,
                "LZSS-015: one input byte was left where a pair needs two"
            );
            assert_eq!(
                produced, MID_CONTROL_OUTPUT,
                "LZSS-015: forty literals decoded before the truncated pair"
            );
            assert_eq!(
                at, MID_STREAM_PAIR_AT_OFFSET,
                "LZSS-015: the error names the input offset the pair started at, \
                 which is not the start of the stream"
            );
            assert_eq!(
                format!("{}", truncated_mid_stream),
                format!(
                    "fo1-lzss: the back reference at input offset \
                     {MID_STREAM_PAIR_AT_OFFSET} started with {MID_PAIR_REMAINING} \
                     input byte(s) left and needs two ({produced} of {declared_out} \
                     produced)"
                ),
                "LZSS-015: the refusal names the offset, the bytes left and the \
                 counts the caller declared"
            );
        }
        other => panic!(
            "synthetic: expected TruncatedMatch, got {}",
            lzss_error_name(&other)
        ),
    }

    // The block entry point reports it too, so a chain driver cannot be handed a
    // block that silently stopped mid pair.
    let mut decoder = LzssDecoder::new();
    match decode_block(&stream, &mut decoder, MID_CONTROL_OUTPUT) {
        Ok(()) => panic!("synthetic: the block entry point must refuse a truncated pair"),
        Err(error) => assert!(
            is_prescribed_failure(&error),
            "synthetic: expected a prescribed failure, got {}",
            lzss_error_name(&error)
        ),
    }
    assert_eq!(
        decoder.produced(),
        MID_CONTROL_OUTPUT,
        "synthetic: the decoder reported the forty bytes it had written"
    );
}

// --- the canary ----------------------------------------------------------------

/// claim: LZSS-021, LZSS-003, LZSS-002
/// oracle: class 4 (independent reference implementation, run at four ring parameter sets)
#[test]
fn synthetic_canary_rejects_write_index_4095_zero_and_prefill_00() {
    // **This fixture must be non-uniform or the test proves nothing.** 24 literals
    // leave ring slots 6 through 4077 unwritten, so the token reads the prefilled
    // region, and the literals vary with their output position, so which slot an
    // output position occupies is visible in the bytes. A uniform run, or one
    // long enough to have overwritten the slot the token reads, decodes
    // identically under every write start.
    let stream = with_token(
        literal_run(CANARY_LITERALS, CANARY_FILL),
        CANARY_TOKEN_B1,
        CANARY_TOKEN_B2,
    );
    // The property the message claims. After 24 literals the write head sits at
    // ring slot `(24 + 4078) mod 4096 == 6`, and the token names that same slot,
    // so it reads *at or ahead of* the head rather than a slot behind it. Slot 6
    // has therefore never been written and the run reads the prefilled region.
    //
    // This is pure test-local arithmetic, so unlike the assertion it replaces it
    // has a satisfiable value: the old form was `(6 + 4096 - 6) mod 4096 == 24`,
    // which is `0 == 24` and could never hold.
    assert_eq!(
        ring_index_of(CANARY_LITERALS),
        CANARY_SLOT,
        "synthetic: the canary token reads forward of the write head, so it reads \
         the prefilled region rather than a written slot"
    );
    assert_eq!(
        usize::from(CANARY_TOKEN_B1) | (usize::from(CANARY_TOKEN_B2 & 0xF0) << 4),
        CANARY_SLOT,
        "synthetic: the canary token names ring slot 6"
    );

    // Canonical, against the hand arithmetic.
    let canonical = RefLzss::new(RING_SIZE, RING_WRITE_START, RING_FILL)
        .decode(&stream, CANARY_DECLARED)
        .expect("synthetic: the canonical ring parameters must decode the canary");
    let mut want = expected_literals(CANARY_LITERALS, CANARY_FILL);
    want.extend_from_slice(&CANARY_CANONICAL_TAIL);
    assert_eq!(
        canonical, want,
        "LZSS-021: the canonical parameters produce the twenty four non-uniform \
         literals and three prefill bytes"
    );
    assert_eq!(
        canonical.len(),
        CANARY_DECLARED,
        "LZSS-021: the canonical length"
    );

    // Every rejected variant keeps the length and changes the bytes. Length never
    // discriminates a wrong ring parameter, which is why this compares bytes and
    // says so (LZSS-C062).
    for (wrong_start, label) in REJECTED_WRITE_STARTS {
        let variant = RefLzss::new(RING_SIZE, wrong_start, RING_FILL)
            .decode(&stream, CANARY_DECLARED)
            .expect("synthetic: a wrong write start still decodes, it decodes differently");
        assert_eq!(
            variant.len(),
            canonical.len(),
            "LZSS-021: {label} keeps the length identical, so a length only canary \
             would pass on it"
        );
        assert_ne!(
            variant, canonical,
            "LZSS-021: {label} must change the bytes. If it does not, this fixture \
             cannot detect a changed RING_WRITE_START."
        );
    }

    let wrong_prefill = RefLzss::new(RING_SIZE, RING_WRITE_START, ZERO_BYTE)
        .decode(&stream, CANARY_DECLARED)
        .expect("synthetic: a wrong prefill still decodes, it decodes differently");
    assert_eq!(
        wrong_prefill.len(),
        canonical.len(),
        "LZSS-021: a 0x00 prefill keeps the length identical too"
    );
    assert_ne!(
        wrong_prefill, canonical,
        "LZSS-021: a 0x00 prefill must change the bytes. A run short enough to \
         read back into the prefilled region is what makes this detectable, and a \
         run longer than the ring would not detect it at all."
    );

    // All three wrong parameter sets rejected, counted rather than assumed.
    let mut rejected = 0usize;
    let mut checked = 0usize;
    for (wrong_start, _) in REJECTED_WRITE_STARTS {
        checked += 1;
        let variant = RefLzss::new(RING_SIZE, wrong_start, RING_FILL)
            .decode(&stream, CANARY_DECLARED)
            .expect("synthetic: a wrong write start decodes");
        if variant != canonical {
            rejected += 1;
        }
    }
    checked += 1;
    if wrong_prefill != canonical {
        rejected += 1;
    }
    assert_eq!(
        checked, REJECTED_VARIANT_COUNT,
        "LZSS-021: three ring parameter sets were checked against the ruling"
    );
    assert_eq!(
        rejected, REJECTED_VARIANT_COUNT,
        "LZSS-021: every rejected ring parameter set changed the bytes"
    );
}

// --- a fresh ring per block ----------------------------------------------------

/// claim: LZSS-016
/// oracle: class 5 (hand-computed vector from the spec: block A writes the slot block B reads, so a shared ring decodes differently)
#[test]
fn synthetic_reset_returns_a_fresh_ring() {
    let block_a = with_token(
        literal_run(RESET_BLOCK_A_LITERALS, RESET_BLOCK_A_FILL),
        OVERLAP_TOKEN_B1,
        OVERLAP_TOKEN_B2,
    );
    let block_b_token = token_for(RESET_BLOCK_B_SLOT, RESET_BLOCK_B_RUN);
    let block_b = with_token(
        literal_run(RESET_BLOCK_B_LITERALS, RESET_BLOCK_B_FILL),
        block_b_token[1],
        block_b_token[2],
    );
    assert_eq!(
        block_a.len(),
        (RESET_BLOCK_A_LITERALS / TOKENS_PER_CONTROL) * CONTROL_BYTE_WIDTH + 3,
        "synthetic: block A is ninety six literals and one token"
    );
    assert_eq!(
        (RESET_BLOCK_B_SLOT + RING_SLOT_COUNT - ring_index_of(RESET_BLOCK_B_LITERALS))
            % RING_SLOT_COUNT,
        RESET_BLOCK_B_DISTANCE,
        "synthetic: block B's token reads six slots forward of its write head, into \
         the region block A wrote and block B has not"
    );
    assert_eq!(
        RESET_BLOCK_A_ONLY_SLOT, RESET_BLOCK_B_SLOT,
        "synthetic: the slot under test is the one block B's token reads"
    );
    assert_ne!(
        RESET_BLOCK_A_DECLARED, RESET_BLOCK_B_DECLARED,
        "synthetic: the two blocks declare different sizes"
    );

    // Block B's token reads a slot block B's own 32 literals never reach, so on a
    // fresh ring it reads the prefill. Hand arithmetic, not a decoder's opinion:
    // slot 20 is below the write head's start of 4078, so it holds 0x20 (LZSS-002).
    let mut want_b = expected_literals(RESET_BLOCK_B_LITERALS, RESET_BLOCK_B_FILL);
    want_b.extend(std::iter::repeat_n(PREFILL_BYTE, RESET_BLOCK_B_RUN));

    // Block B alone, on a decoder that has never seen block A.
    let mut fresh = LzssDecoder::new();
    assert_eq!(
        fresh.produced(),
        0,
        "LZSS-016: a new decoder has produced nothing"
    );
    fresh
        .decode_block(&block_b, RESET_BLOCK_B_DECLARED)
        .expect("synthetic: block B decodes on its own");
    assert_eq!(
        fresh.produced(),
        RESET_BLOCK_B_DECLARED,
        "LZSS-024: produced() reports the block's own length"
    );
    let alone = fresh.take_output();
    assert_eq!(
        alone.len(),
        RESET_BLOCK_B_DECLARED,
        "LZSS-016: block B's length"
    );
    assert_eq!(alone, want_b, "synthetic: block B's bytes on a fresh ring");
    assert_eq!(
        fresh.produced(),
        0,
        "LZSS-024: take_output() leaves the decoder empty"
    );

    // Block A, then block B, with no reset in between.
    let mut shared = LzssDecoder::new();
    decode_block(&block_a, &mut shared, RESET_BLOCK_A_DECLARED)
        .expect("synthetic: block A decodes");
    decode_block(&block_b, &mut shared, RESET_BLOCK_B_DECLARED)
        .expect("synthetic: block B decodes after block A");
    assert_eq!(
        shared.produced(),
        RESET_BLOCK_A_DECLARED + RESET_BLOCK_B_DECLARED,
        "LZSS-024: the output buffer is the only state that crosses a block \
         boundary, so produced() is the running total"
    );
    let carried = shared.take_output();
    assert_eq!(
        carried.len(),
        RESET_BLOCK_A_DECLARED + RESET_BLOCK_B_DECLARED,
        "LZSS-016: the second block appended to the first"
    );
    assert_eq!(
        carried[RESET_BLOCK_A_DECLARED..].to_vec(),
        alone,
        "LZSS-016: every block starts on a fresh ring, so block B's bytes are the \
         same whether or not block A was decoded first; only the output buffer \
         crosses a block boundary"
    );

    // Block A, reset, block B: exactly the bytes block B gives alone.
    let mut reset = LzssDecoder::new();
    decode_block(&block_a, &mut reset, RESET_BLOCK_A_DECLARED).expect("synthetic: block A decodes");
    reset.reset();
    assert_eq!(
        reset.produced(),
        0,
        "LZSS-016: reset empties the output buffer"
    );
    reset
        .decode_block(&block_b, RESET_BLOCK_B_DECLARED)
        .expect("synthetic: block B decodes after the reset");
    let after_reset = reset.take_output();
    assert_eq!(
        after_reset.len(),
        RESET_BLOCK_B_DECLARED,
        "LZSS-016: only block B's bytes are left"
    );
    assert_eq!(
        after_reset, alone,
        "LZSS-016: after reset() block B decodes exactly as it does alone, so no \
         ring state crossed the boundary"
    );
    assert_eq!(
        after_reset, want_b,
        "synthetic: block B's bytes after the reset, computed from the fold"
    );
}

// --- totality ------------------------------------------------------------------

/// claim: LZSS-014, LZSS-013, LZSS-006
/// oracle: class 6 (algebraic law: a total reader has no panic path, so every single bit flip answers)
#[test]
fn synthetic_every_bit_flip_answers_without_panicking() {
    let stream = with_token(
        literal_run(OVERLAP_LITERALS, CANARY_FILL),
        OVERLAP_TOKEN_B1,
        OVERLAP_TOKEN_B2,
    );
    assert_eq!(
        stream.len(),
        BIT_FLIP_STREAM_BYTES,
        "synthetic: the bit flip stream is 111 bytes, which is more than 24"
    );
    assert_eq!(
        stream.len() * 8,
        BIT_FLIP_BITS,
        "synthetic: 888 bits are flipped one at a time"
    );

    // The unflipped stream decodes, so every flip differs from it in exactly one
    // bit and the corpus is not silently wrong to begin with.
    let decoded = decoded_bytes(
        decode_into(&stream, BIT_FLIP_DECLARED),
        "synthetic_every_bit_flip_answers_without_panicking: unflipped",
    );
    assert_eq!(
        decoded.len(),
        BIT_FLIP_DECLARED,
        "synthetic: the unflipped stream produces the declared size"
    );

    // The unflipped stream's output, computed from the spec's fold and from the
    // read before write rule (LZSS-011): the ninety six literals, and then the
    // run's six repetitions of the three bytes in front of the write head.
    let head = expected_from(OVERLAP_DISTANCE, CANARY_FILL, OVERLAP_SEED_POSITION);
    let mut want = expected_literals(OVERLAP_LITERALS, CANARY_FILL);
    for _ in 0..OVERLAP_REPEATS {
        want.extend_from_slice(&head);
    }
    assert_eq!(
        want.len(),
        BIT_FLIP_DECLARED,
        "synthetic: the literals and the run are the whole output"
    );

    // The counter below counts what the decoder answered, not how many times
    // the loop ran: a flip that changed a literal byte has to decode to the
    // exact bytes the spec gives, and the tally is only allowed to reach its
    // hand computed value if every one of those flips was accepted.
    let mut literal_flips_accepted = 0usize;
    let mut mutations: Vec<Vec<u8>> = Vec::with_capacity(BIT_FLIP_BITS);
    for bit in 0..BIT_FLIP_BITS {
        let mut mutated = stream.clone();
        mutated[bit / 8] ^= 1u8 << (bit % 8);
        // The corpus is a corpus of single bit flips: one bit differs from the
        // original and the other 887 do not. A flip applied twice, or to the
        // wrong place, fails here.
        let mut differing = 0u32;
        for index in 0..stream.len() {
            differing += (stream[index] ^ mutated[index]).count_ones();
        }
        assert_eq!(
            differing, 1,
            "synthetic: the mutation at bit {bit} flips exactly one bit of the stream"
        );
        mutations.push(mutated.clone());

        match decode_into(&mutated, BIT_FLIP_DECLARED) {
            Ok(bytes) => {
                assert_eq!(
                    bytes.len(),
                    BIT_FLIP_DECLARED,
                    "LZSS-014: a decode that returns Ok reaches the declared size \
                     exactly, so nothing was padded or truncated"
                );
                // LZSS-010: the produced length places the write head at the
                // slot the spec's shift gives, and that slot is inside the
                // ring.
                assert_eq!(
                    ring_index_of(bytes.len()),
                    BIT_FLIP_HEAD_SLOT,
                    "LZSS-010: the write head after the declared output is ring \
                     slot 96, which is inside the ring"
                );
                // A flip in a literal byte changes one output byte and nothing
                // else, so the whole output is known before the decoder runs:
                // the literal itself, and, for the three bytes the run reads,
                // the six places the run repeats them in.
                if let Some(position) = literal_position_of(bit / 8) {
                    let mask = 1u8 << (bit % 8);
                    let mut expected = want.clone();
                    expected[position] ^= mask;
                    if position >= OVERLAP_SEED_POSITION {
                        for repeat in 0..OVERLAP_REPEATS {
                            let index = OVERLAP_LITERALS
                                + repeat * OVERLAP_DISTANCE
                                + (position - OVERLAP_SEED_POSITION);
                            expected[index] ^= mask;
                        }
                    }
                    // LZSS-006: a set bit is a literal. The expectation above is
                    // the spec's fold: one input byte per set control bit,
                    // written to the output and to the ring at the write index,
                    // which advances by one and wraps at RING_SIZE.
                    assert_eq!(
                        bytes, expected,
                        "LZSS-006: the flip at bit {bit} changes the one byte it \
                         names and nothing else"
                    );
                    literal_flips_accepted += 1;
                }
            }
            Err(error) => {
                assert!(
                    is_prescribed_failure(&error),
                    "LZSS-014: bit {bit} was refused with {}, which is not one of \
                     the three failures the spec prescribes",
                    lzss_error_name(&error)
                );
            }
        }
    }
    // 96 literal bytes carry 768 bits, and a literal carries no structure, so
    // every one of those flips still decodes to the declared size. The count
    // only moves on the accepted path, so a decoder that refused one of them
    // fails here instead of passing on a tally.
    assert_eq!(
        literal_flips_accepted, BIT_FLIP_LITERAL_BITS,
        "every flip that only changed a literal byte decoded to the bytes the \
         spec's fold gives"
    );
    // Every flip produced its own stream: a mutation index that named the
    // same bit twice would collapse two of them into one.
    let mut distinct = mutations.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        BIT_FLIP_BITS,
        "every single bit flip produced its own stream"
    );
}

/// claim: LZSS-012, LZSS-001, LZSS-004
/// oracle: class 6 (algebraic law: every token pair addresses a slot inside the ring, and the largest is 4095)
#[test]
fn synthetic_no_ring_index_above_4095_is_addressable() {
    // Every one of the 65,536 byte pairs a stream can present is driven through
    // the decoder itself. The decoder is told the run length the pair encodes
    // and nothing else, so a decoder that read the source out of the pair
    // wrongly, or the length out of the pair wrongly, or reached a slot above
    // 4095, answers with different bytes or a different length here. The
    // expectations come from the spec's ring rules, not from a copy of the
    // decoder's own expression: `b1` contributes at most `0xFF` and the high
    // nibble at most `0xF00`, so the source is at most `0xFFF` (LZSS-012), and
    // the low nibble is the run's bias (LZSS-008).
    // Every pair is driven through the decoder, and the pairs that refuse are
    // collected rather than counted: the tally below is the number of pairs
    // that decoded, so a pair the decoder refused shows up as a short count
    // and names itself.
    let mut decoded_pairs = 0usize;
    let mut refusals: Vec<String> = Vec::new();
    let mut shortest = LONGEST_RUN;
    let mut longest = 0usize;
    for b1 in 0..BYTE_VALUE_COUNT {
        for b2 in 0..BYTE_VALUE_COUNT {
            let case_name = format!("pair {b1:02X} {b2:02X}");
            let b1_byte = to_u8(b1);
            let b2_byte = to_u8(b2);
            let source = usize::from(b1_byte) | ((usize::from(b2_byte) & POSITION_HIGH_MASK) << 4);
            let length = (usize::from(b2_byte) & LENGTH_BIAS) + SHORTEST_RUN;
            // The pair as the decoder sees it: a control byte whose first bit is
            // clear, so the first token is a back reference, and the pair behind
            // it. Nothing else is in the stream, so the block ends when the pair
            // is used up (LZSS-013).
            let pair_stream = [0x00u8, b1_byte, b2_byte];
            match decode_into(&pair_stream, length) {
                Ok(decoded) => {
                    assert_eq!(
                        decoded.len(),
                        length,
                        "{case_name}: LZSS-008: the run length the pair's bias encodes"
                    );
                    assert_eq!(
                        decoded,
                        expected_ring_run(source, length, |slot| match slot {
                            0..=PREFILLED_LAST_SLOT => PREFILL_BYTE,
                            _ => ZERO_BYTE,
                        }),
                        "{case_name}: LZSS-012: the bytes the pair's source names, read out \
                         of a fresh ring"
                    );
                    if decoded.len() < shortest {
                        shortest = decoded.len();
                    }
                    if decoded.len() > longest {
                        longest = decoded.len();
                    }
                    decoded_pairs += 1;
                }
                Err(error) => refusals.push(format!("{}: {}", case_name, lzss_error_name(&error))),
            }
        }
    }
    // 256 times 256 pairs, and every one of them decoded. The crate keeps an
    // `IndexOverflow` variant for the ring arithmetic its constants cannot
    // reach (LZSS-024), and this sweep is the evidence that the public API
    // never produces it: a pair that reported it would be in the refusals and
    // would make this count short.
    assert_eq!(
        decoded_pairs, TOKEN_PAIR_COUNT,
        "LZSS-012: every token pair must decode, and these did not: {refusals:?}"
    );
    assert_eq!(
        shortest, MIN_MATCH,
        "LZSS-008: the shortest run the decoder produced over the whole pair space"
    );
    assert_eq!(
        longest, MAX_MATCH,
        "LZSS-008: the longest run the decoder produced over the whole pair space"
    );

    // Observable: the top of the addressable range is addressable, and the
    // original binary's declared 4116 byte ring would have had 20 slots nothing
    // could reach.
    let top = token_for(LARGEST_SOURCE, SHORTEST_RUN);
    assert_eq!(top[1], 0xFF, "synthetic: 4095 is 0xFFF");
    assert_eq!(top[2], 0xF0, "synthetic: the high nibble of 4095 is 0xF");
    let decoded = decoded_bytes(
        decode_into(&top, BOUNDARY_DECLARED),
        "synthetic_no_ring_index_above_4095_is_addressable",
    );
    // The source cursor wraps at 4096 (LZSS-011), so this run of three reads
    // slots 4095, 0 and 1: 0x00 from the zero region, then 0x20 and 0x20 from the
    // prefilled region that the write head, starting at 4078, has not reached.
    //
    // **Not `[0x00, 0x00, 0x00]`** — that is the 4116 byte ring's reading, and it
    // is byte-exact for 0 of the 31 archive/loose pairs where this one is
    // byte-exact for 28. See the same note in
    // `synthetic_fresh_ring_prefills_0_to_4077_and_zeroes_4078_to_4095`.
    assert_eq!(
        decoded,
        vec![ZERO_BYTE, PREFILL_BYTE, PREFILL_BYTE],
        "LZSS-002 and LZSS-012: slot 4095 is addressable and holds 0x00 in a \
         fresh ring, and the source cursor wraps from there into the still \
         prefilled slots 0 and 1"
    );
    assert_eq!(
        LARGEST_SOURCE + 1,
        RING_SLOT_COUNT,
        "LZSS-001: allocating 4096 loses no reachable slot, which is why the \
         original's unreachable tail is invisible"
    );
    assert_eq!(
        DECLARED_ORIGINAL_RING - RING_SLOT_COUNT,
        UNREACHABLE_SLOTS,
        "LZSS-C013: the original declares 4116 bytes and this spec allocates 4096, \
         so 20 slots were unreachable"
    );

    // LZSS-024: the crate keeps an `IndexOverflow` variant for the ring
    // arithmetic its own constants cannot reach, and the public API cannot
    // produce it. The reason is the bound above: a pair names a source of at
    // most 4095 and the ring has 4096 slots, so no ring read can miss; the
    // write head starts at 4078 and only moves forward, wrapping at 4096
    // (LZSS-001, LZSS-003); and a run is at most 18 bytes long (LZSS-008). The
    // sweep over all 65,536 pairs above is the runtime evidence: not one of
    // them produced an error.
    //
    // The wording is still the crate's own and `docs/spec/lzss.md` "## Error
    // text" fixes it, so it is graded here on a value built from the constants
    // in this file. The numbers are this fixture's own; the sentence is what
    // the spec fixes, and a message that printed nothing, dropped the offset
    // or permuted the counts fails on the literal below.
    let overflow = LzssError::IndexOverflow {
        at: INDEX_OVERFLOW_AT,
        declared_out: INDEX_OVERFLOW_DECLARED_OUT,
        produced: INDEX_OVERFLOW_PRODUCED,
    };
    assert_eq!(
        format!("{}", overflow),
        "fo1-lzss: ring arithmetic overflowed at input offset 7 (5 of 12 produced)",
        "LZSS-024: the wording of the variant the public API cannot produce is \
         the wording docs/spec/lzss.md fixes"
    );
}
