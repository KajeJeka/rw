//! Total LZSS decoder.
//!
//! One 4096 byte ring, write index starting at 4078, one control byte per
//! eight tokens read from bit 0 upwards, back references of two bytes with the
//! length in the low nibble of the second byte (3 to 18).
//!
//! The decoder receives the declared output size **only** to detect an
//! overrun. It never cuts, pads or defaults to reach a length, because a
//! decoder that stops at the declared size passes a size gate for any input
//! (guide: No truncation, and a consumption check).
//!
//! Every LZSS block inside a DAT block chain starts with a fresh ring buffer.
//!
//! # How this crate satisfies its own lint table
//!
//! `#![deny(clippy::arithmetic_side_effects)]` is in force, so there is no `+`,
//! `-`, `%` or `<<` on any index or length anywhere in the decoder. The four
//! places arithmetic would have been are `next_slot`, `plus_one`,
//! `decode_token` and `reserve`, and each one names its failure instead of
//! swallowing it:
//!
//! - `next_slot` is the only ring index arithmetic on the hot path. It advances
//!   one slot and wraps at `RING_SIZE` without a modulo.
//! - `plus_one` advances the input cursor.
//! - `decode_token` splits a token pair into a position and a length.
//! - `reserve` is the single place the overrun check happens, so a token cannot
//!   be checked on one branch and forgotten on the other.
//!
//! Every ring and input access goes through `Vec::get` or `Vec::get_mut` and
//! matches on the `Option`, because `clippy::indexing_slicing` is denied. There
//! is no `unsafe`, no `unwrap`, no `expect`, no `panic!` and no cast.
//!
//! See `docs/spec/lzss.md`.

#![deny(clippy::arithmetic_side_effects)]

/// Ring slots. The write head wraps at this value (LZSS-001).
pub const RING_SIZE: usize = 4096;

/// Value prefilled into ring slots `0..RING_WRITE_START` (LZSS-002).
///
/// Slots `RING_WRITE_START..RING_SIZE` start as zero. Both halves of that rule
/// are load bearing: a decoder that prefills the whole ring with `RING_FILL`, or
/// with zero, decodes the corpus to plausible bytes that are wrong.
pub const RING_FILL: u8 = 0x20;

/// Where the write head starts inside every fresh ring (LZSS-003, LZSS-020).
///
/// `RING_SIZE - RING_WRITE_START` is 18, the standard LZSS lookahead, which is
/// why the ruling is this value and not 0, 4077, 4095 or 4096.
pub const RING_WRITE_START: usize = 4078;

/// Shortest run a back reference can encode (LZSS-008).
pub const MIN_MATCH: usize = 3;

/// Longest run a back reference can encode (LZSS-008).
pub const MAX_MATCH: usize = 18;

/// Mask that keeps the high four bits of the second byte, which hold the top of
/// the source position (LZSS-008).
const POSITION_HIGH_MASK: u8 = 0xF0;

/// Mask that keeps the low four bits of the second byte, which hold the length
/// bias (LZSS-008).
const LENGTH_BIAS_MASK: u8 = 0x0F;

/// Bits the position's high nibble is shifted left into `b1`'s eight bits
/// (LZSS-008).
const POSITION_HIGH_SHIFT: u32 = 4;

/// Tokens one control byte carries, bit 0 first (LZSS-005).
const TOKENS_PER_CONTROL: u32 = 8;

/// The bit `0` of a control byte selects the first token (LZSS-005). Shifting it
/// by the token's bit number selects the rest.
const TOKEN_BIT: u8 = 1;

/// What a fresh ring holds in slots `RING_WRITE_START..RING_SIZE` (LZSS-002).
const ZERO_BYTE: u8 = 0x00;

