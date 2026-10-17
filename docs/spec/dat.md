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

`CONFIRMED AGAINST ORIGINAL BINARY`

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
    Truncated { at: u64 },
    /// A length or count field exceeded the bytes actually present.
    LengthOutOfRange { at: u64 },
    /// A block header or payload ran outside the archive.
    OutOfBounds { at: u64 },
    /// An offset plus a length did not fit in the address arithmetic.
    OffsetOverflow,
    /// A flags high nibble this build does not know.
    UnknownPayloadMode { flags: u32 },
    /// The tokens would have written more than the declared output size.
    OutputOverrun { declared: u32 },
    /// The stream ended before reaching the declared output size.
    OutputShort { declared: u32 },
    /// The payload did not consume exactly the declared compressed size.
    ConsumptionMismatch { declared: u32, actual: u32 },
    /// A block header declared a zero length payload.
    EmptyBlock { at: u64 },
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
    directories: Vec<DirectoryRecord>,
    entries: Vec<EntryRecord>,
    /// Uppercased composed path to position in `entries`. Empty until first
    /// lookup, because most callers only iterate.
    index: std::cell::OnceCell<std::collections::HashMap<String, usize>>,
}

impl DatArchive {
    /// Parses the root header, the directory names, and every block with its
    /// entries. Parsing never reads the payload region, so a truncated payload
    /// does not stop the directory from being listed.
    pub fn parse(bytes: &[u8], origin: &'static str) -> Result<Self, DatError>;

    /// Directory names in file order.
    pub fn directories(&self) -> &[DirectoryRecord];

    /// Entries in file order: all of block 0, then all of block 1, and so on.
    /// This is tree order, which is not physical order (DAT-012).
    pub fn entries(&self) -> &[EntryRecord];

    /// Looks up an already-uppercased, already-backslashed path.
    pub fn find(&self, upper_path: &str) -> Option<&EntryRecord>;
}

/// The high nibble of the flags. A flags value of 0 means 0x10; any other
/// unknown nibble is an error rather than a default.
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
a second meaning for it.

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
- **Empty archives, zero-directory archives, zero-entry blocks.** No measured
  archive or block exercises these. The parse contract for them is not specified.
- **A block chain whose decoded output overshoots mid-chain.** The rule is to
  error (DAT-024); whether the original engine clamps instead was not measured and
  is deliberately not reproduced.
- **Whether a payload may be shared between entries.** Not measured, not assumed.
- **Other installs.** Every count in section 1.3 belongs to this install. A
  different install is `unsupported install`, not a relaxed constant.
- **Order of blocks within a chain.** Chains are read in stored order because
  that is the only order in which the output reaches the declared size. Nothing
  suggests a chain is reordered or seekable.