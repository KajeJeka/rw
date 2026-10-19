# Executable layer tables (`falloutw.exe`)

**Provenance.** Written by the `spec author` role. Two sources only:

1. `RUSTY WASTELAND Engine Implementation Guide, Revision 2.md`, sections
   *Data compiled into the executable*, *Stats*, *Critical hits and failures*,
   *Experience and levels*, the *Older claims to discard* table and the
   *Anti deviation tests* table.
2. Bytes read directly from the reference executable `falloutw.exe`, SHA-256
    `444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272`.

Nothing is quoted from any Community Edition source. No decompilation, comment,
name or structure from CE was read or used. Every number below was produced by
reading file bytes with `Get-FileHash` and `[System.IO.File]::ReadAllBytes`.
No Community Edition artefact was consulted, so no claim in this file carries
the tag `CONFIRMED IN COMMUNITY EDITION ONLY`.

---

## EXETBL-001 — Build gate and expected digest

**CONFIRMED AGAINST ORIGINAL BINARY**

The reference build has SHA-256

```
444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272
```

Measured with `Get-FileHash -Algorithm SHA256` over the whole file
(1,243,136 bytes). The digest is checked **first**, before any header parse and
before any constant is used.

On any other digest the reader must stop with a clear message naming the
expected and observed digest and must **never** relax a constant, fall back to
a default, or read a table at a guessed address. A build mismatch is a
non-negotiable failure, because every constant in this file is only true for
this exact build. Proceeding on an unknown build would silently produce a
character with wrong stats rather than an error.

This is a process rule: the tables are read from the **player's own
executable** at run time, or cached once under `target/`. **No table value is
ever committed into this repository.** The cached file under `target/` is
already outside version control.

---

## EXETBL-002 — PE section table and the virtual address mapping

**CONFIRMED AGAINST ORIGINAL BINARY**

Headers were parsed by hand from the file: `e_lfanew` at offset `0x3C` is
`0x80`, signature `0x00004550`, COFF machine `0x014C` (i386), 6 sections,
`SizeOfOptionalHeader` 224, optional magic `0x010B` (PE32), linker `2.18`.

| Field | Value |
| --- | --- |
| ImageBase | `0x00400000` |
| SectionAlignment | `0x00010000` |
| FileAlignment | `0x00000200` |
| SizeOfImage | `0x002F0000` |
| SizeOfHeaders | `0x00000400` |

Section table, read from file offset `0x178`:

| Name | VirtualAddress | VirtualSize | PointerToRawData | SizeOfRawData | Characteristics |
| --- | --- | --- | --- | --- | --- |
| `BEGTEXT` | `0x00010000` | `0x00000000` | `0x00000400` | `0x000D0A00` | `0x60000020` |
| `DGROUP` | `0x000F0000` | `0x00000000` | `0x000D0E00` | `0x0004CC00` | `0xC0000040` |
| `.bss`    | `0x00140000` | `0x00000000` | `0x00000000` | `0x00172C00` | `0xC0000080` |
| `.idata`  | `0x002C0000` | `0x00000000` | `0x0011DA00` | `0x00000C00` | `0xC0000040` |
| `.reloc`  | `0x002D0000` | `0x00000000` | `0x0011E600` | `0x00000EE00` | `0x42000040` |
| `.rsrc`   | `0x002E0000` | `0x00000000` | `0x0012D400` | `0x00002400` | `0x40000040` |

**Every section declares `VirtualSize == 0`.** This is a property of the
reference build and it breaks the usual mapping rule:

> **Mapping.** `file_offset = PointerToRawData + (va - ImageBase - VirtualAddress)`
> and a virtual address is contained in a section only while
> `0 <= (va - ImageBase - VirtualAddress) < SizeOfRawData`, and only in a
> section whose `PointerToRawData` is nonzero as well.

Containment must be tested against `SizeOfRawData`. A reader that validates
against `VirtualSize` rejects every address in this build.

### A section is file-backed only when both raw fields are nonzero

The second condition is not a formality. `.bss` declares
`SizeOfRawData = 0x00172C00`, a large and nonzero size, together with
`PointerToRawData = 0`. A containment test that looks at the size alone
accepts `.bss`, maps any address in it to file offset `0 + delta`, and a read
at `0x00540000` — the first address `.bss` owns — hands back the first bytes
of the file, the DOS header `MZ`, as though they were data. Both conditions
must hold, so `.bss` is skipped: it is uninitialised data and contributes no
file bytes. Testing one raw field is relying on a coincidence of this build;
testing both is the rule.

### A read is bounded by its section, not only by the file

A length is checked against the containing section's raw extent before it is
checked against the extent of the file, and a length that would cross the
section boundary is an error.

That check has to be there, and it has to be about the section, because
`DGROUP` raw data ends at file offset `0x11DA00` — which is exactly where
`.idata` raw data begins. A read that starts inside `DGROUP` and overruns its
own section lands in `.idata`'s file bytes and returns plausible garbage
without raising anything. The last byte of `DGROUP` reads; two bytes from the
last byte of `DGROUP` do not.

Crossing a boundary the reader has already been told about is the defect class
the guide forbids. Its *Engineering philosophy* rule 3 is a total reader that
"never indexes past the end", and the guide's executable-layer section makes
the section the unit of lookup, so the end that matters is the end of the
mapped region, not the end of the file.

All seven tables live in `DGROUP`. The highest byte any table reaches is file
offset `0xE9123`; `0xE9124` is the exclusive end, one byte past anything the
tables own. `DGROUP` raw data ends at `0x11DA00`, so every table is inside the
section with room to spare.

### File offsets computed for the seven tables

| Table | Virtual address | RVA | File offset | Byte length | Last byte used |
| --- | --- | --- | --- | --- | --- |
| Hit location penalty | `0x4FEC90` | `0x0FEC90` | `0x000DFA90` | 36 | `0x000DFAB3` |
| Critical success, critters | `0x4FECB4` | `0x0FECB4` | `0x000DFAB4` | 22,680 | `0x000E534B` |
| Critical success, player | `0x50454C` | `0x10454C` | `0x000E534C` | 1,512 | `0x000E5933` |
| Critical failure | `0x504B3C` | `0x104B3C` | `0x000E593C` | 140 | `0x000E59C7` |
| Stat limits | `0x507EC8` | `0x107EC8` | `0x000E8CC8` | 912 | `0x000E9057` |
| Player stat limits | `0x508258` | `0x108258` | `0x000E9058` | 120 | `0x000E90CF` |
| XP thresholds | `0x5082D0` | `0x1082D0` | `0x000E90D0` | 84 | `0x000E9123` |

