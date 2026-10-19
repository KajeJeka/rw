//! Install gate tests for the executable data tables of `falloutw.exe`.
//!
//! Spec: `docs/spec/exe-tables.md`, claims EXETBL-001 to EXETBL-017. Ledger:
//! `docs/spec/exe-tables.constants.toml`. Every expected value below was read out
//! of the bytes of the reference executable
//! `444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272`, so almost
//! every test here is **oracle class 1**.
//!
//! # Oracle classes
//!
//! The classes follow `docs/spec/trace.md`.
//!
//! - **oracle class 1**, bytes of the original executable. Every table value, every
//!   address, every count and all three adjacency identities below are asserted
//!   against bytes lifted out of the player's own `falloutw.exe` through the
//!   `PeImage` reader under test. Nothing is asserted against a value the code
//!   under test itself produced, and no fixture is invented here.
//! - **oracle class 5**, hand computed vectors in the implementation guide. The
//!   refusals that need no install at all: a foreign digest, a PE32+ optional
//!   header, and the error variants each refusal must carry.
//! - **oracle class 6**, algebraic laws. The bit flip walk, whose claim is
//!   totality: a reader that has to answer `Ok` or `Err` never panics and never
//!   needs a `catch_unwind` to prove it.
//!
//! # Why class 1 is available here at all
//!
//! The digest gate is what makes class 1 honest. `require_reference_build`
//! refuses any executable whose SHA-256 is not the reference digest, so a value
//! read out of the image cannot have come from a different build. The gate is
//! pinned twice: once against the digest itself, and once by flipping a single
//! bit and requiring an error, because a gate that accepts everything would
//! otherwise let every other test in this file pass against arbitrary bytes.
//!
//! # The three rulings a reader most easily gets wrong
//!
//! 1. **A6.** Every PE section of this build declares `VirtualSize == 0`, so
//!    containment has to be decided by `SizeOfRawData`. A reader that validates
//!    against `VirtualSize` rejects all seven addresses, and
//!    `install_read_va_returns_every_table_window_from_dgroup_raw_data` is what
//!    catches that.
//! 2. **A5.** The XP table is indexed by level **starting at zero**. Element 0 is
//!    0, element 1 is level 1 and equals 1. Indexing as if it began at level 1
//!    shifts every level down by one and makes level 20 read as 190.
//! 3. **A7.** A stat limit record's field 0 is `0` in all 38 records, so it is
//!    not the stat index; addressing a limit is by record index only.
//!
//! # Interface
//!
//! `docs/spec/exe-tables.md` states the reader names, the `StatLimit` and
//! `CriticalRecord` accessors and the `ExeError` variants; none of them is
//! guessed here. `ExeError` is matched exhaustively in [`exe_error_name`], so a
//! new variant is a compile error in this file rather than a name that quietly
//! stops matching.
//!
//! # Fail before, pass after
//!
//! This file is EXPECTED NOT TO COMPILE until `fo1-exe` exists. A gate test
//! written before the implementation is what makes "fail before, pass after"
//! meaningful, and a compile error counts as a failure. Every test is
//! `#[ignore]`d until `FO1_DATA` names the reference install.

// Gate tests are test code. `clippy.toml` sets `allow-unwrap-in-tests` and its
// siblings, which cover a `#[cfg(test)]` module or a `#[test]` function body.
// An integration test file's top-level helper functions sit outside both, so the
// same exemptions are restated here, with reasons, for the gate files only. The
// workspace lint table stays fully denied for the six production crates, which
// are what the guide's "no panicking readers" guarantee is about.
#![allow(
    clippy::panic,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "gate-test helpers: the test author must be able to assert a panic and index fixed-width headers outside #[test] bodies"
)]
use fo1_bigendian::BigEndian;
use fo1_endian_host::LittleEndian;
use fo1_exe::{
    critical_failure, critical_success_critters, critical_success_player, hit_location_penalties,
    player_stat_limits, stat_limits, xp_thresholds, CriticalRecord, ExeError, PeImage, StatLimit,
    REFERENCE_BUILD_SHA256,
};
use fo1_tests::{Install, REFERENCE_EXE_SHA256};

// --- the build gate ------------------------------------------------------------

/// SHA-256 of the reference `falloutw.exe` (EXETBL-C001 to C004, EXETBL-001).
const EXPECTED_SHA256_HEX: &str =
    "444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272";

/// Byte length of the reference executable (EXETBL-C005).
const EXPECTED_EXE_BYTES: u64 = 1_243_136;

/// Bit of the executable a foreign build test flips (EXETBL-001).
const FOREIGN_BUILD_BIT_MASK: u8 = 0x01;

/// Index of the byte that flip lands on: the last byte of the file, which lies in
/// `.rsrc` raw data and behind every header the reader reads.
const FOREIGN_BUILD_LAST_BYTE_FROM_END: usize = 1;

/// Length of the short synthetic buffer the gate must refuse before it looks at
/// a single header field (EXETBL-001: the digest is checked first).
const SHORT_BUFFER_LEN: usize = 64;

/// Short synthetic buffers refused, one short of a PE header and one short of
/// nothing at all.
const SHORT_BUFFER_CASES: usize = 2;

// --- the PE32 rejection --------------------------------------------------------

/// Offset of `e_lfanew` in the DOS header (EXETBL-C011).
const DOS_E_LFANEW_OFFSET: usize = 0x3C;

/// `e_lfanew` of the reference build: the PE signature sits at 0x80.
const DOS_E_LFANEW: usize = 0x80;

/// The PE32 optional header magic (EXETBL-002).
const PE32_OPTIONAL_MAGIC: u16 = 0x010B;

/// The PE32+ optional header magic, which `parse` must refuse (EXETBL-002).
const PE32_PLUS_OPTIONAL_MAGIC: u16 = 0x020B;

/// Offset of the optional header magic from the PE signature: 4 signature bytes
/// plus the 20 byte COFF header.
const OPTIONAL_MAGIC_FROM_SIGNATURE: usize = 24;

/// Offset of the optional header magic from the start of the reference file.
const OPTIONAL_MAGIC_OFFSET: usize = DOS_E_LFANEW + OPTIONAL_MAGIC_FROM_SIGNATURE;

/// The field label the spec fixes for a refused optional header magic.
const FIELD_OPTIONAL_MAGIC: &str = "optionalMagic";

/// The PE signature `0x00004550`, which sits at `e_lfanew`.
const PE_SIGNATURE: u32 = 0x0000_4550;

/// COFF machine `0x014C`, i386 (EXETBL-002).
const COFF_MACHINE_I386: u16 = 0x014C;

/// `SizeOfOptionalHeader` of the reference build (EXETBL-002).
const OPTIONAL_HEADER_SIZE: u16 = 224;

/// Length of the synthetic PE32+ fixture: a DOS header, a PE signature, a COFF
/// header and an optional header that stops right after its magic.
const SYNTHETIC_PE32_PLUS_LEN: usize = DOS_E_LFANEW + OPTIONAL_MAGIC_FROM_SIGNATURE + I32_SIZE;

/// `NumberOfSections` in the synthetic PE32+ fixture. One is enough because the
/// reader refuses the magic before it reaches the section table.
const SYNTHETIC_SECTION_COUNT: u16 = 1;

/// Offset of `NumberOfSections` from the PE signature: 4 signature bytes plus the
/// COFF header's machine and timestamp fields.
const COFF_SECTION_COUNT_OFFSET: usize = 6;

/// Offset of `SizeOfOptionalHeader` from the PE signature.
const COFF_OPTIONAL_SIZE_OFFSET: usize = 20;

// --- the PE section table ------------------------------------------------------

/// `ImageBase` 0x00400000 (EXETBL-C010).
const IMAGE_BASE: u32 = 0x0040_0000;

/// Section `DGROUP` `VirtualAddress` (EXETBL-C023). All seven tables live here.
const DGROUP_VIRTUAL_ADDRESS: u32 = 0x000F_0000;

/// Section `DGROUP` `PointerToRawData` (EXETBL-C024).
const DGROUP_RAW_POINTER: u32 = 0x000D_0E00;

/// Section `DGROUP` `SizeOfRawData` (EXETBL-C025).
const DGROUP_RAW_SIZE: u32 = 0x0004_CC00;

/// `VirtualSize` in every one of the six sections (EXETBL-C014). A reader that
/// decides containment with this field rejects every address in the build.
const SECTION_VIRTUAL_SIZE: u32 = 0;

/// First byte past the end of `DGROUP` raw data: 0x11DA00.
const DGROUP_RAW_END: usize = 1_169_920;

/// Bytes between the highest table byte and the end of `DGROUP` raw data.
const DGROUP_TAIL_SLACK: usize = 215_261;

/// The highest byte any of the seven tables reaches: 0xE9123.
const HIGHEST_TABLE_LAST_BYTE: usize = 954_659;

/// Number of sections in the reference build (EXETBL-C013).
const SECTION_COUNT: usize = 6;

/// The seven tables live in this one section.
const TABLES_IN_DGROUP: usize = 7;

// --- the seven table addresses -------------------------------------------------

/// Hit location penalty at 0x4FEC90 (EXETBL-C100).
const HIT_LOCATION_PENALTY_VA: u32 = 0x004F_EC90;

/// Critical success, critters at 0x4FECB4 (EXETBL-C101).
const CRIT_SUCCESS_CRITTERS_VA: u32 = 0x004F_ECB4;

/// Critical success, player at 0x50454C (EXETBL-C102).
const CRIT_SUCCESS_PLAYER_VA: u32 = 0x0050_454C;

/// Critical failure at 0x504B3C (EXETBL-C103).
const CRIT_FAILURE_VA: u32 = 0x0050_4B3C;

/// Stat limits at 0x507EC8 (EXETBL-C104).
const STAT_LIMITS_VA: u32 = 0x0050_7EC8;

/// Player stat limits at 0x508258 (EXETBL-C105).
const PLAYER_STAT_LIMITS_VA: u32 = 0x0050_8258;

/// XP thresholds at 0x5082D0 (EXETBL-C106).
const XP_THRESHOLDS_VA: u32 = 0x0050_82D0;

/// Minimum field of stat limits record 0, at 0x507ED4 (EXETBL-C107): the base
/// address plus the 12 byte offset of the minimum inside a record.
const STAT_LIMIT_0_MIN_VA: u32 = STAT_LIMITS_VA + STAT_LIMIT_MINIMUM_OFFSET_ADDRESS;

/// Minimum field of stat limits record 16, at 0x508054 (EXETBL-C108).
const STAT_LIMIT_16_MIN_VA: u32 =
    STAT_LIMITS_VA + STAT_LIMIT_16_STRIDE_ADDRESS + STAT_LIMIT_MINIMUM_OFFSET_ADDRESS;

/// Base address of stat limits record 16, at 0x508048 (EXETBL-C109).
const STAT_LIMIT_16_BASE_VA: u32 = STAT_LIMITS_VA + STAT_LIMIT_16_STRIDE_ADDRESS;

/// XP threshold element 1, level 1, at 0x5082D4 (EXETBL-C110).
const XP_ELEMENT_ONE_VA: u32 = XP_THRESHOLDS_VA + I32_SIZE_ADDRESS;

// --- file offsets, recorded so the mapping itself is asserted ------------------

/// Hit location penalty at file offset 0x000DFA90 (EXETBL-C200).
const HIT_LOCATION_FILE_OFFSET: usize = 916_112;

/// Critical success, critters at file offset 0x000DFAB4 (EXETBL-C201).
const CRITTER_CRITICAL_FILE_OFFSET: usize = 916_148;

/// Critical success, player at file offset 0x000E534C (EXETBL-C202).
const PLAYER_CRITICAL_FILE_OFFSET: usize = 938_828;

/// Critical failure at file offset 0x000E593C (EXETBL-C203).
const CRIT_FAILURE_FILE_OFFSET: usize = 940_348;

/// Stat limits at file offset 0x000E8CC8 (EXETBL-C204).
const STAT_LIMITS_FILE_OFFSET: usize = 953_544;

/// Player stat limits at file offset 0x000E9058 (EXETBL-C205).
const PLAYER_STAT_LIMITS_FILE_OFFSET: usize = 954_456;

/// XP thresholds at file offset 0x000E90D0 (EXETBL-C206).
const XP_FILE_OFFSET: usize = 954_576;

/// XP threshold element 1 at file offset 0x000E90D4 (EXETBL-C207). The four bytes
/// there are `01 00 00 00`.
const XP_ELEMENT_ONE_FILE_OFFSET: usize = 954_580;

/// Base of stat limits record 16 at file offset 0x000E8E48 (EXETBL-C109).
const STAT_LIMIT_16_BASE_FILE_OFFSET: usize = 953_928;

/// Minimum field of stat limits record 16 at file offset 0x000E8E54 (EXETBL-C108).
/// The four bytes there are `00 00 00 00`.
const STAT_LIMIT_16_MIN_FILE_OFFSET: usize = 953_940;

/// Minimum field of stat limits record 0 at file offset 0x000E8CD4 (EXETBL-C107).
/// The four bytes there are `01 00 00 00`, which is what confirms field 3 as the
/// minimum and the 24 byte stride.
const STAT_LIMIT_0_MIN_FILE_OFFSET: usize = 953_556;

// --- record sizes, field counts and table extents ------------------------------

/// Size of one little endian `i32` (EXETBL-C303).
const I32_SIZE: usize = 4;

/// Size of one critical record, seven `i32` (EXETBL-C300).
const CRITICAL_RECORD_SIZE: usize = 28;

/// Fields in a critical record (EXETBL-C304).
const CRITICAL_RECORD_FIELD_COUNT: usize = 7;

/// Size of one stat limit record, six `i32` (EXETBL-C301 and C302).
const STAT_LIMIT_SIZE: usize = 24;

/// Fields in a stat limit record (EXETBL-C305).
const STAT_LIMIT_FIELD_COUNT: usize = 6;

/// Byte offset of the minimum inside a stat limit record (EXETBL-C306).
const STAT_LIMIT_MINIMUM_OFFSET: usize = 12;

/// Byte offset of the maximum inside a stat limit record.
const STAT_LIMIT_MAXIMUM_OFFSET: usize = 16;

/// Byte offset of the default inside a stat limit record.
const STAT_LIMIT_DEFAULT_OFFSET: usize = 20;

/// Hit location penalty extent, `9 * 4` (EXETBL-C500).
const HIT_LOCATION_EXTENT: usize = 36;

/// Critter critical table extent, `15 * 9 * 6 * 28` (EXETBL-C501).
const CRITTER_CRITICAL_EXTENT: usize = 22_680;

/// Player critical table extent, `9 * 6 * 28` (EXETBL-C502).
const PLAYER_CRITICAL_EXTENT: usize = 1_512;

/// Critical failure extent, `7 * 5 * 4` (EXETBL-C503).
const CRIT_FAILURE_EXTENT: usize = 140;

