//! Test author's reference LZSS decoder. **Oracle class 4.**
//!
//! This is an independent reference implementation written from
//! `docs/spec/lzss.md` alone, for the test author's own use. Production code
//! never imports it: `fo1-tests` is a dev-dependency and no runtime crate may
//! depend on it (guide: Architecture; `tools/check_layers.sh` enforces it).
//!
//! Why it exists. If the expected bytes of a decode came from `fo1-lzss`, the
//! gate would grade the decoder with itself, which is the "grade with the code
//! under test" move the guide bans (Verification and anti faking, *Independent
//! oracles*). This decoder is written to be obviously correct rather than fast,
//! so that a disagreement points at the production code.
//!
//! **Its ring parameters are arguments, not constants** (guide: Hold out
//! vectors and canaries). That is the whole point of the type: a canary runs it
//! with `write_start = 4095` and with `write_start = 0` and asserts the bytes
//! change, so the ruling at LZSS-003 cannot be quietly changed without failing.

/// Ring size in the canonical ruling (LZSS-001).
pub const CANONICAL_RING_SIZE: usize = 4096;

/// Write index start in the canonical ruling (LZSS-003, LZSS-020).
pub const CANONICAL_WRITE_START: usize = 4078;

/// Ring prefill byte in the canonical ruling (LZSS-002).
pub const CANONICAL_PREFILL: u8 = 0x20;

/// Shortest run a back reference can encode (LZSS-008).
pub const MIN_MATCH: usize = 3;

/// Longest run a back reference can encode (LZSS-008).
pub const MAX_MATCH: usize = 18;

/// Keeps the high four bits of the second byte: the top of the source position.
const POSITION_HIGH_MASK: u8 = 0xF0;

/// Keeps the low four bits of the second byte: the length bias.
const LENGTH_BIAS_MASK: u8 = 0x0F;

/// Everything the reference decoder can fail at. No variant carries payload
/// bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefError {
    /// A token would write past the declared output size (LZSS-014).
    OutputOverrun {
        /// Byte offset into `input` where the token started.
        at: usize,
        /// The declared output size.
        declared_out: usize,
        /// Output bytes produced before the offending token.
        produced: usize,
        /// How many bytes that one token would have written.
        token_len: usize,
    },
    /// The input ran out before the declared output size was reached
    /// (LZSS-014).
    OutputShort {
        /// Byte offset into `input` where the input ran out.
        at: usize,
        /// The declared output size.
        declared_out: usize,
        /// Output bytes produced.
        produced: usize,
    },
    /// A back reference started with fewer than two input bytes left
    /// (LZSS-015). The original binary underflows here; this returns an error.
    TruncatedMatch {
        /// Byte offset of the incomplete pair.
        at: usize,
        /// The declared output size.
        declared_out: usize,
        /// Output bytes produced.
        produced: usize,
        /// Input bytes left when the pair started.
        remaining_input: usize,
    },
    /// The ring parameters cannot be used. `decode` reports this rather than
    /// silently substituting a workable value.
    BadRingParameters {
        /// What was wrong.
        what: &'static str,
        /// The value that was wrong.
        value: usize,
    },
}

impl core::fmt::Display for RefError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OutputOverrun {
                at,
                declared_out,
                produced,
                token_len,
            } => write!(
                f,
                "reference lzss: token at input offset {at} would write {token_len} bytes past \
                 the declared output size {declared_out} (produced {produced})"
            ),
            Self::OutputShort {
                at,
                declared_out,
                produced,
            } => write!(
                f,
                "reference lzss: input ended at offset {at} with {produced} of {declared_out} \
                 declared output bytes produced"
            ),
            Self::TruncatedMatch {
                at,
                declared_out,
                produced,
                remaining_input,
            } => write!(
                f,
                "reference lzss: back reference at input offset {at} started with \
                 {remaining_input} input byte(s) left, needs 2 (produced {produced} of \
                 {declared_out})"
            ),
            Self::BadRingParameters { what, value } => {
                write!(f, "reference lzss: bad ring parameters, {what} is {value}")
            }
        }
    }
}

impl std::error::Error for RefError {}

/// A reference decoder with caller chosen ring parameters.
///
/// Constructing one is cheap and the type is immutable: a canary can hold
/// several variants side by side and compare their output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefLzss {
    ring_size: usize,
    write_start: usize,
    prefill_byte: u8,
}

impl RefLzss {
    /// Builds a decoder with the given ring parameters.
    ///
    /// Every write index start the ruling discusses is legal, including 0,
    /// 4078 and 4095, and the canaries exercise all three. `decode` rejects a
    /// ring size of zero or a write start outside the ring, so this
    /// constructor has nothing to fail at.
    #[must_use]
    pub fn new(ring_size: usize, write_start: usize, prefill_byte: u8) -> Self {
        Self {
            ring_size,
            write_start,
            prefill_byte,
        }
    }

    /// The canonical variant: 4096 byte ring, write index 4078, prefill `0x20`.
    #[must_use]
    pub fn canonical() -> Self {
        Self::new(
            CANONICAL_RING_SIZE,
            CANONICAL_WRITE_START,
            CANONICAL_PREFILL,
        )
    }

    /// Ring size in bytes.
    #[must_use]
    pub fn ring_size(&self) -> usize {
        self.ring_size
    }

    /// Where the write head starts in a fresh ring.
    #[must_use]
    pub fn write_start(&self) -> usize {
        self.write_start
    }

    /// Byte a fresh ring is filled with.
    #[must_use]
    pub fn prefill_byte(&self) -> u8 {
        self.prefill_byte
    }

