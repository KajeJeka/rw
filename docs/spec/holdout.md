# Hold-out vector format

**Provenance.** Written by the `spec author` role. Two sources only:

1. `RUSTY WASTELAND Engine Implementation Guide, Revision 2.md`, sections
   *Hold out vectors and canaries*, *Independent oracles*, *Exact assertions*,
   *Separate the roles*, *Publication rules*, *Hygiene checks*, *Fixtures and
   golden files* and *Banned patterns*.
2. The vocabulary this repository already uses: `docs/spec/dat.md`,
   `docs/spec/lzss.md`, `docs/spec/exe-tables.md`, `docs/spec/trace.md`,
   `crates/fo1-tests` and `crates/fo1-exe`.

Nothing below is a quotation. No Community Edition source was opened. Every
number that belongs to the reference install is named by the claim that already
holds it, and none of those numbers is restated here. This file contains no
byte of the player's install, no path inside it, and no digest of it.

Every claim carries exactly one evidence tag:

| Tag | Meaning |
| --- | --- |
| `CONFIRMED AGAINST ORIGINAL BINARY` | Measured from the real bytes of the reference install |
| `CONFIRMED IN COMMUNITY EDITION ONLY` | CE is the only source |
| `VERSION DIFFERENCE IDENTIFIED` | A measured difference between installs |
| `UNVERIFIED` | Asserted or required, not measured |

This file defines a **format**, not a measurement. Every claim in it is
therefore `UNVERIFIED`: a format is a contract this project commits to, and no
part of it is a claim about the original binary. Where a rule here restates a
rule that lives in another spec, the other spec's claim id is cited and the
number is not copied.

---

## 1. Where the vectors live

### HOLD-001 — The vectors live outside the repository, under `FO1_REF`

`UNVERIFIED`

The hold-out set lives in the folder `FO1_REF/holdout/`, where `FO1_REF` is the
private references directory the guide already names. The guide's
*Hold out vectors and canaries* section puts the owner's extra vectors there
for exactly this purpose.

The folder is outside the working tree. `git add` cannot reach it, the public
hygiene check cannot see it, and nothing the repository ships contains a byte of
it. The repository holds **no vector data at all**: no manifest, no archive
fixture, no expected byte, no digest of a vector file.

### HOLD-002 — The repository holds no vector data

`UNVERIFIED`

No file under this repository may carry a hold-out vector, a vector manifest,
or a digest of one. The manifest is named by this spec but is not committed.
The only thing this repository carries is this document, which describes the
format and carries no data.

The reason is the guide's *What must never be committed* rule and its
*Publication rules*: the repository is public from its first commit and git
keeps everything ever committed. A vector set that was committed once cannot be
un-committed, and a committed vector set is a vector set the implementer can
read.

### HOLD-003 — The folder layout

`UNVERIFIED`

```
FO1_REF/holdout/
    manifest.txt          the manifest, one record per line
    dat/                  one file per DAT vector
        <name>.hex
    exe/                  one file per executable table vector
        <name>.bin
```

Rules:

- `manifest.txt` is the only file directly inside `FO1_REF/holdout/`.
- `dat/` holds exactly the DAT vectors and `exe/` holds exactly the executable
  table vectors. Nothing else lives in either folder.
- A file or a directory that these rules do not name is a failure of the test
  and not something to ignore (HOLD-026).
- A vector file's name is its **case name**. The case name is the file's stem:
  lower case ASCII letters, digits and `_`, beginning with `dat_` for a DAT
  vector and `exe_` for an executable table vector. The stem is unique across
  the whole set.
- The `.hex` extension is used for the DAT vectors because the repository
  already uses `.hex` for synthetic fixtures. The `.bin` extension is used for
  the executable table vectors because their payload is arbitrary binary. Neither
  extension is one the hygiene check bans, so the folder stays publishable by
  accident if it is ever moved.

---

## 2. The manifest

### HOLD-004 — The manifest is line oriented text, and this is deliberate

`UNVERIFIED`

The manifest is a plain text file with one record per line. It is not JSON and
not TOML.

The reason is the workspace rule: the crates carry **no third-party
dependency**, so a manifest reader in Rust has to be written from the standard
library. A line oriented format is read with a line split, a whitespace split
and one integer parse per field. A JSON subset would need a parser of its own,
with its own escape rules, its own number rules and its own failure modes, and
every one of those is a place where a reader and a writer can disagree
silently. TOML would need the same. The guide's *Engineering philosophy* asks
for a total reader that never indexes past the end; the simplest way to get one
over a manifest is to not give the manifest a grammar that can nest.

### HOLD-005 — The encoding and the line rules

`UNVERIFIED`

The manifest is UTF-8 with no byte order mark, and every rule below is a rule
the reader enforces. Each rule names the refusal it produces, so a reader and
the person who wrote the manifest agree on what a broken file is. All of these
refusals are `manifest unreadable` refusals, and the detail names the line and
what is wrong with it.

| Rule | The refusal it produces |
| --- | --- |
| the file is UTF-8 and carries no byte order mark | `manifest unreadable`, naming the file and the encoding |
| every line ends with a single `\n`, and no line carries a `\r` | `manifest unreadable`, naming the line and the carriage return |
| the last line ends with `\n` too | `manifest unreadable`: the manifest does not end with a newline |
| a line is a sequence of **tokens** separated by exactly one space (`0x20`) | `manifest unreadable`, naming the line: two spaces in a row, or a leading space, or a trailing space |
| there is no tab anywhere in the file | `manifest unreadable`, naming the line that carries the tab |
| there are no empty lines | `manifest unreadable`, naming the line that is empty |
| there are no comment lines: a line that begins with `#` is a malformed line | the refusal any unknown key gets: a header line that is not the key it must be, or a record whose kind is neither `dat` nor `exe` |
| every token is non-empty and contains no space | the refusal the token separator produces: the split yields an empty token |

### HOLD-006 — The header lines, in this order

`UNVERIFIED`

The manifest opens with exactly five header lines, in this order, each of them
two tokens:

| Line | Value | Meaning |
| --- | --- | --- |
| `format_version` | one decimal integer | the version of this format |
| `seed` | 64 lower case hex characters | the seed the generator ran with |
| `generator` | one token | the name and version of the generator |
| `dat_count` | one decimal integer | how many `dat` records follow |
| `exe_count` | one decimal integer | how many `exe` records follow |

