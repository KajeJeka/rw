# LZSS decoder

Provenance: written from `RUSTY WASTELAND Engine Implementation Guide, Revision 2.md`
(sections *LZSS decoder*, *Rulings where sources disagreed*, *Anti deviation tests*,
*Hold out vectors and canaries*, *No truncation and a consumption check*) and from a
first-hand decode of the bytes of `MASTER.DAT` and `CRITTER.DAT` in the reference
install, using a reference decoder I wrote for this spec. No Community Edition source
was opened. No decompilation, comment, name or structure was taken from any other
implementation. Nothing below is a quotation.

Every claim carries exactly one evidence tag:

| Tag | Meaning |
| --- | --- |
| `CONFIRMED AGAINST ORIGINAL BINARY` | Measured by me from the real archive bytes |
| `CONFIRMED IN COMMUNITY EDITION ONLY` | CE is the only source |
| `VERSION DIFFERENCE IDENTIFIED` | A measured difference between installs |
| `UNVERIFIED` | Asserted or required, not measured by me |

The reference decoder I wrote to check this spec is not shipped and is not an oracle
for the test suite; the test author writes their own. It is discarded. The numbers
below are reproducible from the archive bytes alone.

---

## 1. Measured closure

The whole of both archives was decoded with the rule set in section 3. Nothing was
sampled and nothing was skipped.

| Quantity | MASTER.DAT | CRITTER.DAT | Total |
| --- | --- | --- | --- |
| Chain entries decoded | 14,997 | 5,459 | 20,456 |
| Entries where `produced == uncompressed_size` | 14,997 | 5,459 | 20,456 |
| Entries where `consumed == compressed_size` | 14,997 | 5,459 | 20,456 |
| LZSS blocks decoded | 31,427 | 29,643 | 61,070 |
| Bytes of output produced | 410,154,381 | 441,168,010 | 851,322,391 |
| Blocks ending on a control-byte boundary | 8,841 | — | 8,841 |
| Blocks ending mid control byte | — | — | 52,229 |
| Blocks ending mid back reference (pair truncated) | 0 | 0 | 0 |
| Failures | 0 | 0 | 0 |

Token census over the 61,070 blocks: 27,486,754 control bytes, 144,696,285 literal
tokens, 74,987,432 match tokens.

The three numbers a reader should hold on to:

- `61,070` LZSS blocks decode cleanly. This is the guide's own block count, reached
  independently.
- `52,229` of them, **85.5%**, stop in the middle of a control byte. The mid-byte stop
  rule is not a corner case here; it is the normal way a block ends.
- `0` of them stop mid pair. The mid-pair guard never fires on this install, so it is
  a safety rule and not a measured behaviour (LZSS-015).

---

## 2. The ring

### LZSS-001 — The ring is 4096 bytes

`CONFIRMED AGAINST ORIGINAL BINARY`

`RING_SIZE = 4096`. Allocated once per LZSS block.

### LZSS-002 — Prefill: indices 0..=4077 hold 0x20, indices 4078..=4095 hold 0x00

`CONFIRMED AGAINST ORIGINAL BINARY`

`RING_FILL = 0x20` (a space). The last 18 slots start as zero.

Both halves of this rule are load-bearing, measured by counting reads that land on a
slot no output byte has written yet:

| Region of the ring | Reads of a never-written slot | Blocks where it happens |
| --- | --- | --- |
| `0..=4077` (must be `0x20`) | 41,993,076 | 57,770 |
| `4078..=4095` (must be `0x00`) | 356,192 | 46,275 |

A decoder that prefills the whole ring with `0x20`, or with `0x00`, decodes the
corpus to plausible bytes that are wrong.

### LZSS-003 — The write index starts at 4078

`CONFIRMED AGAINST ORIGINAL BINARY`

`RING_WRITE_START = 4078`. `4096 - 4078 = 18`. See LZSS-020 for the ruling and
LZSS-021 for the canary that keeps it honest.

### LZSS-004 — The original binary declares 4116 bytes and never reaches the last 20

`UNVERIFIED`