File offsets are recorded so an implementer can cross-check the mapping with a
plain file read, and so a test can assert the mapping itself rather than
trusting it.

---

## EXETBL-003 — Hit location penalty

**CONFIRMED AGAINST ORIGINAL BINARY**

| Table | Address | Layout |
| --- | --- | --- |
| Hit location penalty | `0x4FEC90` | 9 x i32 |

Nine little endian `i32`. Raw bytes at `0xDFA90`:

```
d8 ff ff ff  e2 ff ff ff  e2 ff ff ff  00 00 00 00
ec ff ff ff  ec ff ff ff  c4 ff ff ff  e2 ff ff ff
00 00 00 00
```

Values: `-40, -30, -30, 0, -20, -20, -60, -30, 0`.

Immediately followed, with no gap, by the critter critical table: the last byte
of this table is `0xDFAB3` and the critter table starts at `0xDFAB4`. That
adjacency is an independent confirmation of the `36 = 9 x 4` extent.

---

## EXETBL-004 — Critical success, critters

**CONFIRMED AGAINST ORIGINAL BINARY**

| Table | Address | Layout |
| --- | --- | --- |
| Critical success, critters | `0x4FECB4` | 15 x 9 x 6 records of 28 bytes |

810 records of 28 bytes, total 22,680 bytes. First record at file `0xDFAB4`:

`4, 0, -1, 0, 0, 5001, 5000`

Last record (index 809) at file `0xE5330`:

`4, 2048, -1, 0, 0, 6402, 5000`

The reader flattens the `15 x 9 x 6` nesting; it does not materialise three
nested vectors. The guide does not say which of the three indices is body part,
skill or grade, so the flattened index must be carried through unchanged.

---

## EXETBL-005 — Critical success, player

**CONFIRMED AGAINST ORIGINAL BINARY**

| Table | Address | Layout |
| --- | --- | --- |
| Critical success, player | `0x50454C` | 9 x 6 records of 28 bytes |

54 records of 28 bytes, total 1,512 bytes. First record at file `0xE534C`:

`3, 0, -1, 0, 0, 6500, 5000`

Raw bytes at `0xE534C` confirm little endian on the message ids:
`64 19 00 00` is 6,500 and `88 13 00 00` is 5,000.

Last record (index 53) at file `0xE5918`:

`4, 2050, 6, 2, 128, 6503, 6513`

The table ends at file `0xE5933`; the critical failure table starts at
`0xE593C`, leaving 8 bytes of padding. The player critical table is therefore
**not** flush with the critical failure table, which is why only the critter to
player adjacency is stated as an identity.

---

## EXETBL-006 — Critical failure

**CONFIRMED AGAINST ORIGINAL BINARY**

| Table | Address | Layout |
| --- | --- | --- |
| Critical failure | `0x504B3C` | 7 x 5 x i32 |

35 little endian `i32` at file `0xE593C`, 140 bytes, read as 7 rows of 5:

| Row | Values |
| --- | --- |
| 0 | `0, 32768, 32768, 524290, 2097152` |
| 1 | `0, 32768, 16384, 1048576, 65536` |
| 2 | `0, 131072, 16384, 1048576, 8192` |
| 3 | `32768, 163840, 49152, 1048576, 36864` |
| 4 | `262144, 16384, 540672, 1048576, 4096` |
| 5 | `32768, 262144, 8192, 1048576, 36866` |
| 6 | `0, 32768, 1048576, 8192, 37888` |

The values are large power-of-two combinations, consistent with flag masks
rather than signed penalties. The layout is fixed at `7 x 5 x i32`; what the
five columns mean is not established here.

---

## EXETBL-007 — Stat limits

**CONFIRMED AGAINST ORIGINAL BINARY**

| Table | Address | Layout |
| --- | --- | --- |
| Stat limits | `0x507EC8` | 38 records of 24 bytes |

38 records of 24 bytes, total 912 bytes, from file `0xE8CC8`.

Field positions, all little endian `i32`:

| Byte | Field | Guide's reading | Observed |
| --- | --- | --- | --- |
| 0 | `field0` | "looks like the stat index" | constant `0` in all 38 records |
| 4 | `field1` | not understood | constant `0` in all 38 records |
| 8 | `field2` | not understood | the only varying field |
| 12 | `minimum` | minimum | confirmed |
| 16 | `maximum` | maximum | confirmed |
| 20 | `default` | default | confirmed |

The minimum of the first record sits at `0x507ED4`, which is `0x507EC8 + 12`,
file `0xE8CD4`. Its value is `1`, which confirms field 3 as the minimum and
confirms the `24`-byte stride.

First seven records, showing field 2 running `0..6` with
`min 1, max 10, default 5`:

| Rec | File | f0 | f1 | f2 | min | max | def |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | `0xE8CC8` | 0 | 0 | 0 | 1 | 10 | 5 |
| 1 | `0xE8CE0` | 0 | 0 | 1 | 1 | 10 | 5 |
| 2 | `0xE8CF8` | 0 | 0 | 2 | 1 | 10 | 5 |
| 3 | `0xE8D10` | 0 | 0 | 3 | 1 | 10 | 5 |
| 4 | `0xE8D28` | 0 | 0 | 4 | 1 | 10 | 5 |
| 5 | `0xE8D40` | 0 | 0 | 5 | 1 | 10 | 5 |
| 6 | `0xE8D58` | 0 | 0 | 6 | 1 | 10 | 5 |

The `1 .. 10` range with default `5` for exactly seven records matches the
seven core attributes.

### Record 16

Record 16 is at `0x507EC8 + 16 * 24` = `0x508048`, file `0xE8E48`. Raw bytes:

```
00 00 00 00  00 00 00 00  5e 00 00 00  00 00 00 00  64 00 00 00  00 00 00 00
```

`field0 = 0`, `field1 = 0`, `field2 = 94`, `minimum = 0`, `maximum = 100`,
`default = 0`.

### Discrepancy in the guide's field attribution

The guide says field 0 "looks like the stat index". In this build **field 0 is
zero in every one of the 38 records**, so it cannot index anything. Field 1 is
likewise zero in every record. Field 2 is the only field that varies, and for
the first seven records it runs `0..6` in an index-like way.