Rules:

- `format_version` **must be the first line of the file**. A manifest whose
  first line is anything else is refused.
- All five header lines are required. A missing header line is a refusal, not a
  default.
- The five header lines come before the first record and in the order above.
  A header key that appears after a record has started is a refusal.
- No other header key exists. A key the reader does not know is a refusal
  (HOLD-012).

The five header lines carry no data about the vectors themselves. They say
which set this is, and how many records it holds.

### HOLD-007 — The version rule

`UNVERIFIED`

`format_version` is a single decimal integer. This file defines version `1`.

A reader knows a set of versions. It refuses every manifest whose
`format_version` is not one of them, and it refuses a manifest whose
`format_version` line is missing or is not a decimal integer.

The refusal is a hard failure. It is not a warning, not a skipped test and not
a fallback to the reader's own default version. A reader that guesses a version
is a reader that silently grades a set it does not understand.

The rule is deliberately narrow: a version `1` reader knows version `1` and
nothing else. When the format grows, the new version number is a new number,
and a reader is taught the new number **and** the old one together if the old
one stays readable. A reader is never taught to accept a version it does not
implement.

### HOLD-008 — The seed and the generator

`UNVERIFIED`

`seed` is the seed the generator ran with. `generator` names the generator and
its version, as one token of `[A-Za-z0-9._-]`.

Together they make the set reproducible: the same generator with the same seed
produces the same vectors. The manifest therefore records enough to rebuild the
set byte for byte, which is what lets the owner add a vector, regenerate, and
know that nothing else moved.

Neither field is a secret, and neither is an input to any check. The reader
uses the seed for nothing at all: it does not derive bytes from it, it does not
compare it against anything, and it does not refuse a set whose seed it does
not recognise. The seed is provenance, and the generator name says which
provenance.

### HOLD-009 — The counts

`UNVERIFIED`

`dat_count` and `exe_count` say how many records of each kind the file
carries. A reader counts the records it read and compares that count with the
declared count, with equality. A mismatch is a refusal.

The counts belong to the owner. The reader never hard-codes a count: the owner
may add vectors in a later milestone without changing the format, and the count
in the manifest is what the reader checks against. HOLD-029 and HOLD-031 fix the
count of the **first milestone's** set — 60 DAT vectors and 12 executable table
vectors — which is a rule about the vectors, not about the reader.

### HOLD-010 — The record lines

`UNVERIFIED`

After the header come the records. All `dat` records come first, then all
`exe` records, in the order the manifest lists them. Every record line has
exactly **seven** tokens: the kind, then six fields.

| Kind | Field 1 | Field 2 | Field 3 | Field 4 | Field 5 | Field 6 |
| --- | --- | --- | --- | --- | --- | --- |
| `dat` | `file` | `file_bytes` | `file_sha256` | `decoded_size` | `decoded_sha256` | `path` |
| `exe` | `file` | `file_bytes` | `file_sha256` | `file_offset` | `length` | `va` |

The two kinds share one shape on purpose: one tokenizer, one loop, one record
type with two kinds of payload. A reader that has to know which kind it is
holding before it can parse the line is a reader with two parsers.

- `file` is the vector file's path **relative to the folder that holds the
  manifest**, with `/` as the separator on every platform. It is
  `dat/<name>.hex` or `exe/<name>.bin`.
- The field is **written** with `/` on every platform, whatever separator the
  platform uses for paths of its own. A reader splits the field on `/` and
  joins the two halves with the platform's own separator, so the two folders
  on disk are `dat` and `exe` everywhere and the manifest looks the same on
  every machine.
- `file_bytes` is the exact byte length of that file on disk.
- `file_sha256` is the SHA-256 of that file's bytes **exactly as stored on
  disk**.
- The remaining four fields mean what section 3 and section 4 say.

The per-file digest is what binds the set to the manifest. A file that is
changed, truncated or replaced under the manifest keeps the same name and the
same count, and only its digest changes, so the digest is the one field that
says the set is the set the manifest describes. The count of records, the
declared byte length of each file and the rule that an undeclared file in
either folder is a failure complete the binding (HOLD-026).

### HOLD-011 — The token rules for numbers and digests

`UNVERIFIED`

- Every integer field is a **decimal** unsigned integer: digits `0` to `9`,
  no sign, no `0x` prefix, no thousands separator, no underscore.
- An integer has **no leading zero**, except that the value zero is written as
  the single character `0`. `0` is legal; `00` and `007` are not.
- The width of every integer field is fixed:

| Field | Width |
| --- | --- |
| `format_version`, `dat_count`, `exe_count` | unsigned 64 bit |
| `file_bytes`, `decoded_size` | unsigned 64 bit |
| `file_offset`, `length` | unsigned 64 bit |
| `va` | unsigned 32 bit |

  `va` is 32 bit because that is the width the reader is addressed with:
  `PeImage::read_va` takes a `va: u32`. The other fields are 64 bit because
  they count bytes, and the format does not say they are small.
- A number that does not fit the width of its field is a refusal, never a
  truncation and never a wrap: a number that does not fit 64 bits is
  `manifest unreadable`, naming the line; a `va` that does not fit 32 bits is
  `manifest unreadable`, naming the line and the address.
- A **range** that would not fit is refused as well, and never computed with
  an overflow: the sum `file_offset + length` of an executable table record is
  computed in checked arithmetic, and a sum that would overflow is refused
  like a window that runs past the end of the executable
  (`vector offset outside the executable`, HOLD-023 step 2). A `length` that
  does not fit the machine's own `usize` is refused the same way.
- A digest field is exactly **64 lower case hex characters**: `0` to `9` and
  `a` to `f`. An upper case digest is a refusal, not a case folded comparison.
  The repository's own digest helpers already emit lower case hex, so the
  comparison is a plain string equality.
- The digest is taken over bytes, never over a file's text form. For a `.hex`
  vector file the digest is over the **file's bytes on disk**, which is not the
  same thing as the digest of the archive the file encodes.

### HOLD-012 — The reader refuses a malformed manifest

`UNVERIFIED`

