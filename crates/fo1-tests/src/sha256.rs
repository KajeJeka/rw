//! SHA-256 over a file or a byte slice.
//!
//! The workspace has zero third party dependencies (guide: Engineering
//! philosophy, "Few dependencies"), so the install fingerprint in the harness
//! cannot borrow a crate. This is a plain FIPS 180-4 implementation whose only
//! tables are the round constants and the initial hash values.
//!
//! It is a harness utility, not a security primitive, and not an oracle for the
//! engine. The fingerprint it checks is written as a constant in this crate,
//! and this crate is a protected path (guide: Verification and anti faking).
//!
//! The streaming type is private. Everything the gate needs is one of the two
//! fallible functions at the bottom, so no public item can fail silently.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::HarnessError;

/// Round constants: the first 32 bits of the fractional parts of the cube
/// roots of the first 64 primes. Exactly 64 elements, which is the ceiling
/// `tools/check_public.sh` allows for an integer literal array, because a table
/// longer than that reads like a pasted dump.
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
const H0: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// Chunk size for the streaming file digest.
const CHUNK: usize = 64 * 1024;

/// Streaming SHA-256 state: `feed` any number of chunks, then `finish` once.
///
/// Crate-internal on purpose: the gate only needs `sha256_of_file` and
/// `sha256_of_slice`, so exposing a streaming type would widen the API for no
/// caller.
#[derive(Debug, Clone)]
pub(crate) struct Sha256 {
    h: [u32; 8],
    block: [u8; 64],
    block_len: usize,
    total_len: u64,
}

impl Sha256 {
    pub(crate) fn new() -> Self {
        Self {
            h: H0,
            block: [0u8; 64],
            block_len: 0,
            total_len: 0,
        }
    }

    /// Absorbs `data`. The length counter is checked, so an input larger than
    /// 2^64 bytes is a typed error rather than a wrapped message length.
    pub(crate) fn feed(&mut self, data: &[u8]) -> Result<(), HarnessError> {
        let len = u64::try_from(data.len()).map_err(|_| HarnessError::ArithmeticOverflow {
            what: "input slice length",
            at: self.total_len,
        })?;
        self.total_len =
            self.total_len
                .checked_add(len)
                .ok_or(HarnessError::ArithmeticOverflow {
                    what: "sha256 message length",
                    at: self.total_len,
                })?;

        let mut rest = data;
        if self.block_len > 0 {
            let want = 64usize - self.block_len;
            let take = if rest.len() < want { rest.len() } else { want };
            if let (Some(slot), Some(piece)) = (
                self.block.get_mut(self.block_len..self.block_len + take),
                rest.get(..take),
            ) {
                slot.copy_from_slice(piece);
            }
            self.block_len += take;
            // `take` is `min(rest.len(), free)` by the guard above, so the split
            // is in range by construction. The unreachable branch reports itself
            // instead of yielding an empty slice, which would hash fewer bytes
            // and return a wrong digest with nothing to show for it.
            rest = rest.get(take..).ok_or(HarnessError::InternalInvariant {
                detail: "block split past the end of the remaining input",
            })?;
            if self.block_len == 64 {
                let finished = self.block;
                self.compress(&finished);
                self.block_len = 0;
            }
        }

        let mut chunks = rest.chunks_exact(64);
        for chunk in &mut chunks {
            let mut owned = [0u8; 64];
            owned.copy_from_slice(chunk);
            self.compress(&owned);
        }
        let tail = chunks.remainder();
        if !tail.is_empty() {
            if let Some(slot) = self.block.get_mut(0..tail.len()) {
                slot.copy_from_slice(tail);
            }
            self.block_len = tail.len();
        }
        Ok(())
    }

    pub(crate) fn finish(mut self) -> [u8; 32] {
        let bit_len = self.total_len.wrapping_mul(8);

        // The final message is the pending partial block, then 0x80, then zeros,
        // then the 64 bit big endian bit length, padded to a whole number of
        // 64 byte blocks. `block_len + 1 + 8` is the minimum, so the result is
        // one block unless the pending block leaves fewer than nine bytes.
        let mut buffer = [0u8; 128];
        if let (Some(dst), Some(src)) = (
            buffer.get_mut(..self.block_len),
            self.block.get(..self.block_len),
        ) {
            dst.copy_from_slice(src);
        }
        if let Some(marker) = buffer.get_mut(self.block_len) {
            *marker = 0x80;
        }
        let needed = self.block_len + 9;
        let total = if needed <= 64 { 64usize } else { 128usize };
        let length_at = total - 8;
        if let Some(slot) = buffer.get_mut(length_at..length_at + 8) {
            slot.copy_from_slice(&bit_len.to_be_bytes());
        }

        let mut offset = 0usize;
        while offset < total {
            let mut block = [0u8; 64];
            let end = offset + 64;
            if let (Some(slot), Some(piece)) = (block.get_mut(..), buffer.get(offset..end)) {
                slot.copy_from_slice(piece);
            }
            self.compress(&block);
            offset = end;
        }

        let mut out = [0u8; 32];
        for (i, word) in self.h.iter().enumerate() {
            if let Some(slot) = out.get_mut(i * 4..i * 4 + 4) {
                slot.copy_from_slice(&word.to_be_bytes());
            }
        }
        out
    }