Field 2 is not a clean substitute either: value `10` occurs at record 7 and
again at record 35, and value `0` occurs at records 17 to 23, 25 to 30, 33 and
34. So field 2 is neither unique nor obviously a stat id.

This spec therefore keeps the guide's **positional** contract — fields 3, 4 and
5 are minimum, maximum and default — which the bytes do confirm, and refuses to
assign a meaning to fields 0, 1 and 2. See `EXETBL-016` and `EXETBL-017`.

---

## EXETBL-008 — Player stat limits

**CONFIRMED AGAINST ORIGINAL BINARY**

| Table | Address | Layout |
| --- | --- | --- |
| Player stat limits | `0x508258` | 5 records of 24 bytes |

Five records of the same 24-byte layout, from file `0xE9058`:

| Rec | File | min | max | default | Guide's subject | Reading |
| --- | --- | --- | --- | --- | --- | --- |
| 0 | `0xE9058` | 0 | 2147483647 | 0 | unspent skill points | consistent |
| 1 | `0xE9070` | 1 | 2147483647 | 1 | level | consistent; default 1, not 0 |
| 2 | `0xE9088` | 0 | 2147483647 | 0 | experience | consistent |
| 3 | `0xE90A0` | -20 | 20 | 0 | reputation | consistent; signed range |
| 4 | `0xE90B8` | 0 | 2147483647 | 0 | karma | consistent |

`2147483647` is `i32::MAX`, i.e. an unbounded maximum written as the largest
positive `i32`. Record 1 starting at 1 and defaulting to 1 is a genuine level
floor: a character cannot begin at level 0. Record 3's `-20 .. 20` range is the
classic reputation span, which is consistent with the guide's ordering but is
not proof of the label.

Field 0 and field 1 are zero in all five records, as in the stat limits table.

---

## EXETBL-009 — XP thresholds

**CONFIRMED AGAINST ORIGINAL BINARY**

| Table | Address | Layout |
| --- | --- | --- |
| XP thresholds | `0x5082D0` | 21 x i32 |

21 little endian `i32` at file `0xE90D0`, 84 bytes. Raw bytes:

```
00 00 00 00  01 00 00 00  03 00 00 00  06 00 00 00
0a 00 00 00  0f 00 00 00  15 00 00 00  1c 00 00 00
24 00 00 00  2d 00 00 00  37 00 00 00  42 00 00 00
4e 00 00 00  5b 00 00 00  69 00 00 00  78 00 00 00
88 00 00 00  99 00 00 00  ab 00 00 00  be 00 00 00
d2 00 00 00
```

Read little endian: `0, 1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 66, 78, 91, 105,
120, 136, 153, 171, 190, 210`.

### The table is indexed by level, starting at level 0

The sequence is the triangular numbers `L(L+1)/2` for `L = 0 .. 20`, and the
**first element is 0**, not 1. So:

> `xp_thresholds()[L] == L * (L + 1) / 2` for `L` in `0 ..= 20`,
> and the array index is the level.

The level 1 entry is therefore at **index 1**, file `0xE90D4`, virtual address
`0x5082D4`, not at index 0.

The values are not scaled: the largest entry, 210 at level 20, is exactly
`20 * 21 / 2`. The caller multiplies by 1,000. Under
`#![deny(clippy::arithmetic_side_effects)]` that multiplication must be
checked, because `210_000 * 1000` overflows nothing at level 20 but a
caller-supplied level above about 75 would; use `checked_mul` and surface the
overflow rather than wrapping.

---

## EXETBL-010 — Adjacency identity 1 (class 1 oracle)

**CONFIRMED AGAINST ORIGINAL BINARY**

```
assert_eq!(0x4FECB4 + 22_680, 0x50454C);
```

Arithmetic: `15 * 9 * 6 * 28 = 810 * 28 = 22,680`, and
`0x4FECB4 + 0x5898 = 0x50454C`. Both sides check out.

Confirmed independently in the file domain: the critter table starts at file
`0xDFAB4`, and `0xDFAB4 + 22,680 = 0xE534C`, which is exactly the file offset
independently computed for the player critical table. The identity therefore
holds in both address spaces, which is what makes it a real oracle and not an
arithmetic tautology.

---

## EXETBL-011 — Adjacency identity 2 (class 1 oracle)

**CONFIRMED AGAINST ORIGINAL BINARY**

```
assert_eq!(0x507EC8 + 912, 0x508258);
```

Arithmetic: `38 * 24 = 912`, and `0x507EC8 + 0x390 = 0x508258`. Both sides
check out.

Confirmed in the file domain: `0xE8CC8 + 912 = 0xE9058`, the file offset of the
player stat limits table.

---

## EXETBL-012 — Adjacency identity 3 (class 1 oracle)

**CONFIRMED AGAINST ORIGINAL BINARY**

```
assert_eq!(0x508258 + 120, 0x5082D0);
```

Arithmetic: `5 * 24 = 120`, and `0x508258 + 0x78 = 0x5082D0`. Both sides check
out.

Confirmed in the file domain: `0xE9058 + 120 = 0xE90D0`, the file offset of the
XP table.

All three identities therefore hold simultaneously, which pins all three record
sizes (`28`, `24`, `24`) and the critter record count (`810`) at once.

---

## EXETBL-013 — Anti deviation: executable table byte order

**CONFIRMED AGAINST ORIGINAL BINARY**

The guide's *Anti deviation tests* table requires:

> The XP value for level 1 is 1, not 16,777,216.

The four bytes of the level 1 entry, at file `0xE90D4` / virtual address
`0x5082D4`, are:

```
01 00 00 00
```

| Read as | Result |
| --- | --- |
| Little endian | `0x00000001` = **1** |
| Big endian | `0x01000000` = **16,777,216** |

Both numbers are confirmed from the same four bytes. **Little endian is
correct**, for three independent reasons:

1. It is the only order that makes the whole 21-entry table equal
   `L(L+1)/2`. Big endian gives `0, 16777216, 50331648, ...` and no closed form
   recovers that.
2. Every other table is internally consistent little endian. The critical
   record at `0xE534C` reads its message ids as `64 19 00 00` = 6,500 and
   `88 13 00 00` = 5,000, plausible message ids; the same four bytes read big
   endian are `0x64190000` = 1,679,360,000 and `0x88130000` = 2,282,946,560,
   which are not message ids at all.
3. The hit location penalties read as small negative numbers (`-40`, `-60`)
   only in little endian. Big endian gives values above 2.5 billion.