Every one of the following is a refusal, and a refusal is a hard failure of
the hold-out test (HOLD-026). None of them is a warning, and none of them
causes the test to skip:

| Condition | Why it is refused |
| --- | --- |
| the first line is not `format_version <n>` | the version comes first |
| `format_version` is not a version the reader knows | HOLD-007 |
| a header line is missing | HOLD-006 |
| a header key the reader does not know | a reader must not ignore a field it does not understand |
| a header key appears twice | a repeated key is ambiguous |
| a header key appears after the first record | the header is a prefix |
| a record line does not have exactly seven tokens | the shape is fixed |
| a record kind is neither `dat` nor `exe` | the reader must not skip a record it does not understand |
| a `dat` record follows an `exe` record | the `dat` records come first and the `exe` records follow them (HOLD-010) |
| an integer field is not a canonical decimal integer | HOLD-011 |
| an integer field does not fit the width of its field | HOLD-011 |
| a digest field is not 64 lower case hex characters | HOLD-011 |
| a `file` field is absolute, or contains `..`, or contains `\` | the manifest names files inside the folder and nowhere else |
| the manifest does not end with a newline | HOLD-005 |
| a line is empty | HOLD-005 |
| a line carries a carriage return or a tab | HOLD-005 |
| a line holds two spaces in a row, or a leading space, or a trailing space | HOLD-005 |
| the number of records of a kind differs from the declared count | HOLD-009 |

The reader reads the manifest **once**, into memory, before it runs a single
case. A manifest that does not parse produces no cases at all, and the test
fails with the manifest's line number and the reason.

### HOLD-013 — The shape of a manifest

`UNVERIFIED`

A manifest with two records looks like this. The digests are written as
`<sha256>` here, which is **not** a legal token: a real manifest carries 64
lower case hex characters, and a manifest holding the placeholder is refused by
the rule in HOLD-011. The numbers in the example are illustrative and are not
the reference build's offsets; the real ones live in `docs/spec/exe-tables.md`.

```
format_version 1
seed 9f3c1a5b7d2e48069b8c4d7e0f2a5b3c8e1d4f7a0b3c6d9e2f5a8b1c4d7e0f3a
generator holdout-gen-v1
dat_count 1
exe_count 1
dat dat/dat_0001_stored_single_byte.hex 4096 <sha256> 1 <sha256> SYNTHETIC\ONE.BIN
exe exe/exe_0001_first_window.bin 16 <sha256> 1024 16 1048576
```

---

## 3. The DAT vector

### HOLD-014 — A DAT vector holds a synthetic archive, a path, a size and a digest

`UNVERIFIED`

One DAT vector is one synthetic DAT archive plus three facts about it:

| Field | What it is |
| --- | --- |
| `file` | the archive, held in the vector file |
| `path` | the composed path the entry under test must have |
| `decoded_size` | the number of bytes the reader must produce for that entry |
| `decoded_sha256` | the SHA-256 of exactly those bytes |

The vector is a **complete archive**, not a fragment. The reader parses it with
the same parser it uses for the real archives, so a vector exercises the
container as well as the payload: the root header, the directory names, the
block header, the entry record, the payload mode and the decode.

### HOLD-015 — The vector file holds the archive as lower case hex

`UNVERIFIED`

A DAT vector file holds the archive bytes as **lower case hex**, two characters
per byte, in file order:

- The characters are `0` to `9` and `a` to `f`. Nothing else is legal inside
  the file.
- There is no whitespace inside the file: no space, no tab, no newline between
  bytes.
- The file ends with a single `\n`. The reader removes that one trailing
  newline and then requires the rest of the file to be hex characters of even
  length.
- An odd number of hex characters is a refusal. A character that is not a hex
  character is a refusal. An empty file is a refusal.

Hex rather than raw bytes because the owner's folder is a private research
folder: a hex file can be read, checked and argued about by eye and by any text
tool, while a raw binary file cannot be checked at all without the tool that
made it. The size cost is a factor of two and the archives in this set are
small, which section 6 keeps them.

The digest in the manifest is over the **file's bytes on disk**, including the
trailing newline. The archive the reader works on is the file's bytes with that
one newline removed, decoded from hex.

Only the hex form is carried. The binary archive is not stored a second time
beside it: the reader decodes the hex, and the decoded bytes are the archive.
Two copies of the same archive would be two things to keep in step, and one of
them would be the one a reader used by mistake.

### HOLD-016 — The path rules

`UNVERIFIED`

`path` is the **composed** path of the entry, in the form the reader looks up:

- It is non-empty.
- Its characters are upper case ASCII letters, digits, `.`, `_`, `-` and the
  separator `\` (`0x5C`).
- The character rule is about the characters of the **names**, not about the
  whole path: the `\` that joins two names is the separator and not one of the
  characters a name may hold. Every family this section lists is therefore
  legal, including the ones whose path carries a directory prefix.
- Its segments are separated by a single `\`, and it has no leading `\`, no
  trailing `\` and no empty segment.
- It contains no space, so it is one token.

The path is written unambiguously because the reader never parses it. It
composes the path of every entry in the archive under DAT-011 and compares the
string with the manifest's path as an exact match (HOLD-018 step 4). A
directory name may itself hold a `\` (family C15, DAT-003), so a path cannot be
split back into the names it was made of, and no reader may try to split it.

The path is already upper case, because the repository's rule is that the
caller uppercases and the reader folds nothing (DAT-031, DAT-032).

That has a consequence worth stating: **these vectors cannot tell a reader
that folds case from one that does not**, because every path in the set is
already upper case and folding an upper case path changes nothing. The case
rule is settled by the repository's own install test, not here. A hold-out
vector could only settle it by carrying a lower case path and expecting a
miss, and this format has no kind for "the lookup must fail"; adding one
would be a rule about the manifest that no other record needs.

A path that names nothing in the install is the point: every path in this set
is synthetic.

### HOLD-017 — The decoded size and the decoded digest

`UNVERIFIED`

`decoded_size` is the number of bytes the reader must produce for the entry at
`path`. `decoded_sha256` is the SHA-256 of **exactly those bytes**, and of
nothing else: not of the archive, not of the entry record, not of the payload
as it was stored. Lower case hex, 64 characters.

The digest is the load-bearing check. A reader that stops at the declared
uncompressed size produces the right **length** on every input, wrong input
included (LZSS-014), so the size alone proves nothing. The digest is what
distinguishes a decoder that decoded the stream from one that produced the
right number of bytes by stopping.

### HOLD-018 — What the reader does with a DAT vector

`UNVERIFIED`

For each `dat` record, in this order, and every step is an assertion:

1. The vector file exists, its byte length on disk is `file_bytes`, and the
   SHA-256 of its bytes is `file_sha256`.
2. The file's bytes are decoded per HOLD-015 into the archive bytes.
3. The archive parses. A parse failure is a failure of this case.
4. The archive holds an entry whose composed path is `path`. An entry that is
   not there is a failure of this case.
5. That entry decodes. A decode failure is a failure of this case.
6. The decoded bytes are exactly `decoded_size` bytes long.
7. The SHA-256 of the decoded bytes is `decoded_sha256`.
8. The entry's own declared uncompressed size equals `decoded_size`.

Step 8 is redundant with step 6 for a correct reader and is kept because it
catches a vector whose own numbers disagree, which is a defect in the set and
not in the reader.

### HOLD-019 — The DAT vectors are made by an independent decoder

`UNVERIFIED`

The DAT vectors are produced by a **decoder written from this repository's
specifications alone** — `docs/spec/dat.md`, `docs/spec/lzss.md` and this file
— and written in a **language other than Rust**.

Two properties follow, and both are the reason the hold-out set exists:

- The decoder is never imported by production code and is never part of the
  crate graph. It is a tool the owner runs to make the set, and the set is what
  the gate grades with.
- The expected bytes are therefore not the output of the code under test. They
  are a second implementation's answer, and the two implementations share only
  the specifications. That is what makes the expected bytes an oracle and not
  a snapshot (guide: *Independent oracles*, class 4).

Writing the decoder in another language is not decoration. A second decoder in
Rust, written next to the first, tends to make the same mistake twice; a
decoder in another language, written by someone who has not read the Rust, is
an independent witness.

---

## 4. The executable table vector

### HOLD-020 — An executable table vector holds an offset and the bytes there

`UNVERIFIED`

One executable table vector names a region of the reference executable and the
bytes the reader must produce for it:

| Field | What it is |
| --- | --- |
| `file` | the expected bytes, held in the vector file |
| `file_offset` | the offset of those bytes in the reference executable file |
| `length` | the number of bytes |
| `va` | the virtual address of the same bytes |
| `file_bytes`, `file_sha256` | the size and the digest of the vector file |

The two coordinates are both required and they are not redundant.

- `file_offset` is the anchor. It says where in the executable file the region
  lives, it is what the manifest binds with a digest, and it is how the owner
  finds the region again when the set is regenerated.
- `va` is the coordinate the reader is asked about. The reader in this
  repository is addressed by virtual address: `PeImage::read_va` takes a `va`
  and a length, and it translates. A vector that carried only a file offset
  could not be put to the reader at all; the test would have to translate it
  itself, and then the vector would grade the test's arithmetic instead of the
  reader's.

The reader's own translation is checked against the manifest's file offset, so
the two coordinates cannot drift apart (HOLD-023).

### HOLD-021 — The vector file holds the expected bytes, raw

`UNVERIFIED`

An executable table vector file holds the expected bytes **raw**, exactly as
they appear in the reference executable, and nothing else:

- The file's byte length on disk is `length`, and it is equal to `file_bytes`.
  A reader must check both.
- The file carries no header, no trailer and no newline. The bytes in the file
  are the bytes of the executable region, one for one.
- `file_sha256` is the SHA-256 of the file's bytes on disk, which is therefore
  also the SHA-256 of the expected bytes.

Raw rather than hex because these are arbitrary bytes of an executable, up to
4096 of them per vector, and hex would double the size of the set for no gain:
this folder is not read by eye.

### HOLD-022 — Which executable the reader reads, and how it knows the offset is inside it

`UNVERIFIED`

The reader reads **the player's own copy** of the reference executable, reached
through the install harness, the same copy every other install gate test
reads. The reader must not read an executable from anywhere else.

Before any vector is applied, the executable's digest must be the reference
digest, and a build that is not the reference build is refused outright. The
reader already does this: `PeImage::require_reference_build` checks the digest
before it parses a header and before it uses any constant (EXETBL-001). The
hold-out test uses that entry point and must not add a second, weaker gate. A
digest mismatch is a failure of the whole executable table part of the hold-out
test, not a failure of one case, because every constant in the executable layer
is true for one exact build only.

The offset is inside the file when two things hold:

- `length >= 1`, and
- `file_offset + length` does not pass the end of the executable.

The end of the executable is the byte length of the file the reader actually
read. The digest gate has already established that this file is the reference
build, so that length is the pinned length the repository holds, and no
constant has to be repeated here.

The reader's own section bound applies as well, and it is stricter: a read is
bounded by the raw extent of the section that owns the virtual address, not
only by the end of the file (EXETBL-002). Every window in this set lies inside
one section's raw extent, so a correct reader answers every one of them.

That is also what this set cannot do, and it is said here rather than left to
be discovered: every window lies **strictly** inside its section's raw extent,
so this set cannot tell a reader that bounds a read by the end of the file
alone from one that bounds it by the section. The difference is graded by the
repository's own install test, not by the hold-out set.

### HOLD-023 — What the reader does with an executable table vector

`UNVERIFIED`

For each `exe` record, in this order, and every step is an assertion:

1. The vector file exists, its byte length on disk is `file_bytes` and is
   equal to `length`, and the SHA-256 of its bytes is `file_sha256`.
2. `length >= 1`, `file_offset + length` is computed in checked arithmetic and
   does not overflow, and the sum does not pass the end of the executable. A
   sum that would overflow is refused like a window that runs past the end:
   `vector offset outside the executable`.
3. The reader, asked for `length` bytes at `va`, returns exactly the vector
   file's bytes.
4. The reader's own section table maps `va` to `file_offset`: the section that
   owns `va` exists, and `raw_pointer + (va - IMAGE_BASE - virtual_address)`
   equals `file_offset`. The two coordinates the reader exposes are named
   `IMAGE_BASE` and the section's fields, exactly as EXETBL-002 states them.
5. The bytes at `file_offset` in the executable file are the vector file's
   bytes.

Step 3 asks the reader. Step 4 asks the reader's translation. Step 5 asks the
file. Together the three say the same region three ways, and a reader with a
wrong mapping fails step 3 or step 4 whichever way it is wrong.

The offsets and the virtual addresses of the seven tables are **not** in this
file. The producer takes them from the table *File offsets computed for the
seven tables* in `docs/spec/exe-tables.md`, and derives each window's virtual
address with the mapping rule in EXETBL-002. This file repeats none of those
numbers, so the two cannot drift.

### HOLD-024 — The executable table vectors are made independently too

`UNVERIFIED`

The expected bytes of an executable table vector are **read out of the
reference executable by a separate extraction tool**, not produced by the
reader under test, and not copied out of a test fixture (guide: *Independent
oracles*, class 1).

The extraction tool is a separate program that opens the executable, seeks to
the offset, reads the length and writes the bytes into the vector file. It is
not part of any crate in this workspace, and the reader under test never sees
it.

---

## 5. What the reader must do, and what it must report

### HOLD-025 — Two gate tests, both ignored by default

`UNVERIFIED`

The hold-out test is one gate file with two test functions. The file lives
beside the other gate tests under `tests/gates/`, and this spec fixes its name
as `holdout.rs`, after the convention the gate files already follow. It belongs
to the test author: the implementer may add tests under `src/`, but it may not
edit or delete a gate test.

| Test | What it needs |
| --- | --- |
| the DAT test | the hold-out folder |
| the executable table test | the hold-out folder and the reference install |

Both carry an `#[ignore]` with a reason that names the missing fixture, so the
default `cargo test` run stays fast and the full gate runs them with
`--ignored` (guide: *Exact assertions*, *The gate script*). The reason names
the fixture that is missing, because the repository's banned pattern rule
requires it.

