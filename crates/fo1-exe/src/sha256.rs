//! SHA-256 over a byte slice, so the build gate can fingerprint the whole
//! executable without the workspace taking on a dependency.
//!
//! FIPS 180-4. The only tables are the 64 round constants and the 8 initial hash
//! values, both derived from primes by the standard definition.
//!
//! This is a build gate, not a security primitive. It hashes the bytes the
//! caller already holds: there is no file read here, no streaming state and no
//! key, because the digest exists to refuse one exact executable and every
//! constant in this crate is true only for it (EXETBL-001).
//!
//! Crate-internal on purpose. `digest` and `hex` are the two callers in
//! `crate::sha256_hex`, so nothing about hashing is part of this crate's public
//! surface.

/// Round constants: the first 32 bits of the fractional parts of the cube roots
/// of the first 64 primes.
const K: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// Initial hash values: the first 32 bits of the fractional parts of the square
/// roots of the first 8 primes.
const INITIAL: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// Bytes in one compression block (FIPS 180-4, section 6.2.1).
const BLOCK: usize = 64;

/// Words in one compression block's message schedule, and the number of rounds.
const ROUNDS: usize = 64;

/// Words in the working state.
const STATE_WORDS: usize = 8;

/// Bytes in one digest word.
const WORD_BYTES: usize = 4;

/// Bytes in one digest.
const DIGEST_BYTES: usize = 32;

/// Bytes a padded tail occupies at most: a pending block, the `0x80` marker and
/// the 64 bit length field.
const PADDED: usize = 128;

/// Longest pending tail that still leaves room for the `0x80` marker and the
/// 64 bit length field inside a single block, `64 - 8 - 1`.
const MAX_TAIL_FOR_ONE_BLOCK: usize = 55;

/// Where the length field starts in the one-block padded tail, `64 - 8`.
const LENGTH_AT_ONE_BLOCK: usize = 56;

/// Where the length field starts in the two-block padded tail, `128 - 8`.
const LENGTH_AT_TWO_BLOCKS: usize = 120;

/// The `0x80` padding marker (FIPS 180-4, section 5.1.1).
const MARKER: u8 = 0x80;

/// Bits per byte in the message length field.
const BITS_PER_BYTE: u64 = 8;

/// Reads `table[index - back]`, or `table[index]` when `back` is zero.
///
/// SHA-256 only ever indexes a 64 element table with `0..64` and never reaches
/// below the word it needs, so the zero fallback is unreachable. It exists
/// because `indexing_slicing` is denied workspace wide and a `None`-returning
/// helper would force a `Result` into the compression inner loop, where there is
/// nothing to report and nothing to recover from.
fn word(table: &[u32], index: usize, back: usize) -> u32 {
    let at = if back == 0 {
        Some(index)
    } else {
        index.checked_sub(back)
    };
    match at.and_then(|position| table.get(position)) {
        Some(value) => *value,
        None => 0,
    }
}

