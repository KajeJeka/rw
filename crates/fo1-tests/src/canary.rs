//! Canaries for the LZSS write index ruling. **Oracle class 5** (hand computed
//! vectors) against **oracle class 4** (this crate's reference decoder).
//!
//! Guide: *Hold out vectors and canaries*. "A constant nothing can falsify is
//! not a ruling." These tests are the mechanism: they prove that the reference
//! decoder, and therefore any suite built on it, rejects `write_start = 4095`
//! and `write_start = 0` on the same streams that pass with 4078.
//!
//! They also pin the three worked vectors of `docs/spec/lzss.md`. That is what
//! proves this crate's reference decoder is not the code under test: it agrees
//! with hand arithmetic on streams that occur in the real corpus, and it
//! disagrees with itself under every wrong ring parameter.
//!
//! Claim ids: LZSS-002, LZSS-003, LZSS-008, LZSS-013, LZSS-014, LZSS-015,
//! LZSS-017, LZSS-018, LZSS-019, LZSS-020, LZSS-021.

use crate::ref_dat::RefDatTree;
use crate::ref_lzss::{
    RefError, RefLzss, CANONICAL_PREFILL, CANONICAL_RING_SIZE, CANONICAL_WRITE_START,
};
use crate::sha256::{sha256_of_slice, Sha256};

/// Builds a stream that emits `count` literals, then one control byte holding a
/// single back reference `b1 b2`.
///
/// Every control byte is `0xFF` (eight literal tokens) except the last one,
/// which carries exactly the remaining literals.
///
/// The literal bytes are **not** uniform: `byte ^ (position % 251)`. A uniform
/// run cannot detect a wrong write index at all, because every ring slot holds
/// the same byte whatever the mapping is, so a canary built on one would pass on
/// `write_start = 0` and prove nothing. This was found the hard way: the first
/// version of this canary used a uniform fill and the wrong variant decoded
/// byte-for-byte identically.
/// `byte` folded with the absolute output position, so the run is not uniform.
///
/// The fold is what makes the canary falsifiable: a uniform run decodes
/// identically no matter how the ring maps input to output, so it cannot tell a
/// correct decoder from a wrong one. The first version of this file used a
/// uniform fill and `write_start = 0` decoded byte for byte the same.
fn mix(byte: u8, position: usize) -> u8 {
    // `position % 251` is below 251 by construction, so the conversion cannot
    // fail. `expect` is the loud form: a silent fallback to 0 would make the run
    // uniform and destroy the very property this canary exists to detect, which
    // is the failure mode this file is written against.
    byte ^ u8::try_from(position % 251).expect("position % 251 is below 251")
}