The gate's count file names this file with the key `install_holdout` and the
value `2`. The count check reads the number of `#[test]` functions the file
holds and compares it with that floor: a file that measures **below** the
floor fails, and a file that measures zero fails outright, so the row is not
optional for a gate file. The value may only rise, and only with the owner's
approval.

The number of **cases** the file runs is not in that file and is not a floor:
it belongs to the manifest and rises whenever the owner adds a vector
(HOLD-009). That is why the floor counts the two test functions and not the
cases.

The two are separate because the DAT vectors need nothing but the folder: a
machine with the folder but no install must still be able to run them, and a
missing install must not hide a DAT failure behind an install failure.

The executable table test needs the install because the executable table layer
reads the player's own copy at run time. That is the layer's whole design: no
table value is ever committed to this repository.

The hold-out folder is reached through the `fo1-tests` harness, because the
banned pattern rule forbids a direct read of `FO1_REF` outside the harness and
forbids any read of `FO1_REF` in production code.

The harness holds the one reading of the folder: `fo1_tests::holdout_root`
resolves `FO1_REF/holdout/` and `fo1_tests::holdout_manifest_path` resolves
`FO1_REF/holdout/manifest.txt`, and the test reads the folder through those
two paths. No other crate reads the folder, and no other reading of it exists
(HOLD-028).