/// Everything the decoder can fail at. No variant carries payload bytes: an
/// `LzssError` reports where and how big, never what.
///
/// Fields common to all variants: `at` is the byte offset into the input slice
/// at which the failure was detected, `declared_out` is the size the caller
/// declared for the whole stream or remaining entry, and `produced` is how many
/// output bytes existed when the failure was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LzssError {
    /// The tokens would write more than the declared output size (LZSS-014).
    ///
    /// The offending token is not written at all: `produced` is the count
    /// before it, and `token_len` is how many bytes it would have written.
    OutputOverrun {
        /// Byte offset into `input` where the offending token started.
        at: usize,
        /// The size the caller declared for this block.
        declared_out: usize,
        /// Output bytes the decoder held when the token was refused.
        produced: usize,
        /// How many bytes that one token would have written.
        token_len: usize,
    },
    /// The input ran out before the declared output size was reached
    /// (LZSS-014).
    ///
    /// Reported by [`decode_into`], which can see the end of the input, and not
    /// by a chain block, where a shortfall at a block boundary is the normal
    /// case rather than an error (LZSS-024, decision 3).
    OutputShort {
        /// Byte offset into `input` at which the input ran out.
        at: usize,
        /// The size the caller declared for this block.
        declared_out: usize,
        /// Output bytes produced before the input ran out.
        produced: usize,
        /// Bytes of `input` that were consumed.
        consumed: usize,
    },
    /// A back reference started with fewer than two input bytes left.
    /// The original binary underflows here and loops; this crate does not
    /// (LZSS-015).
    TruncatedMatch {
        /// Byte offset of the incomplete pair.
        at: usize,
        /// The size the caller declared for this block.
        declared_out: usize,
        /// Output bytes produced before the incomplete pair.
        produced: usize,
        /// Input bytes left where the pair needed two.
        remaining_input: usize,
    },
    /// Ring arithmetic did not fit in `usize`. Unreachable with the constants
    /// above, and kept so the crate compiles under the arithmetic lint rather
    /// than by accident (LZSS-024, decision 5).
    IndexOverflow {
        /// Byte offset into `input` at which the arithmetic was attempted.
        at: usize,
        /// The size the caller declared for this block.
        declared_out: usize,
        /// Output bytes produced when the arithmetic was attempted.
        produced: usize,
    },
}

impl core::fmt::Display for LzssError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutputOverrun {
                at,
                declared_out,
                produced,
                token_len,
            } => write!(
                f,
                "fo1-lzss: the token at input offset {at} would write {token_len} byte(s) past \
                 the declared output size {declared_out} ({produced} produced)"
            ),
            Self::OutputShort {
                at,
                declared_out,
                produced,
                consumed,
            } => write!(
                f,
                "fo1-lzss: the input ran out at offset {at} with {produced} of {declared_out} \
                 declared output bytes produced ({consumed} input byte(s) consumed)"
            ),
            Self::TruncatedMatch {
                at,
                declared_out,
                produced,
                remaining_input,
            } => write!(
                f,
                "fo1-lzss: the back reference at input offset {at} started with \
                 {remaining_input} input byte(s) left and needs two ({produced} of \
                 {declared_out} produced)"
            ),
            Self::IndexOverflow {
                at,
                declared_out,
                produced,
            } => write!(
                f,
                "fo1-lzss: ring arithmetic overflowed at input offset {at} \
                 ({produced} of {declared_out} produced)"
            ),
        }
    }
}

impl std::error::Error for LzssError {}

/// One ring and one output buffer, reused across the LZSS blocks of a chain.
///
/// A chain calls [`LzssDecoder::decode_block`] once per block. Each call begins
/// with a freshly filled ring and a write head at [`RING_WRITE_START`]; the
/// output buffer is the only thing that continues across a block boundary
/// (LZSS-016).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LzssDecoder {
    /// Slots `0..RING_WRITE_START` hold [`RING_FILL`], the rest holds zero.
    ring: Vec<u8>,
    /// The next slot a literal or a copied byte is written to.
    write_index: usize,
    /// Decoded bytes. Only this crosses a block boundary.
    output: Vec<u8>,
}