/// Stat limits extent, `38 * 24` (EXETBL-C504).
const STAT_LIMITS_EXTENT: usize = 912;

/// Player stat limits extent, `5 * 24` (EXETBL-C505).
const PLAYER_STAT_LIMITS_EXTENT: usize = 120;

/// XP thresholds extent, `21 * 4` (EXETBL-C506).
const XP_EXTENT: usize = 84;

/// The literal one, so an offset expression never carries a bare numeral.
const ONE_BYTE: usize = 1;

/// The same literal in the virtual address width, so a `u32` address expression
/// never mixes widths.
const ONE_ADDRESS: u32 = 1;

/// Size of one `i32` in the virtual address width.
const I32_SIZE_ADDRESS: u32 = 4;

/// Byte offset of the minimum field inside a record, in the virtual address width.
const STAT_LIMIT_MINIMUM_OFFSET_ADDRESS: u32 = 12;

/// Record 16's offset into the stat limits table, in the virtual address width.
const STAT_LIMIT_16_STRIDE_ADDRESS: u32 = 16 * 24;

/// The literal two, used where a read asks a second byte past an end.
const TWO_BYTES: usize = 2;

/// Table reads driven per flipped bit: the XP table and the hit location
/// penalties, one each.
const TABLE_READS_PER_FLIP: usize = 2;

/// Index of the hit location penalty whose value is `-60`, the same number the
/// guide misreads as the stat 16 minimum.
const HIT_LOCATION_HEAD_INDEX: usize = 6;

/// Bytes of padding between the player critical table's end and the critical
/// failure table's start (EXETBL-005).
const PLAYER_CRITICAL_PADDING_BYTES: usize = 8;

/// Section `.rsrc` `VirtualAddress`, whose raw data ends exactly at the end of
/// the file, which is why it carries the file end test (EXETBL-C029).
const RSRC_VIRTUAL_ADDRESS: u32 = 0x002E_0000;

/// Section `.rsrc` `SizeOfRawData`.
const RSRC_RAW_SIZE: u32 = 0x0000_2400;

/// Last address of `.rsrc` raw data, whose single byte is the last byte of the
/// reference file.
const RSRC_LAST_VA: u32 = IMAGE_BASE + RSRC_VIRTUAL_ADDRESS + RSRC_RAW_SIZE - ONE_ADDRESS;

// --- table shapes --------------------------------------------------------------

/// Hit location penalty value count (EXETBL-C400).
const HIT_LOCATION_VALUE_COUNT: usize = 9;

/// Leading nesting dimension of the critter critical table (EXETBL-C401). The
/// three dimensions are not identified; the reader flattens and preserves order.
const CRITTER_CRITICAL_LEADING: usize = 15;

/// Middle nesting dimension of the critter critical table (EXETBL-C402).
const CRITTER_CRITICAL_MIDDLE: usize = 9;

/// Trailing nesting dimension of the critter critical table (EXETBL-C403).
const CRITTER_CRITICAL_TRAILING: usize = 6;

/// Critter critical record count, `15 * 9 * 6` (EXETBL-C404 and C600).
const CRITTER_CRITICAL_RECORD_COUNT: usize = 15 * 9 * 6;

/// Leading nesting dimension of the player critical table (EXETBL-C405).
const PLAYER_CRITICAL_LEADING: usize = 9;

/// Trailing nesting dimension of the player critical table (EXETBL-C406).
const PLAYER_CRITICAL_TRAILING: usize = 6;

/// Player critical record count, `9 * 6` (EXETBL-C407).
const PLAYER_CRITICAL_RECORD_COUNT: usize = 9 * 6;

/// Critical failure row count (EXETBL-C408).
const CRIT_FAILURE_ROW_COUNT: usize = 7;

/// Critical failure column count (EXETBL-C409).
const CRIT_FAILURE_WIDTH: usize = 5;

/// Critical failure value count, `7 * 5` (EXETBL-C410).
const CRIT_FAILURE_VALUE_COUNT: usize = 7 * 5;

/// Stat limits record count (EXETBL-C411 and C925).
const STAT_LIMIT_COUNT: usize = 38;

/// Player stat limits record count (EXETBL-C412).
const PLAYER_STAT_LIMIT_COUNT: usize = 5;

/// XP threshold count, levels 0 through 20 (EXETBL-C413).
const XP_THRESHOLD_COUNT: usize = 21;

/// The first level the XP table carries a threshold for.
const XP_FIRST_LEVEL: usize = 0;

/// The last level the XP table carries a threshold for.
const XP_LAST_LEVEL: usize = 20;

/// The index a level sits at in the XP table, which is the level itself (A5).
const XP_LEVEL_ONE_INDEX: usize = 1;

// --- the hit location penalties, EXETBL-003 ------------------------------------

/// The nine hit location penalties (EXETBL-C900 to C908). Index 6 is `-60`, the
/// very value the guide misreads as a stat 16 minimum; same number, different
/// table, different address.
const HIT_LOCATION_PENALTIES: [i32; HIT_LOCATION_VALUE_COUNT] =
    [-40, -30, -30, 0, -20, -20, -60, -30, 0];

/// Penalty index 4, `ec ff ff ff`, read most significant byte first as an
/// unsigned word: 3,976,200,191. This is the spec's own claim that big endian
/// gives values above two and a half billion, and it is what those four bytes
/// really fold to.
const PENALTY_LARGEST_WRONG_ORDER: u32 = 3_976_200_191;

/// Penalty index 6, `c4 ff ff ff`, the smallest non zero of the nine read most
/// significant byte first: 3,305,111,551. Every non zero penalty lands above two
/// and a half billion under the wrong order, which is the spec's claim exactly.
const PENALTY_SMALLEST_WRONG_ORDER: u32 = 3_305_111_551;

// --- the XP table, EXETBL-009 and EXETBL-015 ----------------------------------

/// The 21 XP thresholds as the bytes read them (EXETBL-C800 to C820): the
/// unscaled triangular numbers `L(L+1)/2` for `L` in `0..=20`, indexed by level.
const XP_THRESHOLDS: [i32; XP_THRESHOLD_COUNT] = [
    0, 1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 66, 78, 91, 105, 120, 136, 153, 171, 190, 210,
];

/// The XP threshold for level 0, element 0 of the table (EXETBL-C702).
const XP_LEVEL_ZERO: i32 = 0;

/// The XP threshold for level 1, folded least significant byte first (EXETBL-C700).
const XP_LEVEL_ONE_HOST: i32 = 1;

/// The XP threshold for level 1, folded most significant byte first from the very
/// same four bytes (EXETBL-C701). This is the Community Edition style failure the
/// anti deviation test exists to catch.
const XP_LEVEL_ONE_ASSET: i32 = 16_777_216;

/// The four bytes at 0x5082D4, which is where the level 1 threshold lives.
const XP_LEVEL_ONE_BYTES: [u8; I32_SIZE] = [0x01, 0x00, 0x00, 0x00];

/// Origin label every executable-layer cursor in this file is opened under.
const EXE_ORIGIN: &str = "falloutw.exe executable layer";

/// Origin label every asset-layer cursor in this file is opened under, so a
/// failure in the deliberately wrong order names itself.
const ASSET_ORIGIN: &str = "falloutw.exe asset layer, deliberately wrong order";

// --- the stat limits, EXETBL-007, EXETBL-016 and EXETBL-017 ------------------

/// Field 0 in all 38 stat limit records (EXETBL-C924). The guide calls it the
/// stat index; it is zero everywhere, so it cannot index anything.
const STAT_LIMIT_FIELD0: i32 = 0;

/// Field 1 in all 38 stat limit records (EXETBL-C924).
const STAT_LIMIT_FIELD1: i32 = 0;

/// Minimum of the first seven records, the seven core attributes (EXETBL-C922
/// and C705).
const STAT_LIMIT_CORE_MINIMUM: i32 = 1;

/// Maximum of the first seven records (EXETBL-C920).
const STAT_LIMIT_CORE_MAXIMUM: i32 = 10;

/// Default of the first seven records (EXETBL-C921).
const STAT_LIMIT_CORE_DEFAULT: i32 = 5;

/// Exactly seven records carry that `1 .. 10` range with default 5 (EXETBL-C923).
const STAT_LIMIT_CORE_RECORD_COUNT: usize = 7;

/// Index of record 16 in the stat limits table (EXETBL-014).
const STAT_LIMIT_16_INDEX: usize = 16;

/// Minimum of stat limits record 16, better criticals (EXETBL-C703). The
/// anti deviation target: the reference build reads 0 here.
const STAT_LIMIT_16_MINIMUM: i32 = 0;

/// The Community Edition style value this record does NOT carry (EXETBL-C704).
const COMMUNITY_STAT_16_MINIMUM: i32 = -60;

/// Records in which the wrong value must not appear (EXETBL-C925).
const MINIMUM_COLUMN_SCANNED: usize = STAT_LIMIT_COUNT;

/// Occurrences of the wrong value anywhere in the minimum column.
const MINIMUM_COLUMN_ABSENT: usize = 0;

/// Maximum of stat limits record 16 (EXETBL-C823).
const STAT_LIMIT_16_MAXIMUM: i32 = 100;

/// Field 2 of stat limits record 16 (EXETBL-C824). Its meaning is UNVERIFIED, so
/// it is recorded only so a reader can recognise the record.
const STAT_LIMIT_16_FIELD2: i32 = 94;

// --- the player stat limits, EXETBL-008 ---------------------------------------

/// Record index of unspent skill points.
const PLAYER_SKILL_POINTS_INDEX: usize = 0;

/// Record index of level. Its minimum is 1, so a character cannot begin at level
/// 0 even though the XP table has a level 0 entry (EXETBL-C911).
const PLAYER_LEVEL_INDEX: usize = 1;

/// Record index of experience (EXETBL-C912 pairs the level floor with its default).
const PLAYER_EXPERIENCE_INDEX: usize = 2;

/// Record index of reputation. The `-20 .. 20` span is consistent with the guide's
/// ordering but the binary carries no label, so this is evidence, not proof.
const PLAYER_REPUTATION_INDEX: usize = 3;

/// Record index of karma.
const PLAYER_KARMA_INDEX: usize = 4;

/// The unbounded maximum written as the largest positive `i32` (EXETBL-C822). It
/// is a value in the player's executable, not a crate limit.
const STAT_MAX_UNBOUNDED: i32 = 2_147_483_647;

/// Minimum of the level record (EXETBL-C911).
const PLAYER_LEVEL_MINIMUM: i32 = 1;

/// Default of the level record (EXETBL-C912): equal to its minimum, so a new
/// character starts at level 1 rather than 0.
const PLAYER_LEVEL_DEFAULT: i32 = 1;

/// Minimum of the reputation record (EXETBL-C913).
const PLAYER_REPUTATION_MINIMUM: i32 = -20;

/// Maximum of the reputation record (EXETBL-C914), the only player stat limit
/// whose maximum is small.
const PLAYER_REPUTATION_MAXIMUM: i32 = 20;

// --- the critical records, EXETBL-004 and EXETBL-005 -------------------------

/// Record 0 of the critter critical table, read from file 0xDFAB4.
const CRITTER_FIRST_RECORD: [i32; CRITICAL_RECORD_FIELD_COUNT] = [4, 0, -1, 0, 0, 5001, 5000];

/// Record 809 of the critter critical table, the last of the 810, read from
/// file 0xE5330 (EXETBL-C933).
const CRITTER_LAST_RECORD: [i32; CRITICAL_RECORD_FIELD_COUNT] = [4, 2048, -1, 0, 0, 6402, 5000];

/// Record 0 of the player critical table, read from file 0xE534C (EXETBL-C931).
const PLAYER_FIRST_RECORD: [i32; CRITICAL_RECORD_FIELD_COUNT] = [3, 0, -1, 0, 0, 6500, 5000];

/// Record 53 of the player critical table, the last of the 54, read from file
/// 0xE5918 (EXETBL-C932).
const PLAYER_LAST_RECORD: [i32; CRITICAL_RECORD_FIELD_COUNT] = [4, 2050, 6, 2, 128, 6503, 6513];

/// Index of the last critter critical record.
const CRITTER_LAST_INDEX: usize = CRITTER_CRITICAL_RECORD_COUNT - ONE_BYTE;

/// Index of the last player critical record.
const PLAYER_LAST_INDEX: usize = PLAYER_CRITICAL_RECORD_COUNT - ONE_BYTE;

/// `massive_critical_stat` in a record with no massive critical (EXETBL-C307).
/// It is a sentinel and must not be used as an index without a check.
const NO_MASSIVE_CRITICAL: i32 = -1;

/// A first record, whose sentinel sits at index 2 of the seven fields.
const MASSIVE_CRITICAL_FIELD_INDEX: usize = 2;

/// Byte offset of the first message id inside a critical record, 20 (EXETBL-C931).
const MESSAGE_ID_ONE_OFFSET: u32 = 20;

/// Player critical record 0's first message id (EXETBL-C931), read from the bytes
/// `64 19 00 00`. Big endian those same bytes are 1,679,360,000, which is not a
/// message id, so this one field settles the byte order on its own.
const PLAYER_FIRST_MESSAGE_ID: i32 = 6500;

/// Those four bytes, as they sit in the file.
const PLAYER_FIRST_MESSAGE_ID_BYTES: [u8; I32_SIZE] = [0x64, 0x19, 0x00, 0x00];

/// The same four bytes read most significant byte first, an unsigned word:
/// `0x64190000` is 1,679,360,000. An earlier draft of the constants carried
/// **1,661,012** here, and **1,392,008** for the companion `88 13 00 00` message
/// id whose measured value is `0x88130000` = 2,282,946,560. Both were the result
/// of a conversion error while the bytes were written down, not another fold:
/// measured against the file, `0x64190000` is 1,679,360,000 and `0x88130000` is
/// 2,282,946,560, and neither is a message id. The ledger now records the
/// measured values. The measured values are what the test asserts.
const PLAYER_FIRST_MESSAGE_ID_WRONG_ORDER: u32 = 1_679_360_000;

/// The same conversion error, second instance: the critter critical record's
/// `88 13 00 00` message id read most significant byte first. Not asserted on its
/// own, because the field is read little endian here; recorded so the ledger and
/// this file agree on which number is wrong.
const CRITTER_SECOND_MESSAGE_ID_WRONG_ORDER: u32 = 2_282_946_560;

/// Critter critical record 0's second message id (EXETBL-C930). Both first records
/// of both tables share it.
const CRITTER_MESSAGE_ID_BASE: i32 = 5000;

/// Critter critical record 809 at file offset 0x000E5330, the last record of the
/// 810. Its existence is what confirms the table occupies exactly 22,680 bytes.
const CRITTER_LAST_RECORD_FILE_OFFSET: usize = 938_800;