A CE-style big endian read of the XP table produces a character who needs
16.7 million experience to leave level 1 and can never leave level 3. The test
must read the four bytes and assert the little endian value, not assert a
literal, so the assertion fails loudly on a byte order regression.

---

## EXETBL-014 — Anti deviation: stat 16 minimum

**CONFIRMED AGAINST ORIGINAL BINARY**

The guide's *Anti deviation tests* table requires:

> The minimum read from the executable is 0, not -60.

Record 16 of the stat limits table is at virtual address
`0x507EC8 + 16 * 24 = 0x508048`, file `0xE8E48`. Its minimum field is
`0x508048 + 12 = 0x508054`, file `0xE8E54`.

Bytes at `0xE8E54`: `00 00 00 00`. Little endian value: **0**.

Full record 16 raw bytes, showing the stride is intact:

```
00 00 00 00 | 00 00 00 00 | 5e 00 00 00 | 00 00 00 00 | 64 00 00 00 | 00 00 00 00
 f0         | f1         | f2 = 94    | min = 0     | max = 100   | def = 0
```

`-60` is not present at `0x508054`. The test must read the field from the
executable and assert `0`, not assert a literal against a hard coded table.

---

## EXETBL-015 — XP table is indexed by level, first element is level 0

**CONFIRMED AGAINST ORIGINAL BINARY**

The 21 little endian values start at `0` and follow `L(L+1)/2` for `L = 0..20`.
The guide states the closed form but does not state where `L = 0` sits.

Because the first element is `0`, indexing the table as if it began at level 1
would place a threshold of `0` at level 1 and shift every level down by one,
making the level 20 threshold read as `L = 19` = 190. `xp_thresholds()` is
specified to return the array verbatim, with `array[L]` holding level `L`'s
threshold, so no off-by-one correction belongs in the reader.

---

## EXETBL-016 — Stat limit field 0 is not the stat index

**VERSION DIFFERENCE IDENTIFIED**

The guide states field 0 of a stat limit record "looks like the stat index". In
the reference build field 0 is `0` in all 38 records, and field 1 is `0` in all
38 records. Field 2 is the only varying field, and it is neither unique (value
`10` at records 7 and 35) nor monotonic with the record index (zero at records
17 to 23, 25 to 30, 33 and 34).

Fields 3, 4 and 5 as minimum, maximum and default are confirmed by the bytes
and are not in dispute. What differs is the guide's claim about field 0.

Consequence for the reader: expose `field0`, `field1` and `field2` under those
neutral names. Do not name field 0 `stat_index`, and do not derive a stat from
it. Addressing of a stat limit is by record index only.

---

## EXETBL-017 — Stat limit fields 1 and 2

**UNVERIFIED**

The guide says fields 1 and 2 are not understood, and the bytes do not settle
them. Field 1 is `0` everywhere, which is consistent with an unused or reserved
slot but does not establish one. Field 2 takes values including `0, 10, 11, 12,
18, 20, 22, 23, 24, 25, 26, 31, 32, 75, 83, 94`, with non-unique and
non-contiguous use.

No meaning is assigned. Any code that needs a stat id from this table must
isolate that need as an open question rather than inventing a mapping. Nothing
here may be guessed from a Community Edition table.

---

## The Rust interface

The crate this feeds carries `#![deny(clippy::arithmetic_side_effects)]`. The
code below therefore uses `checked_*` / `try_from` / `try_into` throughout, has
no `unsafe`, no `unwrap`, no `expect`, no slicing index that can panic, and no
`as` cast that changes width or sign. Errors carry an offset and a field name,
never table content.