impl Default for LzssDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl LzssDecoder {
    /// Allocates a decoder with a fresh ring. Holds no state across calls
    /// beyond the ring and the output buffer it was constructed with.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ring: fresh_ring(),
            write_index: RING_WRITE_START,
            output: Vec::new(),
        }
    }

    /// Returns the decoder to its just-constructed state: an empty output buffer,
    /// a freshly filled ring and a write head at [`RING_WRITE_START`].
    ///
    /// Called between entries by any caller that pools one decoder across many
    /// archives. A decoder that keeps one ring across a chain produces the right
    /// total length for many chains and wrong bytes for all of them, which is
    /// why [`LzssDecoder::decode_block`] already refills the ring on every call
    /// and this method exists for the caller's own bookkeeping.
    pub fn reset(&mut self) {
        self.output.clear();
        self.ring = fresh_ring();
        self.write_index = RING_WRITE_START;
    }

    /// Decodes one LZSS block, appending to the internal output buffer.
    ///
    /// Every call begins with a fresh ring (LZSS-016). `declared_out` is the
    /// output size **this block** must still reach, which on a chain block is
    /// `entry.uncompressed_size - already_produced` because an LZSS block inside
    /// a chain declares only its compressed length (LZSS-024, decision 2).
    ///
    /// The declared size is never a stopping condition: the block ends when its
    /// input is used up, even part way through a control byte, which is how
    /// 52,229 of the 61,070 blocks in the reference install end (LZSS-013).
    /// `declared_out` is only ever compared by the overrun check, and the output
    /// is never cut, padded or defaulted to reach it (LZSS-014).
    ///
    /// A shortfall against `declared_out` is **not** an error here. A block
    /// inside a chain always ends by input exhaustion, so the check belongs
    /// where the whole entry is visible: in [`decode_into`] for a whole stream,
    /// and in the chain driver for an entry (LZSS-024, decision 3).
    ///
    /// On an error the bytes decoded before the failure stay in the buffer and
    /// [`LzssDecoder::produced`] still reports them.
    pub fn decode_block(&mut self, input: &[u8], declared_out: usize) -> Result<(), LzssError> {
        let base = self.output.len();
        self.ring = fresh_ring();
        self.write_index = RING_WRITE_START;

        let mut ip = 0usize;
        while let Some(control) = input.get(ip).copied() {
            ip = plus_one(ip, ip, declared_out, self.output.len())?;

            for bit in 0..TOKENS_PER_CONTROL {
                // LZSS-013. The input cursor ends the block, never the produced
                // length, so a control byte whose last tokens have no input
                // behind them ends the block and discards those bits. The single
                // read serves the termination test and the literal token below.
                let next_byte = match input.get(ip).copied() {
                    Some(byte) => byte,
                    None => return Ok(()),
                };

                let mask = match TOKEN_BIT.checked_shl(bit) {
                    Some(mask) => mask,
                    None => return Err(index_overflow(ip, declared_out, self.output.len())),
                };

                if control & mask != 0 {
                    // Literal (LZSS-006).
                    self.reserve(1, base, declared_out, ip)?;
                    self.output.push(next_byte);
                    self.ring_put(self.write_index, next_byte, ip, declared_out)?;
                    self.step_head(ip, declared_out)?;
                    ip = plus_one(ip, ip, declared_out, self.output.len())?;
                } else {
                    // Back reference (LZSS-007): b1 then b2, in that order.
                    // `next_byte` is b1. One input byte left here is an error;
                    // the original binary underflows its length counter and loops
                    // in this state and this crate does not (LZSS-015).
                    let pair_end = plus_one(ip, ip, declared_out, self.output.len())?;
                    let b2 = match input.get(pair_end) {
                        Some(byte) => *byte,
                        None => {
                            return Err(LzssError::TruncatedMatch {
                                at: ip,
                                declared_out,
                                produced: self.output.len(),
                                remaining_input: input.len().saturating_sub(ip),
                            })
                        }
                    };

                    let (source, length) = match decode_token(next_byte, b2) {
                        Some(token) => token,
                        None => return Err(index_overflow(ip, declared_out, self.output.len())),
                    };

                    // The overrun check covers the whole token: LZSS-014 says do
                    // not write the token, not do not write its eighteenth byte.
                    self.reserve(length, base, declared_out, ip)?;

                    // Read before write, one byte at a time (LZSS-011). This is
                    // the only place in the crate a ring byte is copied, so the
                    // ordering cannot be right in one branch and wrong in
                    // another: a run whose source trails the write head by less
                    // than its own length reads the bytes it has just written
                    // and therefore replicates itself.
                    let mut source = source;
                    for _ in 0..length {
                        let byte = self.ring_get(source, ip, declared_out)?;
                        self.ring_put(self.write_index, byte, ip, declared_out)?;
                        self.output.push(byte);
                        source = match next_slot(source) {
                            Some(slot) => slot,
                            None => {
                                let produced = self.output.len();
                                return Err(index_overflow(ip, declared_out, produced));
                            }
                        };
                        self.step_head(ip, declared_out)?;
                    }

                    ip = plus_one(pair_end, ip, declared_out, self.output.len())?;
                }
            }
        }
        Ok(())
    }

    /// Number of output bytes accumulated so far, across every block decoded
    /// since construction or the last [`LzssDecoder::take_output`].
    #[must_use]
    pub fn produced(&self) -> usize {
        self.output.len()
    }

    /// Hands the accumulated output to the caller, leaving the decoder empty.
    ///
    /// The ring is untouched; the next [`LzssDecoder::decode_block`] refills it
    /// anyway, because every block starts with a fresh ring (LZSS-016).
    #[must_use]
    pub fn take_output(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.output)
    }

    /// The single place the overrun check happens (LZSS-014).
    ///
    /// `base` is the output length when this block began, so the check is a
    /// running total against the size the caller declared for **this** block.
    /// The token is refused whole: nothing it would have written is written.
    fn reserve(
        &mut self,
        token_len: usize,
        base: usize,
        declared_out: usize,
        at: usize,
    ) -> Result<(), LzssError> {
        let produced = self.output.len();
        let written = produced.saturating_sub(base);
        let wanted = match written.checked_add(token_len) {
            Some(wanted) => wanted,
            None => return Err(index_overflow(at, declared_out, produced)),
        };
        if wanted > declared_out {
            return Err(LzssError::OutputOverrun {
                at,
                declared_out,
                produced,
                token_len,
            });
        }
        Ok(())
    }

    /// Reads one ring slot. `None` means the slot was not inside the ring, which
    /// no token can ask for: `b1` contributes at most `0xFF` and the high nibble
    /// at most `0xF00`, so the largest addressable source is `RING_SIZE - 1`
    /// (LZSS-012).
    fn ring_get(&self, slot: usize, at: usize, declared_out: usize) -> Result<u8, LzssError> {
        match self.ring.get(slot) {
            Some(byte) => Ok(*byte),
            None => Err(index_overflow(at, declared_out, self.output.len())),
        }
    }

    /// Writes one ring slot.
    fn ring_put(
        &mut self,
        slot: usize,
        byte: u8,
        at: usize,
        declared_out: usize,
    ) -> Result<(), LzssError> {
        let produced = self.output.len();
        match self.ring.get_mut(slot) {
            Some(dest) => {
                *dest = byte;
                Ok(())
            }
            None => Err(index_overflow(at, declared_out, produced)),
        }
    }

    /// Advances the write head one slot, wrapping at [`RING_SIZE`].
    fn step_head(&mut self, at: usize, declared_out: usize) -> Result<(), LzssError> {
        let next = match next_slot(self.write_index) {
            Some(slot) => slot,
            None => return Err(index_overflow(at, declared_out, self.output.len())),
        };
        self.write_index = next;
        Ok(())
    }
}