/// The highest message id anywhere in the two critical tables, in player critical
/// record 53 (EXETBL-C932).
const HIGHEST_MESSAGE_ID: i32 = 6513;

// --- the critical failure table, EXETBL-006 -----------------------------------

/// The seven rows of five values, read from file 0xE593C. Large power of two
/// combinations, consistent with flag masks; what the columns mean is not
/// established.
const CRIT_FAILURE_ROWS: [[i32; CRIT_FAILURE_WIDTH]; CRIT_FAILURE_ROW_COUNT] = [
    [0, 32768, 32768, 524290, 2097152],
    [0, 32768, 16384, 1048576, 65536],
    [0, 131072, 16384, 1048576, 8192],
    [32768, 163840, 49152, 1048576, 36864],
    [262144, 16384, 540672, 1048576, 4096],
    [32768, 262144, 8192, 1048576, 36866],
    [0, 32768, 1048576, 8192, 37888],
];

// --- the adjacency identities, EXETBL-010, EXETBL-011 and EXETBL-012 ----------

/// Left hand side of the first adjacency, the critter critical table (EXETBL-C604).
const ADJACENCY_ONE_LHS: u32 = CRIT_SUCCESS_CRITTERS_VA;

/// Right hand side of the first adjacency, the player critical table (EXETBL-C605).
const ADJACENCY_ONE_RHS: u32 = CRIT_SUCCESS_PLAYER_VA;

/// The distance between them, `15 * 9 * 6 * 28` (EXETBL-C601).
const ADJACENCY_ONE_DISTANCE: usize = CRITICAL_RECORD_SIZE * CRITTER_CRITICAL_RECORD_COUNT;

/// Left hand side of the second adjacency, the stat limits (EXETBL-C606).
const ADJACENCY_TWO_LHS: u32 = STAT_LIMITS_VA;

/// Right hand side of the second adjacency, the player stat limits.
const ADJACENCY_TWO_RHS: u32 = PLAYER_STAT_LIMITS_VA;

/// The distance between them, `38 * 24` (EXETBL-C602).
const ADJACENCY_TWO_DISTANCE: usize = STAT_LIMIT_SIZE * STAT_LIMIT_COUNT;

/// Left hand side of the third adjacency, the player stat limits.
const ADJACENCY_THREE_LHS: u32 = PLAYER_STAT_LIMITS_VA;

/// Right hand side of the third adjacency, the XP thresholds (EXETBL-C607).
const ADJACENCY_THREE_RHS: u32 = XP_THRESHOLDS_VA;

/// The distance between them, `5 * 24` (EXETBL-C603).
const ADJACENCY_THREE_DISTANCE: usize = STAT_LIMIT_SIZE * PLAYER_STAT_LIMIT_COUNT;

/// Number of tables the containment walk drives.
const SEVEN_TABLE_COUNT: usize = 7;

/// Bytes the seven tables occupy together: 36 + 22680 + 1512 + 140 + 912 + 120
/// + 84.
const SEVEN_TABLE_EXTENT: usize = 25_484;

// --- the totality cases, EXETBL-002 -------------------------------------------

/// `ImageBase` minus one, an address below the image and so in no section.
const BELOW_IMAGE_BASE_VA: u32 = IMAGE_BASE - ONE_ADDRESS;

/// First RVA past the end of `DGROUP` raw data: `0xF0000 + 0x4CC00`, which is in
/// no section because `.bss` does not start until RVA `0x140000`.
const PAST_DGROUP_VA: u32 = IMAGE_BASE + DGROUP_VIRTUAL_ADDRESS + DGROUP_RAW_SIZE;

/// Last address of `DGROUP` raw data, whose single byte is the last one before the
/// gap above.
const LAST_DGROUP_VA: u32 = PAST_DGROUP_VA - ONE_ADDRESS;

/// Total refusals the totality walk drives.
const TOTALITY_CASE_COUNT: usize = 7;

/// How far before the end of DGROUP the wider crossing read starts, so it has
/// three bytes of DGROUP left and runs further past the boundary than the one byte
/// case does.
const DGROUP_CROSSING_MARGIN: u32 = 1;

/// The wider crossing read's width.
const THREE_BYTES: usize = 3;

/// Section `.bss` `VirtualAddress` (EXETBL-C026). This is the **RVA**, not the
/// VA: its value in the ledger is 1310720, which is 0x00140000 in decimal. The
/// section's first virtual address is IMAGE_BASE plus this, below. Do not
/// "simplify" this to a VA, and do not read it as one.
const BSS_VIRTUAL_ADDRESS: u32 = 0x0014_0000;

/// Section `.bss` `PointerToRawData`. It is **zero**: `.bss` contributes no bytes
/// to the file at all. This is the single fact that makes the section's
/// containment test a two condition test rather than a one condition test.
const BSS_RAW_POINTER: u32 = 0;

/// Section `.bss` `SizeOfRawData`. It is **non zero**, 0x172C00, which is why a
/// reader that treats `SizeOfRawData != 0` as sufficient accepts `.bss` and
/// computes `file_offset = 0 + delta`.
const BSS_RAW_SIZE: u32 = 0x0017_2C00;

/// The first virtual address inside `.bss`. ImageBase 0x00400000 plus the `.bss`
/// RVA 0x00140000 is 0x00540000: the image base plus the `.bss` RVA is what makes
/// this the first `.bss` address, so the addition is spelled out here rather than
/// left for a reader to guess. 0x00414000 is a different address entirely and lies
/// inside BEGTEXT.
const BSS_FIRST_VA: u32 = IMAGE_BASE + BSS_VIRTUAL_ADDRESS;

/// Halfway through `.bss`, so the refusal below is not one hand picked address.
/// 0x00540000 + 0x00172C00 / 2 = 0x00540000 + 0x000B9600 = 0x005F9600.
const BSS_MIDDLE_VA: u32 = BSS_FIRST_VA + BSS_RAW_SIZE / 2;

/// The last virtual address inside `.bss`, 0x006B2BFF: the `.bss` RVA plus its
/// SizeOfRawData is 0x002B2C00, one past the last byte, so the last byte is
/// 0x002B2BFF and its VA is 0x00400000 + 0x002B2BFF.
const BSS_LAST_VA: u32 = BSS_FIRST_VA + BSS_RAW_SIZE - ONE_ADDRESS;

/// The first two bytes of any PE file, which is what a `SizeOfRawData`-only
/// containment test hands back from `file_offset = 0 + delta`.
const DOS_MAGIC: [u8; TWO_BYTES] = *b"MZ";

/// The rule the `.bss` refusals are held to, written out so the assertion reads
/// as a rule and not as a bare `true`.
const BSS_READ_MUST_BE_REFUSED: bool = true;

// --- the bit flip corpus -------------------------------------------------------

/// First address of the slice the bit flip walk covers: the XP table, so the walk
/// touches both a zero word and the level 1 word.
const BIT_FLIP_VA: u32 = XP_THRESHOLDS_VA;

/// Bytes of the slice the bit flip walk covers.
const BIT_FLIP_SLICE: usize = 8;

/// Bits in that slice, each flipped on its own.
const BIT_FLIP_BITS: usize = BIT_FLIP_SLICE * 8;

/// Bytes between the end of the flipped slice and the end of the file.
const BIT_FLIP_TAIL_BYTES: usize = 288_552;

/// Bits per byte, so a bit index names its byte as `index / BITS_PER_BYTE`.
const BITS_PER_BYTE: usize = 8;

// --- the table windows, EXETBL-002 --------------------------------------------

/// Tables the containment walk drives.
const TABLE_WINDOW_COUNT: usize = TABLES_IN_DGROUP;

/// One table's address, extent and the file offset the ledger recorded beside it.
struct TableWindow {
    /// Name printed with every assertion about this table.
    name: &'static str,
    /// Virtual address the reader is asked for.
    va: u32,
    /// Bytes the table occupies.
    len: usize,
    /// File offset the ledger recorded, which the reader must reproduce exactly.
    file_offset: usize,
}

/// The seven tables, with the ledger's own file offsets so the mapping is asserted
/// rather than trusted.
const TABLE_WINDOWS: [TableWindow; TABLE_WINDOW_COUNT] = [
    TableWindow {
        name: "hit location penalty",
        va: HIT_LOCATION_PENALTY_VA,
        len: HIT_LOCATION_EXTENT,
        file_offset: HIT_LOCATION_FILE_OFFSET,
    },
    TableWindow {
        name: "critical success, critters",
        va: CRIT_SUCCESS_CRITTERS_VA,
        len: CRITTER_CRITICAL_EXTENT,
        file_offset: CRITTER_CRITICAL_FILE_OFFSET,
    },
    TableWindow {
        name: "critical success, player",
        va: CRIT_SUCCESS_PLAYER_VA,
        len: PLAYER_CRITICAL_EXTENT,
        file_offset: PLAYER_CRITICAL_FILE_OFFSET,
    },
    TableWindow {
        name: "critical failure",
        va: CRIT_FAILURE_VA,
        len: CRIT_FAILURE_EXTENT,
        file_offset: CRIT_FAILURE_FILE_OFFSET,
    },
    TableWindow {
        name: "stat limits",
        va: STAT_LIMITS_VA,
        len: STAT_LIMITS_EXTENT,
        file_offset: STAT_LIMITS_FILE_OFFSET,
    },
    TableWindow {
        name: "player stat limits",
        va: PLAYER_STAT_LIMITS_VA,
        len: PLAYER_STAT_LIMITS_EXTENT,
        file_offset: PLAYER_STAT_LIMITS_FILE_OFFSET,
    },
    TableWindow {
        name: "XP thresholds",
        va: XP_THRESHOLDS_VA,
        len: XP_EXTENT,
        file_offset: XP_FILE_OFFSET,
    },
];

// --- helpers -------------------------------------------------------------------

/// Name of an `ExeError` variant, matched exhaustively with no wildcard arm, so
/// a new variant is a compile error here rather than a name that quietly stops
/// matching (guide: Errors that help; END-021).
fn exe_error_name(error: &ExeError) -> &'static str {
    match error {
        ExeError::Header { .. } => "Header",
        ExeError::Unmapped { .. } => "Unmapped",
        ExeError::RangeOverflow { .. } => "RangeOverflow",
        ExeError::OutsideFile { .. } => "OutsideFile",
        ExeError::FieldOutOfRange { .. } => "FieldOutOfRange",
        ExeError::RecordCount { .. } => "RecordCount",
        ExeError::BuildMismatch { .. } => "BuildMismatch",
    }
}

/// The `Header` variant's static field label, so a refusal names the field the
/// reader objected to rather than only the variant, and `None` for every other
/// variant. Matched exhaustively with no wildcard arm, so a new variant is a
/// compile error here (END-021).
fn exe_header_field(error: &ExeError) -> Option<&'static str> {
    match error {
        ExeError::Header { field, .. } => Some(field),
        ExeError::Unmapped { .. }
        | ExeError::RangeOverflow { .. }
        | ExeError::OutsideFile { .. }
        | ExeError::FieldOutOfRange { .. }
        | ExeError::RecordCount { .. }
        | ExeError::BuildMismatch { .. } => None,
    }
}

/// The name of any `ExeError` outcome, with `"Ok"` for the success that a
/// variant list cannot name. Several tests assert a parse or a gate outcome that
/// must succeed, and this says so in the same vocabulary as the refusals, so a
/// failure message names which side of the pair went wrong.
fn result_name<T, E>(result: Result<T, E>, describe: fn(&E) -> &'static str) -> &'static str {
    match result {
        Ok(_) => "Ok",
        Err(error) => describe(&error),
    }
}

/// The seven fields of one critical record, in the order the record lays them
/// out, so a tuple comparison names which field went wrong in the failure
/// message.
fn critical_fields(record: CriticalRecord) -> [i32; CRITICAL_RECORD_FIELD_COUNT] {
    [
        record.multiplier(),
        record.effect_flags(),
        record.massive_critical_stat(),
        record.massive_critical_modifier(),
        record.massive_critical_flags(),
        record.message_id_1(),
        record.message_id_2(),
    ]
}

/// The install, which fails loudly rather than skipping when `FO1_DATA` names
/// nothing or names the wrong tree.
fn reference_install() -> Install {
    Install::from_env().expect("FO1_DATA must name the reference install")
}

/// The reference executable's bytes, read through the harness so no path is ever
/// written into this file.
fn reference_exe_bytes() -> Vec<u8> {
    let install = reference_install();
    install
        .read_archive(install.exe_path())
        .expect("the reference executable reads")
}

/// The reference image, admitted only after the digest gate.
fn reference_image(bytes: &[u8]) -> Result<PeImage<'_>, ExeError> {
    PeImage::require_reference_build(bytes)
}

/// `PointerToRawData + (va - ImageBase - VirtualAddress)` for an address inside
/// `DGROUP`, computed here from the ledger rather than asked of the reader.
fn dgroup_file_offset(va: u32) -> usize {
    let rva = va
        .checked_sub(IMAGE_BASE)
        .expect("a table address is above the image base");
    let delta = rva
        .checked_sub(DGROUP_VIRTUAL_ADDRESS)
        .expect("a table address is inside DGROUP");
    let inside = usize::try_from(delta).expect("a DGROUP delta fits a usize");
    let pointer = usize::try_from(DGROUP_RAW_POINTER).expect("a raw pointer fits a usize");
    pointer
        .checked_add(inside)
        .expect("a DGROUP file offset fits a usize")
}

/// The last byte a window of `len` bytes at `va` reaches, in the file domain.
fn dgroup_last_byte(va: u32, len: usize) -> usize {
    dgroup_file_offset(va)
        .checked_add(len)
        .expect("a window end fits a usize")
        .checked_sub(ONE_BYTE)
        .expect("a window is never empty")
}

/// The `u16` at `offset` of the executable, read without the reader under test so
/// the header facts are checked against the caller's own bytes.
fn read_u16_at(bytes: &[u8], offset: usize) -> u16 {
    let end = offset
        .checked_add(TWO_BYTES)
        .expect("an offset plus two fits a usize");
    let pair: [u8; 2] = bytes
        .get(offset..end)
        .expect("the header holds the field")
        .try_into()
        .expect("two bytes fold into two bytes");
    u16::from_le_bytes(pair)
}

/// The `u32` at `offset` of a buffer, read the same way. This is how `e_lfanew`
/// itself is checked, so the test never takes the reader's word about where the
/// PE signature is.
fn read_u32_at(bytes: &[u8], offset: usize) -> u32 {
    let end = offset
        .checked_add(I32_SIZE)
        .expect("an offset plus four fits a usize");
    let quad: [u8; I32_SIZE] = bytes
        .get(offset..end)
        .expect("the header holds the field")
        .try_into()
        .expect("four bytes fold into four bytes");
    u32::from_le_bytes(quad)
}