    fn compress(&mut self, block: &[u8; 64]) {
        let mut w = [0u32; 64];
        for (i, slot) in w.iter_mut().enumerate().take(16) {
            let mut b = [0u8; 4];
            if let Some(raw) = block.get(i * 4..i * 4 + 4) {
                b.copy_from_slice(raw);
            }
            *slot = u32::from_be_bytes(b);
        }
        for i in 16..64usize {
            let a = word(&w, i, 15);
            let b = word(&w, i, 2);
            let s0 = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let s1 = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            let total = word(&w, i, 16)
                .wrapping_add(s0)
                .wrapping_add(word(&w, i, 7))
                .wrapping_add(s1);
            if let Some(slot) = w.get_mut(i) {
                *slot = total;
            }
        }

        let mut va = word(&self.h, 0, 0);
        let mut vb = word(&self.h, 1, 0);
        let mut vc = word(&self.h, 2, 0);
        let mut vd = word(&self.h, 3, 0);
        let mut ve = word(&self.h, 4, 0);
        let mut vf = word(&self.h, 5, 0);
        let mut vg = word(&self.h, 6, 0);
        let mut vh = word(&self.h, 7, 0);

        for i in 0..64usize {
            let s1 = ve.rotate_right(6) ^ ve.rotate_right(11) ^ ve.rotate_right(25);
            let ch = (ve & vf) ^ ((!ve) & vg);
            let t1 = vh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(word(&K, i, 0))
                .wrapping_add(word(&w, i, 0));
            let s0 = va.rotate_right(2) ^ va.rotate_right(13) ^ va.rotate_right(22);
            let maj = (va & vb) ^ (va & vc) ^ (vb & vc);
            let t2 = s0.wrapping_add(maj);

            vh = vg;
            vg = vf;
            vf = ve;
            ve = vd.wrapping_add(t1);
            vd = vc;
            vc = vb;
            vb = va;
            va = t1.wrapping_add(t2);
        }

        let sums = [
            word(&self.h, 0, 0).wrapping_add(va),
            word(&self.h, 1, 0).wrapping_add(vb),
            word(&self.h, 2, 0).wrapping_add(vc),
            word(&self.h, 3, 0).wrapping_add(vd),
            word(&self.h, 4, 0).wrapping_add(ve),
            word(&self.h, 5, 0).wrapping_add(vf),
            word(&self.h, 6, 0).wrapping_add(vg),
            word(&self.h, 7, 0).wrapping_add(vh),
        ];
        for (slot, value) in self.h.iter_mut().zip(sums.iter()) {
            *slot = *value;
        }
    }
}

/// Reads `table[index]`, returning zero for an index that is out of range.
///
/// SHA-256 only ever indexes a 64 element table with 0..64, so the fallback is
/// unreachable; it exists because `indexing_slicing` is denied workspace wide
/// and a `None`-returning helper would force a `Result` into the compression
/// inner loop, where there is nothing to report.
fn word(table: &[u32], index: usize, back: usize) -> u32 {
    let at = if back == 0 {
        Some(index)
    } else {
        index.checked_sub(back)
    };
    match at.and_then(|i| table.get(i)) {
        Some(v) => *v,
        None => 0,
    }
}

/// Lowercase hex SHA-256 of a byte slice, streamed so the length counter is the
/// only state that grows.
///
/// # Errors
///
/// [`HarnessError::ArithmeticOverflow`] when the input is longer than 2^64
/// bytes, which no real slice can be.
pub fn sha256_of_slice(data: &[u8]) -> Result<String, HarnessError> {
    let mut state = Sha256::new();
    state.feed(data)?;
    Ok(hex(&state.finish()))
}

/// Lowercase hex SHA-256 of a file, read in chunks so the file never has to
/// fit in memory.
///
/// # Errors
///
/// [`HarnessError::ReadFailed`] when the file cannot be opened or read, and
/// [`HarnessError::ArithmeticOverflow`] for a length that cannot be counted.
pub fn sha256_of_file(path: &Path) -> Result<String, HarnessError> {
    let mut file = File::open(path).map_err(|source| HarnessError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;
    let mut state = Sha256::new();
    let mut buf = vec![0u8; CHUNK];
    loop {
        let read = file
            .read(&mut buf)
            .map_err(|source| HarnessError::ReadFailed {
                path: path.to_path_buf(),
                source,
            })?;
        if read == 0 {
            break;
        }
        // `read` came from `read()` on this very buffer, so the prefix exists. The
        // unreachable branch reports rather than feeding an empty slice, which
        // would silently hash fewer bytes than the file holds.
        let chunk = buf.get(..read).ok_or(HarnessError::InternalInvariant {
            detail: "read length past the end of the fill buffer",
        })?;
        state.feed(chunk)?;
    }
    Ok(hex(&state.finish()))
}

/// Lowercase hex of `bytes`, two characters per byte.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len().wrapping_mul(2));
    for b in bytes {
        if let (Some(hi), Some(lo)) = (
            DIGITS.get(usize::from(b >> 4)),
            DIGITS.get(usize::from(b & 0x0F)),
        ) {
            out.push(char::from(*hi));
            out.push(char::from(*lo));
        }
    }
    out
}