/// Decodes one LZSS block into `decoder`, appending to its output buffer.
///
/// This is the free-function spelling of
/// [`LzssDecoder::decode_block`] and is exactly equivalent to it, so a caller
/// may use either. `docs/spec/lzss.md` LZSS-024 states both.
pub fn decode_block(
    input: &[u8],
    decoder: &mut LzssDecoder,
    declared_out: usize,
) -> Result<(), LzssError> {
    decoder.decode_block(input, declared_out)
}

/// Decodes one complete LZSS stream of known declared output size.
///
/// This is the whole-stream entry point: it is `LzssDecoder::new()`, one
/// `decode_block`, then `take_output()`, so the two entry points cannot
/// disagree. It enforces both halves of LZSS-014, the overrun check and the
/// shortfall check, because it can see the end of the input. The output is never
/// cut, padded or defaulted to reach `declared_out`.
pub fn decode_into(input: &[u8], declared_out: usize) -> Result<Vec<u8>, LzssError> {
    let mut decoder = LzssDecoder::new();
    decoder.decode_block(input, declared_out)?;

    if decoder.produced() != declared_out {
        return Err(LzssError::OutputShort {
            at: input.len(),
            declared_out,
            produced: decoder.produced(),
            consumed: input.len(),
        });
    }
    Ok(decoder.take_output())
}