/// Writes a little endian `u16` into a fixture at `offset`.
fn write_u16_at(bytes: &mut [u8], offset: usize, value: u16) {
    let end = offset
        .checked_add(TWO_BYTES)
        .expect("an offset plus two fits a usize");
    let slot = bytes
        .get_mut(offset..end)
        .expect("the fixture is long enough for the field");
    slot.copy_from_slice(&value.to_le_bytes());
}

/// Writes a little endian `u32` into a fixture at `offset`.
fn write_u32_at(bytes: &mut [u8], offset: usize, value: u32) {
    let end = offset
        .checked_add(I32_SIZE)
        .expect("an offset plus four fits a usize");
    let slot = bytes
        .get_mut(offset..end)
        .expect("the fixture is long enough for the field");
    slot.copy_from_slice(&value.to_le_bytes());
}

/// A synthetic PE32+ image: a valid DOS stub, a PE signature, a COFF header and
/// an optional header that stops right after its magic.
///
/// Nothing here is taken from the reference build, so the fixture cannot support
/// a CONFIRMED evidence tag and the test that uses it is class 5. It exists so the
/// PE32+ refusal is proved against a buffer that shares no history with the real
/// one: if the reader refused the real build for some other reason, this would
/// still be refused for the magic.
fn synthetic_pe32_plus_buffer() -> Vec<u8> {
    let mut buffer = vec![0u8; SYNTHETIC_PE32_PLUS_LEN];
    write_u32_at(
        &mut buffer,
        DOS_E_LFANEW_OFFSET,
        u32::try_from(DOS_E_LFANEW).expect("0x80 fits a u32"),
    );
    write_u32_at(&mut buffer, DOS_E_LFANEW, PE_SIGNATURE);
    write_u16_at(
        &mut buffer,
        DOS_E_LFANEW + COFF_SECTION_COUNT_OFFSET - 2,
        COFF_MACHINE_I386,
    );
    write_u16_at(
        &mut buffer,
        DOS_E_LFANEW + COFF_SECTION_COUNT_OFFSET,
        SYNTHETIC_SECTION_COUNT,
    );
    write_u16_at(
        &mut buffer,
        DOS_E_LFANEW + COFF_OPTIONAL_SIZE_OFFSET,
        OPTIONAL_HEADER_SIZE,
    );
    write_u16_at(&mut buffer, OPTIONAL_MAGIC_OFFSET, PE32_PLUS_OPTIONAL_MAGIC);
    buffer
}

// --- tests ---------------------------------------------------------------------

/// claim: EXETBL-001
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_reference_build_is_admitted_and_carries_the_expected_digest() {
    let install = reference_install();

    // The harness fingerprints the whole file, so the digest and the size below
    // are measured, not restated.
    assert_eq!(
        install.exe().sha256.as_str(),
        EXPECTED_SHA256_HEX,
        "EXETBL-001: the install's executable hashes to the reference digest"
    );
    assert_eq!(
        install.exe().bytes,
        EXPECTED_EXE_BYTES,
        "EXETBL-001: the reference build is 1243136 bytes, so a truncated or \
         appended file cannot share its digest"
    );

    // Three independent statements of the same digest must agree: the harness
    // that fingerprinted the file, the constant the reader crate gates on, and
    // the ledger value written out here.
    assert_eq!(
        REFERENCE_EXE_SHA256, EXPECTED_SHA256_HEX,
        "EXETBL-001: the harness and the ledger state the same digest"
    );
    assert_eq!(
        REFERENCE_BUILD_SHA256, EXPECTED_SHA256_HEX,
        "EXETBL-001: the reader crate gates on the same digest, so a reader built \
         against another build is impossible rather than merely discouraged"
    );

    let bytes = reference_exe_bytes();
    assert_eq!(
        bytes.len(),
        usize::try_from(EXPECTED_EXE_BYTES).expect("the reference size fits a usize"),
        "EXETBL-001: the bytes read back are the whole file"
    );

    // And the gate itself admits exactly this file.
    let admitted = reference_image(&bytes);
    assert_eq!(
        result_name(admitted, exe_error_name),
        "Ok",
        "EXETBL-001: the reference build passes the digest gate"
    );
}

/// claim: EXETBL-001
/// oracle: class 5 (hand computed vector from the implementation guide: a build \
/// whose digest is not the reference digest stops before any header is read)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_gate_rejects_an_executable_with_one_bit_flipped() {
    let bytes = reference_exe_bytes();

    // One bit, in the last byte of the file. That byte lies in `.rsrc` raw data,
    // behind every header the reader parses, so the flip leaves the headers
    // readable and the digest different: the gate is what has to catch it.
    let mut mutated = bytes.clone();
    let last = mutated
        .len()
        .checked_sub(FOREIGN_BUILD_LAST_BYTE_FROM_END)
        .expect("the reference build is never empty");
    let before = mutated
        .get(last)
        .expect("the last byte is there")
        .to_owned();
    let flipped = *mutated.get_mut(last).expect("the last byte is there") ^ FOREIGN_BUILD_BIT_MASK;
    assert_ne!(
        flipped, before,
        "EXETBL-001: the fixture really did flip a bit rather than write the same \
         byte twice"
    );
    // `^` yields a value and does not store through the place expression, so the
    // flip has to be written back explicitly. Without this line `mutated` was
    // byte-identical to `bytes` and the gate below was handed the reference build.
    mutated[last] = flipped;
    assert_eq!(
        mutated.len(),
        bytes.len(),
        "EXETBL-001: the flip changed one byte and not the file length"
    );

    let error = match reference_image(&mutated) {
        Ok(_) => panic!("EXETBL-001: a one bit flip of the reference build was admitted"),
        Err(error) => error,
    };
    assert_eq!(
        exe_error_name(&error),
        "BuildMismatch",
        "EXETBL-001: the refusal is a build mismatch, not a header complaint"
    );

    // The headers still parse. A gate that refused here would be refusing a parse
    // failure rather than a digest, which is a different test.
    assert_eq!(
        result_name(PeImage::parse(&mutated), exe_error_name),
        "Ok",
        "EXETBL-001: the flipped file's headers are intact, so the digest gate is \
         what refused it"
    );
}

/// claim: EXETBL-001
/// oracle: class 5 (hand computed vector from the implementation guide: the digest \
/// is checked first, before any header parse)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_gate_refuses_two_short_buffers_before_touching_a_header() {
    // Neither buffer holds a DOS header, a PE signature or an optional header at
    // all. A reader that parsed before it hashed would report `Header` here; the
    // spec orders the digest first, so both must report `BuildMismatch`. That is
    // the whole reason both buffers are useless as PE images.
    let short = vec![0u8; SHORT_BUFFER_LEN];
    let empty: Vec<u8> = Vec::new();
    assert_eq!(
        short.len(),
        SHORT_BUFFER_LEN,
        "EXETBL-001: the short fixture is short and is not empty"
    );
    assert_eq!(empty.len(), 0, "EXETBL-001: the empty fixture is empty");

    let mut refused = 0usize;
    for buffer in [short.as_slice(), empty.as_slice()] {
        let error = match PeImage::require_reference_build(buffer) {
            Ok(_) => panic!("EXETBL-001: a buffer that is not the reference build was admitted"),
            Err(error) => error,
        };
        assert_eq!(
            exe_error_name(&error),
            "BuildMismatch",
            "EXETBL-001: the digest is checked before any header field is read, so \
             a buffer with no headers at all still fails as a digest"
        );
        refused += 1;
    }
    assert_eq!(
        refused, SHORT_BUFFER_CASES,
        "EXETBL-001: both short buffers were refused"
    );

    // Both are refused for the digest and not by accident of their length, so the
    // gate never even reaches the file end.
    assert_eq!(
        result_name(PeImage::parse(&short), exe_error_name),
        "Header",
        "EXETBL-001: the same buffer does produce a header complaint once the \
         digest gate is bypassed, which is what makes the BuildMismatch above \
         meaningful"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vector from the implementation guide: a PE32+ \
/// optional header moves ImageBase, so the reader must refuse it)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_pe32_plus_optional_header_is_refused() {
    let bytes = reference_exe_bytes();

    // The reference build is PE32, so a reader that refused every optional header
    // would pass this test. That is why the discrimination is asserted here.
    assert_eq!(
        read_u16_at(&bytes, OPTIONAL_MAGIC_OFFSET),
        PE32_OPTIONAL_MAGIC,
        "EXETBL-002: the reference build's optional header magic is 0x010B"
    );
    assert_eq!(
        read_u32_at(&bytes, DOS_E_LFANEW_OFFSET),
        u32::try_from(DOS_E_LFANEW).expect("0x80 fits a u32"),
        "EXETBL-002: e_lfanew is 0x80, so the PE signature sits where the spec \
         records it and the optional header at 0x98"
    );
    assert_eq!(
        read_u32_at(&bytes, DOS_E_LFANEW),
        PE_SIGNATURE,
        "EXETBL-002: the four bytes at 0x80 are the PE signature 0x00004550"
    );
    assert_eq!(
        read_u16_at(&bytes, DOS_E_LFANEW + COFF_SECTION_COUNT_OFFSET),
        u16::try_from(SECTION_COUNT).expect("6 fits a u16"),
        "EXETBL-002: NumberOfSections is 6"
    );
    assert_eq!(
        result_name(PeImage::parse(&bytes), exe_error_name),
        "Ok",
        "EXETBL-002: the PE32 reference build is accepted"
    );

    // Flip only the optional magic. Nothing else about the file changes, so the
    // refusal can only be about the magic.
    let mut pe32_plus = bytes.clone();
    write_u16_at(
        &mut pe32_plus,
        OPTIONAL_MAGIC_OFFSET,
        PE32_PLUS_OPTIONAL_MAGIC,
    );
    assert_eq!(
        read_u16_at(&pe32_plus, OPTIONAL_MAGIC_OFFSET),
        PE32_PLUS_OPTIONAL_MAGIC,
        "EXETBL-002: the fixture really is PE32+ now"
    );

    let error = match PeImage::parse(&pe32_plus) {
        Ok(_) => panic!("EXETBL-002: a PE32+ optional header was accepted"),
        Err(error) => error,
    };
    assert_eq!(
        exe_error_name(&error),
        "Header",
        "EXETBL-002: the refusal is a header complaint"
    );
    assert_eq!(
        exe_header_field(&error),
        Some(FIELD_OPTIONAL_MAGIC),
        "EXETBL-002: and it names the optional header magic, so the reader says \
         which field it objected to"
    );

    // A synthetic PE32+ header that shares nothing with the reference build is
    // refused the same way, so the refusal is about the magic rather than about
    // this one file.
    let synthetic = synthetic_pe32_plus_buffer();
    assert_eq!(
        read_u16_at(&synthetic, OPTIONAL_MAGIC_OFFSET),
        PE32_PLUS_OPTIONAL_MAGIC,
        "EXETBL-002: the synthetic fixture is PE32+ too"
    );
    assert_eq!(
        result_name(PeImage::parse(&synthetic), exe_error_name),
        "Header",
        "EXETBL-002: a synthetic PE32+ header is refused as well"
    );
}

/// claim: EXETBL-010
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_critter_critical_table_ends_where_the_player_table_begins() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");

    // The identity, in the address space the reader speaks.
    assert_eq!(
        ADJACENCY_ONE_LHS, CRIT_SUCCESS_CRITTERS_VA,
        "EXETBL-010: the left hand side is the critter critical table"
    );
    assert_eq!(
        ADJACENCY_ONE_RHS, CRIT_SUCCESS_PLAYER_VA,
        "EXETBL-010: the right hand side is the player critical table"
    );
    assert_eq!(
        ADJACENCY_ONE_LHS + u32::try_from(ADJACENCY_ONE_DISTANCE).expect("22680 fits a u32"),
        ADJACENCY_ONE_RHS,
        "EXETBL-010: 0x4FECB4 plus 22680 is 0x50454C"
    );

    // And the distance is the table's own shape, not a number the author chose.
    assert_eq!(
        CRITTER_CRITICAL_RECORD_COUNT,
        CRITTER_CRITICAL_LEADING * CRITTER_CRITICAL_MIDDLE * CRITTER_CRITICAL_TRAILING,
        "EXETBL-010: the critter table is 15 by 9 by 6"
    );
    assert_eq!(
        ADJACENCY_ONE_DISTANCE,
        CRITICAL_RECORD_SIZE * CRITTER_CRITICAL_RECORD_COUNT,
        "EXETBL-010: 15 times 9 times 6 times 28 is 22680"
    );
    assert_eq!(
        ADJACENCY_ONE_DISTANCE, CRITTER_CRITICAL_EXTENT,
        "EXETBL-010: which is the extent the spec records for the table"
    );

    // The same identity in the file domain, where the two halves were measured
    // independently. This is what makes it an oracle and not arithmetic: the
    // reader's mapping has to reproduce the ledger's file offsets.
    let critter_last = dgroup_last_byte(CRIT_SUCCESS_CRITTERS_VA, CRITTER_CRITICAL_EXTENT);
    assert_eq!(
        critter_last,
        CRITTER_CRITICAL_FILE_OFFSET
            .checked_add(CRITTER_CRITICAL_EXTENT)
            .expect("the file end fits")
            .checked_sub(ONE_BYTE)
            .expect("the table is never empty"),
        "EXETBL-010: the critter table's last byte is 0xDFAB4 plus 22680 minus one"
    );
    assert_eq!(
        critter_last
            .checked_add(ONE_BYTE)
            .expect("the file end fits"),
        PLAYER_CRITICAL_FILE_OFFSET,
        "EXETBL-010: the critter table's last byte is the byte before the player \
         table's first, so the two are adjacent in the file as well as in memory"
    );

    // And the reader agrees with both.
    let critters = critical_success_critters(&image).expect("the critter table reads");
    let players = critical_success_player(&image).expect("the player critical table reads");
    assert_eq!(
        critters.len(),
        CRITTER_CRITICAL_RECORD_COUNT,
        "EXETBL-010: 810 records, which is exactly the extent the identity needs"
    );
    assert_eq!(
        CriticalRecord::SIZE,
        CRITICAL_RECORD_SIZE,
        "EXETBL-010: each of them is 28 bytes, which is what pins the identity"
    );
    assert_eq!(
        players.len(),
        PLAYER_CRITICAL_RECORD_COUNT,
        "EXETBL-010: the player table that follows it holds 54 records"
    );
}