### HOLD-026 — Nothing is ever skipped, and every failure is loud

`UNVERIFIED`

The hold-out test has **no skip path**. Every condition below is a **failure**
of the test, with a message that names the condition:

| Condition | What the test does |
| --- | --- |
| `FO1_REF` is not set | fails, naming the variable |
| `FO1_REF/holdout/` does not exist | fails, naming the folder |
| `manifest.txt` does not exist | fails, naming the file |
| the manifest does not parse | fails, naming the line number and the reason |
| the manifest's version is not one the reader knows | fails, naming the version found and the versions known |
| the manifest's record count disagrees with its counts | fails, naming the count found and the count declared |
| a vector file is missing | fails, naming the case |
| a vector file's size or digest disagrees with the manifest | fails, naming the case |
| a vector file's bytes are not what its kind says they are | fails, naming the case |
| a file in `dat/` or `exe/` is not declared in the manifest | fails, naming the file |
| a file directly inside `FO1_REF/holdout/` other than `manifest.txt` | fails, naming the file |
| a directory inside `FO1_REF/holdout/` other than `dat` and `exe` | fails, naming the directory |
| a directory inside `dat/` or `exe/` | fails, naming the directory |
| a vector does not parse, does not decode, or produces the wrong bytes | fails, naming the case |
| the reference install is missing or unreachable | fails, naming the install, and only the executable table part fails: the DAT part needs no install (HOLD-025) |
| the executable is not the reference build | fails, naming the build mismatch |

The rows about an unexpected file or an unexpected directory are the ones that
separate this from a normal gate test: a file in `dat/` or `exe/` that the
manifest does not declare is a failure, not something to ignore, and so is a
file or a directory the folder holds that the layout of HOLD-003 does not
name. It is what makes the manifest a statement about the whole set and not a
list of some of it: the set is exactly what the manifest describes, and a file
that nobody declared is a defect in the set and not a file to skip.

The row about the install is the one the executable part can actually hit on a
machine that has the folder and no install. It fails the executable table part
whole, and it does not hide a DAT failure behind it: the DAT part needs
nothing but the folder, and it runs and reports on its own.

The banned pattern rule applies to this file exactly as it applies to the rest
of the gate tests: no `continue` inside a loop over cases, no early `return`
inside a loop, no `if let Ok` on a result under test, no `.ok()`, no
`unwrap_or`. A helper that has to give up uses an assertion or a panic, never
a silent return. The guide's rule is that a case passes or the test fails and
prints the case name.

### HOLD-027 — What the test reports

`UNVERIFIED`

The implementer sees a pass or a fail and a case count. Nothing else.

The test prints one summary line per part, in this form:

```
holdout dat: <cases> cases, <passed> passed
holdout exe: <cases> cases, <passed> passed
```

`<cases>` is the number of cases the part ran and `<passed>` is the number that
passed. Both are exact decimal counts and both are the count the manifest
declared, compared with equality.

A failing case prints one line before the summary, in this form:

```
holdout <kind>: FAILED <case-name>: <reason>
```

