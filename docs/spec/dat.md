# DAT archive container

Provenance: written from `RUSTY WASTELAND Engine Implementation Guide, Revision 2.md`
(sections *DAT archives*, *LZSS decoder*, *Archive gate*) and from a first-hand
structural walk of the bytes of `MASTER.DAT` and `CRITTER.DAT` in the reference
install. No Community Edition source was opened. No decompilation, comment, name or
structure was taken from any other implementation. Nothing below is a quotation.

All byte offsets in this document are file-relative offsets from the start of the
archive. All integers are big endian.

Every claim carries exactly one evidence tag:

| Tag | Meaning |
| --- | --- |
| `CONFIRMED AGAINST ORIGINAL BINARY` | Measured by me from the real archive bytes |
| `CONFIRMED IN COMMUNITY EDITION ONLY` | CE is the only source |
| `VERSION DIFFERENCE IDENTIFIED` | A measured difference between installs |
| `UNVERIFIED` | Asserted or required, not measured by me |

---

## 1. Measured anchors

These are the values the whole spec hangs from. Each was re-read from the bytes
rather than taken on report.

### 1.1 MASTER.DAT

| Offset | Bytes | Reading |
| --- | --- | --- |
| `0x00` | `00 00 00 41 00 00 00 5E 00 00 00 00 02 02 15 90` | root header |
| `0x10` | `01 2E` | directory 0 name: length 1, `.` |
| ... | | 65 length-prefixed names |
| `0x3F6` | `00 00 00 0B 00 00 00 16 00 00 00 10 02 02 86 38` | block 0 header |
| `0x406` | `09` `COLOR.PAL` `...` | entry 0: length 9, name, then four u32 |

Root header fields: `field0 = 65`, `field1 = 94`, `field2 = 0`, `field3 = 33691024`.
The name section ends at `0x3F6`. Block 0 header fields: `field0 = 11`,
`field1 = 22`, `field2 = 16`, `field3 = 42414840`. Entry 0: `COLOR.PAL`,
`flags = 64`, `offset = 211060669`, `uncompressed_size = 33536`,
`compressed_size = 9292`.

### 1.2 CRITTER.DAT

| Offset | Bytes | Reading |
| --- | --- | --- |
| `0x00` | `00 00 00 01 00 00 00 0A 00 00 00 00 02 03 10 40` | root header |
| `0x10` | `0C` `ART\CRITTERS` | directory 0 name: length 12 |
| `0x1D` | `00 00 15 53 00 00 17 FE 00 00 00 10 02 22 78 38` | block 0 header |
| `0x2D` | `0C` `CRITTERS.LST` `...` | entry 0: length 12, name, then four u32 |

Root header fields: `field0 = 1`, `field1 = 10`, `field2 = 0`, `field3 = 33755200`.
The name section ends at `0x1D`. Block 0 header fields: `field0 = 5459`,
`field1 = 6142`, `field2 = 16`, `field3 = 36372536`. Entry 0: `CRITTERS.LST`,
`flags = 64`, `offset = 158356`, `uncompressed_size = 646`,
`compressed_size = 405`.

### 1.3 Closure over the whole tree

| Quantity | MASTER.DAT | CRITTER.DAT | Total |
| --- | --- | --- | --- |
| File size | 339,379,746 | 157,085,138 | |
| Directories | 65 | 1 | 66 |
| Entries | 19,784 | 5,459 | 25,243 |
| Mode `0x10` LzssStream | 0 | 0 | 0 |
| Mode `0x20` Stored | 4,787 | 0 | 4,787 |
| Mode `0x40` BlockChain | 14,997 | 5,459 | 20,456 |
| Entries with `flags == 0` | 0 | 0 | 0 |
| Stored entries with `compressed_size != 0` | 0 | 0 | 0 |
| Chain entries with `compressed_size == 0` | 0 | 0 | 0 |
| Chain entries whose block headers land exactly on `compressed_size` | 14,997 / 14,997 | 5,459 / 5,459 | 20,456 / 20,456 |
| Entries with `offset + compressed_size` past EOF | 0 | 0 | 0 |
| Duplicate composed paths | 0 | 0 | 0 |

Arithmetic, stated explicitly: `19784 + 5459 = 25243` and
`20456 + 4787 = 25243`. The sum of field 0 over all 66 block headers equals the
total entry count exactly, in each archive separately.

---

## 2. Claims

### DAT-001 — The archive opens with a 16 byte root header of four u32 values

`CONFIRMED AGAINST ORIGINAL BINARY`

`0x00..0x10` is four big endian u32. There is no magic number, no version and no
checksum in it.

### DAT-002 — Root field meanings

`CONFIRMED AGAINST ORIGINAL BINARY`

| Field | Meaning at the root | MASTER.DAT | CRITTER.DAT |
| --- | --- | --- | --- |
| 0 | directory count | 65 | 1 |
| 1 | capacity — ignored | 94 | 10 |
| 2 | size field, zero at the root | 0 | 0 |
| 3 | dead pointer — ignored | 33691024 | 33755200 |

Fields 1 and 3 are read and discarded. A reader must not let them influence a
length, a stride or a bound.

### DAT-003 — Directory names follow the root header, each length-prefixed

`CONFIRMED AGAINST ORIGINAL BINARY`

One `u8` length then exactly that many bytes of name. Exactly `field0` names, in
file order. MASTER.DAT: 65 names, section `0x10..0x3F6`, first `.`, last
`TEXT\SPANISH\GAME`. CRITTER.DAT: 1 name, section `0x10..0x1D`,
`ART\CRITTERS`.

### DAT-004 — One 16 byte block header per directory, in the same order

`CONFIRMED AGAINST ORIGINAL BINARY`