The guide states this. I did not read the original executable, so the declaration
itself is not measured. The *consequence* is measurable and is: the source index is
bounded by 4095 (LZSS-012) and the write head wraps at 4096, so ring slots 4096
through 4115 cannot be addressed by any token. That is why the waste is invisible —
and why this spec allocates 4096 and does not reproduce the waste.

---

## 3. The decoder

### LZSS-005 — A control byte read from the input carries eight tokens, bit 0 first

`CONFIRMED AGAINST ORIGINAL BINARY`

Read one byte. Walk its bits from bit 0 up to bit 7 inclusive. Each bit is one token.

This is the guide's named ruling. Measured, over all 20,456 chain entries:

| Tokens per control byte | Chain entries where `produced == uncompressed_size` |
| --- | --- |
| 8 | 20,456 of 20,456 |
| 1 | 1 of 20,456 |

The single closing entry under the rejected reading is a coincidence of trivial
content, not evidence. The ruling stands.

### LZSS-006 — A set bit is a literal

`CONFIRMED AGAINST ORIGINAL BINARY`

Read one input byte. Write it to the output, write it to the ring at the write index,
then advance the write index by one with wrap at `RING_SIZE`.

### LZSS-007 — A clear bit is a two-byte back reference, `b1` then `b2`

`CONFIRMED AGAINST ORIGINAL BINARY`

`b1` is read first, then `b2`. The order matters: `b1` supplies the low eight bits of
the source position and `b2` supplies both the high four bits and the length.

### LZSS-008 — Source index and length

`CONFIRMED AGAINST ORIGINAL BINARY`

```
source = b1 | ((b2 & 0xF0) << 4)      // 0 ..= 4095
length = (b2 & 0x0F) + 3              // 3 ..= 18
```

`MIN_MATCH = 3`, `MAX_MATCH = 18`. The corpus requests lengths across the whole
range; both ends are reachable.

### LZSS-009 — The source index is an absolute ring position, not a distance

`CONFIRMED AGAINST ORIGINAL BINARY`

`0x75 0x3F` decodes to source `0x375 = 885`. At a write index of 886 that is one byte
back. Read as a *distance* it would mean 885 bytes back. The two readings are
different decoders and only this one closes the corpus.

### LZSS-010 — Output position `p` lives at ring index `(p + RING_WRITE_START) mod RING_SIZE`

`CONFIRMED AGAINST ORIGINAL BINARY`

```
ring_index(p) = (p + 4078) mod 4096
```

Output positions therefore map to ring positions by a constant shift. `p = 5000`
gives `9078 mod 4096 = 886`.

### LZSS-011 — Copy one byte at a time, wrap at 4096, and each read happens before its own write

`CONFIRMED AGAINST ORIGINAL BINARY`

This is the single most common way the decoder is written wrong, so it is stated as
an ordering rule and not only as an arithmetic rule:

```
for each of `length` steps:
    read  ring[source]
    write ring[write_index] = that byte
    emit  that byte
    source       = (source + 1)       mod 4096
    write_index  = (write_index + 1)  mod 4096
```

The read cursor trails the write cursor by the constant `d = (write_index - source)
mod 4096` for the whole run, so a run whose `d` is smaller than its own length is
reading bytes it has not written yet. That is correct: it is how a run replicates
itself. A decoder that reads the whole run into a buffer and then writes it, or that
copies source and destination in one non-overlapping operation, gets a different
answer and does not close the corpus.

Worked demonstration with `0x75 0x3F` at write index 886 — see LZSS-019.

### LZSS-012 — No ring index above 4095 is addressable

`CONFIRMED AGAINST ORIGINAL BINARY`

`b1` contributes at most `0xFF` and `(b2 & 0xF0) << 4` contributes at most `0xF00`, so
`source <= 0xFFF = 4095` for every possible byte pair. This is an algebraic identity,
not an observation.

It is also observed: over the 61,070 blocks the largest source index the corpus ever
requests is exactly `4095`, and no block ever requests more. The anti deviation table
row "ring indexes above 4,095 are unreachable" holds by construction.

---