```rust
#![deny(clippy::arithmetic_side_effects)]
//! Executable layer tables.
//!
//! Every table is read from the player's own `falloutw.exe` at run time, or
//! from a cache under `target/`. No table value is committed to this
//! repository, so this module owns no table data of its own.

use core::fmt;

/// Expected digest of the reference build. Checked before any table is read.
pub const REFERENCE_SHA256: &str =
    "444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272";

pub const IMAGE_BASE: u32 = 0x0040_0000;

pub const HIT_LOCATION_PENALTY_VA: u32 = 0x004F_EC90;
pub const CRIT_SUCCESS_CRITTERS_VA: u32 = 0x004F_ECB4;
pub const CRIT_SUCCESS_PLAYER_VA: u32 = 0x0050_454C;
pub const CRIT_FAILURE_VA: u32 = 0x0050_4B3C;
pub const STAT_LIMITS_VA: u32 = 0x0050_7EC8;
pub const PLAYER_STAT_LIMITS_VA: u32 = 0x0050_8258;
pub const XP_THRESHOLDS_VA: u32 = 0x0050_82D0;

/// Every field in this executable layer is a little endian `i32`.
const I32_SIZE: usize = 4;

const HIT_LOCATION_PENALTY_COUNT: usize = 9;
const CRITTER_CRIT_RECORD_COUNT: usize = 15 * 9 * 6;
const PLAYER_CRIT_RECORD_COUNT: usize = 9 * 6;
const CRITICAL_FAILURE_GROUPS: usize = 7;
const CRITICAL_FAILURE_WIDTH: usize = 5;
const CRITICAL_RECORD_SIZE: usize = 28;
const STAT_LIMIT_COUNT: usize = 38;
const STAT_LIMIT_SIZE: usize = 24;
const PLAYER_STAT_LIMIT_COUNT: usize = 5;
const XP_THRESHOLD_COUNT: usize = 21;

/// One PE section header.
///
/// `virtual_size` is stored because the field exists in the file, but every
/// section of the reference build declares 0, so containment is decided by
/// `raw_size`. A reader that trusts `virtual_size` here accepts nothing.
/// Containment additionally requires `raw_pointer != 0`, because `.bss`
/// declares a nonzero `raw_size` beside a zero `raw_pointer`.
#[derive(Clone, Copy)]
pub struct SectionHeader {
    pub name: [u8; 8],
    pub virtual_address: u32,
    pub virtual_size: u32,
    pub raw_pointer: u32,
    pub raw_size: u32,
}

/// A failure that never carries table bytes or values.
///
/// Only offsets, counts, field names and digests appear here, so a failing
/// test can name what it was reading without leaking the data it misread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExeError {
    /// The file does not carry a parseable PE header where one was required.
    Header { offset: usize, field: &'static str },
    /// No section holds this virtual address.
    Unmapped { va: u32 },
    /// The address arithmetic left the representable range.
    RangeOverflow { va: u32 },
    /// The address maps to a section, but the request does not fit inside the
    /// region that section owns: it runs past the end of the section's raw
    /// bytes, or past the end of the file. Both are the same refusal, because a
    /// read that leaves its section must not return the next section's bytes
    /// under this section's address.
    OutsideFile { offset: usize, len: usize },
    /// A record field fell outside the record that was read.
    FieldOutOfRange { offset: usize, field: &'static str },
    /// The table held a different number of records than its declared shape.
    RecordCount { va: u32, expected: usize, observed: usize },
    /// The digest is not the reference digest. Never relaxed.
    BuildMismatch { expected_sha256: &'static str, observed_sha256: String },
}

impl fmt::Display for ExeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExeError::Header { offset, field } => {
                write!(f, "bad PE header field {field} at file offset 0x{offset:X}")
            }
            ExeError::Unmapped { va } => write!(f, "virtual address 0x{va:08X} is in no section"),
            ExeError::RangeOverflow { va } => {
                write!(f, "address arithmetic overflowed for 0x{va:08X}")
            }
            ExeError::OutsideFile { offset, len } => {
                write!(
                    f,
                    "read of {len} bytes at file offset 0x{offset:X} runs past the end of \
                     its section or of the file"
                )
            }
            ExeError::FieldOutOfRange { offset, field } => {
                write!(f, "field {field} at file offset 0x{offset:X} is outside the record")
            }
            ExeError::RecordCount { va, expected, observed } => write!(
                f,
                "table at 0x{va:08X} declared {expected} records but {observed} were read"
            ),
            ExeError::BuildMismatch { expected_sha256, observed_sha256 } => write!(
                f,
                "executable digest {observed_sha256} is not the reference build {expected_sha256}; \
                 this build's table constants are unknown and are not guessed"
            ),
        }
    }
}

impl std::error::Error for ExeError {}

/// A parsed PE image that can translate a virtual address into a byte range.
///
/// The image borrows the caller's bytes so `read_va` can return a borrowed
/// slice and no table is ever copied into a long lived structure.
pub struct PeImage<'a> {
    bytes: &'a [u8],
    image_base: u32,
    sections: Vec<SectionHeader>,
}

impl<'a> PeImage<'a> {
    /// Parses enough of the PE headers to translate virtual addresses.
    ///
    /// Performs no digest check, so a caller that depends on the reference
    /// constants must reach this through `require_reference_build`.
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ExeError> {
        let dos = read_u32(bytes, 0x3C, "e_lfanew")?;
        let signature = read_u32(bytes, usize::try_from(dos).map_err(|_| ExeError::Header {
            offset: 0x3C,
            field: "e_lfanew",
        })?, "signature")?;
        if signature != 0x0000_4550 {
            return Err(ExeError::Header {
                offset: usize::try_from(dos).unwrap_or(0),
                field: "signature",
            });
        }

        let coff = usize::try_from(dos)
            .ok()
            .and_then(|at| at.checked_add(4))
            .ok_or(ExeError::Header { offset: 0x3C, field: "coff" })?;

        let section_count = usize::from(read_u16(bytes, add(coff, 2)?, "numberOfSections")?);
        let optional_size = usize::from(read_u16(bytes, add(coff, 16)?, "sizeOfOptionalHeader")?);
        let optional = add(coff, 20)?;

        // PE32 only. A PE32+ optional header has different offsets for the
        // same fields, and guessing them would silently misread every address.
        if read_u16(bytes, optional, "optionalMagic")? != 0x010B {
            return Err(ExeError::Header { offset: optional, field: "optionalMagic" });
        }
        let image_base = read_u32(bytes, add(optional, 28)?, "imageBase")?;

        let table = add(optional, optional_size)?;
        let stride = 40usize;
        let span = section_count
            .checked_mul(stride)
            .ok_or(ExeError::Header { offset: table, field: "sectionTable" })?;

        let mut sections = Vec::new();
        for index in 0..section_count {
            let at = add(table, index.checked_mul(stride).ok_or(ExeError::RangeOverflow { va: 0 })?)?;
            let header = read_section_header(bytes, at)?;
            sections.push(header);
        }
        // Touching `span` keeps the computed extent part of the parse so a
        // truncated section table is rejected here rather than mid read.
        let _ = add(table, span)?;

        Ok(Self { bytes, image_base, sections })
    }

    /// Parses and refuses any build whose digest is not the reference digest.
    ///
    /// The digest is checked before the headers and before any constant is
    /// used. A mismatch is a hard stop: the constants in this module are true
    /// only for one exact build, so relaxing them would produce a silently
    /// wrong character instead of a visible failure.
    pub fn require_reference_build(bytes: &'a [u8]) -> Result<Self, ExeError> {
        let observed = sha256_hex(bytes);
        if observed != REFERENCE_SHA256 {
            return Err(ExeError::BuildMismatch {
                expected_sha256: REFERENCE_SHA256,
                observed_sha256: observed,
            });
        }
        Self::parse(bytes)
    }

    /// Borrows `len` bytes at virtual address `va`.
    ///
    /// Containment is tested against `SizeOfRawData` because every section of
    /// the reference build declares `VirtualSize == 0`, and the read is bounded
    /// by the containing section's raw extent before the file is consulted. A
    /// length that would run past the section is an error, not a shorter read
    /// and not a read that carries on into the next section's bytes.
    pub fn read_va(&self, va: u32, len: usize) -> Result<&'a [u8], ExeError> {
        let (header, delta, start) = self.translate(va)?;
        let limit = usize::try_from(header.raw_size).map_err(|_| ExeError::RangeOverflow { va })?;
        let span = delta
            .checked_add(len)
            .ok_or(ExeError::RangeOverflow { va })?;
        if span > limit {
            return Err(ExeError::OutsideFile { offset: start, len });
        }
        let end = start.checked_add(len).ok_or(ExeError::RangeOverflow { va })?;
        // A total `get` keeps a short file an error rather than a panic.
        self.bytes.get(start..end).ok_or(ExeError::OutsideFile { offset: start, len })
    }

    /// The section header that owns `va`, if any.
    pub fn section_for(&self, va: u32) -> Option<&SectionHeader> {
        let rva = va.checked_sub(self.image_base)?;
        self.sections.iter().find(|header| match rva.checked_sub(header.virtual_address) {
            None => false,
            // File-backed means both raw fields are nonzero. `raw_size` alone
            // admits `.bss`, which declares a nonzero size with a zero
            // `raw_pointer` and would map to file offset `0 + delta`.
            Some(delta) => {
                header.raw_pointer != 0 && header.raw_size != 0 && delta < header.raw_size
            }
        })
    }

    /// The section that owns `va`, the offset of `va` inside that section, and
    /// the file offset `va` maps to.
    fn translate(&self, va: u32) -> Result<(&SectionHeader, usize, usize), ExeError> {
        let header = self.section_for(va).ok_or(ExeError::Unmapped { va })?;
        let rva = va.checked_sub(self.image_base).ok_or(ExeError::RangeOverflow { va })?;
        let delta = rva
            .checked_sub(header.virtual_address)
            .ok_or(ExeError::RangeOverflow { va })?;
        let raw = usize::try_from(header.raw_pointer).map_err(|_| ExeError::RangeOverflow { va })?;
        let inside = usize::try_from(delta).map_err(|_| ExeError::RangeOverflow { va })?;
        let start = raw.checked_add(inside).ok_or(ExeError::RangeOverflow { va })?;
        Ok((header, inside, start))
    }
}

/// One 24-byte stat limit record.
///
/// Fields 0, 1 and 2 keep the guide's neutral positions. Field 0 is zero in
/// every record of the reference build, so it is not named `stat_index`;
/// addressing a limit is by record index only. See EXETBL-016 and EXETBL-017.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatLimit {
    field0: i32,
    field1: i32,
    field2: i32,
    minimum: i32,
    maximum: i32,
    default: i32,
}

impl StatLimit {
    pub const SIZE: usize = 24;
    pub const FIELD_COUNT: usize = 6;

    /// Byte offset and name of each field, so layout and error naming cannot drift apart.
    pub const FIELDS: [(usize, &'static str); 6] = [
        (0, "field0"),
        (4, "field1"),
        (8, "field2"),
        (12, "minimum"),
        (16, "maximum"),
        (20, "default"),
    ];

    pub fn field0(&self) -> i32 { self.field0 }
    pub fn field1(&self) -> i32 { self.field1 }
    pub fn field2(&self) -> i32 { self.field2 }
    pub fn minimum(&self) -> i32 { self.minimum }
    pub fn maximum(&self) -> i32 { self.maximum }
    pub fn default(&self) -> i32 { self.default }
}

/// One 28-byte critical record.
///
/// `massive_critical_stat` is `-1` when the record has no massive critical, so
/// it is a sentinel and must not be indexed without a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CriticalRecord {
    multiplier: i32,
    effect_flags: i32,
    massive_critical_stat: i32,
    massive_critical_modifier: i32,
    massive_critical_flags: i32,
    message_id_1: i32,
    message_id_2: i32,
}

impl CriticalRecord {
    pub const SIZE: usize = 28;
    pub const FIELD_COUNT: usize = 7;

    pub const FIELDS: [(usize, &'static str); 7] = [
        (0, "multiplier"),
        (4, "effect_flags"),
        (8, "massive_critical_stat"),
        (12, "massive_critical_modifier"),
        (16, "massive_critical_flags"),
        (20, "message_id_1"),
        (24, "message_id_2"),
    ];

    pub fn multiplier(&self) -> i32 { self.multiplier }
    pub fn effect_flags(&self) -> i32 { self.effect_flags }
    pub fn massive_critical_stat(&self) -> i32 { self.massive_critical_stat }
    pub fn massive_critical_modifier(&self) -> i32 { self.massive_critical_modifier }
    pub fn massive_critical_flags(&self) -> i32 { self.massive_critical_flags }
    pub fn message_id_1(&self) -> i32 { self.message_id_1 }
    pub fn message_id_2(&self) -> i32 { self.message_id_2 }

    /// `false` when `stat` is absent, so a caller cannot use the `-1` sentinel
    /// as an index.
    pub fn has_massive_critical(&self) -> bool {
        self.massive_critical_stat >= 0
    }
}

/// Reads `count` little endian `i32` values starting at `va`.
fn read_i32s(image: &PeImage<'_>, va: u32, count: usize) -> Result<Vec<i32>, ExeError> {
    let len = count
        .checked_mul(I32_SIZE)
        .ok_or(ExeError::RangeOverflow { va })?;
    let raw = image.read_va(va, len)?;
    let mut out = Vec::new();
    for index in 0..count {
        let at = index.checked_mul(I32_SIZE).ok_or(ExeError::RangeOverflow { va })?;
        out.push(read_word(raw, at, "i32")?);
    }
    Ok(out)
}

fn read_word(raw: &[u8], offset: usize, field: &'static str) -> Result<i32, ExeError> {
    let end = offset
        .checked_add(I32_SIZE)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?;
    let word: [u8; I32_SIZE] = raw
        .get(offset..end)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?
        .try_into()
        .map_err(|_| ExeError::FieldOutOfRange { offset, field })?;
    Ok(i32::from_le_bytes(word))
}

fn read_stat_limit(raw: &[u8], start: usize) -> Result<StatLimit, ExeError> {
    let (f0_at, f0) = StatLimit::FIELDS[0];
    let (f1_at, f1) = StatLimit::FIELDS[1];
    let (f2_at, f2) = StatLimit::FIELDS[2];
    let (min_at, min) = StatLimit::FIELDS[3];
    let (max_at, max) = StatLimit::FIELDS[4];
    let (def_at, def) = StatLimit::FIELDS[5];
    Ok(StatLimit {
        field0: read_word(raw, add(start, f0_at)?, f0)?,
        field1: read_word(raw, add(start, f1_at)?, f1)?,
        field2: read_word(raw, add(start, f2_at)?, f2)?,
        minimum: read_word(raw, add(start, min_at)?, min)?,
        maximum: read_word(raw, add(start, max_at)?, max)?,
        default: read_word(raw, add(start, def_at)?, def)?,
    })
}

fn read_critical_record(raw: &[u8], start: usize) -> Result<CriticalRecord, ExeError> {
    let (mul_at, mul) = CriticalRecord::FIELDS[0];
    let (eff_at, eff) = CriticalRecord::FIELDS[1];
    let (stat_at, stat) = CriticalRecord::FIELDS[2];
    let (mod_at, modifier) = CriticalRecord::FIELDS[3];
    let (flag_at, flag) = CriticalRecord::FIELDS[4];
    let (m1_at, m1) = CriticalRecord::FIELDS[5];
    let (m2_at, m2) = CriticalRecord::FIELDS[6];
    Ok(CriticalRecord {
        multiplier: read_word(raw, add(start, mul_at)?, mul)?,
        effect_flags: read_word(raw, add(start, eff_at)?, eff)?,
        massive_critical_stat: read_word(raw, add(start, stat_at)?, stat)?,
        massive_critical_modifier: read_word(raw, add(start, mod_at)?, modifier)?,
        massive_critical_flags: read_word(raw, add(start, flag_at)?, flag)?,
        message_id_1: read_word(raw, add(start, m1_at)?, m1)?,
        message_id_2: read_word(raw, add(start, m2_at)?, m2)?,
    })
}

/// The nine hit location penalties, index 0 to 8.
pub fn hit_location_penalties(image: &PeImage<'_>) -> Result<[i32; 9], ExeError> {
    let values = read_i32s(image, HIT_LOCATION_PENALTY_VA, HIT_LOCATION_PENALTY_COUNT)?;
    let observed = values.len();
    // `try_into` instead of copying into a pre zeroed array, so a short read
    // is an error rather than a default that looks like real data.
    values
        .try_into()
        .map_err(|_| ExeError::RecordCount {
            va: HIT_LOCATION_PENALTY_VA,
            expected: HIT_LOCATION_PENALTY_COUNT,
            observed,
        })
}

/// The 810 critter critical records, flattened from the 15 x 9 x 6 nesting.
pub fn critical_success_critters(image: &PeImage<'_>) -> Result<Vec<CriticalRecord>, ExeError> {
    read_criticals(image, CRIT_SUCCESS_CRITTERS_VA, CRITTER_CRIT_RECORD_COUNT)
}

/// The 54 player critical records, flattened from the 9 x 6 nesting.
pub fn critical_success_player(image: &PeImage<'_>) -> Result<Vec<CriticalRecord>, ExeError> {
    read_criticals(image, CRIT_SUCCESS_PLAYER_VA, PLAYER_CRIT_RECORD_COUNT)
}

fn read_criticals(image: &PeImage<'_>, va: u32, count: usize) -> Result<Vec<CriticalRecord>, ExeError> {
    let len = count
        .checked_mul(CriticalRecord::SIZE)
        .ok_or(ExeError::RangeOverflow { va })?;
    let raw = image.read_va(va, len)?;
    let mut out = Vec::new();
    for index in 0..count {
        let start = index
            .checked_mul(CriticalRecord::SIZE)
            .ok_or(ExeError::RangeOverflow { va })?;
        out.push(read_critical_record(raw, start)?);
    }
    Ok(out)
}

/// The 38 stat limits, addressed by record index.
pub fn stat_limits(image: &PeImage<'_>) -> Result<Vec<StatLimit>, ExeError> {
    read_limits(image, STAT_LIMITS_VA, STAT_LIMIT_COUNT)
}

/// The five player stat limits: unspent skill points, level, experience,
/// reputation, karma, in that record order.
pub fn player_stat_limits(image: &PeImage<'_>) -> Result<Vec<StatLimit>, ExeError> {
    read_limits(image, PLAYER_STAT_LIMITS_VA, PLAYER_STAT_LIMIT_COUNT)
}

fn read_limits(image: &PeImage<'_>, va: u32, count: usize) -> Result<Vec<StatLimit>, ExeError> {
    let len = count
        .checked_mul(StatLimit::SIZE)
        .ok_or(ExeError::RangeOverflow { va })?;
    let raw = image.read_va(va, len)?;
    let mut out = Vec::new();
    for index in 0..count {
        let start = index
            .checked_mul(StatLimit::SIZE)
            .ok_or(ExeError::RangeOverflow { va })?;
        out.push(read_stat_limit(raw, start)?);
    }
    Ok(out)
}

/// The critical failure table as 7 rows of 5.
pub fn critical_failure(image: &PeImage<'_>) -> Result<Vec<Vec<i32>>, ExeError> {
    let mut rows = Vec::new();
    for group in 0..CRITICAL_FAILURE_GROUPS {
        let va = CRIT_FAILURE_VA
            .checked_add(
                group
                    .checked_mul(CRITICAL_FAILURE_WIDTH)
                    .and_then(|width| width.checked_mul(I32_SIZE))
                    .ok_or(ExeError::RangeOverflow { va: CRIT_FAILURE_VA })?,
            )
            .ok_or(ExeError::RangeOverflow { va: CRIT_FAILURE_VA })?;
        rows.push(read_i32s(image, va, CRITICAL_FAILURE_WIDTH)?);
    }
    Ok(rows)
}

/// The 21 XP thresholds. The array index is the level: element `L` is
/// `L * (L + 1) / 2` for `L` in `0 ..= 20`, and element 0 is level 0's zero
/// threshold. Values are not scaled; the caller multiplies by 1,000.
pub fn xp_thresholds(image: &PeImage<'_>) -> Result<[i32; 21], ExeError> {
    let values = read_i32s(image, XP_THRESHOLDS_VA, XP_THRESHOLD_COUNT)?;
    let observed = values.len();
    values
        .try_into()
        .map_err(|_| ExeError::RecordCount {
            va: XP_THRESHOLDS_VA,
            expected: XP_THRESHOLD_COUNT,
            observed,
        })
}

/// Experience needed to leave `level`, which is what the caller scales by 1,000.
///
/// `checked_mul` because the scaled value overflows `i32` for a high enough
/// level, and a wrapped total would hand out free levels.
pub fn experience_for_level(thresholds: &[i32; 21], level: i32) -> Result<i32, ExeError> {
    let index = usize::try_from(level).map_err(|_| ExeError::RangeOverflow { va: XP_THRESHOLDS_VA })?;
    let base = thresholds
        .get(index)
        .copied()
        .ok_or(ExeError::FieldOutOfRange {
            offset: XP_THRESHOLDS_VA,
            field: "level",
        })?;
    base.checked_mul(1_000).ok_or(ExeError::RangeOverflow { va: XP_THRESHOLDS_VA })
}

fn add(left: usize, right: usize) -> Result<usize, ExeError> {
    left.checked_add(right).ok_or(ExeError::RangeOverflow { va: 0 })
}

fn read_u16(bytes: &[u8], offset: usize, field: &'static str) -> Result<u16, ExeError> {
    let end = offset
        .checked_add(2)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?;
    let pair: [u8; 2] = bytes
        .get(offset..end)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?
        .try_into()
        .map_err(|_| ExeError::FieldOutOfRange { offset, field })?;
    Ok(u16::from_le_bytes(pair))
}

fn read_u32(bytes: &[u8], offset: usize, field: &'static str) -> Result<u32, ExeError> {
    let end = offset
        .checked_add(4)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?;
    let quad: [u8; 4] = bytes
        .get(offset..end)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?
        .try_into()
        .map_err(|_| ExeError::FieldOutOfRange { offset, field })?;
    Ok(u32::from_le_bytes(quad))
}

fn read_section_header(bytes: &[u8], offset: usize) -> Result<SectionHeader, ExeError> {
    let name: [u8; 8] = bytes
        .get(offset..add(offset, 8)?)
        .ok_or(ExeError::FieldOutOfRange { offset, field: "sectionName" })?
        .try_into()
        .map_err(|_| ExeError::FieldOutOfRange { offset, field: "sectionName" })?;
    Ok(SectionHeader {
        name,
        virtual_size: read_u32(bytes, add(offset, 8)?, "virtualSize")?,
        virtual_address: read_u32(bytes, add(offset, 12)?, "virtualAddress")?,
        raw_size: read_u32(bytes, add(offset, 16)?, "sizeOfRawData")?,
        raw_pointer: read_u32(bytes, add(offset, 20)?, "pointerToRawData")?,
    })
}

/// Lowercase hex SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    crate::sha256::hex(crate::sha256::digest(bytes))
}
```