A failure that belongs to the **whole part** rather than to one case prints one
line before the summary too, in this form:

```
holdout <kind>: FAILED <scope>: <reason>
```

`<kind>` is `dat` or `exe`. `<case-name>` is the vector file's stem.
`<scope>` is one of four fixed names, and it is never a vector's stem:

| Scope | The failure it names |
| --- | --- |
| `manifest` | the manifest cannot be read, cannot be parsed, or carries a version the reader does not know |
| `set` | the hold-out folder cannot be located, or the folder holds a file the manifest does not declare |
| `install` | the reference install is missing or unreachable |
| `executable` | the executable at run time is not the reference build |

A whole-part failure happens before any case runs, so the summary line of that
part is `holdout <kind>: 0 cases, 0 passed` — the same form as every other
summary line, and printed in both cases.

`<reason>` is one of a fixed vocabulary:

```
manifest unreadable
manifest version not understood
manifest count mismatch
references directory not set
undeclared file in the vector folder
vector file missing
vector file size mismatch
vector file digest mismatch
vector file malformed
archive parse failed
entry not found
decode failed
decoded size mismatch
decoded digest mismatch
reader bytes differ from the expected bytes
reader mapping disagrees with the vector offset
executable is not the reference build
vector offset outside the executable
```

Three of these name a failure that HOLD-026 states in words, and each one
names exactly one failure:

| Reason | The failure it names |
| --- | --- |
| `references directory not set` | `FO1_REF` is not set, so the hold-out folder cannot be located. The scope is `set`. |
| `undeclared file in the vector folder` | a file in `dat/` or `exe/` that the manifest does not declare. The scope is `set`. |
| `vector file malformed` | a vector file whose bytes are not what its kind says they are: a `.hex` file that does not end in one newline, that holds an odd number of hex characters, that holds a character that is not a hex character, or that is empty. The scope is the case. |

`archive parse failed` is the neighbouring reason and names the other half of
the same pair: the file's bytes are well formed hex, and the **archive** they
decode to does not parse as a DAT archive.

The vocabulary is fixed so that the reason can be read by a machine and so
that no reason can carry data. A failing line **must not** print the expected
bytes, the expected digest, the vector file's contents, or any part of the
archive. The implementer learns which case failed and what kind of failure it
was, and nothing about the answer.

A part prints its summary line whether it passed or failed, so that the case
count is reported in both cases. The test's exit status is not zero when any
case failed.

### HOLD-028 — The production code never sees the set

`UNVERIFIED`

Only one crate in this workspace may read `FO1_REF` or the hold-out folder:
**`crates/fo1-tests`**, because that is the harness the gate test reads the
folder through (HOLD-025). Every other crate may not, and no crate of
production code may read `FO1_REF` at all. The guide's banned pattern rule
already settles it: a read of `FO1_REF` in production code is forbidden, and a
read of `FO1_DATA` or `FO1_REF` outside the `fo1-tests` harness is forbidden.

This is what makes the set a hold-out. The implementer never sees a vector, so
it cannot special case one, and it cannot fit its code to the set, because it
does not have the set (guide: *How an agent fakes a pass*, the *Hold out
vectors* row).

---

## 6. The rules the vectors obey

### HOLD-029 — The DAT vectors: 60 of them, covering 32 families

`UNVERIFIED`

The first milestone's set holds **60 DAT vectors**. The **32 families** below
fix the shape of an archive; the payload bytes come from the seed.

The coverage rule is this: **every family below is covered by at least one
vector, and a family may be covered by more than one vector.** The 60 vectors
cover all 32 families, and 28 of them are second instances of a family — the
same shape with a different payload, a different path or a different size. One
vector per family is not the rule, and it could not be: it would make 32
vectors, and the set holds 60.