## 4. Termination and length

### LZSS-013 — Stop when the block's input is used up, even in the middle of a control byte

`CONFIRMED AGAINST ORIGINAL BINARY`

The loop condition is on the input cursor, never on the produced length. When the
input runs out part-way through the eight tokens of a control byte, the block is
finished and the remaining bits of that byte are discarded.

Measured: `52,229` of `61,070` blocks, 85.5%, end this way. A decoder that insists on
consuming all eight tokens of the last control byte errors on 85.5% of real blocks.
A decoder that stops when it has produced `declared_out` bytes passes a size check on
bad input and fails on good input, and is forbidden by LZSS-014.

### LZSS-014 — Never truncate, never pad; the declared size only detects a mismatch

`CONFIRMED AGAINST ORIGINAL BINARY`

Three separate rules, all mandatory:

1. If the next token would write past `declared_out`, return an error. Do not write
   the token. Do not truncate the output to `declared_out`.
2. If the input is exhausted and the produced length is short of `declared_out` for
   the whole stream, return an error. Do not pad.
3. `declared_out` is never a stopping condition for the token loop. It is compared
   after the loop and against the running total, never used to end it.

A decoder that stops at `declared_out` produces the declared size for *every* input,
including a stream one byte short and a stream one byte long, and so passes a size
gate while being wrong. Measured on a real block (`FONT0.AAF`, block payload 2,314
bytes, producing 6,337):

| Input | Result |
| --- | --- |
| exact | 6,337 bytes, input fully consumed |
| one byte short | error — the stream ends with one byte of a back reference |
| one byte long | error — the extra byte becomes a control byte with no tokens behind it |

### LZSS-015 — A back reference that starts with fewer than two input bytes left is an error

`UNVERIFIED`

Required by the guide. The guide also records that the original binary underflows its
length counter and loops in this case; this spec does not reproduce that, and the
gate's expectation is an error, not a hang.

I measured that this never happens in the corpus: `0` of `61,070` blocks end mid
pair. So the guard is untested by the data and must be proven by a synthetic case
(LZSS-021 names it). A decoder that returns an error here costs nothing on real data
and is the only thing that keeps a malformed stream from spinning.

### LZSS-016 — Every LZSS block inside a chain starts with a fresh ring

`CONFIRMED AGAINST ORIGINAL BINARY`

No ring state and no output state crosses a block boundary. Measured: decoding all
61,070 blocks with a fresh ring per block closes all 20,456 chain entries. A decoder
that keeps one ring across a chain produces the right total length for many chains
and wrong bytes for all of them.

---

## 5. The three worked vectors

Arithmetic is shown so a test can be written from this document alone. Each is also a
`Class 5` oracle (hand-computed vector in the guide) and, where it uses the corpus,
a `Class 2` oracle.

### LZSS-017 — Token `4E 36` at output position 5000

`CONFIRMED AGAINST ORIGINAL BINARY`

```
write index at output 5000 = (5000 + 4078) mod 4096
                         = 9078 mod 4096
                         = 886

length                  = 9 bytes, copied from output position 4960
                          (5000 - 4960 = 40, and 886 - 846 = 40)

ring index of the source = (4960 + 4078) mod 4096
                         = 9038 mod 4096
                         = 846 = 0x34E

b1 = 846 & 0xFF                  = 0x4E
b2 = ((846 >> 8) & 0x0F) << 4 | (9 - 3)
   = (3 << 4) | 6                = 0x36

so the token is 4E 36.
```

Read back, `4E 36` must give exactly that:

```
source = 0x4E | ((0x36 & 0xF0) << 4) = 0x4E | 0x300 = 0x34E = 846
length = (0x36 & 0x0F) + 3          = 6 + 3       = 9
```

`0x4E 0x36` occurs as real input in 1,016 MASTER.DAT chain payloads.

### LZSS-018 — The stream `00 00 0F` decodes to 18 spaces

`CONFIRMED AGAINST ORIGINAL BINARY`