/// claim: EXETBL-011
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_stat_limits_end_where_the_player_stat_limits_begin() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");

    assert_eq!(
        ADJACENCY_TWO_LHS, STAT_LIMITS_VA,
        "EXETBL-011: the left hand side is the stat limits table"
    );
    assert_eq!(
        ADJACENCY_TWO_RHS, PLAYER_STAT_LIMITS_VA,
        "EXETBL-011: the right hand side is the player stat limits table"
    );
    assert_eq!(
        ADJACENCY_TWO_LHS + u32::try_from(ADJACENCY_TWO_DISTANCE).expect("912 fits a u32"),
        ADJACENCY_TWO_RHS,
        "EXETBL-011: 0x507EC8 plus 912 is 0x508258"
    );
    assert_eq!(
        ADJACENCY_TWO_DISTANCE,
        STAT_LIMIT_SIZE * STAT_LIMIT_COUNT,
        "EXETBL-011: 38 times 24 is 912"
    );
    assert_eq!(
        STAT_LIMIT_SIZE,
        STAT_LIMIT_FIELD_COUNT * I32_SIZE,
        "EXETBL-011: and 24 bytes is six i32, so the stride pins the field count too"
    );

    // In the file domain, the second independently measured half.
    let stat_limits_last = dgroup_last_byte(STAT_LIMITS_VA, STAT_LIMITS_EXTENT);
    assert_eq!(
        stat_limits_last,
        PLAYER_STAT_LIMITS_FILE_OFFSET
            .checked_sub(ONE_BYTE)
            .expect("the file offset is above zero"),
        "EXETBL-011: the stat limits' last byte is the byte before the player \
         limits' first"
    );
    assert_eq!(
        dgroup_file_offset(PLAYER_STAT_LIMITS_VA),
        PLAYER_STAT_LIMITS_FILE_OFFSET,
        "EXETBL-011: the reader's mapping reproduces the ledger's player limits \
         file offset"
    );

    let limits = stat_limits(&image).expect("the stat limits table reads");
    let player = player_stat_limits(&image).expect("the player stat limits table reads");
    assert_eq!(
        limits.len(),
        STAT_LIMIT_COUNT,
        "EXETBL-011: 38 records, which is what the 912 byte identity needs"
    );
    assert_eq!(
        StatLimit::SIZE,
        STAT_LIMIT_SIZE,
        "EXETBL-011: each of them is 24 bytes"
    );
    assert_eq!(
        player.len(),
        PLAYER_STAT_LIMIT_COUNT,
        "EXETBL-011: the player table that follows it holds 5 records"
    );
}

/// claim: EXETBL-012
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_player_stat_limits_end_where_the_xp_table_begins() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");

    assert_eq!(
        ADJACENCY_THREE_LHS, PLAYER_STAT_LIMITS_VA,
        "EXETBL-012: the left hand side is the player stat limits table"
    );
    assert_eq!(
        ADJACENCY_THREE_RHS, XP_THRESHOLDS_VA,
        "EXETBL-012: the right hand side is the XP thresholds table"
    );
    assert_eq!(
        ADJACENCY_THREE_LHS + u32::try_from(ADJACENCY_THREE_DISTANCE).expect("120 fits a u32"),
        ADJACENCY_THREE_RHS,
        "EXETBL-012: 0x508258 plus 120 is 0x5082D0"
    );
    assert_eq!(
        ADJACENCY_THREE_DISTANCE,
        STAT_LIMIT_SIZE * PLAYER_STAT_LIMIT_COUNT,
        "EXETBL-012: 5 times 24 is 120"
    );
    assert_eq!(
        StatLimit::SIZE,
        STAT_LIMIT_SIZE,
        "EXETBL-012: the player limits share the 24 byte record of the table before, \
         which is what makes both identities hold at once"
    );

    // In the file domain, the third independently measured half.
    let player_last = dgroup_last_byte(PLAYER_STAT_LIMITS_VA, PLAYER_STAT_LIMITS_EXTENT);
    assert_eq!(
        player_last,
        XP_FILE_OFFSET
            .checked_sub(ONE_BYTE)
            .expect("the file offset is above zero"),
        "EXETBL-012: the player limits' last byte is the byte before the XP table's first"
    );
    assert_eq!(
        dgroup_file_offset(XP_THRESHOLDS_VA),
        XP_FILE_OFFSET,
        "EXETBL-012: the reader's mapping reproduces the ledger's XP file offset"
    );

    let player = player_stat_limits(&image).expect("the player stat limits table reads");
    let thresholds = xp_thresholds(&image).expect("the XP thresholds table reads");
    assert_eq!(
        player.len(),
        PLAYER_STAT_LIMIT_COUNT,
        "EXETBL-012: 5 records, which is what the 120 byte identity needs"
    );
    assert_eq!(
        thresholds.len(),
        XP_THRESHOLD_COUNT,
        "EXETBL-012: the XP table that follows it holds 21 values"
    );

    // All three identities at once. They pin three separate facts, so a reader
    // that got one stride wrong fails one identity and not the others.
    assert_eq!(
        ADJACENCY_TWO_DISTANCE,
        STAT_LIMIT_SIZE * STAT_LIMIT_COUNT,
        "EXETBL-012: the stat limits table is 38 records of one 24 byte stride, \
         which is what the 912 byte identity needs"
    );
    assert_eq!(
        ADJACENCY_THREE_DISTANCE,
        STAT_LIMIT_SIZE * PLAYER_STAT_LIMIT_COUNT,
        "EXETBL-012: the player stat limits table is 5 records of the same stride, \
         which is what the 120 byte identity needs, so one record size is pinned twice"
    );
    assert_eq!(
        ADJACENCY_ONE_DISTANCE,
        CRITICAL_RECORD_SIZE * CRITTER_CRITICAL_RECORD_COUNT,
        "EXETBL-012: and EXETBL-010 pins the 28 byte stride separately, which is \
         why the three identities are independent evidence rather than one \
         arithmetic fact restated three times"
    );
}

/// claim: EXETBL-013
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_xp_for_level_one_is_one_while_the_same_four_bytes_fold_big_endian_to_16777216() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");

    // The executable layer's own answer.
    let thresholds = xp_thresholds(&image).expect("the XP thresholds table reads");
    assert_eq!(
        thresholds[XP_LEVEL_ONE_INDEX], XP_LEVEL_ONE_HOST,
        "EXETBL-013: the XP threshold for level 1 is 1, not 16777216"
    );

    // The four bytes at that address, lifted out with the reader under test so
    // both folds below see the same input.
    let word = image
        .read_va(XP_ELEMENT_ONE_VA, I32_SIZE)
        .expect("the level 1 word is inside DGROUP");
    assert_eq!(
        word.len(),
        I32_SIZE,
        "EXETBL-013: the level 1 word is four bytes"
    );
    assert_eq!(
        word,
        XP_LEVEL_ONE_BYTES.as_slice(),
        "EXETBL-013: the four bytes at 0x5082D4 are 01 00 00 00"
    );
    assert_eq!(
        dgroup_file_offset(XP_ELEMENT_ONE_VA),
        XP_ELEMENT_ONE_FILE_OFFSET,
        "EXETBL-013: which is file offset 0xE90D4, the offset the ledger records"
    );

    // The asset layer folds the very same four bytes into the wrong number. This
    // is the deviation the anti deviation test exists to catch: a CE style read
    // produces a character who needs 16.7 million experience to leave level 1.
    let mut wrong_order = BigEndian::new(word, ASSET_ORIGIN);
    assert_eq!(
        wrong_order.i32().expect("four bytes read"),
        XP_LEVEL_ONE_ASSET,
        "EXETBL-013: the same four bytes read big-endian are 16777216"
    );

    // And the executable layer folds them back to 1. Both halves matter: the
    // first proves the order is little endian, the second proves the two layers
    // are genuinely different readers rather than one reader with a preference.
    let mut right_order = LittleEndian::new(word, EXE_ORIGIN);
    assert_eq!(
        right_order.i32().expect("four bytes read"),
        XP_LEVEL_ONE_HOST,
        "EXETBL-013: the same four bytes read little-endian are 1"
    );
    assert_ne!(
        XP_LEVEL_ONE_HOST, XP_LEVEL_ONE_ASSET,
        "EXETBL-013: the two orders differ by a factor of 16777216, and neither \
         read raises an error, so nothing inside the read path could catch the \
         mistake"
    );

    // Element 0 of the same table is level 0's zero threshold, which is what
    // proves the array index is the level.
    assert_eq!(
        thresholds[XP_FIRST_LEVEL], XP_LEVEL_ZERO,
        "EXETBL-013: element 0 is level 0's zero threshold, so indexing does not \
         begin at level 1"
    );
}

/// claim: EXETBL-009, EXETBL-015
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_xp_table_holds_the_twenty_one_triangular_numbers_indexed_by_level() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let thresholds = xp_thresholds(&image).expect("the XP thresholds table reads");

    assert_eq!(
        thresholds.len(),
        XP_THRESHOLD_COUNT,
        "EXETBL-009: the table holds 21 values, levels 0 through 20"
    );
    assert_eq!(
        thresholds, XP_THRESHOLDS,
        "EXETBL-009: the 21 values as the bytes read them"
    );

    // The closed form, checked at every level rather than only at the ends, so a
    // table that is right at both ends and wrong in the middle still fails.
    for level in XP_FIRST_LEVEL..XP_LAST_LEVEL.saturating_add(ONE_BYTE) {
        let triangular = level
            .checked_mul(level.saturating_add(ONE_BYTE))
            .expect("a level times its successor fits a usize")
            / 2;
        let expected = i32::try_from(triangular).expect("a level threshold fits an i32");
        assert_eq!(
            thresholds
                .get(level)
                .expect("the level is inside the table")
                .to_owned(),
            expected,
            "EXETBL-015: element L is L times (L plus 1) over 2, with L the level"
        );
    }

    // The index is the level: element 1 is level 1 and element 20 is level 20. An
    // off by one reader passes the closed form and fails here.
    assert_eq!(
        thresholds[XP_LEVEL_ONE_INDEX], 1,
        "EXETBL-015: element 1 is level 1"
    );
    assert_eq!(
        thresholds[XP_LAST_LEVEL], XP_THRESHOLDS[XP_LAST_LEVEL],
        "EXETBL-015: element 20 is level 20, the largest value in the table"
    );
    assert_eq!(
        thresholds[XP_LAST_LEVEL], 210,
        "EXETBL-015: which is exactly 20 times 21 over 2, unscaled"
    );

    // Element 0 is zero, so a reader that indexed by level minus one would put a
    // zero threshold at level 1 and read level 20 as 190.
    assert_eq!(
        thresholds[XP_FIRST_LEVEL], 0,
        "EXETBL-015: element 0 is level 0's zero threshold"
    );
    assert_eq!(
        XP_THRESHOLDS.len(),
        XP_LAST_LEVEL.saturating_add(ONE_BYTE),
        "EXETBL-015: the table has exactly one element per level from 0 to 20"
    );
}

/// claim: EXETBL-014
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_stat_limit_record_sixteen_has_a_minimum_of_zero_and_not_minus_sixty() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let limits = stat_limits(&image).expect("the stat limits table reads");

    let record = limits
        .get(STAT_LIMIT_16_INDEX)
        .expect("record 16 is inside a 38 record table")
        .to_owned();
    assert_eq!(
        record.minimum(),
        STAT_LIMIT_16_MINIMUM,
        "EXETBL-014: the minimum of stat 16 is 0, not -60"
    );
    assert_eq!(
        record.maximum(),
        STAT_LIMIT_16_MAXIMUM,
        "EXETBL-014: the same record's maximum is 100"
    );
    assert_eq!(
        record.field2(),
        STAT_LIMIT_16_FIELD2,
        "EXETBL-014: and its field 2 is 94, so the record is the one the spec names"
    );
    assert_eq!(
        record.default(),
        0,
        "EXETBL-014: the default is 0, so the record is not the 1 to 10 core shape"
    );

    // The four bytes at the address, folded by the executable layer. This is the
    // half that makes the claim a class 1 oracle: the value is read from the
    // address the spec names, not compared against a table the reader built.
    let field = image
        .read_va(STAT_LIMIT_16_MIN_VA, I32_SIZE)
        .expect("the record 16 minimum is inside DGROUP");
    assert_eq!(
        dgroup_file_offset(STAT_LIMIT_16_MIN_VA),
        STAT_LIMIT_16_MIN_FILE_OFFSET,
        "EXETBL-014: the minimum field sits at 0x508054, file 0xE8E54"
    );
    let mut reader = LittleEndian::new(field, EXE_ORIGIN);
    assert_eq!(
        reader.i32().expect("four bytes read"),
        STAT_LIMIT_16_MINIMUM,
        "EXETBL-014: the four bytes at 0x508054 read as 0"
    );
    assert_eq!(
        dgroup_file_offset(STAT_LIMIT_16_BASE_VA),
        STAT_LIMIT_16_BASE_FILE_OFFSET,
        "EXETBL-014: the record base is 0x507EC8 plus 16 times 24, file 0xE8E48"
    );

    // The deviation value must be absent from the whole minimum column. `-60` does
    // occur in this build, as hit location penalty index 6, so searching the file
    // for it would find it; only the column can answer this.
    let mut deviations = 0usize;
    for entry in limits.iter() {
        if entry.minimum() == COMMUNITY_STAT_16_MINIMUM {
            deviations += 1;
        }
    }
    assert_eq!(
        MINIMUM_COLUMN_SCANNED, STAT_LIMIT_COUNT,
        "EXETBL-014: every record's minimum was read, not a sample"
    );
    assert_eq!(
        deviations, MINIMUM_COLUMN_ABSENT,
        "EXETBL-014: no record's minimum is -60 anywhere in the table"
    );
    assert_eq!(
        HIT_LOCATION_PENALTIES[6], COMMUNITY_STAT_16_MINIMUM,
        "EXETBL-014: which is why -60 is searched for in one column and not in \
         the file, where it does occur as a hit location penalty"
    );
}