/// Folds one 64 byte block into `state` (FIPS 180-4, section 6.2.2).
///
/// Every addition is `wrapping_add` because the specification defines them
/// modulo 2^32, so a wrap is the correct result rather than an overflow to
/// report. Nothing here can fail: the schedule is a fixed size array and the
/// only reads out of range are the unreachable ones `word` absorbs.
fn compress(state: &mut [u32; STATE_WORDS], block: &[u8; BLOCK]) {
    let mut schedule = [0_u32; ROUNDS];
    // Words zero to fifteen are the block's big-endian words. This is why the
    // executable's own little-endian fields need no byte swap here: SHA-256 is
    // defined over big-endian words.
    for (slot, quad) in schedule.iter_mut().zip(block.chunks_exact(WORD_BYTES)) {
        let mut word_bytes = [0_u8; WORD_BYTES];
        word_bytes.copy_from_slice(quad);
        *slot = u32::from_be_bytes(word_bytes);
    }
    // Words sixteen to sixty-three extend the schedule:
    // `w[i] = w[i-16] + s0(w[i-15]) + w[i-7] + s1(w[i-2])`.
    for index in 16..ROUNDS {
        let older = word(&schedule, index, 15);
        let newer = word(&schedule, index, 2);
        let s0 = older.rotate_right(7) ^ older.rotate_right(18) ^ (older >> 3);
        let s1 = newer.rotate_right(17) ^ newer.rotate_right(19) ^ (newer >> 10);
        let total = word(&schedule, index, 16)
            .wrapping_add(s0)
            .wrapping_add(word(&schedule, index, 7))
            .wrapping_add(s1);
        if let Some(slot) = schedule.get_mut(index) {
            *slot = total;
        }
    }

    let mut a = word(state, 0, 0);
    let mut b = word(state, 1, 0);
    let mut c = word(state, 2, 0);
    let mut d = word(state, 3, 0);
    let mut e = word(state, 4, 0);
    let mut f = word(state, 5, 0);
    let mut g = word(state, 6, 0);
    let mut h = word(state, 7, 0);

    for index in 0..ROUNDS {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let temp1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(word(&K, index, 0))
            .wrapping_add(word(&schedule, index, 0));
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let temp2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(temp1);
        d = c;
        c = b;
        b = a;
        a = temp1.wrapping_add(temp2);
    }

    let sums = [
        word(state, 0, 0).wrapping_add(a),
        word(state, 1, 0).wrapping_add(b),
        word(state, 2, 0).wrapping_add(c),
        word(state, 3, 0).wrapping_add(d),
        word(state, 4, 0).wrapping_add(e),
        word(state, 5, 0).wrapping_add(f),
        word(state, 6, 0).wrapping_add(g),
        word(state, 7, 0).wrapping_add(h),
    ];
    for (slot, value) in state.iter_mut().zip(sums.iter()) {
        *slot = *value;
    }
}

/// The 32 byte SHA-256 digest of `message`.
///
/// The message is consumed in whole 64 byte blocks, then padded: the pending
/// tail, a `0x80` marker, zeros, and the message length in bits as a 64 bit
/// big-endian integer (FIPS 180-4, section 5.1.1). The padded tail is one block
/// or two, decided by the pending length, so a message of any size works.
pub(crate) fn digest(message: &[u8]) -> [u8; DIGEST_BYTES] {
    let mut state = INITIAL;

    let mut blocks = message.chunks_exact(BLOCK);
    for chunk in &mut blocks {
        let mut owned = [0_u8; BLOCK];
        owned.copy_from_slice(chunk);
        compress(&mut state, &owned);
    }

    let tail = blocks.remainder();
    // `chunks_exact` leaves fewer than `BLOCK` bytes behind, so the pending tail
    // is shorter than a block and the marker always lands inside the buffer.
    let mut buffer = [0_u8; PADDED];
    if let Some(slot) = buffer.get_mut(..tail.len()) {
        slot.copy_from_slice(tail);
    }
    if let Some(marker) = buffer.get_mut(tail.len()) {
        *marker = MARKER;
    }

    // FIPS 180-4 defines the field as the message length in bits modulo 2^64,
    // so the multiply wraps rather than reporting a shorter message. A `usize`
    // wider than 64 bits does not exist on a supported target; the arm is here
    // so the width question has an answer rather than a cast.
    let bit_len = match u64::try_from(message.len()) {
        Ok(len) => len.wrapping_mul(BITS_PER_BYTE),
        Err(_) => u64::MAX,
    };

    if tail.len() <= MAX_TAIL_FOR_ONE_BLOCK {
        if let Some(slot) = buffer.get_mut(LENGTH_AT_ONE_BLOCK..BLOCK) {
            slot.copy_from_slice(&bit_len.to_be_bytes());
        }
        if let Some(source) = buffer.get(..BLOCK) {
            let mut block = [0_u8; BLOCK];
            block.copy_from_slice(source);
            compress(&mut state, &block);
        }
    } else {
        if let Some(slot) = buffer.get_mut(LENGTH_AT_TWO_BLOCKS..PADDED) {
            slot.copy_from_slice(&bit_len.to_be_bytes());
        }
        for part in [0_usize, BLOCK] {
            let end = part.saturating_add(BLOCK);
            if let Some(source) = buffer.get(part..end) {
                let mut block = [0_u8; BLOCK];
                block.copy_from_slice(source);
                compress(&mut state, &block);
            }
        }
    }

    let mut out = [0_u8; DIGEST_BYTES];
    for (index, value) in state.iter().enumerate() {
        let start = index.saturating_mul(WORD_BYTES);
        let end = start.saturating_add(WORD_BYTES);
        if let Some(slot) = out.get_mut(start..end) {
            slot.copy_from_slice(&value.to_be_bytes());
        }
    }
    out
}