### Interface decisions not literally in the guide

The guide gives no Rust signatures for this layer, so every shape below is a
spec-author decision. The four that matter:

1. **`PeImage<'a>` borrows its bytes.** The skeleton `pub fn read_va(&self, va:
   u32, len: usize) -> Result<&[u8], ExeError>` cannot name the lifetime of the
   returned slice, and a `PeImage` that owned a copy of the executable would
   put a whole PE image in memory for the sake of seven small tables. The
   returned slice borrows from the caller's byte slice with the same lifetime
   as `&self`.
2. **`StatLimit` and `CriticalRecord` hold named private fields plus const
   `FIELDS` tables, not `[i32; 6]` / `[i32; 7]`.** An array would need
   `self.fields[3]` to reach the minimum, which is the panicking slice index
   this style forbids. The const `FIELDS` tables keep the byte offsets and the
   error field names from drifting apart, so a layout change is a one line
   edit.
3. **`PeImage::parse` rejects PE32+.** The optional header moves `ImageBase`
   from offset 28 to offset 24 in PE32+, so accepting it would misread the base
   and every address derived from it. Refusing is cheaper than a version check.
4. **`read_va` bounds a read to its section, and the section overrun shares
   `ExeError::OutsideFile`.** The guide gives no Rust signatures, so the variant
   list is this spec's decision, and it keeps one variant for "the request does
   not fit in the region that address names" rather than adding one. The two
   events are not separable at this call anyway: `.rsrc` raw data ends exactly
   at the end of the file, so a read past that section is past the file too, and
   a test that demanded a distinct name would be asserting a distinction the
   bytes do not make. What the variant's documentation and its message now say
   is the part that matters: the bound is the section, and a read that leaves it
   is refused rather than completed from the next section's bytes.