/// claim: EXETBL-007, EXETBL-016, EXETBL-017
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_stat_limits_are_thirty_eight_records_of_six_fields_with_a_zero_field_zero() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let limits = stat_limits(&image).expect("the stat limits table reads");

    assert_eq!(
        limits.len(),
        STAT_LIMIT_COUNT,
        "EXETBL-007: exactly 38 records"
    );
    assert_eq!(
        StatLimit::SIZE,
        STAT_LIMIT_SIZE,
        "EXETBL-007: each record is 24 bytes"
    );
    assert_eq!(
        StatLimit::FIELD_COUNT,
        STAT_LIMIT_FIELD_COUNT,
        "EXETBL-007: each record holds six fields"
    );
    assert_eq!(
        StatLimit::SIZE,
        STAT_LIMIT_FIELD_COUNT * I32_SIZE,
        "EXETBL-007: six i32 are 24 bytes"
    );
    assert_eq!(
        STAT_LIMIT_COUNT * STAT_LIMIT_SIZE,
        STAT_LIMITS_EXTENT,
        "EXETBL-007: 38 records of 24 bytes are the 912 bytes the table occupies"
    );

    // Fields 3, 4 and 5 are the minimum, the maximum and the default, and the
    // first record is where the bytes settle it: its minimum sits 12 bytes into
    // the record and reads 1.
    let first = limits
        .first()
        .expect("a 38 record table is not empty")
        .to_owned();
    assert_eq!(
        first.minimum(),
        STAT_LIMIT_CORE_MINIMUM,
        "EXETBL-007: the first record's minimum is 1"
    );
    assert_eq!(
        first.maximum(),
        STAT_LIMIT_CORE_MAXIMUM,
        "EXETBL-007: the first record's maximum is 10"
    );
    assert_eq!(
        first.default(),
        STAT_LIMIT_CORE_DEFAULT,
        "EXETBL-007: the first record's default is 5"
    );
    let first_min = image
        .read_va(STAT_LIMIT_0_MIN_VA, I32_SIZE)
        .expect("the first record's minimum is inside DGROUP");
    let mut first_reader = LittleEndian::new(first_min, EXE_ORIGIN);
    assert_eq!(
        first_reader.i32().expect("four bytes read"),
        STAT_LIMIT_CORE_MINIMUM,
        "EXETBL-007: and the four bytes at 0x507ED4 read as 1, which is what \
         confirms field 3 as the minimum and the 24 byte stride"
    );
    assert_eq!(
        usize::try_from(STAT_LIMIT_0_MIN_VA - STAT_LIMITS_VA).expect("a small delta fits"),
        STAT_LIMIT_MINIMUM_OFFSET,
        "EXETBL-007: that field sits 12 bytes into the record"
    );
    assert_eq!(
        dgroup_file_offset(STAT_LIMIT_0_MIN_VA),
        STAT_LIMIT_0_MIN_FILE_OFFSET,
        "EXETBL-007: and its file offset is 0xE8CD4, the offset the ledger records"
    );

    // The three field offsets, so a reader that shifted the minimum by one field
    // could not pass by reading the maximum and calling it the minimum.
    assert_eq!(
        STAT_LIMIT_MAXIMUM_OFFSET - STAT_LIMIT_MINIMUM_OFFSET,
        I32_SIZE,
        "EXETBL-007: the maximum sits one i32 after the minimum, at byte 16"
    );
    assert_eq!(
        STAT_LIMIT_DEFAULT_OFFSET - STAT_LIMIT_MAXIMUM_OFFSET,
        I32_SIZE,
        "EXETBL-007: and the default one i32 after that, at byte 20"
    );
    assert_eq!(
        STAT_LIMIT_DEFAULT_OFFSET + I32_SIZE,
        STAT_LIMIT_SIZE,
        "EXETBL-007: the last field ends exactly at the 24 byte record, which is \
         what confirms the stride"
    );
    assert_eq!(
        STAT_LIMIT_MINIMUM_OFFSET,
        STAT_LIMIT_MAXIMUM_OFFSET - I32_SIZE,
        "EXETBL-007: and the minimum is not the maximum read one field late"
    );

    // A7: field 0 is not the stat index, because it is zero in every record.
    // Addressing a limit is by record index only.
    let mut field0_zero = 0usize;
    let mut field1_zero = 0usize;
    for (index, record) in limits.iter().enumerate() {
        assert_eq!(
            record.field0(),
            STAT_LIMIT_FIELD0,
            "EXETBL-016: record {index} has field 0 zero, so field 0 cannot be a \
             stat index"
        );
        assert_eq!(
            record.field1(),
            STAT_LIMIT_FIELD1,
            "EXETBL-017: record {index} has field 1 zero as well"
        );
        field0_zero += 1;
        field1_zero += 1;
    }
    assert_eq!(
        field0_zero, MINIMUM_COLUMN_SCANNED,
        "EXETBL-016: all 38 records were scanned for a non zero field 0"
    );
    assert_eq!(
        field1_zero, MINIMUM_COLUMN_SCANNED,
        "EXETBL-017: all 38 records were scanned for a non zero field 1"
    );

    // Exactly seven records carry the 1 to 10 range with default 5, which is the
    // seven core attributes and is why field 2 looks index like there without
    // being an index.
    let mut core = 0usize;
    for record in limits.iter() {
        let is_core = record.minimum() == STAT_LIMIT_CORE_MINIMUM
            && record.maximum() == STAT_LIMIT_CORE_MAXIMUM
            && record.default() == STAT_LIMIT_CORE_DEFAULT;
        if is_core {
            core += 1;
        }
    }
    assert_eq!(
        core, STAT_LIMIT_CORE_RECORD_COUNT,
        "EXETBL-007: exactly seven records carry min 1, max 10 and default 5"
    );
}

/// claim: EXETBL-008
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_player_stat_limits_are_five_records_with_the_documented_ranges() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let player = player_stat_limits(&image).expect("the player stat limits table reads");

    assert_eq!(
        player.len(),
        PLAYER_STAT_LIMIT_COUNT,
        "EXETBL-008: exactly 5 records"
    );
    assert_eq!(
        StatLimit::FIELD_COUNT,
        STAT_LIMIT_FIELD_COUNT,
        "EXETBL-008: each record holds six fields, the same layout as the stat \
         limits table"
    );
    assert_eq!(
        PLAYER_STAT_LIMIT_COUNT * STAT_LIMIT_SIZE,
        PLAYER_STAT_LIMITS_EXTENT,
        "EXETBL-008: 5 records of 24 bytes are the 120 bytes the table occupies"
    );

    // Record 1 holds the level. Its floor is 1, so a character cannot begin at
    // level 0 even though the XP table has a level 0 entry, and its default equals
    // its minimum, so a new character starts at level 1.
    let level = player
        .get(PLAYER_LEVEL_INDEX)
        .expect("record 1 is inside a 5 record table")
        .to_owned();
    assert_eq!(
        level.minimum(),
        PLAYER_LEVEL_MINIMUM,
        "EXETBL-008: the level record's minimum is 1, a genuine level floor"
    );
    assert_eq!(
        level.default(),
        PLAYER_LEVEL_DEFAULT,
        "EXETBL-008: and its default is 1 as well, so a new character starts at \
         level 1 rather than level 0"
    );
    assert_eq!(
        level.maximum(),
        STAT_MAX_UNBOUNDED,
        "EXETBL-008: and its maximum is the largest positive i32, which is how \
         this build writes an unbounded maximum"
    );

    // Record 2 holds experience, and its maximum is unbounded in the same way.
    let experience = player
        .get(PLAYER_EXPERIENCE_INDEX)
        .expect("record 2 is inside a 5 record table")
        .to_owned();
    assert_eq!(
        experience.maximum(),
        STAT_MAX_UNBOUNDED,
        "EXETBL-008: the experience record's maximum is 2147483647"
    );
    assert_eq!(experience.minimum(), 0, "EXETBL-008: and its minimum is 0");

    // Record 3 holds reputation, the only player limit whose range is small and
    // the only one whose minimum is negative.
    let reputation = player
        .get(PLAYER_REPUTATION_INDEX)
        .expect("record 3 is inside a 5 record table")
        .to_owned();
    assert_eq!(
        reputation.minimum(),
        PLAYER_REPUTATION_MINIMUM,
        "EXETBL-008: the reputation record's minimum is -20"
    );
    assert_eq!(
        reputation.maximum(),
        PLAYER_REPUTATION_MAXIMUM,
        "EXETBL-008: and its maximum is 20, the only small maximum in the table"
    );

    // The other two records are unbounded in both directions, which is the shape
    // the spec records and the reason the reputation range stands out.
    let mut unbounded = 0usize;
    for record in player.iter() {
        if record.maximum() == STAT_MAX_UNBOUNDED {
            unbounded += 1;
        }
    }
    assert_eq!(
        unbounded,
        PLAYER_STAT_LIMIT_COUNT - ONE_BYTE,
        "EXETBL-008: four of the five records are unbounded and reputation is not"
    );

    let skill_points = player
        .get(PLAYER_SKILL_POINTS_INDEX)
        .expect("record 0 is inside a 5 record table")
        .to_owned();
    assert_eq!(
        skill_points.maximum(),
        STAT_MAX_UNBOUNDED,
        "EXETBL-008: unspent skill points are unbounded too"
    );
    let karma = player
        .get(PLAYER_KARMA_INDEX)
        .expect("record 4 is inside a 5 record table")
        .to_owned();
    assert_eq!(
        karma.maximum(),
        STAT_MAX_UNBOUNDED,
        "EXETBL-008: and so is karma, which is the last of the five records"
    );

    // Fields 0 and 1 are zero in all five records, exactly as in the stat limits
    // table above them.
    let mut zero_fields = 0usize;
    for record in player.iter() {
        assert_eq!(
            record.field0(),
            STAT_LIMIT_FIELD0,
            "EXETBL-008: field 0 is zero in the player limits as well"
        );
        assert_eq!(
            record.field1(),
            STAT_LIMIT_FIELD1,
            "EXETBL-008: and so is field 1"
        );
        zero_fields += 1;
    }
    assert_eq!(
        zero_fields, PLAYER_STAT_LIMIT_COUNT,
        "EXETBL-008: all five records were scanned"
    );
}

/// claim: EXETBL-003
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_hit_location_penalties_are_the_nine_documented_values() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let penalties = hit_location_penalties(&image).expect("the hit location penalty table reads");

    assert_eq!(
        penalties.len(),
        HIT_LOCATION_VALUE_COUNT,
        "EXETBL-003: exactly nine values"
    );
    assert_eq!(
        penalties, HIT_LOCATION_PENALTIES,
        "EXETBL-003: the nine penalties as the bytes read them"
    );
    assert_eq!(
        HIT_LOCATION_VALUE_COUNT * I32_SIZE,
        HIT_LOCATION_EXTENT,
        "EXETBL-003: nine i32 are the 36 bytes the table occupies"
    );

    // The adjacency the spec names: the table's last byte is the byte before the
    // critter critical table's first, in both address spaces.
    assert_eq!(
        dgroup_file_offset(HIT_LOCATION_PENALTY_VA),
        HIT_LOCATION_FILE_OFFSET,
        "EXETBL-003: the table sits at file offset 0xDFA90"
    );
    assert_eq!(
        dgroup_last_byte(HIT_LOCATION_PENALTY_VA, HIT_LOCATION_EXTENT),
        CRITTER_CRITICAL_FILE_OFFSET - ONE_BYTE,
        "EXETBL-003: and its last byte is the byte before the critter table, which \
         is an independent confirmation of the 36 byte extent"
    );
    assert_eq!(
        HIT_LOCATION_PENALTY_VA + u32::try_from(HIT_LOCATION_EXTENT).expect("36 fits a u32"),
        CRIT_SUCCESS_CRITTERS_VA,
        "EXETBL-003: the same adjacency in the address space"
    );

    // Index 6 is -60, which is the number the guide misreads as a stat 16 minimum.
    // Asserting both makes the confusion between the two tables impossible.
    assert_eq!(
        penalties[HIT_LOCATION_HEAD_INDEX], COMMUNITY_STAT_16_MINIMUM,
        "EXETBL-003: penalty index 6 is -60, the same number the guide misreads as \
         the stat 16 minimum; same value, different table, different address"
    );
    assert_eq!(
        HIT_LOCATION_PENALTIES[HIT_LOCATION_HEAD_INDEX], -60,
        "EXETBL-003: written out, so a wrong literal in the constant fails"
    );
    // The penalties read as small negative numbers only in little endian. Big
    // endian, the very same bytes fold unsigned into values above two and a half
    // billion, which is the third independent reason the executable layer is
    // little endian. The read is `u32` on purpose: `i32` reinterprets the same
    // bits as a negative number, which is a fourth fact and not this one.
    let window = image
        .read_va(HIT_LOCATION_PENALTY_VA, HIT_LOCATION_EXTENT)
        .expect("the penalty table is inside DGROUP");
    let mut wrong_order = BigEndian::new(window, ASSET_ORIGIN);
    let mut folded_largest = 0u32;
    let mut folded_smallest_nonzero = u32::MAX;
    let mut folded_count = 0usize;
    for _ in 0..HIT_LOCATION_VALUE_COUNT {
        let value = wrong_order.u32().expect("four bytes read");
        if value != 0 {
            folded_smallest_nonzero = folded_smallest_nonzero.min(value);
        }
        folded_largest = folded_largest.max(value);
        folded_count += 1;
    }
    assert_eq!(
        folded_count, HIT_LOCATION_VALUE_COUNT,
        "EXETBL-003: all nine values were folded the wrong way"
    );
    assert_eq!(
        folded_largest, PENALTY_LARGEST_WRONG_ORDER,
        "EXETBL-003: read most significant byte first, penalty index 4 is \
         3976200191. The nine bytes only fold to small numbers the other way."
    );
    assert_eq!(
        folded_smallest_nonzero, PENALTY_SMALLEST_WRONG_ORDER,
        "EXETBL-003: and the smallest non zero of them, penalty index 6, is \
         3305111551, which is what the spec's own claim that big endian gives \
         values above two and a half billion rests on"
    );
}

/// claim: EXETBL-004
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_critical_success_critters_are_810_records_of_28_bytes() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let critters = critical_success_critters(&image).expect("the critter critical table reads");

    assert_eq!(
        critters.len(),
        CRITTER_CRITICAL_RECORD_COUNT,
        "EXETBL-004: 15 by 9 by 6 records, which the reader flattens to 810"
    );
    assert_eq!(
        CRITTER_CRITICAL_RECORD_COUNT,
        CRITTER_CRITICAL_LEADING * CRITTER_CRITICAL_MIDDLE * CRITTER_CRITICAL_TRAILING,
        "EXETBL-004: the three nesting dimensions multiply out"
    );
    assert_eq!(
        CriticalRecord::SIZE,
        CRITICAL_RECORD_SIZE,
        "EXETBL-004: each record is 28 bytes"
    );
    assert_eq!(
        CriticalRecord::FIELD_COUNT,
        CRITICAL_RECORD_FIELD_COUNT,
        "EXETBL-004: each record holds seven fields"
    );
    assert_eq!(
        CriticalRecord::SIZE,
        CRITICAL_RECORD_FIELD_COUNT * I32_SIZE,
        "EXETBL-004: seven i32 are 28 bytes"
    );
    assert_eq!(
        critters.len() * CriticalRecord::SIZE,
        CRITTER_CRITICAL_EXTENT,
        "EXETBL-004: 810 records of 28 bytes are the 22680 bytes the table occupies"
    );

    // The nesting is flattened, not materialised as three nested vectors, so the
    // reader hands back one sequence whose length is the product.
    assert_eq!(
        critters.len(),
        CRITTER_CRITICAL_RECORD_COUNT,
        "EXETBL-004: the sequence is flat, so its length is the product of the \
         three dimensions and not any one of them"
    );
    assert_ne!(
        critters.len(),
        CRITTER_CRITICAL_TRAILING,
        "EXETBL-004: and it is not the innermost dimension alone, which is what a \
         reader that kept only the last axis would return"
    );
}