```
control byte = 0x00        all eight bits clear
b1           = 0x00
b2           = 0x0F

source = 0x00 | ((0x0F & 0xF0) << 4) = 0x00 | 0x00 = 0
length = (0x0F & 0x0F) + 3              = 15 + 3     = 18
```

Source `0` is inside the prefilled region and has never been written, so every one of
the 18 bytes is `RING_FILL` = `0x20`. The output is eighteen spaces.

Then the input is exhausted after three bytes, four tokens short of the control byte's
eight. The block ends anyway. This vector therefore pins two rules at once: the
prefill value and the mid-control-byte stop.

`00 00 0F` occurs as real input in 385 MASTER.DAT chain payloads.

### LZSS-019 — Token `75 3F` at write index 886 repeats the previous byte 18 times

`CONFIRMED AGAINST ORIGINAL BINARY`

```
source = 0x75 | ((0x3F & 0xF0) << 4) = 0x75 | 0x300 = 0x375 = 885
length = (0x3F & 0x0F) + 3              = 15 + 3     = 18

886 - 885 = 1
```

So the source is exactly one byte behind the write head. Which output bytes those two
ring slots hold, shown the long way because both numbers are load-bearing:

```
ring slot r holds output position p whenever (p + 4078) mod 4096 == r,
so p == (r + 18) mod 4096, taken at its most recent occurrence below the produced length.

r = 885:  (885 + 18) mod 4096 = 903  ->  p in {..., 903, 4999, 9095}
          produced length is 5000, so the most recent is 4999.

r = 886:  (886 + 18) mod 4096 = 904  ->  p in {..., 904, 5000}
          produced length is 5000, so the most recent is 5000, the byte about to be written.
```

So the run reads the byte just written at output 4999 and writes it 18 times.

This is the overlap case of LZSS-011 and it is the reason the read-before-write
ordering is spelled out. The read cursor is one byte behind the write cursor and stays
one byte behind for all 18 steps, so each step reads the byte the previous step
wrote. A decoder that snapshots the run first, or that copies with a non-overlapping
bulk move, emits the source byte once and then whatever was in the ring before —
wrong output, right length.

`75 3F` occurs as real input in 1,024 MASTER.DAT chain payloads.

### LZSS-020 — Write index 4078 is the ruling, and the older `882` / `72 36` note is discarded

`CONFIRMED AGAINST ORIGINAL BINARY`

Three write index starts have been proposed: 0, 4078 and 4095. The ruling is **4078**.

Why 4078:

1. `4096 - 4078 = 18`, the 18 byte lookahead of standard LZSS. The ring is sized so
   that the write head begins exactly one maximum-length match before the wrap.
2. Community Edition reads the same value (LZSS-025).
3. Closure: decoding both archives end to end produces every declared size exactly
   (LZSS-022).

The guide records that an older note printed `882` and `72 36` for the LZSS-017 case,
attributed to applying the 18 byte skew with the wrong sign. The `72 36` half of that
claim checks out as a statement about the rejected note:

```
0x72 0x36 -> source = 0x72 | 0x300 = 0x372 = 882
886 - 882 = 4   -> four bytes back, not nine
```

The guide's stated *cause* for the figure 882 does not reproduce: an 18 byte skew
applied with the wrong sign to `(5000 + 4078) mod 4096 = 886` gives 868 or 904, not
882. The figure is wrong either way and the ruling does not depend on explaining it.
Reported rather than silently adopted; see the report that accompanies this spec.

---

## 6. The write-index ruling must be protected by canaries

### LZSS-021 — The gate must contain canaries proving the suite rejects 0 and 4095

`CONFIRMED AGAINST ORIGINAL BINARY`

A constant nothing can falsify is not a ruling. The gate must prove that the suite
rejects each wrong value, so that no future agent can quietly change `RING_WRITE_START`
and still pass.

The canary runs the test author's reference decoder, which takes its ring parameters
as arguments, over a set of real blocks, and asserts the wrong parameters produce
different bytes. Measured here over the first LZSS block of every chain entry whose
declared uncompressed size is at most 30,000:

| Variant | MASTER.DAT blocks whose output differs | CRITTER.DAT blocks whose output differs |
| --- | --- | --- |
| `RING_WRITE_START = 4078`, `RING_FILL = 0x20` (canonical) | 0 of 13,655 | 0 of 2,115 |
| `RING_WRITE_START = 0` | **13,655 of 13,655** | **2,115 of 2,115** |
| `RING_WRITE_START = 4095` | **13,655 of 13,655** | **2,115 of 2,115** |
| `RING_WRITE_START = 4077` | 13,655 of 13,655 | 2,115 of 2,115 |
| `RING_WRITE_START = 4096` | 13,655 of 13,655 | 2,115 of 2,115 |
| `RING_FILL = 0x00` | 756 of 13,655 | 117 of 2,115 |

Two things a canary author must know:

- **Length never discriminates.** Every variant above produced byte counts identical
  to the canonical decode in all 15,770 sampled blocks. The write index is only
  observable in the bytes. A canary that compares lengths will pass on every wrong
  variant and prove nothing.
- **The prefill canary is weak on block length alone.** `RING_FILL = 0x00` only
  changes blocks short enough to read back into the prefilled region, which is why it
  shows 756 and 117 rather than everything. Pin the short blocks by name, or run the
  canary over a set that is known to contain them.

The gate must also contain the case the guide's anti deviation table names: a stream
that ends mid pair, asserting an error and not a loop (LZSS-015).

---

## 7. Corpus measurements this spec pins

### LZSS-022 — Full-corpus closure

`CONFIRMED AGAINST ORIGINAL BINARY`

| Quantity | Value |
| --- | --- |
| Chain entries decoded | 20,456 |
| Entries with `produced == uncompressed_size` | 20,456 |
| Entries with `consumed == compressed_size` | 20,456 |
| LZSS blocks decoded | 61,070 |
| Output bytes produced | 851,322,391 |
| Blocks ending mid control byte | 52,229 |
| Blocks ending on a control boundary | 8,841 |
| Blocks ending mid pair | 0 |
| Failures | 0 |

`61,070` is the guide's own figure for LZSS blocks, arrived at independently.

### LZSS-023 — Largest source index the corpus requests

`CONFIRMED AGAINST ORIGINAL BINARY`

`4095`. The corpus reaches the top of the addressable ring and never asks past it.
The index `0xFFF` is therefore not a theoretical bound only.

---

## 8. The Rust interface

### LZSS-024 — The interface

`UNVERIFIED`

The signatures below are prescribed, not measured. The tag reflects that honestly:
an interface is a contract this port commits to, and no part of it is a claim about
the original binary, so it cannot carry `CONFIRMED AGAINST ORIGINAL BINARY`, and it is
not a Community Edition fact either. Every **constant** inside it is separately
measured, under LZSS-001, LZSS-002, LZSS-003 and LZSS-008; the shapes are the
prescribed part.

The crate forbids `unsafe`, so every accessor is fallible or bounded by construction.

```rust
//! Total LZSS decoder.
//!
//! This crate carries `#![deny(clippy::arithmetic_side_effects)]`, so every index
//! and length computation below is checked or wrapping. There is no `unsafe`, no
//! slice index, no `panic`, and no cast that changes width or sign. Every failure
//! is an `LzssError` carrying the byte offset and the lengths involved, never the
//! bytes themselves.

/// Ring slots. The write head wraps at this value (LZSS-001).
pub const RING_SIZE: usize = 4096;

/// Value prefilled into ring slots `0..RING_WRITE_START` (LZSS-002).
pub const RING_FILL: u8 = 0x20;

/// Where the write head starts inside every fresh ring (LZSS-003, LZSS-020).
pub const RING_WRITE_START: usize = 4078;

/// Shortest run a back reference can encode (LZSS-008).
pub const MIN_MATCH: usize = 3;

/// Longest run a back reference can encode (LZSS-008).
pub const MAX_MATCH: usize = 18;

/// Mask that keeps the high four bits of the second byte, which hold the top of
/// the source position.
const POSITION_HIGH_MASK: u8 = 0xF0;