/// A fresh ring: [`RING_FILL`] in slots `0..RING_WRITE_START`, zero in the rest
/// (LZSS-001, LZSS-002).
///
/// `resize` twice rather than filling the whole ring and then overwriting a
/// range, because a range on a `Vec` is an index expression and
/// `clippy::indexing_slicing` is denied.
fn fresh_ring() -> Vec<u8> {
    let mut ring = Vec::with_capacity(RING_SIZE);
    ring.resize(RING_WRITE_START, RING_FILL);
    ring.resize(RING_SIZE, ZERO_BYTE);
    ring
}

/// One slot further around the ring, wrapping at [`RING_SIZE`] (LZSS-001).
///
/// This is the only ring index arithmetic on the hot path, and it is total.
/// `None` means the incoming index was not a ring slot: no token can ask for one
/// (LZSS-012) and the write head starts inside the ring and only ever moves
/// through here, so a `None` is the unreachable arm that keeps the arithmetic
/// lint satisfiable by construction rather than by an `allow`.
fn next_slot(index: usize) -> Option<usize> {
    let next = index.checked_add(1)?;
    if next < RING_SIZE {
        Some(next)
    } else if next == RING_SIZE {
        Some(0)
    } else {
        None
    }
}

/// One past `value`, or the arithmetic failure the crate lint forces to be
/// named (LZSS-024, decision 5). `ok_or_else` so the hot path builds no error.
fn plus_one(
    value: usize,
    at: usize,
    declared_out: usize,
    produced: usize,
) -> Result<usize, LzssError> {
    value
        .checked_add(1)
        .ok_or_else(|| index_overflow(at, declared_out, produced))
}

/// Splits one back reference pair into an absolute ring position and a run
/// length (LZSS-007, LZSS-008, LZSS-009).
///
/// `b1` carries the low eight bits of the position and `b2` both its high four
/// bits and the length bias. The position is an absolute ring slot, never a
/// distance. `None` covers the shift and the addition the arithmetic lint makes
/// explicit; a four bit nibble can lose neither, so no byte pair reaches it.
fn decode_token(b1: u8, b2: u8) -> Option<(usize, usize)> {
    let high = usize::from(b2 & POSITION_HIGH_MASK).checked_shl(POSITION_HIGH_SHIFT)?;
    let source = usize::from(b1) | high;
    let length = usize::from(b2 & LENGTH_BIAS_MASK).checked_add(MIN_MATCH)?;
    Some((source, length))
}

/// The one arithmetic failure the crate's constants cannot reach, built in one
/// place so every arm constructs it identically.
fn index_overflow(at: usize, declared_out: usize, produced: usize) -> LzssError {
    LzssError::IndexOverflow {
        at,
        declared_out,
        produced,
    }
}