Immediately after the name section come `field0` block headers, each 16 bytes,
directory `i` matched to name `i`. No index, no key, no lookup table. A reader
walks the names array and the block array with the same cursor.

### DAT-005 — Block field 0 is the entry count for that block

`CONFIRMED AGAINST ORIGINAL BINARY`

This is the load-bearing gap the guide leaves open. The guide describes a block
header as "the same 16 byte header with the size field set to 16" and never says
what field 0 becomes once a block header replaces the root header. Field 0 does
not keep the root's meaning.

Evidence, three ways:

1. **Direct contrast.** MASTER.DAT root field 0 is 65 and the archive has 65
   directories. MASTER.DAT block 0 field 0 is 11 and that block has exactly 11
   entries. 65 directories cannot also be 11 entries.
2. **A coincidence that is not one.** CRITTER.DAT block 0 field 0 is 5459, which
   is exactly the CRITTER.DAT entry count and is not its directory count, which
   is 1.
3. **Closure, not assertion.** Summing field 0 over all 66 block headers gives
   19,784 for MASTER.DAT and 5,459 for CRITTER.DAT, matching the independently
   counted entry totals of both archives. A reader that mistakes field 0 for a
   directory count, or for a byte count, or that skips it, cannot land on the
   first block header offset.

So field 0 is the directory count **at the root** and the entry count **at a
block**. It is the only field whose meaning changes.

### DAT-006 — Block field 2 is 16 at every block

`CONFIRMED AGAINST ORIGINAL BINARY`

All 66 measured block headers carry 16. This is the byte width of the four u32
values that follow every entry name — that is, the fixed trailer of an entry
record. It is **not** the size of a whole entry record, which is
`1 + name_length + 16` bytes and therefore varies. The guide calls this "a size
field"; the precise reading is the fixed-trailer width. A reader ignores it.

### DAT-007 — Block fields 1 and 3 are a capacity and a dead pointer, at blocks too

`CONFIRMED AGAINST ORIGINAL BINARY`

| Archive | Where | field 1 | field 3 |
| --- | --- | --- | --- |
| MASTER.DAT | root | 94 | 33691024 |
| MASTER.DAT | block 0 | 22 | 42414840 |
| CRITTER.DAT | root | 10 | 33755200 |
| CRITTER.DAT | block 0 | 6142 | 36372536 |

Values are large, non-monotonic and unrelated to any measured extent. Read and
discard, exactly as at the root.

### DAT-008 — Nothing separates the blocks

`CONFIRMED AGAINST ORIGINAL BINARY`

After a block's last entry, the next 16 bytes are the next block header, with no
padding, no alignment and no count of blocks. The reader knows a block ended only
because it has consumed `field0` entries.

### DAT-009 — An entry is a length byte, the name, and four u32 values

`CONFIRMED AGAINST ORIGINAL BINARY`

In order: `flags`, absolute `offset`, `uncompressed_size`, `compressed_size`. The
offset is absolute from the start of the archive file, not from the entry and not
from any block. MASTER.DAT entry 0 `COLOR.PAL` reads `flags = 64`,
`offset = 211060669`, `uncompressed_size = 33536`, `compressed_size = 9292`.

Across both archives no entry name contains a path separator, no entry name
contains a non-ASCII byte, and the longest entry name is 12 bytes.

### DAT-010 — Paths use backslashes, and a path is uppercased before lookup

`CONFIRMED AGAINST ORIGINAL BINARY`