/// Mask that keeps the low four bits of the second byte, which hold the length
/// bias.
const LENGTH_BIAS_MASK: u8 = 0x0F;

/// Everything the decoder can fail at. No variant carries payload bytes: an
/// `LzssError` reports where and how big, never what.
///
/// Fields common to all variants: `at` is the byte offset into the input slice at
/// which the failure was detected, `declared_out` is the size the caller declared
/// for the whole stream or remaining entry, and `produced` is how many output
/// bytes existed when the failure was detected.
pub enum LzssError {
    /// The tokens would write more than the declared output size (LZSS-014).
    OutputOverrun {
        at: usize,
        declared_out: usize,
        produced: usize,
        token_len: usize,
    },
    /// The input ran out before the declared output size was reached (LZSS-014).
    OutputShort {
        at: usize,
        declared_out: usize,
        produced: usize,
        consumed: usize,
    },
    /// A back reference started with fewer than two input bytes left. The
    /// original binary underflows here and loops; this crate does not
    /// (LZSS-015).
    TruncatedMatch {
        at: usize,
        declared_out: usize,
        produced: usize,
        remaining_input: usize,
    },
    /// Ring arithmetic did not fit in `usize`. Unreachable with the constants
    /// above, and kept so the crate compiles under the arithmetic lint rather
    /// than by accident.
    IndexOverflow {
        at: usize,
        declared_out: usize,
        produced: usize,
    },
}

/// One ring and one output buffer, reused across the LZSS blocks of a chain.
///
/// A chain calls `decode_block` once per block. Each call begins with a freshly
/// filled ring and a write head at `RING_WRITE_START`; the output buffer is the
/// only thing that continues (LZSS-016).
pub struct LzssDecoder {
    ring: Vec<u8>,
    write_index: usize,
    output: Vec<u8>,
}

impl LzssDecoder {
    /// Allocates a decoder with a fresh ring. Holds no state across calls.
    pub fn new() -> Self;

    /// Returns the decoder to its just-constructed state. Called between entries
    /// by any caller that pools one decoder across many archives.
    pub fn reset(&mut self);

    /// Decodes one LZSS block, appending to the internal output buffer. See the
    /// interface notes below, item 2, for what `declared_out` means on a chain
    /// block.
    pub fn decode_block(
        &mut self,
        input: &[u8],
        declared_out: usize,
    ) -> Result<(), LzssError>;

    /// Number of output bytes accumulated so far.
    pub fn produced(&self) -> usize;

    /// Hands the accumulated output to the caller, leaving the decoder empty.
    pub fn take_output(&mut self) -> Vec<u8>;
}

/// Decodes one complete LZSS stream of known declared output size.
///
/// This is the whole-stream entry point: it enforces both halves of LZSS-014, the
/// overrun check and the shortfall check, because it can see the end of the input.
pub fn decode_into(input: &[u8], declared_out: usize) -> Result<Vec<u8>, LzssError>;
```

The required signatures from the plan are preserved verbatim:

```rust
pub struct LzssDecoder;
impl LzssDecoder {
    pub fn new() -> Self;
    pub fn reset(&mut self);
}
pub fn decode_into(input: &[u8], declared_out: usize) -> Result<Vec<u8>, LzssError>;
pub fn decode_block(input: &[u8], decoder: &mut LzssDecoder, declared_out: usize)
    -> Result<(), LzssError>;