    /// Decodes one whole LZSS stream.
    ///
    /// `declared_out` is used **only** to detect an overrun and a shortfall. It
    /// is never a stopping condition and the output is never cut, padded or
    /// defaulted to reach it (LZSS-014).
    ///
    /// # Errors
    ///
    /// [`RefError::BadRingParameters`] when this variant's ring size is zero
    /// or its write start is outside the ring, [`RefError::OutputOverrun`],
    /// [`RefError::OutputShort`] or [`RefError::TruncatedMatch`].
    pub fn decode(&self, input: &[u8], declared_out: usize) -> Result<Vec<u8>, RefError> {
        if self.ring_size == 0 {
            return Err(RefError::BadRingParameters {
                what: "ring_size",
                value: self.ring_size,
            });
        }
        if self.write_start >= self.ring_size {
            return Err(RefError::BadRingParameters {
                what: "write_start",
                value: self.write_start,
            });
        }

        // A fresh ring per stream (LZSS-016), and both halves of the prefill rule
        // are honoured (LZSS-002): slots `0..write_start` hold `prefill_byte`,
        // slots `write_start..ring_size` start as zero. The tail is not an
        // implementation detail of the canonical parameters; it is what makes
        // this oracle agree with the specification at a slot such as 4078, and a
        // canary that varies `prefill_byte` must not be the only way to reach a
        // zero in the tail.
        //
        // `resize` twice rather than filling the whole ring and then overwriting
        // a range, because a range on a `Vec` is an index expression.
        let mut ring: Vec<u8> = Vec::with_capacity(self.ring_size);
        ring.resize(self.write_start, self.prefill_byte);
        ring.resize(self.ring_size, 0);
        let mut write_index = self.write_start;
        let mut out: Vec<u8> = Vec::new();

        // The input cursor ends the block, never the output length (LZSS-013).
        let mut ip = 0usize;
        'outer: while ip < input.len() {
            let control = match input.get(ip) {
                Some(b) => *b,
                None => break,
            };
            ip += 1;

            for bit in 0..8u32 {
                if ip >= input.len() {
                    // Mid control byte: the normal end of a block, 85.5% of
                    // them (LZSS-013). The remaining bits are discarded.
                    return finish(out, input.len(), declared_out);
                }

                let is_literal = 1u8.checked_shl(bit).is_some_and(|m| control & m != 0);

                if is_literal {
                    let byte = match input.get(ip) {
                        Some(b) => *b,
                        None => {
                            return Err(RefError::OutputShort {
                                at: ip,
                                declared_out,
                                produced: out.len(),
                            })
                        }
                    };
                    push_one(&mut out, byte, declared_out, ip)?;
                    if let Some(slot) = ring.get_mut(write_index) {
                        *slot = byte;
                    }
                    write_index = (write_index + 1) % self.ring_size;
                    ip += 1;
                } else {
                    let remaining = input.len() - ip;
                    if remaining < 2 {
                        return Err(RefError::TruncatedMatch {
                            at: ip,
                            declared_out,
                            produced: out.len(),
                            remaining_input: remaining,
                        });
                    }
                    let pair = match input.get(ip..ip + 2) {
                        Some(p) => p,
                        None => break 'outer,
                    };
                    let b1 = match pair.first() {
                        Some(b) => *b,
                        None => break 'outer,
                    };
                    let b2 = match pair.get(1) {
                        Some(b) => *b,
                        None => break 'outer,
                    };

                    // Absolute ring position, never a distance (LZSS-009).
                    let source = usize::from(b1) | (usize::from(b2 & POSITION_HIGH_MASK) << 4);
                    let length = usize::from(b2 & LENGTH_BIAS_MASK) + MIN_MATCH;

                    // One back reference writes `length` bytes. The room check happens once, on the
                    // whole token, because LZSS-014 says "do not write the
                    // token", not "do not write its 18th byte".
                    if out.len() + length > declared_out {
                        return Err(RefError::OutputOverrun {
                            at: ip,
                            declared_out,
                            produced: out.len(),
                            token_len: length,
                        });
                    }

                    // Read before write, one byte at a time, so an overlapping
                    // run replicates itself (LZSS-011).
                    let mut cursor = source % self.ring_size;
                    for _ in 0..length {
                        let byte = match ring.get(cursor) {
                            Some(b) => *b,
                            None => {
                                return Err(RefError::BadRingParameters {
                                    what: "source index",
                                    value: cursor,
                                })
                            }
                        };
                        out.push(byte);
                        if let Some(slot) = ring.get_mut(write_index) {
                            *slot = byte;
                        }
                        cursor = (cursor + 1) % self.ring_size;
                        write_index = (write_index + 1) % self.ring_size;
                    }
                    ip += 2;
                }
            }
        }

        finish(out, input.len(), declared_out)
    }
}

fn push_one(out: &mut Vec<u8>, byte: u8, declared_out: usize, at: usize) -> Result<(), RefError> {
    if out.len() + 1 > declared_out {
        return Err(RefError::OutputOverrun {
            at,
            declared_out,
            produced: out.len(),
            token_len: 1,
        });
    }
    out.push(byte);
    Ok(())
}

fn finish(out: Vec<u8>, consumed: usize, declared_out: usize) -> Result<Vec<u8>, RefError> {
    if out.len() != declared_out {
        return Err(RefError::OutputShort {
            at: consumed,
            declared_out,
            produced: out.len(),
        });
    }
    Ok(out)
}