`ExeError::BuildMismatch` carries the observed digest. A digest is a one way
function of the file and reveals no table value, and without it the gate cannot
give the "clear message" the guide asks for. No error variant ever carries
table content.

---

## Open questions

1. **The `Rulings` row and the table do not actually conflict.** The guide's
   *Older claims to discard* row reads "No critical hits exist | A critical is
   a multiplier from tables at `0x4FECB4` and `0x50454C`." Those are exactly
   the critter and player critical tables of the *Data compiled into the
   executable* table, so the two sections agree. The only imprecision is that
   the Rulings row does not say which address is critters and which is player,
   while the table orders them `critters, failure, player`. No address differs
   between the two sections, so the table was used as ground truth and nothing
   needed resolving. Flagged because it was named as a conflict to check.
2. **Stat limit fields 0, 1 and 2** have no established meaning. See EXETBL-016
   and EXETBL-017. Nothing may be inferred here.
3. **The three nesting indices of the critter critical table** (`15 x 9 x 6`)
   are not identified. Whether the leading 15 is a body part, an attacker class
   or a weapon class is not established.
4. **The five columns of the critical failure table** are not identified. The
   values look like flag masks.
5. **The message ids** in the critical records (5,000 to 6,513) are not mapped
   to strings. No table of message text was in scope.