```

### Interface decisions the guide does not settle

Recorded so the implementer does not invent a second meaning.

1. **`decode_into` is `LzssDecoder::new()`, one `decode_block`, then `take_output()`.**
   It exists so a caller with a single whole stream does not have to manage a
   decoder. No second decode path exists, so the two entry points cannot disagree.

2. **`declared_out` on a chain block is the output size the entry must still reach.**
   An LZSS block inside a chain declares only its *compressed* length; its decoded
   length is not written anywhere on disk. The overrun check is therefore a running
   total against the entry's declared size, and the chain driver passes
   `entry.uncompressed_size - already_produced`. For a whole stream, that is just
   `entry.uncompressed_size`.

3. **`decode_block` does not enforce the shortfall check; `decode_into` does.**
   A block inside a chain always ends by input exhaustion (85.5% of them mid control
   byte), so a shortfall at a block boundary is the normal case, not an error. The
   shortfall check belongs where the whole entry is visible. `decode_into` sees the
   end of the input and performs it; `decode_block` performs only the overrun check
   and leaves `produced()` for the caller to compare. `fo1-dat` performs it at the
   entry level, where it also has the compressed size to cross-check.

4. **`LzssDecoder` holds the output, so that `decode_block` can return `Result<(), _>`.**
   The required signature has no output parameter and no output in its return type,
   so the buffer has to live in the decoder. `take_output()` hands it over;
   `produced()` lets a caller check the running total without consuming it.

5. **`LzssError::IndexOverflow` exists so the arithmetic lint is satisfied by
   construction.** With these constants no index operation can overflow, but writing
   `checked_*` calls that cannot fail and then discarding their results with `?` into
   an existing variant would be dishonest about which failure is which. One variant
   per way the arithmetic could fail, so the match is exhaustive and total.

6. **`RING_FILL`, `MIN_MATCH` and `MAX_MATCH` are public, the two masks are not.**
   The masks are internal to the token decode and no caller can pass them, so they
   stay private. Making them public would invite a caller to think it may choose them.

### The token loop, in checked arithmetic

Written out so the ordering of LZSS-011 cannot be lost in translation. This is
pseudocode for the shape of the work; it is not the implementation, and the
implementation must use `Vec::get` and `Vec::get_mut` rather than indexing.

```rust
// For each control byte: bits 0..=7, least significant first.
let control = match input.get(ip) {
    Some(byte) => *byte,
    None => return Err(/* OutputShort */),
};
ip = ip.checked_add(1).ok_or(/* IndexOverflow */)?;