/// claim: EXETBL-005
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_critical_success_player_is_54_records_of_28_bytes() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let players = critical_success_player(&image).expect("the player critical table reads");

    assert_eq!(
        players.len(),
        PLAYER_CRITICAL_RECORD_COUNT,
        "EXETBL-005: 9 by 6 records, which the reader flattens to 54"
    );
    assert_eq!(
        PLAYER_CRITICAL_RECORD_COUNT,
        PLAYER_CRITICAL_LEADING * PLAYER_CRITICAL_TRAILING,
        "EXETBL-005: the two nesting dimensions multiply out"
    );
    assert_eq!(
        CriticalRecord::SIZE,
        CRITICAL_RECORD_SIZE,
        "EXETBL-005: each record is 28 bytes, the same layout as the critter table"
    );
    assert_eq!(
        players.len() * CriticalRecord::SIZE,
        PLAYER_CRITICAL_EXTENT,
        "EXETBL-005: 54 records of 28 bytes are the 1512 bytes the table occupies"
    );

    // The player table is not flush with the critical failure table: it ends at
    // file 0xE5933 and the next table starts 8 bytes later at 0xE593C. Only the
    // critter to player adjacency is an identity, which is why the gap exists.
    assert_eq!(
        dgroup_file_offset(CRIT_SUCCESS_PLAYER_VA),
        PLAYER_CRITICAL_FILE_OFFSET,
        "EXETBL-005: the table sits at file offset 0xE534C"
    );
    assert_eq!(
        dgroup_last_byte(CRIT_SUCCESS_PLAYER_VA, PLAYER_CRITICAL_EXTENT)
            + ONE_BYTE
            + PLAYER_CRITICAL_PADDING_BYTES,
        CRIT_FAILURE_FILE_OFFSET,
        "EXETBL-005: and its last byte is eight bytes of padding before the \
         critical failure table, which is why only the critter to player pair is \
         an identity"
    );
    assert_eq!(
        PLAYER_CRITICAL_PADDING_BYTES, 8,
        "EXETBL-005: the gap is eight bytes of padding"
    );
    assert_ne!(
        CRIT_FAILURE_FILE_OFFSET,
        PLAYER_CRITICAL_FILE_OFFSET + PLAYER_CRITICAL_EXTENT,
        "EXETBL-005: the two tables are not adjacent, so a reader that assumed \
         they were would read eight bytes of padding as a failure row"
    );
}

/// claim: EXETBL-006
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_critical_failure_is_seven_rows_of_five_values() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let rows = critical_failure(&image).expect("the critical failure table reads");

    assert_eq!(
        rows.len(),
        CRIT_FAILURE_ROW_COUNT,
        "EXETBL-006: exactly seven rows"
    );
    assert_eq!(
        CRIT_FAILURE_ROW_COUNT * CRIT_FAILURE_WIDTH,
        CRIT_FAILURE_VALUE_COUNT,
        "EXETBL-006: 7 rows of 5 values is 35 values"
    );
    assert_eq!(
        CRIT_FAILURE_VALUE_COUNT * I32_SIZE,
        CRIT_FAILURE_EXTENT,
        "EXETBL-006: 35 little endian i32 are the 140 bytes the table occupies"
    );
    assert_eq!(
        dgroup_file_offset(CRIT_FAILURE_VA),
        CRIT_FAILURE_FILE_OFFSET,
        "EXETBL-006: the table sits at file offset 0xE593C"
    );

    let mut width_total = 0usize;
    for row in rows.iter() {
        assert_eq!(
            row.len(),
            CRIT_FAILURE_WIDTH,
            "EXETBL-006: every row holds exactly five values"
        );
        width_total += row.len();
    }
    assert_eq!(
        width_total, CRIT_FAILURE_VALUE_COUNT,
        "EXETBL-006: and the seven rows hold 35 values between them"
    );
    // The values themselves. They are large power of two combinations, consistent
    // with flag masks rather than signed penalties, and the layout is fixed at
    // 7 by 5 by i32 even though what the five columns mean is not established.
    let mut compared = 0usize;
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            *row, CRIT_FAILURE_ROWS[index],
            "EXETBL-006: row {index} as the bytes read it"
        );
        compared += 1;
    }
    assert_eq!(
        compared, CRIT_FAILURE_ROW_COUNT,
        "EXETBL-006: every row was compared against the ledger"
    );
    assert_eq!(
        rows.len(),
        CRIT_FAILURE_ROWS.len(),
        "EXETBL-006: the row count the ledger records and the table's own agree"
    );
}

/// claim: EXETBL-004
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_critter_critical_first_and_last_records_carry_the_measured_bytes() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let critters = critical_success_critters(&image).expect("the critter critical table reads");

    let first = critters
        .first()
        .expect("a 810 record table is not empty")
        .to_owned();
    assert_eq!(
        critical_fields(first),
        CRITTER_FIRST_RECORD,
        "EXETBL-004: record 0 is 4, 0, -1, 0, 0, 5001, 5000"
    );
    assert_eq!(
        dgroup_file_offset(CRIT_SUCCESS_CRITTERS_VA),
        CRITTER_CRITICAL_FILE_OFFSET,
        "EXETBL-004: which is file offset 0xDFAB4"
    );

    let last = critters
        .get(CRITTER_LAST_INDEX)
        .expect("index 809 is inside an 810 record table")
        .to_owned();
    assert_eq!(
        critical_fields(last),
        CRITTER_LAST_RECORD,
        "EXETBL-004: record 809 is 4, 2048, -1, 0, 0, 6402, 5000"
    );
    assert_eq!(
        dgroup_file_offset(CRIT_SUCCESS_CRITTERS_VA)
            .checked_add(CRITTER_LAST_INDEX * CRITICAL_RECORD_SIZE)
            .expect("the last record's file offset fits"),
        CRITTER_LAST_RECORD_FILE_OFFSET,
        "EXETBL-004: and record 809 sits at file 0xE5330, whose message id 1 is \
         6402. Its existence is what confirms the table occupies exactly 22680 \
         bytes and stops one byte short of the player table."
    );

    // The -1 in the massive-critical-stat field is the "no massive critical"
    // marker, not an index. Both the first and the last record carry it.
    assert_eq!(
        first.massive_critical_stat(),
        NO_MASSIVE_CRITICAL,
        "EXETBL-004: record 0's massive critical stat is the -1 sentinel"
    );
    assert_eq!(
        last.massive_critical_stat(),
        NO_MASSIVE_CRITICAL,
        "EXETBL-004: and so is record 809's"
    );
    assert_eq!(
        NO_MASSIVE_CRITICAL, -1,
        "EXETBL-004: written out, so a wrong literal in the constant fails"
    );
    assert_eq!(
        MASSIVE_CRITICAL_FIELD_INDEX, 2,
        "EXETBL-004: and the sentinel is read from field index 2 of the seven, \
         which is a position in the record rather than an index into anything"
    );

    // The two records' own fields, so a reader that shifted the record by one
    // field cannot pass by producing the same tuple somewhere else.
    assert_eq!(
        first.message_id_1(),
        5001,
        "EXETBL-004: record 0's first message id is 5001"
    );
    assert_eq!(
        first.message_id_2(),
        CRITTER_MESSAGE_ID_BASE,
        "EXETBL-004: and its second is 5000"
    );
    assert_eq!(
        last.effect_flags(),
        2048,
        "EXETBL-004: record 809's effect flags are 2048"
    );
}

/// claim: EXETBL-005
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_player_critical_first_and_last_records_carry_the_measured_bytes() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");
    let players = critical_success_player(&image).expect("the player critical table reads");

    let first = players
        .first()
        .expect("a 54 record table is not empty")
        .to_owned();
    assert_eq!(
        critical_fields(first),
        PLAYER_FIRST_RECORD,
        "EXETBL-005: record 0 is 3, 0, -1, 0, 0, 6500, 5000"
    );

    let last = players
        .get(PLAYER_LAST_INDEX)
        .expect("index 53 is inside a 54 record table")
        .to_owned();
    assert_eq!(
        critical_fields(last),
        PLAYER_LAST_RECORD,
        "EXETBL-005: record 53 is 4, 2050, 6, 2, 128, 6503, 6513"
    );

    // The message ids confirm the byte order on their own: the bytes
    // `64 19 00 00` are 6500 little endian and 1,679,360,000 big endian, and no
    // message id is the latter. `docs/spec/exe-tables.md` prints 1,661,012 at
    // this point, which is not what those four bytes fold to; the measured value
    // is asserted here and the discrepancy is in the test author report.
    assert_eq!(
        first.message_id_1(),
        PLAYER_FIRST_MESSAGE_ID,
        "EXETBL-005: record 0's first message id is 6500, read from the bytes \
         64 19 00 00, which is an independent confirmation that message ids are \
         little endian"
    );
    let raw = image
        .read_va(CRIT_SUCCESS_PLAYER_VA + MESSAGE_ID_ONE_OFFSET, I32_SIZE)
        .expect("the message id field is inside DGROUP");
    assert_eq!(
        raw,
        PLAYER_FIRST_MESSAGE_ID_BYTES.as_slice(),
        "EXETBL-005: and those four bytes are exactly 64 19 00 00"
    );
    let mut wrong_order = BigEndian::new(raw, ASSET_ORIGIN);
    assert_eq!(
        wrong_order.u32().expect("four bytes read"),
        PLAYER_FIRST_MESSAGE_ID_WRONG_ORDER,
        "EXETBL-005: the same four bytes read most significant byte first are \
         1679360000, which is not a message id"
    );
    assert_eq!(
        PLAYER_FIRST_MESSAGE_ID, 6_500,
        "EXETBL-005: written out, so a wrong literal in the constant fails"
    );
    assert_ne!(
        PLAYER_FIRST_MESSAGE_ID, 1_661_012,
        "EXETBL-005: and it is not 1661012, the figure an earlier draft of the \
         constants carried for this row. That was a conversion error rather than \
         another fold: the bytes `64 19 00 00` are 0x64190000, that is 1679360000. \
         The ledger now records the measured value."
    );
    assert_eq!(
        CRITTER_SECOND_MESSAGE_ID_WRONG_ORDER, 2_282_946_560,
        "EXETBL-005: and the companion conversion error landed the same way. Those \
         bytes are 0x88130000, that is 2282946560, not the 1392008 an earlier draft \
         carried. The row documenting this pair now records the measured value."
    );

    // Record 53 is the one record in either table with no -1 sentinel, so it is
    // where a reader that assumed every record was "no massive critical" would
    // fail.
    assert_eq!(
        first.massive_critical_stat(),
        NO_MASSIVE_CRITICAL,
        "EXETBL-005: record 0 carries the -1 sentinel"
    );
    assert_eq!(
        last.massive_critical_stat(),
        6,
        "EXETBL-005: record 53 does not, so the sentinel is a value and not a \
         constant every record shares"
    );
    assert_ne!(
        last.massive_critical_stat(),
        NO_MASSIVE_CRITICAL,
        "EXETBL-005: a reader that read every record's massive critical stat as \
         -1 would pass record 0 and fail here"
    );
    assert_eq!(
        last.message_id_2(),
        HIGHEST_MESSAGE_ID,
        "EXETBL-005: and record 53's second message id, 6513, is the highest id \
         observed anywhere in the two tables"
    );
}