fn literal_run(count: usize, byte: u8) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut position = 0usize;
    let mut left = count;
    while left >= 8 {
        out.push(0xFF);
        for _ in 0..8 {
            out.push(mix(byte, position));
            position += 1;
        }
        left -= 8;
    }
    if left > 0 {
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

/// Appends a back reference token as `0x00 b1 b2`.
fn with_token(mut stream: Vec<u8>, b1: u8, b2: u8) -> Vec<u8> {
    stream.push(0x00);
    stream.push(b1);
    stream.push(b2);
    stream
}

/// LZSS-018: the stream `00 00 0F` decodes to eighteen spaces.
///
/// All eight bits of the control byte are clear, so the first token is a back
/// reference. The pair is `00 00`, then the input runs out mid control byte,
/// which is the normal end of a block (LZSS-013).
#[test]
fn vector_lzss_018_control_00_00_0f_is_eighteen_spaces() {
    let stream = [0x00u8, 0x00, 0x0F];

    // LZSS-018 arithmetic, recomputed here so the expected value does not come
    // from the decoder:
    //   source = 0x00 | ((0x0F & 0xF0) << 4) = 0
    //   length = (0x0F & 0x0F) + 3            = 15 + 3 = 18
    let source = usize::from(0x00u8) | (usize::from(0x0Fu8 & 0xF0) << 4);
    let length = usize::from(0x0Fu8 & 0x0F) + 3;
    assert_eq!(source, 0, "LZSS-018: source is inside the prefilled region");
    assert_eq!(length, 18, "LZSS-018: length");

    let decoded = RefLzss::canonical().decode(&stream, 18).expect("18 bytes");
    assert_eq!(decoded.len(), 18, "LZSS-018: output length");
    assert_eq!(
        decoded,
        vec![CANONICAL_PREFILL; 18],
        "LZSS-018: source 0 is never written, so all 18 bytes are the prefill"
    );
}

/// LZSS-002: the prefill byte is observable. A `0x00` prefill produces the same
/// length and the wrong bytes.
#[test]
fn vector_lzss_018_is_wrong_under_prefill_0x00() {
    let stream = [0x00u8, 0x00, 0x0F];
    let wrong = RefLzss::new(CANONICAL_RING_SIZE, CANONICAL_WRITE_START, 0x00);
    let decoded = wrong.decode(&stream, 18).expect("18 bytes");

    assert_eq!(
        decoded.len(),
        18,
        "LZSS-021: length alone never discriminates a wrong ring parameter"
    );
    assert_eq!(
        decoded,
        vec![0x00u8; 18],
        "LZSS-002: a 0x00 prefill yields zeros, so the prefill is observable"
    );
}

/// LZSS-017: token `4E 36` at output position 5000.
#[test]
fn vector_lzss_017_token_4e_36_at_output_position_5000() {
    // The token's arithmetic, recomputed by hand so the expected value does not
    // come from the decoder:
    //   write index at output 5000 = (5000 + 4078) mod 4096 = 886
    //   ring index of source 4960   = (4960 + 4078) mod 4096 = 846 = 0x34E
    //   b1 = 846 & 0xFF                          = 0x4E
    //   b2 = ((846 >> 8) & 0x0F) << 4 | (9 - 3)  = 0x30 | 0x06 = 0x36
    let write_index_at_5000 = (5000usize + 4078) % 4096;
    let source_ring_index = (4960usize + 4078) % 4096;
    assert_eq!(write_index_at_5000, 886, "LZSS-017: write index at 5000");
    assert_eq!(source_ring_index, 846, "LZSS-017: source ring index");

    let b1 = u8::try_from(source_ring_index & 0xFF).expect("b1 fits a byte");
    let b2 = u8::try_from(((source_ring_index >> 8) & 0x0F) << 4 | (9usize - 3)).expect("b2 fits");
    assert_eq!((b1, b2), (0x4Eu8, 0x36u8), "LZSS-017: the token is 4E 36");

    // Read the token back the way LZSS-008 states.
    let read_source = usize::from(b1) | (usize::from(b2 & 0xF0) << 4);
    let read_length = usize::from(b2 & 0x0F) + 3;
    assert_eq!(
        read_source, 846,
        "LZSS-008: absolute ring position, not a distance"
    );
    assert_eq!(read_length, 9, "LZSS-008: length is (b2 & 0x0F) + 3");
    assert_eq!(
        write_index_at_5000 - read_source,
        40,
        "LZSS-017: the run starts 40 bytes back"
    );

    // 5000 literals, then the token. Output positions 4960..=4968 all hold the
    // same value under the canonical mapping, so the run must produce nine
    // copies of it.
    let fill = 0xABu8;
    let stream = with_token(literal_run(5000, fill), b1, b2);
    let decoded = RefLzss::canonical()
        .decode(&stream, 5009)
        .expect("5009 bytes");

    assert_eq!(decoded.len(), 5009, "LZSS-017: produced length");
    let mut expected = [0u8; 5009];
    for (p, slot) in expected.iter_mut().enumerate() {
        *slot = if p < 5000 {
            mix(fill, p)
        } else {
            mix(fill, p - 40)
        };
    }
    assert_eq!(
        decoded, expected,
        "LZSS-017: nine repeats of the bytes 40 back, and every literal"
    );
}

/// LZSS-019: token `75 3F` at write index 886 repeats the previous byte 18
/// times, because source 885 is one byte behind the write head. This is the
/// overlap case that proves read-before-write ordering (LZSS-011).
#[test]
fn vector_lzss_019_token_75_3f_repeats_the_previous_byte() {
    // LZSS-019 arithmetic.
    //   source = 0x75 | ((0x3F & 0xF0) << 4) = 0x75 | 0x300 = 885
    //   length = (0x3F & 0x0F) + 3            = 15 + 3     = 18
    //   886 - 885 = 1, one byte behind the write head.
    let source = usize::from(0x75u8) | (usize::from(0x3Fu8 & 0xF0) << 4);
    let length = usize::from(0x3Fu8 & 0x0F) + 3;
    assert_eq!(source, 885, "LZSS-019: source is an absolute ring position");
    assert_eq!(length, 18, "LZSS-019: length");
    assert_eq!(
        886usize - source,
        1,
        "LZSS-019: the source is one byte behind the head"
    );

    let fill = 0x5Au8;
    let stream = with_token(literal_run(5000, fill), 0x75, 0x3F);
    let decoded = RefLzss::canonical()
        .decode(&stream, 5018)
        .expect("5018 bytes");
    assert_eq!(decoded.len(), 5018, "LZSS-019: produced length");

    let head = decoded[4999];
    assert_eq!(
        head,
        mix(fill, 4999),
        "LZSS-019: output 4999 is the literal before the token"
    );
    let tail: Vec<u8> = decoded.iter().skip(5000).copied().collect();
    assert_eq!(
        tail,
        vec![head; 18],
        "LZSS-019: eighteen repeats of the byte just written. A decoder that \
         snapshots the run first gets this wrong."
    );
}

/// LZSS-021: the canary itself.
///
/// The two long streams must change or be rejected under **both** rejected write
/// index starts, 0 and 4095.
///
/// `00 00 0F` is deliberately **not** in that set. Its only token reads ring
/// slot 0, which lives inside the prefilled region and is never written, so the
/// write index is invisible to it under any variant. Asserting that it detects
/// a wrong write index would be asserting something false; `00 00 0F` pins the
/// prefill byte and the mid control byte stop instead, which
/// `vector_lzss_018_is_wrong_under_prefill_0x00` and
/// `lzss_013_mid_control_byte_end_is_not_an_error` cover.
#[test]
fn canary_lzss_021_rejects_write_index_4095_and_0() {
    let long_a = with_token(literal_run(5000, 0xABu8), 0x4E, 0x36);
    let long_b = with_token(literal_run(5000, 0x5Au8), 0x75, 0x3F);
    let streams: Vec<(&str, &[u8], usize)> = vec![
        ("LZSS-017 5000 literals then 4E 36", &long_a, 5009),
        ("LZSS-019 5000 literals then 75 3F", &long_b, 5018),
    ];

    let canonical = RefLzss::new(
        CANONICAL_RING_SIZE,
        CANONICAL_WRITE_START,
        CANONICAL_PREFILL,
    );
    for (name, stream, declared) in &streams {
        let decoded = canonical
            .decode(stream, *declared)
            .expect("LZSS-021: the canonical parameters must decode");
        assert_eq!(
            decoded.len(),
            *declared,
            "LZSS-021: canonical {name} produced length"
        );
    }

    let mut rejected = 0usize;
    let mut total = 0usize;
    for wrong_start in [0usize, 4077, 4095] {
        let variant = RefLzss::new(CANONICAL_RING_SIZE, wrong_start, CANONICAL_PREFILL);
        for (name, stream, declared) in &streams {
            total += 1;
            let expected = canonical.decode(stream, *declared);
            let actual = variant.decode(stream, *declared);
            let differs = match (&expected, &actual) {
                (Ok(a), Ok(b)) => a != b,
                _ => true,
            };
            if differs {
                rejected += 1;
            } else {
                panic!(
                    "LZSS-021: write_start {wrong_start} must change or reject {name}. If it does \
                     not, the suite cannot detect a changed RING_WRITE_START."
                );
            }
        }
    }
    assert_eq!(
        rejected, total,
        "LZSS-021: every rejected write index must be detected on every long stream"
    );
}

/// LZSS-020 and LZSS-021: a wrong write index never changes the length, so a
/// length-only canary would pass on every wrong variant.
#[test]
fn canary_lzss_021_length_alone_does_not_detect_a_wrong_write_index() {
    let stream = with_token(literal_run(5000, 0xABu8), 0x4E, 0x36);
    let declared = 5009usize;

    let canonical = RefLzss::new(
        CANONICAL_RING_SIZE,
        CANONICAL_WRITE_START,
        CANONICAL_PREFILL,
    );
    let expected = canonical
        .decode(&stream, declared)
        .expect("canonical decodes");

    for wrong_start in [0usize, 4077, 4095] {
        let variant = RefLzss::new(CANONICAL_RING_SIZE, wrong_start, CANONICAL_PREFILL);
        let actual = variant
            .decode(&stream, declared)
            .expect("variant decodes the same length");
        assert_eq!(
            expected.len(),
            actual.len(),
            "LZSS-021: write_start {wrong_start} keeps the length identical, so a length-only \
             canary would pass on it"
        );
        assert_ne!(
            expected, actual,
            "LZSS-020: write_start {wrong_start} must change the bytes"
        );
    }
}

/// LZSS-013: a stream that ends in the middle of a control byte is finished,
/// not an error. This is how 85.5% of real blocks end.
#[test]
fn lzss_013_mid_control_byte_end_is_not_an_error() {
    let mut stream = literal_run(40, 0x11);
    stream.push(0x00);
    let decoded = RefLzss::canonical().decode(&stream, 40).expect("40 bytes");
    assert_eq!(
        decoded.len(),
        40,
        "LZSS-013: the trailing control byte has no tokens behind it and is discarded"
    );
}

/// LZSS-014: the declared size only detects a mismatch. A stream one byte short
/// errors, and a stream that would write one byte past the declared size errors.
#[test]
fn lzss_014_short_stream_and_overrun_both_error() {
    let exact = literal_run(40, 0x22);
    let decoded = RefLzss::canonical().decode(&exact, 40).expect("40 bytes");
    assert_eq!(decoded.len(), 40, "LZSS-014: the exact stream decodes");

    let short: Vec<u8> = exact.iter().copied().take(39).collect();
    match RefLzss::canonical().decode(&short, 40) {
        Err(RefError::OutputShort { .. }) => {}
        other => panic!("LZSS-014: a stream one byte short must error, got {other:?}"),
    }

    // One control byte and one literal: the 41st byte would be written.
    let mut over = exact.clone();
    over.push(0x01);
    over.push(0x33);
    match RefLzss::canonical().decode(&over, 40) {
        Err(RefError::OutputOverrun { .. }) => {}
        other => panic!("LZSS-014: an overrun must error, got {other:?}"),
    }
}

/// LZSS-015: a back reference that starts with one input byte left is an error,
/// not a loop. The original binary underflows here; the suite requires an error.
#[test]
fn lzss_015_back_reference_with_one_byte_left_is_an_error() {
    match RefLzss::canonical().decode(&[0x00u8, 0x00], 3) {
        Err(RefError::TruncatedMatch {
            remaining_input: 1, ..
        }) => {}
        other => {
            panic!("LZSS-015: expected TruncatedMatch with one input byte left, got {other:?}")
        }
    }
}

/// The harness SHA-256 against the FIPS 180-4 published vectors. Without this
/// the fingerprint constant in the harness would be unfalsifiable.
#[test]
fn sha256_matches_published_vectors() {
    assert_eq!(
        sha256_of_slice(b"").expect("empty digest"),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "sha256 of the empty string"
    );
    assert_eq!(
        sha256_of_slice(b"abc").expect("abc digest"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "sha256 of abc"
    );
    assert_eq!(
        sha256_of_slice(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")
            .expect("two block digest"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        "sha256 of the 56 byte two block vector"
    );
    assert_eq!(
        sha256_of_slice(&[0x61u8; 1_000_000]).expect("million a digest"),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
        "sha256 of one million 0x61, the FIPS long vector"
    );
}

/// The digest must not depend on the chunk size, because the fingerprint reads a
/// file in 64 KiB chunks.
#[test]
fn sha256_is_independent_of_chunk_size() {
    let data: Vec<u8> = std::iter::repeat_n(0xA5u8, 1000).collect();
    let one_shot = sha256_of_slice(&data).expect("one shot");
    for chunk in [1usize, 7, 63, 64, 65, 128, 999] {
        let mut state = Sha256::new();
        for piece in data.chunks(chunk) {
            state.feed(piece).expect("feed");
        }
        assert_eq!(
            crate::sha256::hex(&state.finish()),
            one_shot,
            "chunk size {chunk} must not change the digest"
        );
    }
}

/// The reference walker must refuse a short archive rather than panic. The total
/// reader rule applies to the oracle itself.
#[test]
fn ref_dat_refuses_a_truncated_archive() {
    let truncated = [0x00u8; 8];
    let result = RefDatTree::walk(&truncated);
    assert!(
        result.is_err(),
        "DAT-001: 8 bytes cannot hold a 16 byte root header"
    );
}

/// The reference walker reads a synthetic tree built in code, so it can never
/// support a CONFIRMED tag (guide: Fixtures and golden files).
#[test]
fn ref_dat_reads_a_synthetic_tree() {
    let mut archive: Vec<u8> = Vec::new();
    for field in [1u32, 10, 0, 0] {
        archive.extend_from_slice(&field.to_be_bytes());
    }
    archive.push(1);
    archive.push(b'.');
    for field in [2u32, 4, 16, 0] {
        archive.extend_from_slice(&field.to_be_bytes());
    }
    archive.push(9);
    archive.extend_from_slice(b"COLOR.PAL");
    for field in [64u32, 900, 33536, 9292] {
        archive.extend_from_slice(&field.to_be_bytes());
    }
    archive.push(9);
    archive.extend_from_slice(b"FONT0.AAF");
    for field in [64u32, 1000, 2060, 1200] {
        archive.extend_from_slice(&field.to_be_bytes());
    }

    let tree = RefDatTree::walk(&archive).expect("synthetic tree walks");
    assert_eq!(tree.directory_count(), 1, "synthetic_dat: one directory");
    assert_eq!(tree.entries_total(), 2, "synthetic_dat: two entries");
    assert_eq!(
        tree.directory_names(),
        vec!["."],
        "synthetic_dat: the root directory name"
    );
    assert_eq!(
        tree.paths(),
        vec!["COLOR.PAL".to_string(), "FONT0.AAF".to_string()],
        "synthetic_dat: the `.` directory contributes no separator"
    );
    assert_eq!(
        tree.count_by_mode(0x40),
        2,
        "synthetic_dat: both entries are chains"
    );
    assert_eq!(
        tree.count_by_mode(0x20),
        0,
        "synthetic_dat: no stored entries"
    );
    assert!(
        tree.duplicate_paths().is_empty(),
        "synthetic_dat: zero duplicate paths"
    );

    let entry = tree.find("COLOR.PAL").expect("lookup by path");
    assert_eq!(
        entry.offset, 900,
        "synthetic_dat: offset comes from the record"
    );
    assert_eq!(
        entry.uncompressed_size, 33536,
        "synthetic_dat: uncompressed size"
    );
    assert_eq!(
        entry.compressed_size, 9292,
        "synthetic_dat: compressed size"
    );

    assert!(
        tree.find("NOPE.TXT").is_err(),
        "synthetic_dat: an unknown path is an error"
    );
}