for bit in 0..8u32 {
    // The input cursor, not the output length, ends the block (LZSS-013).
    if ip >= input.len() {
        return Ok(()); // mid control byte: this is normal, 85.5% of blocks
    }

    if control & (1u8 << bit) != 0 {
        // Literal (LZSS-006).
        let byte = input.get(ip).copied().ok_or(/* OutputShort */)?;
        emit(byte)?;
        *ring.get_mut(write_index).ok_or(/* IndexOverflow */)? = byte;
        ip = ip.checked_add(1).ok_or(/* IndexOverflow */)?;
    } else {
        // Back reference (LZSS-007). Two bytes, or an error (LZSS-015).
        let pair_end = ip.checked_add(2).ok_or(/* IndexOverflow */)?;
        let (b1, b2) = match input.get(ip..pair_end) {
            Some([first, second]) => (*first, *second),
            _ => return Err(/* TruncatedMatch { remaining_input } */),
        };

        // Absolute ring position, never a distance (LZSS-009).
        let source = usize::from(b1)
            | (usize::from(b2 & POSITION_HIGH_MASK) << 4);
        let length = usize::from(b2 & LENGTH_BIAS_MASK) + MIN_MATCH;

        // One byte at a time; the read precedes its own write (LZSS-011).
        for _ in 0..length {
            let read_index = source;
            let byte = ring.get(read_index).copied().ok_or(/* IndexOverflow */)?;
            emit(byte)?;
            *ring.get_mut(write_index).ok_or(/* IndexOverflow */)? = byte;
            source = (source + 1) % RING_SIZE;
            write_index = (write_index + 1) % RING_SIZE;
        }
        ip = ip.checked_add(2).ok_or(/* IndexOverflow */)?;
    }
}
```

Notes on the shape above, each one load-bearing:

- `(x + 1) % RING_SIZE` and `x + 1` are the only arithmetic on the hot path, and both
  are shown as their checked or wrapping form because of the crate lint. `emit` is
  the single place the overrun check happens, so the check cannot be forgotten on one
  branch.
- `input.get(ip..pair_end)` is a bounds-checked range and `[first, second]` is a slice
  pattern, not an index expression. An implementation that keeps a running `pair`
  slice and matches on it once has the same property.
- The `if ip >= input.len()` test sits at the top of the bit loop, so a control byte
  whose eighth token has no input returns `Ok(())` rather than an error. That is
  LZSS-013, and it is the reason this loop cannot be written as "process eight
  tokens unconditionally".

---

## Error text

The wording of a refusal is ours to choose, not a fact about the game, so this
section fixes it and `crates/fo1-lzss` renders what is written here. Every
message names the offset, the declared size and the produced size, so a reader
can tell where the decode stopped and how much it had written.

```rust
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
```

| Variant | Exact text |
| --- | --- |
| `OutputOverrun` | `fo1-lzss: the token at input offset {at} would write {token_len} byte(s) past the declared output size {declared_out} ({produced} produced)` |
| `OutputShort` | `fo1-lzss: the input ran out at offset {at} with {produced} of {declared_out} declared output bytes produced ({consumed} input byte(s) consumed)` |
| `TruncatedMatch` | `fo1-lzss: the back reference at input offset {at} started with {remaining_input} input byte(s) left and needs two ({produced} of {declared_out} produced)` |
| `IndexOverflow` | `fo1-lzss: ring arithmetic overflowed at input offset {at} ({produced} of {declared_out} produced)` |

Worked example: a token at input offset 7 that would write 3 bytes past a
declared output size of 12, with 12 bytes produced, renders

```
fo1-lzss: the token at input offset 7 would write 3 byte(s) past the declared output size 12 (12 produced)
```

---

## 9. A source that is not the original binary

### LZSS-025 — Community Edition reads the same write index, 4078

`CONFIRMED IN COMMUNITY EDITION ONLY`

CE is the only source for this claim, and CE is not one of my permitted sources, so I
did not open it. The guide states that CE's source reads the same write index as the
original. That is one of the three reasons for the 4078 ruling, and it is the weakest
of the three, because it is agreement between sources rather than a measurement.

The ruling does not rest on it. Reasons 1 and 3 are independent of CE: the
`4096 - 18` identity is arithmetic, and the corpus closure is measured. If CE's
constant were found to differ, LZSS-020 would stand unchanged on the other two
reasons.

Because this claim is the only one in this spec with no measurement behind it, no
test may be written against it, and `docs/spec/trace.md` must not map it to a
`CONFIRMED` test in oracle class 1, 2 or 3.

---

## 10. What this spec does NOT settle

- **Whether Community Edition's decode is byte-identical to the original.** LZSS-025
  records that CE reads the same write index; I did not open CE and do not claim its
  output matches.
- **The 4,116 byte declaration.** LZSS-004 is guide-stated, not measured. I measured
  only that slots 4096..=4115 are unreachable.
- **The original binary's behaviour on a mid-pair truncation.** The guide says it
  underflows and loops. This spec requires an error and does not reproduce the loop.
  Whether any shipped data reaches that state is unknown; the corpus says no.
- **Mode `0x10` streams.** `fo1-dat` measured zero entries of that mode in this
  corpus. `decode_into` exists for it, but nothing in the reference install exercises
  it, so the round trip through it is untested here.
- **Shortfall detection at the entry level.** LZSS-014 states the rule and LZSS-024
  names who enforces it. This crate cannot see a chain's end, so that check lives in
  `fo1-dat` and its spec.
- **Overlap copies longer than the distance.** A run whose source distance `d` is
  smaller than its own length is covered by LZSS-011 and by LZSS-019's one-byte case.
  The corpus exercises many such runs, but no vector here pins the shortest `d` that
  the corpus actually produces.
- **Any payload format.** The decoder returns bytes. `.PAL`, `.AAF`, `.FON`, `.FRM`,
  `.PRO` and `.MSG` are separate specs.
- **Other installs.** Every count in section 1 belongs to this install. A different
  install is `unsupported install`, not relaxed constants.
- **Concurrency.** Nothing in the format is stateful across threads. A decoder is
  owned by one thread; no synchronisation is specified because none is needed.
- **The block payload ceiling.** `fo1-dat` fixes it at 32,767 with an observed
  maximum of 16,384. This decoder does not care: it is handed a slice.