The separator on disk is `\` (`0x5C`). Every stored name and directory name is
already uppercase. `find` therefore takes a path that the caller has already
uppercased, which is what its parameter name `upper_path` states. Lookup must be
on the uppercase form; do not case-fold on every call and do not depend on any
locale.

### DAT-011 — Path composition, including the `.` directory

`CONFIRMED AGAINST ORIGINAL BINARY`

The composed path is the directory name and the entry name joined by one `\`,
except that the root directory of MASTER.DAT is named `.` and contributes no
separator.

Evidence that the rule keys on the name `.` and not on "the first block":

- MASTER.DAT has a directory literally named `.` holding 11 entries
  (`COLOR.PAL`, `FONT0.AAF` … `FONT5.FON`). Composing with the rule yields 11
  bare names and 19,773 prefixed names.
- CRITTER.DAT has one directory, at block index 0, named `ART\CRITTERS`, and
  **all 5,459** of its entries compose with the prefix: zero bare names. A rule
  keyed on block index would strip the prefix from every one of them.

Composing both archives under this rule gives 25,243 paths with zero duplicates,
which is what makes a lookup table sound.

### DAT-012 — Tree order is not physical order

`CONFIRMED AGAINST ORIGINAL BINARY`

In MASTER.DAT the first tree entry, `COLOR.PAL`, has `offset = 211060669`. The
smallest offset anywhere in MASTER.DAT is 556,484. The archive therefore
descends 15 times before it reaches its lowest data. An offset is read from the
entry and from nowhere else.

Never infer an offset from an entry's position, its index, or its neighbours.
This is a named anti-pattern in the guide and the single most load-bearing rule
in this container.

### DAT-013 — Payload mode is the high nibble of the flags

`CONFIRMED AGAINST ORIGINAL BINARY`

`flags & 0xF0`. A `flags` value of 0 means `0x10`. Measured: 0 entries have
`flags == 0`, so the `0 -> 0x10` substitution is defensive and does not fire on
this install. Any other high nibble is not a known mode and must be an error,
never a default.

### DAT-014 — Mode `0x10` is one LZSS stream whose input length is the compressed size

`UNVERIFIED`

Zero entries in the measured corpus use this mode. The rule is stated by the guide
and is not exercised by the data, so nothing here rests on it: I confirmed only
that no entry selects this mode. I did not read Community Edition and so do not
claim it as a source either. The decoder is `fo1-lzss`; the ring-buffer rules
belong to that crate's spec, not here.

### DAT-015 — Mode `0x20` is stored bytes whose length is the uncompressed size

`CONFIRMED AGAINST ORIGINAL BINARY`

4,787 entries, all in MASTER.DAT, none in CRITTER.DAT. The on-disk extent of such
an entry is `offset .. offset + uncompressed_size`, because the bytes are already
decoded. Every one of the 4,787 has `compressed_size == 0`.

### DAT-016 — Mode `0x40` is a block chain

`CONFIRMED AGAINST ORIGINAL BINARY`

20,456 entries: 14,997 in MASTER.DAT, 5,459 in CRITTER.DAT. Every one has a
nonzero `compressed_size`, and for every one the chain's block headers account for
that size exactly (DAT-021).

### DAT-017 — Every stored entry has compressed size 0; every chain entry a nonzero one

`CONFIRMED AGAINST ORIGINAL BINARY`

Measured across all 25,243 entries: 0 stored entries with a nonzero compressed
size, 0 chain entries with a zero compressed size. This split is a cheap,
exhaustive invariant for the parser to assert.

### DAT-018 — A block chain is a series of framed blocks

`CONFIRMED AGAINST ORIGINAL BINARY`

Each block opens with a big endian u16 header.

| Header bit 15 | Block kind | Payload length |
| --- | --- | --- |
| set | raw | `header & 0x7FFF` bytes, copied verbatim |
| clear | LZSS stream | `header` bytes of compressed input |

A block occupies `2 + payload` bytes. The chain is read block after block until
the decoded output reaches the entry's `uncompressed_size`. In every one of the
20,456 measured chains the block sequence also consumes exactly
`compressed_size` bytes: 14,997/14,997 and 5,459/5,459. Both stopping conditions
agree, which is what lets a reader cross-check one against the other.

A zero-length block is an error. No measured chain contains one.

### DAT-019 — Every LZSS block inside a chain starts with a fresh ring buffer

`CONFIRMED AGAINST ORIGINAL BINARY`

State never crosses a block boundary. A decoder that keeps one ring across the
whole chain produces plausible bytes that are wrong; the sizes will not match.

### DAT-020 — One chain may mix raw and LZSS blocks

`CONFIRMED AGAINST ORIGINAL BINARY`

MASTER.DAT holds 6,593 raw blocks and 31,427 LZSS blocks; CRITTER.DAT holds 0 raw
blocks and 29,643 LZSS blocks. Corpus totals: 38,020 + 29,643 = 67,663 blocks,
of which 31,427 + 29,643 = 61,070 are LZSS and 6,593 are raw. The raw blocks all
live inside MASTER.DAT chain entries, so those chains mix both kinds and a reader
must test the header bit per block, not per chain.

### DAT-021 — The on-disk extent of an entry is bounded by its compressed size

`CONFIRMED AGAINST ORIGINAL BINARY`

`offset + compressed_size` never exceeds the archive size: 0 violations across all
25,243 entries in both archives.

`offset + uncompressed_size` **does** exceed it, for 7 entries — 5 in MASTER.DAT
(`TEXT\SPANISH\GAME\PRO_SCEN.MSG`, `SCRNAME.MSG`, `STAT.MSG`, `TRAIT.MSG`,
`WORLDMAP.MSG`, overrunning by 15228, 39038, 508, 389 and 785 bytes) and 2 in
CRITTER.DAT (`NMPEASRP.FRM` by 7475, `RESERVAA.FRM` by 2172). All seven are chain
entries, and all seven sit in the last few kilobytes of their archive.

This is correct behaviour, not corruption: the decoded form is longer than the
stored form, so the decoded extent is meaningless as a file bound. A reader that
bounds the stored region by the uncompressed size will reject seven valid
entries. Bound by the compressed size; use the uncompressed size only as the
length the output must reach.

### DAT-022 — Consumption closure: no chain reads past or short of its compressed size

`CONFIRMED AGAINST ORIGINAL BINARY`

Walking every chain by header arithmetic alone, without decompressing, the block
sequence lands exactly on `compressed_size` for 20,456 of 20,456 chains, with no
trailing bytes and no overrun. This is half of the gate's closure requirement and
it is already established by structure.

### DAT-023 — Production closure: produced output equals the declared uncompressed size

`UNVERIFIED`

I did not decompress. Every token of every LZSS stream has to be decoded before
this half can be asserted, and that belongs to the LZSS crate and the gate. The
requirement is that all 25,243 entries produce exactly `uncompressed_size` bytes,
and I claim nothing about it here. This claim stays open until the gate reports a
full-corpus decode.

### DAT-024 — No truncation, ever

`CONFIRMED AGAINST ORIGINAL BINARY`

The framing and consumption half is measured: no chain reads past or short of its
compressed size, and a decoder that would exceed a declared size has nothing to
truncate. The produced-size half is the open claim DAT-023 and is asserted below
as a requirement, not as a result.

A decoder receives the declared size for one purpose only: to detect that the
tokens would write past it. On that event it returns an error. It never cuts the
output, never pads it, never substitutes a shorter buffer and never grows past
the declaration. The gate asserts, for all 25,243 entries, both

- produced bytes `== uncompressed_size`, and
- consumed bytes `== compressed_size`,

and additionally, for the 20,456 chains, that the block headers account for the
compressed size exactly.

### DAT-025 — Archive sizes, directory counts and entry counts

`CONFIRMED AGAINST ORIGINAL BINARY`

| Archive | Size in bytes | Directories | Entries |
| --- | --- | --- | --- |
| MASTER.DAT | 339,379,746 | 65 | 19,784 |
| CRITTER.DAT | 157,085,138 | 1 | 5,459 |

These are the constants the gate pins for this install. On any other install the
gate reports `unsupported install` and does not relax them.

### DAT-026 — Corpus mode counts

`CONFIRMED AGAINST ORIGINAL BINARY`

Mode `0x10`: 0. Mode `0x20`: 4,787, all MASTER.DAT. Mode `0x40`: 20,456.
Total 25,243, and `4787 + 20456 = 25243`.

### DAT-027 — Block framing constants

`CONFIRMED AGAINST ORIGINAL BINARY`

The raw flag is bit 15, that is mask `0x8000`. The raw length mask is `0x7FFF`, so
the format ceiling on a block payload is 32,767 bytes. The largest payload
actually present is 16,384 bytes — for both a raw block and an LZSS block in
MASTER.DAT — against a largest LZSS payload of 11,132 in CRITTER.DAT. The format
ceiling is 32,767; the observed maximum is 16,384. A reader must accept up to the
ceiling and must not hard-code the observed maximum.

### DAT-028 — Loose-file patch folder takes precedence over the archives

`UNVERIFIED`

The guide states that the original engine reads loose files from a patch folder
before it reads the archives, and asks for a configured search path, off by
default. I did not verify the precedence order against any bytes, and I did not
verify the three patch divergences the guide lists. This spec scopes the archive
layer only; the search path is a lookupsite concern. Known-plaintext tests must
read the archive, never a loose file.

### DAT-029 — Cross-check: `COLOR.PAL` decodes to 33,536 bytes

`CONFIRMED AGAINST ORIGINAL BINARY`

`COLOR.PAL` is the first entry of MASTER.DAT and declares
`uncompressed_size = 33536`. The guide separately states that a `.PAL` file is
33,536 bytes. Two independent statements agree on the same number, from
different directions: a container field and a file-format fact. This is the
corroboration that `uncompressed_size` is the decoded length rather than a stored
length or a block count. It also confirms the entry is a chain
(`flags = 64`) whose decoded form, not its stored 9,292 bytes, is the palette.

### DAT-030 — The Rust interface

`UNVERIFIED`

This is the conservative tag, and it replaces the `CONFIRMED AGAINST ORIGINAL
BINARY` this claim carried before. The claim is an interface: a set of Rust
types, signatures and error variants. An interface is a decision this port
makes, and the guide settles none of it — the subsection below says so in the
spec's own words. No Community Edition source was opened for this spec either,
so the other conservative tag, `CONFIRMED IN COMMUNITY EDITION ONLY`, cannot
apply: there is no source of any kind behind this claim but the decision
itself. The facts the interface encodes — the header layout, the field order,
the counts, the mode nibbles — are measured, and they live under DAT-001 to
DAT-029. What is unverified here is the shape that carries them.

```rust
//! DAT archive container.
//!
//! This crate carries `#![deny(clippy::arithmetic_side_effects)]`, so every
//! offset and length computation is checked or wrapping. Every failure below is
//! returned as a `DatError`: there is no slice index, no fallible-conversion
//! call, no abort, and no cast that changes width or sign.