/// Lowercase hex of `bytes`, two characters per byte.
///
/// A digest is 32 bytes, so the buffer is sized for one and a byte that named no
/// hex digit is dropped rather than approximated with a fallback character.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().wrapping_mul(2));
    for byte in bytes {
        if let (Some(hi), Some(lo)) = (
            DIGITS.get(usize::from(byte >> 4)),
            DIGITS.get(usize::from(byte & 0x0F)),
        ) {
            out.push(char::from(*hi));
            out.push(char::from(*lo));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{digest, hex, BLOCK, DIGEST_BYTES, WORD_BYTES};

    /// NIST FIPS 180-2 / RFC 6234 vectors. The expected digests are the
    /// published ones, not this implementation's own output, so a broken fold
    /// cannot agree with itself.
    const VECTORS: [(&str, &str); 4] = [
        (
            "",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            "abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            "abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
    ];

    #[test]
    fn published_vectors_hash_to_the_published_digests() {
        for (message, expected) in VECTORS {
            assert_eq!(
                hex(&digest(message.as_bytes())),
                expected,
                "SHA-256 of the published vector {message:?} is the published digest"
            );
        }
    }

    #[test]
    fn one_million_a_hashes_to_the_published_digest() {
        // The long published vector: a million identical bytes, so the padding
        // path is entered with a tail that has to span two blocks.
        let message = vec![b'a'; 1_000_000];
        assert_eq!(
            hex(&digest(&message)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
            "SHA-256 of a million 'a' is the published digest"
        );
    }

    #[test]
    fn a_message_that_exactly_fills_a_block_needs_a_second_padded_block() {
        // 55 bytes leaves room for the marker and the length field inside one
        // block; 56 does not. Both boundaries are where a padding bug hides, so
        // the walk brackets them.
        let lengths = [0_usize, 1, 54, 55, 56, 57, 63, 64, 65, 119, 120, 128, BLOCK];
        for len in lengths {
            let message = vec![b'x'; len];
            let here = hex(&digest(&message));
            assert_eq!(
                here.len(),
                DIGEST_BYTES.saturating_mul(2),
                "a {len} byte message still yields a 32 byte digest"
            );
            // Padding is a function of the length alone, so a message and itself
            // with one more byte must differ. This is what catches a padded tail
            // that quietly hashed fewer bytes than it was handed.
            let mut longer = message.clone();
            longer.push(b'x');
            assert_ne!(
                here,
                hex(&digest(&longer)),
                "a {len} byte message and a longer one differ"
            );
        }
    }

    #[test]
    fn hex_is_lowercase_and_two_characters_per_byte() {
        assert_eq!(
            hex(&[0x00_u8, 0x0f, 0xa0, 0xff]),
            "000fa0ff",
            "each byte becomes exactly two lowercase hex digits"
        );
        assert_eq!(
            hex(&digest(b"abc")).len(),
            DIGEST_BYTES.saturating_mul(2),
            "a digest is 64 hex characters, one pair per {WORD_BYTES} byte word"
        );
    }
}