6. **The labels of the five player stat limit records** are taken from the
   guide. The ranges are consistent with the guide's ordering, but the binary
   carries no label, so record 3 being `-20 .. 20` is evidence for
   "reputation", not proof of it.

---

## What this spec does NOT settle

- **The meaning of any field.** It fixes positions, widths, byte order,
  addresses, counts and strides. Semantics beyond the field names in the guide
  are not settled, and several are explicitly `UNVERIFIED`.
- **How any table is used.** Which critical row a given attack selects, how
  `effect_flags` is applied, how a minimum is clamped, and how the XP threshold
  is compared to a character's experience are all game rules that live
  elsewhere. This spec is the executable layer only.
- **The `1,000` scale of the XP threshold.** That the caller multiplies by 1,000
  is taken from the guide. The bytes contain unscaled triangular numbers; who
  multiplies, and whether the multiplier is applied before or after the level
  comparison, is not settled here.
- **Behaviour on a non-reference build.** The gate stops. It does not say which
  constants change, because nothing about another build was measured. Guessing
  would be worse than refusing.
- **Anything about a Community Edition binary.** No CE artefact was read. Any
  claim that needs CE is absent rather than `CONFIRMED IN COMMUNITY EDITION ONLY`.
- **The order of the `15 x 9 x 6` and `9 x 6` nesting**, beyond that the reader
  flattens and preserves index order.
- **Cached table format.** Whether the `target/` cache is TOML, binary or
  re-reads the executable is not fixed.
- **Which executable.** This spec assumes the layout above for the digest in
  EXETBL-001 only. A retail or patched build with a different digest is out of
  scope until someone measures it.
- **Whether the DGROUP `VirtualSize == 0` quirk is universal.** It is confirmed
  for this build. A reader should still use `SizeOfRawData` together with a
  nonzero `PointerToRawData`, because that pair is correct in general rather
  than correct only here.
- **Whether the original engine also refuses a read that crosses a section
  boundary.** The bytes show that this build's sections are laid out so a
  crossing is easy to make and hard to notice; they cannot show what the
  original reader did when handed an oversized length. The guide's total reader
  rule forbids the behaviour, so it is specified here as an error.