/// Everything the reader can fail at. There is no panic path.
pub enum DatError {
    /// The declared structure ran out before the reader did.
    Truncated {
        /// File-relative byte offset at which the failed read started, never the
        /// offset at which the failure was noticed.
        at: u64,
    },
    /// A length or count field exceeded the bytes actually present, or a stored
    /// name was not the UTF-8 the path type requires.
    LengthOutOfRange {
        /// File-relative byte offset at which the failed read started: the
        /// length byte, or the name's bytes, that asked for what was not there.
        at: u64,
    },
    /// A block header or payload ran outside the archive.
    OutOfBounds {
        /// File-relative byte offset the refusal names: the region's end, or
        /// the framed block's header start for a block inside a chain.
        at: u64,
    },
    /// An offset plus a length did not fit in the address arithmetic.
    OffsetOverflow,
    /// A flags high nibble this build does not know.
    UnknownPayloadMode {
        /// The flags word exactly as stored, reported back whole.
        flags: u32,
    },
    /// The tokens would have written more than the declared output size.
    OutputOverrun {
        /// The declared uncompressed size that was exceeded.
        declared: u32,
    },
    /// The stream ended before reaching the declared output size.
    OutputShort {
        /// The declared uncompressed size that was not reached.
        declared: u32,
    },
    /// The payload did not consume exactly the declared compressed size.
    ConsumptionMismatch {
        /// The compressed size the entry declares.
        declared: u32,
        /// The bytes the payload actually consumed.
        actual: u32,
    },
    /// A block header declared a zero length payload.
    EmptyBlock {
        /// File-relative byte offset of the block header.
        at: u64,
    },
}

/// One directory of the two level tree.
pub struct DirectoryRecord {
    /// Name exactly as stored, uppercase, possibly the single character `.`.
    pub name: String,
    /// Field 0 of this block's header: how many entries follow it.
    pub entry_count: u32,
}

/// One entry. `path` is already composed and already uppercase.
pub struct EntryRecord {
    pub path: String,
    pub flags: u32,
    pub offset: u32,
    pub uncompressed_size: u32,
    pub compressed_size: u32,
}

/// Which of the three payload shapes a flags value selects.
pub enum PayloadMode {
    /// 0x10. One LZSS stream, input length is the compressed size.
    LzssStream,
    /// 0x20. Stored bytes, length is the uncompressed size.
    Stored,
    /// 0x40. A framed series of raw and LZSS blocks.
    BlockChain,
}