| # | Family | Mode | Where the entry sits | The rule it exercises |
| --- | --- | --- | --- | --- |
| S1 | one stored byte | `0x20` | the `.` root directory | DAT-015 |
| S2 | a stored payload whose length is not a multiple of four | `0x20` | a named directory | DAT-015 |
| S3 | a stored payload as long as one ring | `0x20` | a named directory | LZSS-001 |
| S4 | a stored payload as long as the largest raw block payload the corpus holds | `0x20` | a named directory | DAT-027 |
| S5 | a stored payload as long as a palette entry | `0x20` | a named directory | DAT-029 |
| S6 | a stored payload holding every byte value from `0x00` to `0xFF` | `0x20` | a named directory | DAT-015 |
| L1 | an LZSS stream of literals only | `0x10` | the `.` root directory | LZSS-005, LZSS-006 |
| L2 | an LZSS stream with a back reference whose distance is at least its length | `0x10` | a named directory | LZSS-008, LZSS-009 |
| L3 | an LZSS stream with a back reference whose distance is smaller than its length | `0x10` | a named directory | LZSS-011 |
| L4 | an LZSS stream that ends exactly on a control byte boundary | `0x10` | a named directory | LZSS-005, LZSS-013 |
| L5 | an LZSS stream that ends in the middle of a control byte | `0x10` | a named directory | LZSS-013 |
| L6 | an LZSS stream with a back reference that reads the prefilled `0x20` region | `0x10` | a named directory | LZSS-002, LZSS-003 |
| L7 | an LZSS stream with a back reference that reads the prefilled `0x00` region | `0x10` | a named directory | LZSS-002 |
| L8 | an LZSS stream with a back reference whose source index is the highest addressable ring slot | `0x10` | a named directory | LZSS-012 |
| L9 | an LZSS stream whose output is longer than one ring, so the write head wraps | `0x10` | a named directory | LZSS-010, LZSS-011 |
| L10 | an LZSS stream that repeats the longest run the token can encode | `0x10` | a named directory | LZSS-008 |
| C1 | a chain of one raw block | `0x40` | a named directory | DAT-018 |
| C2 | a chain of one LZSS block | `0x40` | a named directory | DAT-018, LZSS-016 |
| C3 | a chain of a raw block then an LZSS block | `0x40` | a named directory | DAT-020 |
| C4 | a chain of an LZSS block then a raw block | `0x40` | a named directory | DAT-020 |
| C5 | a chain of three blocks: raw, LZSS, raw | `0x40` | a named directory | DAT-020 |
| C6 | a chain whose last LZSS block ends in the middle of a control byte | `0x40` | a named directory | LZSS-013, LZSS-022 |
| C7 | a chain whose last LZSS block ends exactly on a control byte boundary | `0x40` | a named directory | LZSS-005, LZSS-013 |
| C8 | a chain with a raw block whose payload is one byte | `0x40` | a named directory | DAT-018 |
| C9 | a chain with a raw block whose payload is the format's block ceiling | `0x40` | a named directory | DAT-027 |
| C10 | a chain with an LZSS block whose payload is the format's block ceiling | `0x40` | a named directory | DAT-027 |
| C11 | a chain of eight blocks | `0x40` | a named directory | DAT-018 |
| C12 | a chain with an LZSS block that carries no match token | `0x40` | a named directory | LZSS-005, LZSS-006 |
| C13 | a chain of two LZSS blocks whose outputs differ if one ring is shared across the chain | `0x40` | a named directory | LZSS-016 |
| C14 | a chain whose entry name is 12 bytes long, the longest entry name the corpus holds (DAT-009), and names nothing in the install | `0x40` | a named directory | DAT-009 |
| C15 | a chain whose directory name itself holds a `\` (DAT-003) | `0x40` | a directory name with a `\` in it | DAT-003, DAT-011 |
| C16 | a chain in the `.` root directory, so the composed path carries no prefix | `0x40` | the `.` root directory | DAT-011 |

Six stored, ten LZSS streams, sixteen chains: `6 + 10 + 16 = 32` families, and
the 60 vectors cover all of them.

Every rule in the last column of the stored and the chain families is
`CONFIRMED AGAINST ORIGINAL BINARY`, and the corpus exercises it: 4,787 stored
entries and 20,456 chain entries (DAT-026), with the closure over the chain
entries measured (LZSS-022).

The ten LZSS families are the ones that need the truth told carefully, because
two different rules are in play. What each of them exercises is a **codec**
rule, and every codec rule in the last column is `CONFIRMED AGAINST ORIGINAL
BINARY` and is exercised by the corpus: the corpus holds LZSS blocks inside
its chain entries, and the closure over them is measured (LZSS-022). What the
corpus does **not** exercise is the **entry mode** those families carry their
stream in. A mode `0x10` entry is one LZSS stream whose input length is the
compressed size (DAT-014), that rule is `UNVERIFIED`, and 0 of the 25,243
entries select the mode (DAT-026).

A family is therefore worth a vector twice over, and the second reason is the
one the corpus cannot give: it grades the codec under the mode the guide
states, and it grades the mode rule that no corpus entry can reach. The rule
the corpus cannot check is exactly the one a hold-out set exists for.

The rules that govern each archive are the ones `docs/spec/dat.md` and
`docs/spec/lzss.md` state. This file does not restate their numbers.

### HOLD-030 — The DAT vectors: the rules every one of them obeys

`UNVERIFIED`

Whichever family a vector belongs to:

1. The archive is a **complete, self-consistent** synthetic DAT archive, built
   to the container rules of `docs/spec/dat.md`: a root header, the directory
   names, one block header per directory, and the entry records.
2. It holds **exactly one directory and exactly one entry**, so the path in the
   manifest names one thing and the lookup cannot be ambiguous.
3. The directory name is either the single character `.` or a synthetic upper
   case name. A directory name may hold a `\` (family C15), because one of the
   real archives has such a name (DAT-003). The name itself must name nothing
   in the install, so the family keeps the **shape** and not the name: the
   corpus's own directory name is used in no vector. A directory name is one
   length-prefixed name, so it holds at most 255 bytes (DAT-003).
4. The entry name is upper case, holds no `\`, and is no longer than the
   longest entry name the corpus holds: 12 bytes (DAT-009). The name itself is
   synthetic: it is a name of that length that names nothing in the install
   (family C14).
5. The composed path is the directory name and the entry name joined by one
   `\`, except that the `.` directory contributes no separator. The path in the
   manifest is that composed path.
6. The entry's payload mode is the high nibble of its flags, and the flags
   high nibble is the mode the family names.
7. A stored entry declares a compressed size of zero; a chain entry declares a
   compressed size that is not zero (DAT-017). A mode `0x10` entry declares a
   compressed size that is not zero either, because its input length *is* its
   compressed size (DAT-014). The compressed size therefore does not tell a
   reader the mode: the mode is the high nibble of the flags (DAT-013), and a
   reader must never read a nonzero compressed size as "chain".
8. The entry's offset is absolute in the archive and points at real bytes
   inside it. The stored region is bounded by the compressed size for a chain
   entry and by the uncompressed size for a stored entry.
9. A chain's block headers account for the compressed size exactly, and the
   decoded output is exactly the uncompressed size.
10. The archive holds one entry, its payload and the framing around them, and
    nothing else. The size of the file follows the family: most of the
    families produce a file of a few hundred bytes, and the four families that
    fix a payload of the corpus's own size — S4, C9 and C10 (DAT-027) and S5
    (DAT-029) — produce files of tens of kilobytes. The size is the payload's,
    and the payload is what the family fixes.

Rules 6 to 9 are exactly the invariants the container spec calls cheap and
exhaustive to assert. A vector that broke one of them would be a broken
fixture, and the owner would find it when the set was generated, because the
generator asserts them.

### HOLD-031 — The executable table vectors: 12 of them, covering 16 families

`UNVERIFIED`

The first milestone's set holds **12 executable table vectors**. Each names a
region of the reference executable and the bytes the reader must produce for
it.

The coverage rule is the one HOLD-029 uses: **every family below is covered by
at least one vector, and a vector may cover more than one family** when its
window spans more than one family's region. The 12 vectors cover all 16
families, because the crossing window holds the hit location penalty table
and the critter table's first record, the stat limits window holds the whole
XP thresholds table, and the player stat limits window holds its own whole
table. One vector per family is not the rule, and it could not be: it would
make 16 vectors, and the set holds 12.

| # | Family | The region |
| --- | --- | --- |
| X1 | the whole Hit location penalty table | the table's own extent |
| X2 | the first record of the Critical success, critters table | one record |
| X3 | the last record of the Critical success, critters table | one record |
| X4 | the first record of the Critical success, player table | one record |
| X5 | the last record of the Critical success, player table | one record |
| X6 | the whole Critical failure table | the table's own extent |
| X7 | the first record of the Stat limits table | one record |
| X8 | the last record of the Stat limits table | one record |
| X9 | the whole Player stat limits table | the table's own extent |
| X10 | the whole XP thresholds table | the table's own extent |
| X11 | a window that crosses the boundary between the Hit location penalty table and the Critical success, critters table | two tables in one window |
| X12 | a 4096-byte window starting at the first byte of the Critical success, critters table | a long window |
| X13 | a 4096-byte window ending exactly at the last byte of the Critical success, critters table | a long window |
| X14 | a 4096-byte window starting at the first byte of the Stat limits table | a long window |
| X15 | a 4096-byte window ending exactly at the last byte of the XP thresholds table | a long window |
| X16 | a 4096-byte window starting at the first byte of the Player stat limits table | a long window |

The table names are the ones `docs/spec/exe-tables.md` uses, word for word:
Hit location penalty (EXETBL-003), Critical success, critters (EXETBL-004),
Critical success, player (EXETBL-005), Critical failure (EXETBL-006), Stat
limits (EXETBL-007), Player stat limits (EXETBL-008) and XP thresholds
(EXETBL-009). The offsets and the extents come from that file's table of file
offsets and from the mapping rule in EXETBL-002, and are not repeated here.

The X11 window is bounded on both sides, so that it really is a crossing
window: it starts at or before the first byte of the Hit location penalty
table and ends at or after the last byte of the Critical success, critters
table's first record. The two tables are adjacent, so a window that crosses
the boundary and still holds the whole penalty table starts before that
table's first byte.

The table a window belongs to is carried by the **case name**, which is the
vector file's stem: the `exe` record has no field for the table's name, and a
reader must not expect one.

### HOLD-032 — The executable table vectors: the rules every one of them obeys

`UNVERIFIED`

1. Every window lies inside the raw extent of the section that owns the seven
   tables, so no window crosses a section boundary.
2. No window is longer than 4096 bytes and no window is empty.
3. A window that ends a table ends exactly on the table's last byte, and a
   window that starts a table starts exactly on the table's first byte.
4. The windows are the ones the family names. The producer does not choose a
   different window because it liked the bytes.
5. The expected bytes are read out of the reference executable by the separate
   extraction tool of HOLD-024, so they are that build's bytes and not another
   build's.

### HOLD-033 — The set is reproducible from the generator and the seed

`UNVERIFIED`

The manifest names the generator and the seed, and the same pair rebuilds the
same set byte for byte. The generator asserts the rules of HOLD-030 and
HOLD-032 on every vector it makes, so a vector that breaks one of them never
reaches the manifest.

This is what makes the set auditable. The owner can regenerate the set, diff
it against the manifest's digests, and see that nothing moved. It is also what
makes the set grow: a later milestone adds families and raises the counts in
the manifest, and the format does not change, so a version `1` reader keeps
working.

---

## 7. What this file does NOT settle

- **The payload bytes.** The families in HOLD-029 and HOLD-031 fix the *shape*
  of every vector; the bytes inside them are the generator's, and they come
  from the seed. Two people who run the same generator with the same seed get
  the same set; two people who do not have the generator do not get the same
  bytes from this file, and this file does not claim they would.
- **The generator itself.** Its program, its algorithm and its language are the
  owner's business. The manifest names it so a set can be told from another
  set made by a different tool.
- **The offsets and the extents of the seven executable tables.** They live in
  `docs/spec/exe-tables.md` and are not restated here.
- **The archive rules and the codec rules.** The vectors obey the container and
  codec specs; those specs state the rules. This file says which rules the
  vectors are chosen to exercise.
- **The oracle class of each hold-out case.** The DAT cases are class 4 (an
  independent reference implementation) and the executable table cases are
  class 1 (bytes of the original executable). `docs/spec/trace.md` is where a
  claim is mapped to a test and a class, and this file adds no row to it.
- **The number of cases the hold-out file runs.** The gate compares the counts
  a run produced with `expected_counts.toml`, and that file holds one floor per
  gate file. The hold-out file's own row is settled: the key is
  `install_holdout` and the value is `2`, the number of test functions the
  file holds (HOLD-025). The count of **cases** belongs to the manifest and
  rises whenever the owner adds a vector, so it is not a floor and it is not
  in that file.
- **Whether the owner wants the executable table vectors to grade the table
  accessors rather than raw byte windows.** A window grades the reader's
  mapping and its bounds; an accessor grade would also grade the record
  shapes. The repository's own install gate already grades the accessors
  against the reference build, and the hold-out vectors add the region the
  implementer has never seen. Widening the families is the owner's call.
- **Whether the hold-out set should carry vectors for the families the guide
  names but this repository has not reached yet** — more RNG seeds, more damage
  cases. This file defines the format for the two families the repository can
  read today. Those families would be more records of the same two kinds, and
  the counts in the manifest would say so, but the families themselves belong
  to the specs of those layers and not to this file.
- **Any behaviour of the original binary.** Nothing here is a measurement.

---

## 8. Open questions

1. Should the two parts be one test function, so that the implementer sees a
   single case count? The split exists so that a missing install cannot hide a
   DAT failure, and it costs one extra test function.
2. Should the DAT vector's hex file be allowed to hold the archive as raw
   bytes instead? Hex was chosen for auditability; raw bytes would halve the
   size and make the file unreadable by eye.
3. Should the executable table vectors also carry the table's **name**, so a
   failing case says which table it was rather than only which file? The file
   stem already carries a name, but a separate field would let a reader group
   failures without a naming convention.
4. Should the seed be required to be a specific length, or should any
   non-empty token be allowed so that a different generator's seed can be
   recorded? This file requires 64 hex characters, which is 32 bytes, which is
   what a normal generator draws.
5. Should a later milestone's set be a **new** manifest in the same folder, or
   a new version of the same manifest? The manifest is versioned by number and
   the folder holds exactly one manifest, so a new set means a new folder. The
   guide says the owner keeps vectors "for each milestone", which reads as one
   folder per milestone, but this file does not say so.