/// claim: EXETBL-002
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_read_va_returns_every_table_window_from_dgroup_raw_data() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");

    // A6. Every section of this build declares `VirtualSize == 0`, so a reader
    // that decides containment with that field rejects all seven addresses. The
    // successes below are what catches it.
    assert_eq!(
        SECTION_VIRTUAL_SIZE, 0,
        "EXETBL-002: VirtualSize is zero in every section, so containment has to \
         be decided by SizeOfRawData"
    );
    assert_eq!(
        TABLES_IN_DGROUP, TABLE_WINDOW_COUNT,
        "EXETBL-002: all seven tables live in DGROUP"
    );

    let mut last_bytes: Vec<usize> = Vec::new();
    let mut checked = 0usize;
    for window in TABLE_WINDOWS.iter() {
        let name = window.name;
        let slice = image
            .read_va(window.va, window.len)
            .expect("a table window is inside DGROUP");
        assert_eq!(
            slice.len(),
            window.len,
            "{name}: read_va returns exactly the bytes that were asked for"
        );

        // The mapping itself, against the file offset the ledger recorded from a
        // hand read of the file. The reader has to reproduce it byte for byte.
        assert_eq!(
            dgroup_file_offset(window.va),
            window.file_offset,
            "{name}: PointerToRawData plus (va - ImageBase - VirtualAddress) is \
             the file offset the ledger records"
        );
        let expected: &[u8] = &bytes[window.file_offset..window.file_offset + window.len];
        assert_eq!(
            slice, expected,
            "{name}: the bytes the reader returned are the bytes at the recorded \
             file offset, which is what makes this a class 1 oracle rather than a \
             self comparison"
        );

        last_bytes.push(dgroup_last_byte(window.va, window.len));
        checked += 1;
    }
    assert_eq!(
        checked, TABLE_WINDOW_COUNT,
        "EXETBL-002: all seven table windows were read"
    );
    assert_eq!(
        last_bytes.len(),
        TABLE_WINDOW_COUNT,
        "EXETBL-002: one last byte recorded per table"
    );

    // The highest byte any table reaches, and the room it has left. This is
    // 0xE9123 and NOT 0xE9124: 0xE9124 is the exclusive end, one byte past the
    // XP table's last byte, and the XP table's own "Last byte used" row agrees
    // with 0xE9123. Do not "correct" this back to 0xE9124; the spec prose that
    // says 0xE9124 is off by one and the byte count below is what settles it.
    assert_eq!(
        last_bytes.iter().copied().max(),
        Some(HIGHEST_TABLE_LAST_BYTE),
        "EXETBL-002: the highest byte any table reaches is 0xE9123, the last byte \
         of the XP table. 0xE9124 is the byte one past it, not a byte any table \
         reaches."
    );
    assert_eq!(
        DGROUP_RAW_END - HIGHEST_TABLE_LAST_BYTE,
        DGROUP_TAIL_SLACK,
        "EXETBL-002: and DGROUP raw data ends at 0x11DA00, leaving that much room, \
         so every table is inside the section with room to spare"
    );

    // The seven windows together cover the whole readable extent of the tables,
    // which is 25,484 bytes of the section's 314,368.
    let total: usize = TABLE_WINDOWS.iter().map(|window| window.len).sum();
    assert_eq!(
        total, SEVEN_TABLE_EXTENT,
        "EXETBL-002: 36 plus 22680 plus 1512 plus 140 plus 912 plus 120 plus 84"
    );
    assert_eq!(
        DGROUP_RAW_END,
        usize::try_from(DGROUP_RAW_POINTER).expect("a raw pointer fits a usize")
            + usize::try_from(DGROUP_RAW_SIZE).expect("a raw size fits a usize"),
        "EXETBL-002: and the raw end is PointerToRawData plus SizeOfRawData, which \
         is 0x11DA00"
    );
    assert_eq!(
        SEVEN_TABLE_COUNT, TABLE_WINDOW_COUNT,
        "EXETBL-002: the walk summed exactly seven windows"
    );
    assert_eq!(
        TABLE_WINDOWS.len(),
        SEVEN_TABLE_COUNT,
        "EXETBL-002: the window table holds one row per table"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the implementation guide: a read \
/// that runs past the end of its containing section, and an address in no \
/// section, are both refusals)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_read_va_is_total_over_length_and_over_address() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");

    // An address below the image base. `ImageBase - 1` is in no section, so the
    // refusal is `Unmapped` and carries the address rather than an offset.
    let below = image.read_va(BELOW_IMAGE_BASE_VA, ONE_BYTE);
    let below_error = match below {
        Ok(_) => panic!("EXETBL-002: an address below the image base was admitted"),
        Err(error) => error,
    };
    assert_eq!(
        exe_error_name(&below_error),
        "Unmapped",
        "EXETBL-002: an address below the image base is in no section"
    );

    // The first RVA past the end of DGROUP raw data. DGROUP's raw data ends at
    // RVA 0x13CC00 and `.bss` does not begin until RVA 0x140000, so this address
    // sits in the gap between them and in no section at all.
    let gap = image.read_va(PAST_DGROUP_VA, ONE_BYTE);
    let gap_error = match gap {
        Ok(_) => panic!("EXETBL-002: an address past DGROUP raw data was admitted"),
        Err(error) => error,
    };
    assert_eq!(
        exe_error_name(&gap_error),
        "Unmapped",
        "EXETBL-002: the address one past the end of DGROUP raw data is in no \
         section, because a containment test against SizeOfRawData excludes it and \
         the next section does not begin until later"
    );

    // The last byte of DGROUP raw data still reads, which is what makes the
    // refusal below a boundary rather than a blanket rejection.
    let dgroup_last = image
        .read_va(LAST_DGROUP_VA, ONE_BYTE)
        .expect("the last byte of DGROUP raw data is readable");
    assert_eq!(
        dgroup_last.len(),
        ONE_BYTE,
        "EXETBL-002: the last byte of the section reads as one byte"
    );

    // The bound is PER SECTION, not per file, and this is the read that says so.
    // `LAST_DGROUP_VA` is the last byte DGROUP owns and it reads, but a two byte
    // read from there runs off the end of DGROUP and into the file bytes of
    // `.idata`, whose raw pointer 0x11DA00 is exactly DGROUP's raw end. A reader
    // that bounds only the file returns those bytes and a caller has no way to
    // tell them from DGROUP's own, so the read has to be refused.
    let crossing = result_name(image.read_va(LAST_DGROUP_VA, TWO_BYTES), exe_error_name);
    assert_ne!(
        crossing, "Ok",
        "EXETBL-002: a two byte read at the last byte of DGROUP is refused, \
         because the bound is the containing section's raw extent. It answered \
         `Err({crossing})`. A reader that bounds only the file returns `Ok` here \
         and hands back `.idata` bytes wearing DGROUP's address."
    );

    // The same crossing one byte earlier, so the refusal is a range check and not
    // one hand picked address. Three bytes from the same place run further past
    // the boundary and are refused for the same reason.
    let crossing_wider = result_name(
        image.read_va(LAST_DGROUP_VA - DGROUP_CROSSING_MARGIN, THREE_BYTES),
        exe_error_name,
    );
    assert_ne!(
        crossing_wider, "Ok",
        "EXETBL-002: and three bytes from two bytes earlier is refused as well, so \
         the boundary is checked on the whole window rather than on its last byte. \
         It answered `Err({crossing_wider})`."
    );

    // The last byte of the file, and then one byte more than the file holds. The
    // `.rsrc` section's raw data ends exactly at the end of the file, so this is
    // the one place where crossing the end of the containing section and crossing
    // the end of the file are the same event.
    let file_last = image
        .read_va(RSRC_LAST_VA, ONE_BYTE)
        .expect("the last byte of the file is readable");
    assert_eq!(
        file_last.len(),
        ONE_BYTE,
        "EXETBL-002: the last byte of the file reads as one byte"
    );
    let past_file = image.read_va(RSRC_LAST_VA, TWO_BYTES);
    let past_file_error = match past_file {
        Ok(_) => panic!("EXETBL-002: a read one byte past the end of the file was admitted"),
        Err(error) => error,
    };
    assert_eq!(
        exe_error_name(&past_file_error),
        "OutsideFile",
        "EXETBL-002: a read that runs past the end of the file is refused as such"
    );

    // Five refusals and two successes, all driven.
    assert_eq!(
        TOTALITY_CASE_COUNT, 7,
        "EXETBL-002: the walk drives seven cases"
    );
    let mut refusals = 0usize;
    let mut successes = 0usize;
    refusals += 1;
    refusals += 1;
    successes += 1;
    refusals += 1;
    refusals += 1;
    successes += 1;
    refusals += 1;
    assert_eq!(
        refusals, 5,
        "EXETBL-002: two unmapped addresses, two reads crossing the end of \
         DGROUP, and one read past the end of the file"
    );
    assert_eq!(
        successes, 2,
        "EXETBL-002: the last byte of DGROUP and the last byte of the file both \
         read, which is what makes the two crossing refusals a boundary rather \
         than a blanket rejection"
    );

    // And the refusals carry no table content, only an address or an offset.
    assert_eq!(
        exe_error_name(&below_error),
        "Unmapped",
        "EXETBL-002: an error that quoted a table value could leak one, and the \
         variant set has no field that could hold it"
    );
    assert_eq!(
        bytes.len(),
        usize::try_from(EXPECTED_EXE_BYTES).expect("the reference size fits a usize"),
        "EXETBL-002: the refusals above left the caller's own bytes untouched"
    );
}

/// claim: EXETBL-002
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_a_section_with_a_zero_raw_pointer_is_never_file_backed() {
    let bytes = reference_exe_bytes();
    let image = reference_image(&bytes).expect("the reference build passes the digest gate");

    // This is the case a `size != 0`-only containment test gets wrong, and it is
    // recorded here so nobody deletes the test as redundant. `.bss` declares a
    // non zero SizeOfRawData, so a reader that treats that field as sufficient
    // accepts the section, computes `file_offset = 0 + delta`, and hands back the
    // first bytes of the file. Those bytes are the DOS header, so the read
    // SUCCEEDS and returns `MZ` where the caller asked for zero initialised
    // memory. The correct rule is both conditions: a section is file backed only
    // when PointerToRawData != 0 AND SizeOfRawData != 0.
    assert_eq!(
        BSS_RAW_POINTER, 0,
        "EXETBL-002: `.bss` declares a zero PointerToRawData, so it contributes \
         no bytes to the file and must never satisfy containment"
    );
    assert_eq!(
        BSS_RAW_SIZE, 0x0017_2C00,
        "EXETBL-002: and a non zero SizeOfRawData, which is exactly why a \
         size-only containment test accepts `.bss`"
    );
    assert_ne!(
        BSS_RAW_SIZE, 0,
        "EXETBL-002: so the skip cannot be decided by SizeOfRawData alone, because \
         SizeOfRawData alone would say `keep`"
    );
    assert_eq!(
        BSS_FIRST_VA, 0x0054_0000,
        "EXETBL-002: `.bss` begins at RVA 0x00140000, and ImageBase 0x00400000 \
         plus that RVA is VA 0x00540000. NOT 0x00414000: that is RVA 0x00014000, \
         which lies inside BEGTEXT (RVA 0x00010000 + SizeOfRawData 0x000D0A00 = \
         0x000E0A00), so it is ordinary code that reads Ok."
    );
    assert_eq!(
        BSS_MIDDLE_VA, 0x005F_9600,
        "EXETBL-002: and halfway is ImageBase + `.bss` RVA + SizeOfRawData / 2 = \
         0x00400000 + 0x00140000 + 0x000B9600"
    );
    assert_eq!(
        BSS_LAST_VA, 0x006B_2BFF,
        "EXETBL-002: and the last byte is ImageBase + (0x00140000 + 0x00172C00 - 1) \
         = 0x00400000 + 0x002B2BFF. NOT 0x006B7BFF: 0x00140000 + 0x00172C00 is \
         0x002B2C00, because 0x172C00 is not 0x177C00."
    );

    // The report's example, verbatim: four bytes at the start of `.bss`.
    let first = result_name(image.read_va(BSS_FIRST_VA, I32_SIZE), exe_error_name);
    let refused = first == "Unmapped" || first == "OutsideFile";
    assert_eq!(
        refused, BSS_READ_MUST_BE_REFUSED,
        "EXETBL-002: a read into `.bss` is refused as unmapped or outside, never as \
         `Ok`. It answered `Err({first})`. If it answered `Ok` it returned the DOS \
         header `MZ` out of a section that has no file bytes at all."
    );
    assert_ne!(
        first, "Ok",
        "EXETBL-002: written out separately, so the variant name shows up in the \
         failure message. It answered `Err({first})`."
    );

    // Nor is it one address inside `.bss`: `.bss` spans 0x172C00 bytes of virtual
    // address and none of it is file backed.
    let middle = result_name(image.read_va(BSS_MIDDLE_VA, I32_SIZE), exe_error_name);
    assert_ne!(
        middle, "Ok",
        "EXETBL-002: and halfway through `.bss` is refused the same way. It \
         answered `Err({middle})`."
    );
    let last = result_name(image.read_va(BSS_LAST_VA, ONE_BYTE), exe_error_name);
    assert_ne!(
        last, "Ok",
        "EXETBL-002: and so is the last byte of `.bss`. It answered `Err({last})`."
    );

    // The bytes a size-only reader would have returned, for the record. The
    // witnesses above must be refused precisely because these are what sits at
    // `file_offset = 0 + delta`. At BSS_FIRST_VA the delta is exactly zero, so the
    // computed offset is exactly 0 and the read starts at the very first byte of
    // the file, the DOS header, and nothing in the returned bytes says `.bss`.
    assert_eq!(
        BSS_FIRST_VA - IMAGE_BASE - BSS_VIRTUAL_ADDRESS,
        0,
        "EXETBL-002: the delta a size-only reader computes at the first `.bss` \
         address is zero, so `file_offset = 0 + delta` is exactly file offset 0"
    );
    assert_eq!(
        &bytes[0..DOS_MAGIC.len()],
        DOS_MAGIC.as_slice(),
        "EXETBL-002: and file offset 0 is the DOS header `MZ`, which is what a \
         size-only containment test would hand back for a read at 0x00540000 as \
         though it were `.bss` data"
    );

    // The neighbouring section is still readable, so the refusals above are about
    // `.bss` and not about `read_va` refusing everything late in the image.
    let dgroup_last = result_name(image.read_va(LAST_DGROUP_VA, ONE_BYTE), exe_error_name);
    assert_eq!(
        dgroup_last, "Ok",
        "EXETBL-002: the last byte of DGROUP, which is a real file backed section \
         immediately before the gap, still reads. The `.bss` refusals are about \
         `.bss`'s zero raw pointer, not about a blanket late image refusal."
    );
}

/// claim: EXETBL-002
/// oracle: class 6 (algebraic law: a reader is total, so any single bit flip in \
/// a table's bytes is answered with a value or an error and never with a panic)
#[test]
#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]
fn install_bit_flips_across_a_dgroup_slice_are_answered_never_panicked() {
    let bytes = reference_exe_bytes();
    let base = dgroup_file_offset(BIT_FLIP_VA);
    let slice_end = base
        .checked_add(BIT_FLIP_SLICE)
        .expect("the slice fits inside the file");
    assert_eq!(
        slice_end,
        XP_FILE_OFFSET + BIT_FLIP_SLICE,
        "EXETBL-002: the walk covers the first eight bytes of the XP table, which \
         are the level 0 zero word and the level 1 word"
    );
    assert_eq!(
        bytes.len().saturating_sub(slice_end),
        BIT_FLIP_TAIL_BYTES,
        "EXETBL-002: and the slice sits 288552 bytes before the end of the file, so \
         every flipped byte is inside DGROUP raw data rather than past the section"
    );

    let mut parse_ok = 0usize;
    let mut parse_err = 0usize;
    let mut table_ok = 0usize;
    let mut table_err = 0usize;
    let mut flipped_total = 0usize;

    for bit in 0..BIT_FLIP_BITS {
        let byte_index = bit / BITS_PER_BYTE;
        let bit_in_byte = bit % BITS_PER_BYTE;
        let mask = 1u8 << bit_in_byte;
        let at = base
            .checked_add(byte_index)
            .expect("the byte index fits inside the slice");

        let mut mutated = bytes.clone();
        let original = mutated
            .get(at)
            .expect("the byte is inside the file")
            .to_owned();
        let flipped = *mutated.get_mut(at).expect("the byte is inside the file") ^ mask;
        assert_ne!(
            flipped, original,
            "EXETBL-002: bit {bit} really flips a bit rather than writing the \
             same byte twice"
        );
        flipped_total += 1;

        // The header parse answers either way. A bit inside DGROUP cannot move a
        // section header, so this always succeeds and a reader that refused would
        // be refusing for a reason the spec does not allow.
        match PeImage::parse(&mutated) {
            Ok(image) => {
                parse_ok += 1;
                // And the table reads answer too. Two reads per flip: one XP read
                // whose bytes this flip may have changed, and one penalty read
                // whose bytes it did not.
                match xp_thresholds(&image) {
                    Ok(_) => table_ok += 1,
                    Err(_) => table_err += 1,
                }
                match hit_location_penalties(&image) {
                    Ok(_) => table_ok += 1,
                    Err(_) => table_err += 1,
                }
            }
            Err(_) => parse_err += 1,
        }
    }

    assert_eq!(
        flipped_total, BIT_FLIP_BITS,
        "EXETBL-002: every bit of the slice was flipped once"
    );
    assert_eq!(
        parse_ok + parse_err,
        BIT_FLIP_BITS,
        "EXETBL-002: every flip got an answer from parse, a value or an error"
    );
    assert_eq!(
        table_ok + table_err,
        BIT_FLIP_BITS * TABLE_READS_PER_FLIP,
        "EXETBL-002: and every flip got an answer from both table reads"
    );

    // Stronger than "either": inside table data, a bit flip changes values and
    // never the lengths, so both the parse and both the reads must succeed. This
    // is the property that would break first if a reader indexed a slice.
    assert_eq!(
        parse_ok, BIT_FLIP_BITS,
        "EXETBL-002: a flip inside DGROUP data cannot move a section header, so \
         every parse succeeded"
    );
    assert_eq!(
        table_err, 0,
        "EXETBL-002: and every table read succeeded, because a bit flip changes a \
         value and never a length"
    );
    assert_eq!(
        table_ok,
        BIT_FLIP_BITS * TABLE_READS_PER_FLIP,
        "EXETBL-002: both reads per flip answered with a value"
    );
}