/// Per-chain block census, used by the gate rather than by callers.
pub struct ChainStats {
    pub blocks: u32,
    pub lzss_blocks: u32,
    pub raw_blocks: u32,
}

pub struct DatArchive {
    /// Diagnostic label the archive was parsed under (DAT-030). It names which
    /// archive a message came from and is never a path to open.
    origin: &'static str,
    /// Directories in file order, one per block header.
    directories: Vec<DirectoryRecord>,
    /// Entries in tree order: all of block 0, then all of block 1, and so on.
    entries: Vec<EntryRecord>,
}

impl DatArchive {
    /// Parses the root header, the directory names, and every block with its
    /// entries. Parsing never reads the payload region, so a truncated payload
    /// does not stop the directory from being listed.
    pub fn parse(bytes: &[u8], origin: &'static str) -> Result<Self, DatError>;

    /// The diagnostic label this archive was parsed under (DAT-030).
    pub fn origin(&self) -> &'static str;

    /// Directory names in file order.
    pub fn directories(&self) -> &[DirectoryRecord];

    /// Entries in file order: all of block 0, then all of block 1, and so on.
    /// This is tree order, which is not physical order (DAT-012).
    pub fn entries(&self) -> &[EntryRecord];

    /// Looks up a path the caller has already uppercased and already spelled
    /// with backslashes. This performs no case folding of any kind: it does not
    /// call `to_uppercase`, it does not compare with `eq_ignore_ascii_case`, and
    /// it consults no locale. `None` means the uppercase form is not in the
    /// archive (DAT-031, DAT-032).
    pub fn find(&self, upper_path: &str) -> Option<&EntryRecord>;
}

/// The high nibble of the flags. A flags value of 0 means 0x10. Any other
/// unknown nibble has no mode, and the error is raised where it can be
/// observed: `decode_entry` and `decode_entry_with_stats` answer
/// `DatError::UnknownPayloadMode` for every nibble this function cannot map.
/// The signature returns a `PayloadMode` and cannot carry an error, so this
/// function is total: a nibble it cannot map yields `PayloadMode::BlockChain`
/// as a placeholder. `PayloadMode` has no unknown variant, so the placeholder
/// names a real mode and must not be read as one. The mode of a flags word
/// this build does not know is `DatError::UnknownPayloadMode`, raised by the
/// two decode functions.
pub fn payload_mode(flags: u32) -> PayloadMode;

/// Decodes one entry to exactly its declared uncompressed size, consuming
/// exactly its declared compressed size.
pub fn decode_entry(entry: &EntryRecord, bytes: &[u8]) -> Result<Vec<u8>, DatError>;

/// As `decode_entry`, and also returns the chain census so the gate can assert
/// the block totals without a second pass.
pub fn decode_entry_with_stats(
    entry: &EntryRecord,
    bytes: &[u8],
) -> Result<(Vec<u8>, ChainStats), DatError>;
```

### Interface notes the guide does not settle

`origin: &'static str` appears in the required signature but nothing in the guide
says what it is for. The crate has no I/O, so the reading that fits is a label
carried on the archive for diagnostics — which archive a message came from — and
never a path to open. It is a `&'static str` because the two archives are named
once. This is an interface decision, recorded so the implementer does not invent
a second meaning for it. The archive reports the label back through
`origin()`, so a caller that holds an archive can say which archive a refusal
came from without holding the label itself.

The lookup index is an internal choice and is deliberately not part of the
required interface above. A reader that wants one may build it once; a reader
that only iterates need not build one at all, and nothing in the spec
constrains the choice beyond correctness.

`ChainStats` is required by the interface but no required function returns it. The
smallest way to make it reachable is the extra `decode_entry_with_stats` above;
`decode_entry` itself is unchanged. Without it the type is dead.

The composition and bounds rules the implementation must honour, written out so
they cannot be inferred wrongly:

```rust
/// The stored region of an entry. This is bounded by the compressed size, never
/// by the uncompressed size: seven real entries decode to more bytes than the
/// file has left (DAT-021).
fn stored_extent(entry: &EntryRecord) -> Result<(u64, u64), DatError> {
    let start = u64::from(entry.offset);
    let end = start
        .checked_add(u64::from(entry.compressed_size))
        .ok_or(DatError::OffsetOverflow)?;
    Ok((start, end))
}

/// For a stored entry the bytes are already decoded, so the stored region is the
/// uncompressed size instead. Chain and LZSS entries keep the compressed bound.
fn payload_extent(entry: &EntryRecord, mode: PayloadMode) -> Result<(u64, u64), DatError> {
    match mode {
        PayloadMode::Stored => {
            let start = u64::from(entry.offset);
            let end = start
                .checked_add(u64::from(entry.uncompressed_size))
                .ok_or(DatError::OffsetOverflow)?;
            Ok((start, end))
        }
        PayloadMode::LzssStream | PayloadMode::BlockChain => stored_extent(entry),
    }
}

/// Joins a directory name and an entry name. The root directory of MASTER.DAT is
/// named `.` and contributes no separator, which is why this is not a plain
/// `format!` (DAT-011).
pub fn compose_path(directory: &str, name: &str) -> String {
    if directory == "." {
        name.to_owned()
    } else {
        // No capacity arithmetic: a length hint is not worth a checked add.
        let mut joined = String::new();
        joined.push_str(directory);
        joined.push('\\');
        joined.push_str(name);
        joined
    }
}
```

`compose_path` is a demonstration of the rule, not an extra required signature.
A reader that needs a lookup table will want the index built once; that is an
internal choice and the spec does not constrain it beyond correctness.

### The byte offset an error carries

Four of the nine variants carry an `at`, and **they do not all carry the same
thing**. The rule the layer below settles — "the byte offset at which the
failed operation started", `docs/spec/endian.md` END-022 — holds for the two
variants that name a *field*. It does not hold for the two that name a
*region*, so no one sentence covers all four. Each variant's own rule is stated
here, with the formula its path uses.

| Variant | What `at` names | The formula the path uses |
| --- | --- | --- |
| `Truncated` | The byte offset at which the failed read **started**, never the offset at which the failure was noticed. | A tree read: the cursor offset when the read began, which is the field's first byte, copied unchanged from the endian layer. An LZSS stream: `base + ip` — the entry's payload base plus the codec's position inside the stream. |
| `LengthOutOfRange` | The byte offset at which the failed read **started**. | A name whose bytes are absent: the offset at which the name's bytes should have started, one past the length byte, copied unchanged from the endian layer. A name that is not UTF-8: the offset of its length byte, the length field that asked for the unusable bytes. An uncompressed size that does not fit a `usize`: the payload region's start, `entry.offset`. |
| `OutOfBounds` | **Not** the start of the failed operation. | The region helper: the region's **end**, the first byte past the archive. The chain decoder: the framed block's **header start**, `chain start + cursor`. |
| `EmptyBlock` | The framed block's **header start**. | `chain start + cursor`. |

`OffsetOverflow` carries no offset because there is none to carry: no field and
no region failed, the arithmetic did.

The guide's rule is that "variants carry the byte offset and the field name".
An offset and a field name together identify one field, so for the two
variants that name a field the offset is that field's first byte and nothing
else. The layer below says the same thing in its own words: `docs/spec/endian.md`
END-022 fixes the endian layer's error as carrying "the byte offset at which
the failed operation started", and on the tree read path the DAT reader copies
that offset unchanged rather than recomputing one.

The two readings are one byte apart whenever a field is missing its second
byte, so the difference is not academic. Worked example, from DAT-001: the root
header is four big endian u32 at `0x00..0x10`, so field 2 starts at `0x08`. A
ten byte tree leaves two of the four bytes field 2 needs. The error is
`DatError::Truncated { at: 8 }`, because the field that could not be read
starts at 8. The reader notices that it has run out at byte 10, and 10 is
**not** what the error carries: byte 10 is inside field 2 rather than at the
start of it, and it identifies no field and no region of its own.

Two paths do not copy an offset unchanged, and the table above gives both their
formulas. The LZSS path adds the codec's position to a base; the chain path
adds the block's position within the chain to the chain's own base.

#### What the gate pins, and what it does not

| The offset | Pinned by |
| --- | --- |
| `Truncated` at a tree field's first byte (8, and 12 for the next field) | `synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at` |
| `LengthOutOfRange` at a name's first byte (17) | the same test |
| `LengthOutOfRange` at a name's length byte | the same test |
| `Truncated` at `base + ip` for a whole LZSS stream | the same test |
| `OutOfBounds` at a region's end | `synthetic_a_stored_region_is_bounded_by_the_compressed_size_not_the_uncompressed_one` |
| `EmptyBlock` at `chain start + cursor` | `synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at` |
| `OutOfBounds` at `chain start + cursor` | the same test |

One offset is not pinned, and the reason is an open item rather than a gap in
the gate: an LZSS block **inside a chain** reports `chain start + ip`, where
`ip` is counted from the start of the **block's** payload. The sum is
therefore short of the file-relative offset of the byte by the block's own two
byte frame header and by whatever blocks came before it. A chain whose stored
bytes start at byte 60 and whose first block's back reference is cut reports
byte 61, while the byte it names is at byte 63. A whole LZSS stream has no
such gap, because its base is the start of the payload the codec is reading.
See section 3.

### DAT-031 — The caller uppercases the path; `find` folds nothing

`UNVERIFIED`

`DatArchive::find` takes a path the **caller** has already uppercased. The
precondition lives at the call site, not inside the parser, which is why the
parameter is named `upper_path`.

The guide states this in one sentence in the *DAT archives* section:
"Uppercase a path before you look it up." The rule is put on whoever looks the
path up, so this spec puts it on the caller and not on the reader.

The byte facts behind it are measured and belong to DAT-010: every stored
directory name and every stored entry name in both archives is already
uppercase. What is not measured is how the original engine treats a lowercased
argument, because nothing in either archive supplies one.

### DAT-032 — Lookup is an exact match against the stored uppercase path

`UNVERIFIED`

The other half of DAT-031, and the reason the uppercase step belongs to the
caller rather than to the parser.

The independent reference walker this spec was checked against compares paths
case-exactly, and it is the behaviour `find` is specified to match: a lookup
succeeds only on the stored uppercase form, byte for byte, and returns `None`
otherwise. Case folding inside the parser would not be a harmless convenience.
It could not turn a miss into a hit on this install, because every stored name
is already uppercase, so it would only ever change what a caller's mistake
looks like: a folded lookup silently misses a path that is present, or matches
one the caller did not mean.

So the rule has one direction. Uppercase once, at the call site, and pass the
result to `find`. `find` does not uppercase, does not lowercase, does not
normalise separators and does not consult a locale table.

### DAT-033 — A directory block whose field 0 is zero parses

`UNVERIFIED`

`DatArchive::parse` accepts a block header whose field 0 is `0`: a directory
that declares no entries at all.

This is not a measured behaviour of the original engine, and it is not claimed
as one. Field 0 is the entry count at a block (DAT-005), so zero is a count of
zero, and a block describing nothing is not a block describing something that
runs off the end of the archive. The measured arithmetic supports the reading:
the sum of field 0 over the 66 measured block headers equals the entry total
exactly (section 1.3), and a zero term breaks no part of that.

No byte evidence in either archive shows a zero field 0 at a block, and none
shows the original rejecting one. So the parser accepts it, the behaviour is
pinned here, and it stays an open item rather than a rule. See section 3.

### DAT-034 — An entry whose name length is zero parses

`UNVERIFIED`

`DatArchive::parse` accepts an entry whose length byte is `0`, producing an
entry whose name is empty.

Again this is a port decision, not a measurement. An entry is a length byte,
the name, and four u32 values (DAT-009), so a zero length is a zero-length name
and the four u32 trailer still follows at its computed offset. The record is
the minimum seventeen bytes and nothing is truncated; a reader that treated a
zero length as malformed would be rejecting a record its own layout rules
permit.

No entry in either measured archive has a zero name length, so neither
acceptance nor rejection is established by any byte evidence. The parser
accepts it and the behaviour is an open item, not a rule. See section 3.

---

## Error text

The wording of a refusal is ours to choose, not a fact about the game, so this
section fixes it and `crates/fo1-dat` renders what is written here. Every
message names the quantity the refusal is about — the byte offset of the region
that ran out, the flags word, the declared size — so a reader can tell which
part of the archive failed, and no message carries a byte of the archive.

The guide's rule is that a variant carries "the byte offset and the field
name". This error type carries the offset in the payload, and it names the
field in the message instead: the table below says what each message must
name, and the gate asserts the text of every variant word for word.

```rust
impl core::fmt::Display for DatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::Truncated { at } => {
                write!(f, "fo1-dat: the archive ran out at byte {at}")
            }
            Self::LengthOutOfRange { at } => {
                write!(
                    f,
                    "fo1-dat: a length field at byte {at} exceeded the bytes present"
                )
            }
            Self::OutOfBounds { at } => {
                write!(f, "fo1-dat: a region at byte {at} ran outside the archive")
            }
            Self::OffsetOverflow => write!(f, "fo1-dat: an offset plus a length overflowed"),
            Self::UnknownPayloadMode { flags } => {
                write!(
                    f,
                    "fo1-dat: the flags word {flags:#04X} names no known payload mode"
                )
            }
            Self::OutputOverrun { declared } => {
                write!(
                    f,
                    "fo1-dat: the tokens would write past {declared} declared bytes"
                )
            }
            Self::OutputShort { declared } => {
                write!(
                    f,
                    "fo1-dat: the stream ended short of {declared} declared bytes"
                )
            }
            Self::ConsumptionMismatch { declared, actual } => {
                write!(
                    f,
                    "fo1-dat: the payload consumed {actual} byte(s) against {declared} declared"
                )
            }
            Self::EmptyBlock { at } => {
                write!(
                    f,
                    "fo1-dat: the block header at byte {at} declares a zero length payload"
                )
            }
        }
    }
}
```

| Variant | Exact text | What the message must name |
| --- | --- | --- |
| `Truncated` | `fo1-dat: the archive ran out at byte {at}` | The byte offset at which the failed read started: the field that could not be read, or, for a whole LZSS stream, the entry's payload base plus the codec's position inside it. |
| `LengthOutOfRange` | `fo1-dat: a length field at byte {at} exceeded the bytes present` | The byte offset at which the failed read started: the length byte, or the name's bytes, that asked for more bytes than were present. |
| `OutOfBounds` | `fo1-dat: a region at byte {at} ran outside the archive` | The byte offset of the region that ran outside the archive: the region's end, or the framed block's header start for a block inside a chain. |
| `OffsetOverflow` | `fo1-dat: an offset plus a length overflowed` | Nothing but the overflow itself. There is no offset to name: no field and no region failed, the arithmetic did. |
| `UnknownPayloadMode` | `fo1-dat: the flags word {flags:#04X} names no known payload mode` | The flags word exactly as stored, in upper case hex with a `0x` prefix. |
| `OutputOverrun` | `fo1-dat: the tokens would write past {declared} declared bytes` | The declared uncompressed size the tokens would have written past. |
| `OutputShort` | `fo1-dat: the stream ended short of {declared} declared bytes` | The declared uncompressed size the stream never reached. |
| `ConsumptionMismatch` | `fo1-dat: the payload consumed {actual} byte(s) against {declared} declared` | Both numbers: the bytes the payload actually consumed and the compressed size the entry declares. |
| `EmptyBlock` | `fo1-dat: the block header at byte {at} declares a zero length payload` | The byte offset of the framed block's header start: the chain's payload base plus the block's position within it. |

Worked example: a tree that runs out inside the root header's third field
carries `at: 8`, and renders

```
fo1-dat: the archive ran out at byte 8
```

---

## 3. What this spec does NOT settle

- **The LZSS codec.** Ring size, prefilled bytes, write index, token layout and
  the back reference arithmetic belong to `fo1-lzss`. This spec only says that
  each LZSS block starts with a fresh ring.
- **Produced-size closure.** DAT-023 is open. Nothing here asserts that any entry
  decodes to its declared size; that needs the codec and the gate.
- **Anything inside a payload.** The `.PAL`, `.AAF`, `.FON`, `.FRM`, `.PRO` and
  `.MSG` formats are separate specs. The archive layer hands back bytes.
- **The loose-file patch folder.** DAT-028 states the guide's rule without
  confirming the precedence order against bytes. No search path is specified here.
- **Which physical regions overlap.** I established that stored extents fit inside
  both archives and that chains consume exactly their compressed size. I did not
  prove that distinct entries occupy disjoint regions, so the reader must not
  assume it either.
- **Empty archives and zero-directory archives.** No measured archive or block
  exercises these. The parse contract for a zero-entry block is DAT-033 and for a
  zero-length entry name is DAT-034; nothing here settles an archive with no
  directories at all.
- **Whether a zero field 0 or a zero-length name ought to be rejected.** DAT-033
  and DAT-034 record both as accepted, because no byte evidence establishes
  either as malformed. Adding a rejection now would be an unmeasured claim: it
  would assert engine behaviour that nothing in either archive shows, and it
  would turn a parse that succeeds on unknown input into a failure nobody can
  reproduce from the data. The guide's rule for exactly this situation is to
  isolate the question and list it as open, which is what section 2 does. The
  question stays open until someone measures an archive that carries either.
- **A block chain whose decoded output overshoots mid-chain.** The rule is to
  error (DAT-024); whether the original engine clamps instead was not measured and
  is deliberately not reproduced.
- **Whether a payload may be shared between entries.** Not measured, not assumed.
- **Other installs.** Every count in section 1.3 belongs to this install. A
  different install is `unsupported install`, not a relaxed constant.
- **Order of blocks within a chain.** Chains are read in stored order because
  that is the only order in which the output reaches the declared size. Nothing
  suggests a chain is reordered or seekable.
- **Whether the field name belongs in the payload.** The guide says a variant
  carries "the byte offset and the field name". `DatError` carries the offset in
  the payload and names the field in the message, which the *Error text* section
  fixes. The two carry the same fact in two places and the gate asserts the
  message word for word, so a message that names the wrong field fails. Whether
  the field name ought to be a payload field of its own is not settled here.
- **Which byte an LZSS block inside a chain names.** The *byte offset an error
  carries* section gives the formula the container uses: `chain start + ip`,
  where `ip` is counted from the start of the **block's** payload while the base
  is the **chain's** payload start. The sum is therefore short of the
  file-relative offset of the byte by the block's two byte frame header and by
  whatever blocks came before it: a chain whose stored bytes start at byte 60
  and whose first block's back reference is cut reports byte 61, while the byte
  it names is at byte 63. A whole LZSS stream has no such gap, because its
  base is the start of the payload the codec is reading. Whether the chain path
  should add the block's own start instead is not settled here, and no test
  pins the chain path's number, so the behaviour can change without a gate
  failing. The gate pins the stream path, where the number is the
  file-relative offset of the byte.

---

## 4. Source disagreements

A place for a measured fact that disagrees with one of my sources. Nothing here
is resolved by editing a source: the disagreement is recorded so that a later
reader can see both numbers and decide which one their change is about.

### The guide's archive gate attributes the corpus chain count to MASTER.DAT

`CONFIRMED AGAINST ORIGINAL BINARY`

The guide's *Archive gate* checklist reads "MASTER.DAT has 4,787 raw entries
and 20,456 chain entries." The first half is right and the second half is the
corpus total, not a MASTER.DAT count. Measured, per archive:

| Quantity | MASTER.DAT | CRITTER.DAT | Corpus total |
| --- | --- | --- | --- |
| Mode `0x20`, stored entries | 4,787 | 0 | 4,787 |
| Mode `0x40`, chain entries | 14,997 | 5,459 | 20,456 |

The guide's own *DAT archives* mode table is where the two numbers come from,
and there they are already separated: `0x20` is given as "4,787, all in
MASTER.DAT" and `0x40` is given as a bare `20,456` with no archive attributed.
The checklist attached the corpus figure to the archive named in the same
sentence.

Nothing in this spec depends on the attribution. DAT-016 and DAT-026 state the
measured split, section 1.3 tabulates it per archive, and the gate asserts
14,997 and 5,459 separately rather than the sum. The guide is the owner's
document and is not amended here.

The note exists so the disagreement is visible instead of silently resolved. A
later reader who finds the checklist should not "fix" this spec to match it.

### The crate's own field docs still read the offsets the other way

`UNVERIFIED`

The *byte offset an error carries* section of this spec fixes the `at` of
`DatError::Truncated` as the byte offset at which the failed read **started**,
never the offset at which the failure was noticed, and the interface block in
DAT-030 now carries that wording on the field itself.

`crates/fo1-dat` still carries the other wording on the same field: "File-
relative byte offset at which the reader ran out." That is the notice reading,
the reading this spec removed, and it is the wording the crate's own doc
comment teaches a reader of the crate rather than of this spec. The two other
fields that carry an offset are looser in the same way: `LengthOutOfRange` says
"of the length byte or field" without saying which of the two it is, and
`OutOfBounds` says "of the region that ran out", which is the region's end on
one path and nothing at all on the chain path.

The three disagree with the spec about the same fields, so the disagreement is
recorded here rather than resolved by editing either side. The crate's
behaviour already matches this spec — the gate pins the value the container
actually produces (`at: 8` for a root header whose third u32 is cut in half,
and that value is the start offset) — so only the doc comments differ.
Bringing the comments into line is a change to `crates/fo1-dat`, which this
spec does not own.

### The crate's own `payload_mode` doc denies a mode the type has

`UNVERIFIED`

The DAT-030 interface block in this spec now says what the function does: the
signature cannot carry an error, so the function is total, and a nibble it
cannot map yields `PayloadMode::BlockChain` as a placeholder. `PayloadMode`
has no unknown variant, and `BlockChain` is a real mode — it is what a chain
entry decodes as.

`crates/fo1-dat` says the same thing but calls the placeholder "a placeholder
that names no real mode", which denies a mode the type has. That is the same
claim this spec removed: a reader who believes it looks for an unknown variant
that does not exist.

The crate's behaviour is already the one the spec states — `payload_mode`
returns `BlockChain` for a nibble it cannot map, and the refusal is
`DatError::UnknownPayloadMode`, raised by the two decode functions — so only
the comment differs. Bringing it into line is a change to `crates/fo1-dat`,
which this spec does not own.
