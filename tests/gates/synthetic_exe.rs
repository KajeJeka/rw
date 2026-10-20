//! Synthetic gate tests for the executable layer tables (`fo1-exe`).
//!
//! Spec: `docs/spec/exe-tables.md`. Every fixture in this file is a PE image
//! built byte by byte here and named `synthetic_*`, so the suite needs no game
//! install and runs in CI.
//!
//! A synthetic fixture cannot support a CONFIRMED evidence tag (guide:
//! Fixtures and golden files), so **no test below can be oracle class 1, 2 or
//! 3**: none of those classes can be produced out of bytes this file invented.
//! Every test is class 5, a hand computed vector the spec states, or class 6,
//! an algebraic law about the addresses the crate publishes. The class 1 run
//! that lifts the bytes out of the original executable belongs to
//! `tests/gates/install_exe_tables.rs`.
//!
//! The oracle classes used below follow `docs/spec/trace.md`: oracle class 5
//! (hand computed vectors from the spec) and oracle class 6 (algebraic laws).
//!
//! ## The fixture
//!
//! One image is built, and it is built to reproduce the reference build's own
//! file layout: the same image base, the same section table offset, and the
//! same raw pointer for the section that holds the tables. That is what lets
//! the table tests assert the spec's own recorded file offsets
//! (`EXETBL-C200` to `EXETBL-C206`) instead of offsets this file invented.
//!
//! ```text
//!   file 0x00000000  "MZ", the DOS header
//!   file 0x0000003C  e_lfanew = 0x00000080
//!   file 0x00000080  PE signature 0x00004550
//!   file 0x00000084  COFF header, 3 sections
//!   file 0x00000098  PE32 optional header, image base 0x00400000
//!   file 0x00000178  section table, 3 headers of 40 bytes
//!   file 0x000D0E00  SYNTHD raw data, 0x18400 bytes, the seven tables live here
//!   file 0x000E9200  SYNTHB raw data, 0x200 bytes
//! ```
//!
//! Every section declares `VirtualSize == 0`, exactly as the reference build
//! does, so a reader that validated containment against the virtual size would
//! admit nothing at all (EXETBL-002). The third section declares a large
//! nonzero `SizeOfRawData` beside a zero `PointerToRawData`, which is the
//! uninitialised data case the spec calls out: it is not file backed, and a
//! read inside it must not hand back the DOS header.
//!
//! ## What the tables hold
//!
//! Where the spec states a value, the fixture carries that value and the test
//! asserts it. Where the spec states nothing, the fixture carries a pattern
//! whose six or seven fields are all different from one another, so a reader
//! that swapped or aliased two fields cannot pass. The pattern values are
//! written into the image as literal bytes; nothing in the test derives an
//! expected value from the code under test.

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
use std::sync::LazyLock;

use fo1_exe::{
    critical_failure, critical_success_critters, critical_success_player, hit_location_penalties,
    player_stat_limits, stat_limits, xp_thresholds, CriticalRecord, ExeError, PeImage, StatLimit,
};

// --- ledger constants ---------------------------------------------------------

/// Width of one `i32` field in the executable layer, in bytes (`EXETBL-C303`).
const I32_WIDTH: usize = 4;

/// Size of one 28-byte critical record (`EXETBL-C300`).
const CRITICAL_RECORD_SIZE: usize = 28;

/// Size of one 24-byte stat limit record (`EXETBL-C301`).
const STAT_LIMIT_SIZE: usize = 24;

/// The digest of the reference build (`EXETBL-001`).
const REFERENCE_SHA256: &str = "444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272";

/// The SHA-256 of the empty message, as published in FIPS 180-4 and in the
/// NIST CAVP vectors. Used as the observed digest of an empty buffer.
const SHA256_OF_EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// The SHA-256 of the three byte message `abc`, as published in FIPS 180-4.
const SHA256_OF_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

/// The SHA-256 of a 512 bit all zero message, the value any SHA-256
/// implementation gives for it.
const SHA256_OF_64_ZERO_BYTES: &str =
    "f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b";

/// The SHA-256 of the 448 bit message of FIPS 180-4, the two block example.
///
/// Published in FIPS 180-4, appendix B, and in the NIST CAVP vectors. It is a
/// public constant, not a game datum.
const SHA256_OF_448_BIT_MESSAGE: &str =
    "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1";

/// The empty message of FIPS 180-4.
const FIPS_EMPTY_MESSAGE: &[u8] = b"";

/// The three byte message `abc` of FIPS 180-4.
const FIPS_ABC_MESSAGE: &[u8] = b"abc";

/// The 448 bit message of FIPS 180-4, the two block example.
const FIPS_448_BIT_MESSAGE: &[u8] = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";

/// The image base the reference build declares (`EXETBL-C010`).
const SPEC_IMAGE_BASE: u32 = 0x0040_0000;

/// `e_lfanew` of the reference build (`EXETBL-C011`).
const SPEC_E_LFANEW: u32 = 128;

/// The section table of the reference build (`EXETBL-C012`).
const SPEC_SECTION_TABLE_OFFSET: usize = 376;

/// The virtual size every section of the reference build declares
/// (`EXETBL-C014`).
const SPEC_SECTION_VIRTUAL_SIZE: u32 = 0;

/// The size of one `IMAGE_SECTION_HEADER`.
///
/// No ledger row holds 40. `EXETBL-C015` is `SizeOfHeaders`, 1024 bytes, and
/// it names 40 only as "a separate, unrelated constant" in its note. The spec
/// fixes the 40 byte section header in the reader itself, at
/// `docs/spec/exe-tables.md:719`, which strides the section table by 40.
const SECTION_HEADER_SIZE: usize = 40;

/// The seven table addresses, as the spec records them (`EXETBL-C100` to
/// `EXETBL-C106`).
const SPEC_HIT_LOCATION_PENALTY_VA: u32 = 0x004F_EC90;
const SPEC_CRIT_SUCCESS_CRITTERS_VA: u32 = 0x004F_ECB4;
const SPEC_CRIT_SUCCESS_PLAYER_VA: u32 = 0x0050_454C;
const SPEC_CRIT_FAILURE_VA: u32 = 0x0050_4B3C;
const SPEC_STAT_LIMITS_VA: u32 = 0x0050_7EC8;
const SPEC_PLAYER_STAT_LIMITS_VA: u32 = 0x0050_8258;
const SPEC_XP_THRESHOLDS_VA: u32 = 0x0050_82D0;

/// The seven table file offsets, as the spec records them (`EXETBL-C200` to
/// `EXETBL-C206`). The fixture is built so that these are the offsets the
/// reader has to reach.
const SPEC_FILE_HIT_LOCATION_PENALTY: usize = 0x000D_FA90;
const SPEC_FILE_CRIT_SUCCESS_CRITTERS: usize = 0x000D_FAB4;
const SPEC_FILE_CRIT_SUCCESS_PLAYER: usize = 0x000E_534C;
const SPEC_FILE_CRIT_FAILURE: usize = 0x000E_593C;
const SPEC_FILE_STAT_LIMITS: usize = 0x000E_8CC8;
const SPEC_FILE_PLAYER_STAT_LIMITS: usize = 0x000E_9058;
const SPEC_FILE_XP_THRESHOLDS: usize = 0x000E_90D0;

/// The seven table extents, as the spec records them (`EXETBL-C500` to
/// `EXETBL-C506`).
const SPEC_HIT_LOCATION_PENALTY_LEN: usize = 36;
const SPEC_CRIT_SUCCESS_CRITTERS_LEN: usize = 22_680;
const SPEC_CRIT_SUCCESS_PLAYER_LEN: usize = 1_512;
const SPEC_CRIT_FAILURE_LEN: usize = 140;
const SPEC_STAT_LIMITS_LEN: usize = 912;
const SPEC_PLAYER_STAT_LIMITS_LEN: usize = 120;
const SPEC_XP_THRESHOLDS_LEN: usize = 84;

/// The seven table shapes, as the spec records them (`EXETBL-C400` to
/// `EXETBL-C413`).
const SPEC_HIT_LOCATION_PENALTY_COUNT: usize = 9;
const SPEC_CRITTER_CRITICAL_COUNT: usize = 810;
const SPEC_PLAYER_CRITICAL_COUNT: usize = 54;
const SPEC_CRITICAL_FAILURE_ROW_COUNT: usize = 7;
const SPEC_CRITICAL_FAILURE_COLUMNS: usize = 5;
const SPEC_STAT_LIMIT_COUNT: usize = 38;
const SPEC_PLAYER_STAT_LIMIT_COUNT: usize = 5;
const SPEC_XP_THRESHOLD_COUNT: usize = 21;

/// The three adjacency extents the spec states as class 1 oracles
/// (`EXETBL-C601` to `EXETBL-C603`).
const SPEC_CRITTER_CRITICAL_EXTENT: u32 = 22_680;
const SPEC_STAT_LIMITS_EXTENT: u32 = 912;
const SPEC_PLAYER_STAT_LIMITS_EXTENT: u32 = 120;

/// The scale the caller applies to an experience threshold (`EXETBL-C821`).
const SPEC_EXPERIENCE_SCALE: i32 = 1_000;

/// The first record of the critter critical table, as the spec records it
/// (`EXETBL-004`).
const SPEC_CRITTER_FIRST_RECORD: [i32; 7] = [4, 0, -1, 0, 0, 5001, 5000];

/// The last record of the critter critical table, as the spec records it
/// (`EXETBL-004`).
const SPEC_CRITTER_LAST_RECORD: [i32; 7] = [4, 2048, -1, 0, 0, 6402, 5000];

/// The first record of the player critical table, as the spec records it
/// (`EXETBL-005`).
const SPEC_PLAYER_FIRST_RECORD: [i32; 7] = [3, 0, -1, 0, 0, 6500, 5000];

/// The last record of the player critical table, as the spec records it
/// (`EXETBL-005`).
const SPEC_PLAYER_LAST_RECORD: [i32; 7] = [4, 2050, 6, 2, 128, 6503, 6513];

/// The nine hit location penalties, as the spec records them (`EXETBL-003`).
const SPEC_HIT_LOCATION_PENALTIES: [i32; 9] = [-40, -30, -30, 0, -20, -20, -60, -30, 0];

/// The seven critical failure rows, as the spec records them (`EXETBL-006`).
const SPEC_CRITICAL_FAILURE_ROWS: [[i32; 5]; 7] = [
    [0, 32_768, 32_768, 524_290, 2_097_152],
    [0, 32_768, 16_384, 1_048_576, 65_536],
    [0, 131_072, 16_384, 1_048_576, 8_192],
    [32_768, 163_840, 49_152, 1_048_576, 36_864],
    [262_144, 16_384, 540_672, 1_048_576, 4_096],
    [32_768, 262_144, 8_192, 1_048_576, 36_866],
    [0, 32_768, 1_048_576, 8_192, 37_888],
];

/// The field 2 values of the first seven stat limit records, as the spec
/// records them (`EXETBL-007`).
const SPEC_STAT_LIMIT_CORE_FIELD2: [i32; 7] = [0, 1, 2, 3, 4, 5, 6];

/// The stat limit record at index 16, as the spec records it (`EXETBL-007`).
const SPEC_STAT_LIMIT_RECORD_16: [i32; 6] = [0, 0, 94, 0, 100, 0];

/// The five player stat limit records, as the spec records them
/// (`EXETBL-008`): the minimum, the maximum and the default of each record,
/// in that order.
///
/// The spec states the minimum, the maximum and the default of all five
/// records, and says that field 0 and field 1 are zero in all five. It says
/// nothing about field 2 of these five records, so this table carries no
/// field 2 and no test asserts one.
const SPEC_PLAYER_STAT_LIMITS: [[i32; 3]; 5] = [
    [0, 2_147_483_647, 0],
    [1, 2_147_483_647, 1],
    [0, 2_147_483_647, 0],
    [-20, 20, 0],
    [0, 2_147_483_647, 0],
];

/// The field 2 value the fixture writes into the five player stat limit
/// records.
///
/// This is a fixture value and nothing else: the spec states no field 2 for
/// these five records, so no test asserts it.
const SYNTHETIC_PLAYER_STAT_LIMIT_FIELD2: i32 = 51;

/// The byte offset of the field one past the last experience threshold.
///
/// The table holds 21 little endian `i32` values (`EXETBL-009`), so it is 21
/// times 4 bytes long, and the field one past its last value sits at that
/// length. This is a byte offset, the unit every other `FieldOutOfRange` in
/// this crate and in the sibling crates uses.
///
/// The spec fixes the same number. `docs/spec/exe-tables.md:1061` writes
/// `offset: 84` in the `FieldOutOfRange` arm of `experience_for_level`, and
/// `docs/spec/exe-tables.md:1052` gives the reason: the table holds 21 `i32`
/// values of 4 bytes each, and the level out of range is the field after the
/// last one. The offset is a byte offset and not the table's virtual address,
/// because the variant's offset is a `usize` measured from the start of the
/// table (`docs/spec/exe-tables.md:630`). The number here is therefore the
/// table's byte length, which `EXETBL-C506` holds as 84 under claim
/// `EXETBL-009`.
const SPEC_XP_LEVEL_OUT_OF_RANGE_OFFSET: usize = 84;

/// The twenty one experience thresholds, as the spec records them
/// (`EXETBL-009`, `EXETBL-015`).
const SPEC_XP_THRESHOLDS: [i32; 21] = [
    0, 1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 66, 78, 91, 105, 120, 136, 153, 171, 190, 210,
];

// --- the synthetic image's own layout ----------------------------------------

/// The image base the synthetic fixture declares.
const SYNTHETIC_IMAGE_BASE: u32 = 0x0040_0000;

/// `e_lfanew` of the synthetic fixture: the PE signature sits at 0x80.
const SYNTHETIC_E_LFANEW: u32 = 0x80;

/// The PE signature the synthetic fixture declares.
const SYNTHETIC_PE_SIGNATURE: u32 = 0x0000_4550;

/// The COFF machine the synthetic fixture declares: i386.
const SYNTHETIC_MACHINE: u16 = 0x014C;

/// The number of sections the synthetic fixture declares.
const SYNTHETIC_SECTION_COUNT: u16 = 3;

/// The size of the optional header the synthetic fixture declares.
const SYNTHETIC_OPTIONAL_HEADER_SIZE: u16 = 224;

/// The optional header magic the synthetic fixture declares: PE32.
const SYNTHETIC_OPTIONAL_MAGIC: u16 = 0x010B;

/// The optional header magic of a PE32+ image, which the reader must refuse.
const SYNTHETIC_PE32_PLUS_MAGIC: u16 = 0x020B;

/// The COFF header offset of the synthetic fixture.
const SYNTHETIC_COFF_HEADER_OFFSET: usize = 0x84;

/// The optional header offset of the synthetic fixture.
const SYNTHETIC_OPTIONAL_HEADER_OFFSET: usize = 0x98;

/// The section table offset of the synthetic fixture.
const SYNTHETIC_SECTION_TABLE_OFFSET: usize = 0x178;

/// The offset of the image base inside the optional header.
const SYNTHETIC_IMAGE_BASE_OFFSET: usize = 0xB4;

/// The number of sections that cannot fit in the file, for the malformed
/// section count case.
const SYNTHETIC_IMPOSSIBLE_SECTION_COUNT: u16 = 0xFFFF;

/// The virtual address of the section that holds the tables, in RVA terms.
const SYNTHETIC_DATA_SECTION_RVA: u32 = 0x000F_0000;

/// The file pointer of the section that holds the tables.
const SYNTHETIC_DATA_SECTION_RAW_POINTER: u32 = 0x000D_0E00;

/// The file extent of the section that holds the tables.
const SYNTHETIC_DATA_SECTION_RAW_SIZE: u32 = 0x0001_8400;

/// The virtual address of the trailing section, in RVA terms.
const SYNTHETIC_TAIL_SECTION_RVA: u32 = 0x0030_0000;

/// The file pointer of the trailing section.
const SYNTHETIC_TAIL_SECTION_RAW_POINTER: u32 = 0x000E_9200;

/// The file extent of the trailing section.
const SYNTHETIC_TAIL_SECTION_RAW_SIZE: u32 = 0x0000_0200;

/// The virtual address of the uninitialised data section, in RVA terms. Its
/// first address is 0x00540000, the address the spec names.
const SYNTHETIC_UNINIT_SECTION_RVA: u32 = 0x0014_0000;

/// The file pointer of the uninitialised data section: zero, beside a large
/// nonzero file extent.
const SYNTHETIC_UNINIT_SECTION_RAW_POINTER: u32 = 0x0000_0000;

/// The file extent of the uninitialised data section: large and nonzero.
const SYNTHETIC_UNINIT_SECTION_RAW_SIZE: u32 = 0x0017_2C00;

/// The length of the synthetic image.
const SYNTHETIC_FILE_LEN: usize = 0x000E_9400;

/// The virtual address of the first byte of the data section.
const SYNTHETIC_DATA_SECTION_FIRST_VA: u32 = 0x004F_0000;

/// The virtual address of the last byte of the data section.
const SYNTHETIC_DATA_SECTION_LAST_VA: u32 = 0x0050_83FF;

/// The file offset of the last byte of the data section.
const SYNTHETIC_DATA_SECTION_LAST_OFFSET: usize = 0x000E_91FF;

/// The virtual address of the first byte of the uninitialised data section.
const SYNTHETIC_UNINIT_SECTION_FIRST_VA: u32 = 0x0054_0000;

/// The virtual address of the first byte of the trailing section.
const SYNTHETIC_TAIL_SECTION_FIRST_VA: u32 = 0x0070_0000;

/// A virtual address that falls in the gap between the data section and the
/// uninitialised data section, so it belongs to no section at all.
const SYNTHETIC_GAP_VA: u32 = 0x0050_9000;

/// The byte that fills the tail of the data section, past the last table.
const SYNTHETIC_TAIL_FILLER: u8 = 0xA5;

/// The byte that fills the trailing section, so a read that crossed the
/// section boundary would return something recognisable.
const SYNTHETIC_NEXT_SECTION_FILLER: u8 = 0x5A;

/// The first byte of a DOS header, which an uninitialised section read must
/// not return.
const SYNTHETIC_DOS_FIRST_BYTE: u8 = 0x4D;

/// The second byte of a DOS header, which an uninitialised section read must
/// not return.
const SYNTHETIC_DOS_SECOND_BYTE: u8 = 0x5A;

/// The length the image is truncated to in the truncated image case, which
/// stops before the section table.
const SYNTHETIC_TRUNCATED_LEN: usize = 0x0000_0100;

/// The length the image is truncated to in the impossible section count case,
/// which stops inside the first section header.
const SYNTHETIC_SHORT_SECTION_TABLE_LEN: usize = 0x0000_0190;

/// The file offset of the second section header.
const SYNTHETIC_SECOND_SECTION_HEADER_OFFSET: usize = 0x0000_01A0;

// --- the boundary fixture -----------------------------------------------------
//
// A second, smaller image, built for the one test that needs a section's end
// to be a boundary of its own. Its first section's raw data ends exactly where
// the next section's raw data begins, and a table is placed at the address that
// is exactly the first section's end. That end is exclusive, so the address
// belongs to no section at all.

/// The number of sections the boundary fixture declares.
const SYNTHETIC_BOUNDARY_SECTION_COUNT: u16 = 2;

/// The length of the boundary fixture.
const SYNTHETIC_BOUNDARY_FILE_LEN: usize = 0x0000_0480;

/// The virtual address of the section that holds the table, in RVA terms.
const SYNTHETIC_BOUNDARY_SECTION_RVA: u32 = 0x0001_0000;

/// The file pointer of the section that holds the table.
const SYNTHETIC_BOUNDARY_SECTION_RAW_POINTER: u32 = 0x0000_0400;

/// The file extent of the section that holds the table: 64 bytes.
const SYNTHETIC_BOUNDARY_SECTION_RAW_SIZE: u32 = 0x0000_0040;

/// The virtual address of the next section, in RVA terms. It is far enough
/// away that the boundary address falls in no section but the first one's end.
const SYNTHETIC_BOUNDARY_NEXT_SECTION_RVA: u32 = 0x0002_0000;

/// The file pointer of the next section: exactly where the first section's raw
/// data ends, so the file bytes of one section follow the other's with no gap.
const SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_POINTER: u32 = 0x0000_0440;

/// The file extent of the next section.
const SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_SIZE: u32 = 0x0000_0040;

/// The first address of the section that holds the table: the image base plus
/// the section's own address.
const SYNTHETIC_BOUNDARY_SECTION_FIRST_VA: u32 = 0x0041_0000;

/// The last address inside the section that holds the table.
const SYNTHETIC_BOUNDARY_SECTION_LAST_VA: u32 = 0x0041_003F;

/// The address exactly at the section's end, one past its last byte.
///
/// The section's file extent is 64 bytes, so this address is 64 bytes into it,
/// and the spec admits an address into a section only while that distance is
/// smaller than the file extent.
const SYNTHETIC_BOUNDARY_END_VA: u32 = 0x0041_0040;

/// The address just inside the section's end: the last four bytes of it.
const SYNTHETIC_BOUNDARY_INSIDE_VA: u32 = 0x0041_003C;

/// The file offset of the four bytes just inside the section's end.
const SYNTHETIC_BOUNDARY_INSIDE_OFFSET: usize = 0x0000_043C;

/// The byte the section that holds the table is filled with, so a read just
/// inside the end is recognisable.
const SYNTHETIC_BOUNDARY_FILLER: u8 = 0xA5;

/// The four bytes of the one i32 the table at the boundary holds.
///
/// They are written in the file where the first section's raw data ends, so
/// they are present in the file and outside every section's raw extent.
const SYNTHETIC_BOUNDARY_TABLE: [u8; 4] = [0x11, 0x22, 0x33, 0x44];

// --- the pattern values the fixture uses where the spec states nothing ------

/// The seven field values of every filler critical record. Each field carries a
/// different value, so a reader that swapped two fields cannot pass.
const SYNTHETIC_CRITICAL_PATTERN: [i32; 7] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77];

/// The six field values of every filler stat limit record. Each field carries
/// a different value, so a reader that swapped two fields cannot pass.
const SYNTHETIC_STAT_LIMIT_PATTERN: [i32; 6] = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66];

// --- the fixture --------------------------------------------------------------

/// One synthetic section header to write into the fixture.
struct SyntheticSection {
    /// The eight byte section name, already padded with zeros.
    name: [u8; 8],
    /// The section's virtual address, in RVA terms.
    virtual_address: u32,
    /// The section's virtual size. Every section of this fixture declares zero.
    virtual_size: u32,
    /// The section's file pointer.
    raw_pointer: u32,
    /// The section's file extent.
    raw_size: u32,
    /// The section's characteristics word.
    characteristics: u32,
}

/// The section that holds the seven tables.
const SYNTHETIC_DATA_SECTION: SyntheticSection = SyntheticSection {
    name: *b"SYNTHD\0\0",
    virtual_address: SYNTHETIC_DATA_SECTION_RVA,
    virtual_size: SPEC_SECTION_VIRTUAL_SIZE,
    raw_pointer: SYNTHETIC_DATA_SECTION_RAW_POINTER,
    raw_size: SYNTHETIC_DATA_SECTION_RAW_SIZE,
    characteristics: 0xC000_0040,
};

/// The section that follows the tables, so a read that crosses the section
/// boundary would land in it.
const SYNTHETIC_TAIL_SECTION: SyntheticSection = SyntheticSection {
    name: *b"SYNTHB\0\0",
    virtual_address: SYNTHETIC_TAIL_SECTION_RVA,
    virtual_size: SPEC_SECTION_VIRTUAL_SIZE,
    raw_pointer: SYNTHETIC_TAIL_SECTION_RAW_POINTER,
    raw_size: SYNTHETIC_TAIL_SECTION_RAW_SIZE,
    characteristics: 0xC000_0040,
};

/// The uninitialised data section: a large nonzero file extent beside a zero
/// file pointer.
const SYNTHETIC_UNINIT_SECTION: SyntheticSection = SyntheticSection {
    name: *b"SYNTHU\0\0",
    virtual_address: SYNTHETIC_UNINIT_SECTION_RVA,
    virtual_size: SPEC_SECTION_VIRTUAL_SIZE,
    raw_pointer: SYNTHETIC_UNINIT_SECTION_RAW_POINTER,
    raw_size: SYNTHETIC_UNINIT_SECTION_RAW_SIZE,
    characteristics: 0xC000_0080,
};

/// The section that holds the table in the boundary fixture. Its file extent
/// and its file pointer are both nonzero, so it is file backed.
const SYNTHETIC_BOUNDARY_SECTION: SyntheticSection = SyntheticSection {
    name: *b"SYNTHA\0\0",
    virtual_address: SYNTHETIC_BOUNDARY_SECTION_RVA,
    virtual_size: SPEC_SECTION_VIRTUAL_SIZE,
    raw_pointer: SYNTHETIC_BOUNDARY_SECTION_RAW_POINTER,
    raw_size: SYNTHETIC_BOUNDARY_SECTION_RAW_SIZE,
    characteristics: 0xC000_0040,
};

/// The section whose file bytes begin exactly where the first section's raw
/// data ends.
const SYNTHETIC_BOUNDARY_NEXT_SECTION: SyntheticSection = SyntheticSection {
    name: *b"SYNTHB\0\0",
    virtual_address: SYNTHETIC_BOUNDARY_NEXT_SECTION_RVA,
    virtual_size: SPEC_SECTION_VIRTUAL_SIZE,
    raw_pointer: SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_POINTER,
    raw_size: SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_SIZE,
    characteristics: 0xC000_0040,
};

/// Writes a little endian `u16` at `at`.
fn synthetic_put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

/// Writes a little endian `u32` at `at`.
fn synthetic_put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// Writes a little endian `i32` at `at`.
fn synthetic_put_i32(bytes: &mut [u8], at: usize, value: i32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// Writes one 40-byte section header at `at`.
///
/// The reloc and line number fields stay zero, because no reader under test
/// looks at them and the spec does not state them.
fn synthetic_put_section(bytes: &mut [u8], at: usize, section: &SyntheticSection) {
    bytes[at..at + 8].copy_from_slice(&section.name);
    synthetic_put_u32(bytes, at + 8, section.virtual_size);
    synthetic_put_u32(bytes, at + 12, section.virtual_address);
    synthetic_put_u32(bytes, at + 16, section.raw_size);
    synthetic_put_u32(bytes, at + 20, section.raw_pointer);
    synthetic_put_u32(bytes, at + 36, section.characteristics);
}

/// Writes `values` as little endian `i32` fields, one record every `stride`
/// bytes, starting at `at`.
fn synthetic_put_records(bytes: &mut [u8], at: usize, stride: usize, values: &[i32]) {
    for (index, value) in values.iter().enumerate() {
        synthetic_put_i32(bytes, at + index * stride, *value);
    }
}

/// Writes `count` filler critical records at `at`, each carrying the seven
/// distinct pattern values.
fn synthetic_put_critical_pattern(bytes: &mut [u8], at: usize, count: usize) {
    for index in 0..count {
        synthetic_put_records(
            bytes,
            at + index * CRITICAL_RECORD_SIZE,
            I32_WIDTH,
            &SYNTHETIC_CRITICAL_PATTERN,
        );
    }
}

/// Writes `count` filler stat limit records at `at`, each carrying the six
/// distinct pattern values.
fn synthetic_put_stat_limit_pattern(bytes: &mut [u8], at: usize, count: usize) {
    for index in 0..count {
        synthetic_put_records(
            bytes,
            at + index * STAT_LIMIT_SIZE,
            I32_WIDTH,
            &SYNTHETIC_STAT_LIMIT_PATTERN,
        );
    }
}

/// Writes the critter critical table: the first record and the last record
/// carry the values the spec records, every record between them carries the
/// seven distinct pattern values.
fn synthetic_put_critter_criticals(bytes: &mut [u8], at: usize) {
    synthetic_put_records(bytes, at, I32_WIDTH, &SPEC_CRITTER_FIRST_RECORD);
    synthetic_put_critical_pattern(
        bytes,
        at + CRITICAL_RECORD_SIZE,
        SPEC_CRITTER_CRITICAL_COUNT - 2,
    );
    synthetic_put_records(
        bytes,
        at + (SPEC_CRITTER_CRITICAL_COUNT - 1) * CRITICAL_RECORD_SIZE,
        I32_WIDTH,
        &SPEC_CRITTER_LAST_RECORD,
    );
}

/// Writes the player critical table: the first record and the last record
/// carry the values the spec records, every record between them carries the
/// seven distinct pattern values.
fn synthetic_put_player_criticals(bytes: &mut [u8], at: usize) {
    synthetic_put_records(bytes, at, I32_WIDTH, &SPEC_PLAYER_FIRST_RECORD);
    synthetic_put_critical_pattern(
        bytes,
        at + CRITICAL_RECORD_SIZE,
        SPEC_PLAYER_CRITICAL_COUNT - 2,
    );
    synthetic_put_records(
        bytes,
        at + (SPEC_PLAYER_CRITICAL_COUNT - 1) * CRITICAL_RECORD_SIZE,
        I32_WIDTH,
        &SPEC_PLAYER_LAST_RECORD,
    );
}

/// Writes the 38 stat limit records.
///
/// Records 0 to 6 are the seven core attributes the spec describes. Record 16
/// is the record the anti deviation target names. Every other record carries
/// the six distinct pattern values, so a reader that mis-strides lands on a
/// record whose fields cannot be mistaken for a neighbour's.
fn synthetic_put_stat_limits(bytes: &mut [u8], at: usize) {
    for (index, field2) in SPEC_STAT_LIMIT_CORE_FIELD2.iter().enumerate() {
        synthetic_put_records(
            bytes,
            at + index * STAT_LIMIT_SIZE,
            I32_WIDTH,
            &[0, 0, *field2, 1, 10, 5],
        );
    }
    synthetic_put_stat_limit_pattern(bytes, at + 7 * STAT_LIMIT_SIZE, 9);
    synthetic_put_records(
        bytes,
        at + 16 * STAT_LIMIT_SIZE,
        I32_WIDTH,
        &SPEC_STAT_LIMIT_RECORD_16,
    );
    synthetic_put_stat_limit_pattern(bytes, at + 17 * STAT_LIMIT_SIZE, 21);
}

/// Writes the five player stat limit records.
///
/// The minimum, the maximum and the default are the ones the spec states
/// (`EXETBL-008`). Field 0 and field 1 are zero because the spec says they
/// are zero in all five records. Field 2 carries the fixture's own value,
/// because the spec states none.
fn synthetic_put_player_stat_limits(bytes: &mut [u8], at: usize) {
    for (index, record) in SPEC_PLAYER_STAT_LIMITS.iter().enumerate() {
        synthetic_put_records(
            bytes,
            at + index * STAT_LIMIT_SIZE,
            I32_WIDTH,
            &[
                0,
                0,
                SYNTHETIC_PLAYER_STAT_LIMIT_FIELD2,
                record[0],
                record[1],
                record[2],
            ],
        );
    }
}

/// Builds the synthetic PE image.
///
/// The layout reproduces the reference build's own file offsets, so the
/// seven tables land exactly where the spec says they are (`EXETBL-C200` to
/// `EXETBL-C206`).
fn synthetic_build_image() -> Vec<u8> {
    let mut bytes = vec![0u8; SYNTHETIC_FILE_LEN];

    // DOS header: the "MZ" magic and the pointer to the PE signature.
    bytes[0] = SYNTHETIC_DOS_FIRST_BYTE;
    bytes[1] = SYNTHETIC_DOS_SECOND_BYTE;
    synthetic_put_u32(&mut bytes, 0x3C, SYNTHETIC_E_LFANEW);

    // PE signature and COFF header.
    synthetic_put_u32(&mut bytes, 0x80, SYNTHETIC_PE_SIGNATURE);
    synthetic_put_u16(&mut bytes, SYNTHETIC_COFF_HEADER_OFFSET, SYNTHETIC_MACHINE);
    synthetic_put_u16(
        &mut bytes,
        SYNTHETIC_COFF_HEADER_OFFSET + 2,
        SYNTHETIC_SECTION_COUNT,
    );
    synthetic_put_u16(
        &mut bytes,
        SYNTHETIC_COFF_HEADER_OFFSET + 16,
        SYNTHETIC_OPTIONAL_HEADER_SIZE,
    );

    // Optional header: PE32 magic and the image base.
    synthetic_put_u16(
        &mut bytes,
        SYNTHETIC_OPTIONAL_HEADER_OFFSET,
        SYNTHETIC_OPTIONAL_MAGIC,
    );
    synthetic_put_u32(
        &mut bytes,
        SYNTHETIC_IMAGE_BASE_OFFSET,
        SYNTHETIC_IMAGE_BASE,
    );

    // Section table.
    synthetic_put_section(
        &mut bytes,
        SYNTHETIC_SECTION_TABLE_OFFSET,
        &SYNTHETIC_DATA_SECTION,
    );
    synthetic_put_section(
        &mut bytes,
        SYNTHETIC_SECTION_TABLE_OFFSET + SECTION_HEADER_SIZE,
        &SYNTHETIC_TAIL_SECTION,
    );
    synthetic_put_section(
        &mut bytes,
        SYNTHETIC_SECTION_TABLE_OFFSET + 2 * SECTION_HEADER_SIZE,
        &SYNTHETIC_UNINIT_SECTION,
    );

    // The seven tables, at the file offsets the spec records.
    synthetic_put_records(
        &mut bytes,
        SPEC_FILE_HIT_LOCATION_PENALTY,
        I32_WIDTH,
        &SPEC_HIT_LOCATION_PENALTIES,
    );
    synthetic_put_critter_criticals(&mut bytes, SPEC_FILE_CRIT_SUCCESS_CRITTERS);
    synthetic_put_player_criticals(&mut bytes, SPEC_FILE_CRIT_SUCCESS_PLAYER);
    for (index, row) in SPEC_CRITICAL_FAILURE_ROWS.iter().enumerate() {
        synthetic_put_records(
            &mut bytes,
            SPEC_FILE_CRIT_FAILURE + index * SPEC_CRITICAL_FAILURE_COLUMNS * I32_WIDTH,
            I32_WIDTH,
            row,
        );
    }
    synthetic_put_stat_limits(&mut bytes, SPEC_FILE_STAT_LIMITS);
    synthetic_put_player_stat_limits(&mut bytes, SPEC_FILE_PLAYER_STAT_LIMITS);
    synthetic_put_records(
        &mut bytes,
        SPEC_FILE_XP_THRESHOLDS,
        I32_WIDTH,
        &SPEC_XP_THRESHOLDS,
    );

    // The tail of the data section past the last table, and the section that
    // follows it. Both are filled with a byte of their own, so a read that
    // crossed the section boundary would return something recognisable.
    for byte in bytes
        [SPEC_FILE_XP_THRESHOLDS + SPEC_XP_THRESHOLDS_LEN..SYNTHETIC_DATA_SECTION_LAST_OFFSET + 1]
        .iter_mut()
    {
        *byte = SYNTHETIC_TAIL_FILLER;
    }
    for byte in bytes[SYNTHETIC_TAIL_SECTION_RAW_POINTER as usize..SYNTHETIC_FILE_LEN].iter_mut() {
        *byte = SYNTHETIC_NEXT_SECTION_FILLER;
    }

    bytes
}

/// The one synthetic image every test in this file reads.
static SYNTHETIC_IMAGE: LazyLock<Vec<u8>> = LazyLock::new(synthetic_build_image);

/// Builds the boundary fixture.
///
/// The layout is the one the spec describes for the reference build's own
/// `DGROUP` and `.idata`: the first section's raw data ends exactly where the
/// next section's raw data begins, so the file offset at the first section's
/// end is the next section's first byte (`EXETBL-002`). The table is placed at
/// the address that is exactly the first section's end, and its bytes are
/// written there in the file.
fn synthetic_build_boundary_image() -> Vec<u8> {
    let mut bytes = vec![0u8; SYNTHETIC_BOUNDARY_FILE_LEN];

    // DOS header: the "MZ" magic and the pointer to the PE signature.
    bytes[0] = SYNTHETIC_DOS_FIRST_BYTE;
    bytes[1] = SYNTHETIC_DOS_SECOND_BYTE;
    synthetic_put_u32(&mut bytes, 0x3C, SYNTHETIC_E_LFANEW);

    // PE signature and COFF header.
    synthetic_put_u32(&mut bytes, 0x80, SYNTHETIC_PE_SIGNATURE);
    synthetic_put_u16(&mut bytes, SYNTHETIC_COFF_HEADER_OFFSET, SYNTHETIC_MACHINE);
    synthetic_put_u16(
        &mut bytes,
        SYNTHETIC_COFF_HEADER_OFFSET + 2,
        SYNTHETIC_BOUNDARY_SECTION_COUNT,
    );
    synthetic_put_u16(
        &mut bytes,
        SYNTHETIC_COFF_HEADER_OFFSET + 16,
        SYNTHETIC_OPTIONAL_HEADER_SIZE,
    );

    // Optional header: PE32 magic and the image base.
    synthetic_put_u16(
        &mut bytes,
        SYNTHETIC_OPTIONAL_HEADER_OFFSET,
        SYNTHETIC_OPTIONAL_MAGIC,
    );
    synthetic_put_u32(
        &mut bytes,
        SYNTHETIC_IMAGE_BASE_OFFSET,
        SYNTHETIC_IMAGE_BASE,
    );

    // Section table.
    synthetic_put_section(
        &mut bytes,
        SYNTHETIC_SECTION_TABLE_OFFSET,
        &SYNTHETIC_BOUNDARY_SECTION,
    );
    synthetic_put_section(
        &mut bytes,
        SYNTHETIC_SECTION_TABLE_OFFSET + SECTION_HEADER_SIZE,
        &SYNTHETIC_BOUNDARY_NEXT_SECTION,
    );

    // The section that holds the table is filled with its own byte, so a read
    // just inside its end is recognisable.
    for byte in bytes[SYNTHETIC_BOUNDARY_SECTION_RAW_POINTER as usize
        ..SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_POINTER as usize]
        .iter_mut()
    {
        *byte = SYNTHETIC_BOUNDARY_FILLER;
    }

    // The table's own bytes sit in the file where the first section's raw data
    // ends, which is the first byte of the next section's raw data. They are
    // in the file and in no section's raw extent.
    bytes[SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_POINTER as usize
        ..SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_POINTER as usize + 4]
        .copy_from_slice(&SYNTHETIC_BOUNDARY_TABLE);

    bytes
}

/// Asserts that no two of the named values are equal.
///
/// A record whose accessors were swapped or aliased would make two of these
/// equal, which is exactly what the check is for.
fn synthetic_assert_all_distinct(values: &[(&'static str, i32)]) {
    for (index, (left_name, left)) in values.iter().enumerate() {
        for (right_name, right) in values.iter().skip(index + 1) {
            assert_ne!(
                left, right,
                "{} and {} must not read the same field",
                left_name, right_name
            );
        }
    }
}

/// The outcome of one boundary case: the read was accepted and returned these
/// bytes, or it was refused with exactly this error.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SyntheticBoundaryOutcome {
    /// The read was accepted and returned exactly these bytes.
    Accepted(Vec<u8>),
    /// The read was refused. The error is carried whole, payload included, so
    /// a refusal for the wrong reason cannot pass.
    Refused(ExeError),
}

/// One boundary case: the address and the length to read, and the outcome the
/// spec's section rule produces for it.
#[derive(Debug, Clone)]
struct SyntheticBoundaryCase {
    /// What the case is about, for the failure message.
    label: &'static str,
    /// The virtual address to read at.
    va: u32,
    /// How many bytes to ask for.
    len: usize,
    /// The outcome the spec's rule produces.
    expected: SyntheticBoundaryOutcome,
}

/// The boundary cases the section rule of `EXETBL-002` decides.
///
/// The last byte of the section that holds the tables reads, and the read one
/// byte longer does not, because a read is bounded by its section and not
/// only by the file. The two addresses that fall in no file backed section are
/// refused as unmapped, and the first byte of the section that follows the
/// tables reads, because that section is file backed.
fn synthetic_boundary_cases() -> Vec<SyntheticBoundaryCase> {
    vec![
        SyntheticBoundaryCase {
            label: "the first byte of the section that holds the tables",
            va: SYNTHETIC_DATA_SECTION_FIRST_VA,
            len: 1,
            expected: SyntheticBoundaryOutcome::Accepted(vec![0]),
        },
        SyntheticBoundaryCase {
            label: "the last byte of the section that holds the tables",
            va: SYNTHETIC_DATA_SECTION_LAST_VA,
            len: 1,
            expected: SyntheticBoundaryOutcome::Accepted(vec![SYNTHETIC_TAIL_FILLER]),
        },
        SyntheticBoundaryCase {
            label: "two bytes from the last byte of that section",
            va: SYNTHETIC_DATA_SECTION_LAST_VA,
            len: 2,
            expected: SyntheticBoundaryOutcome::Refused(ExeError::OutsideFile {
                offset: SYNTHETIC_DATA_SECTION_LAST_OFFSET,
                len: 2,
            }),
        },
        SyntheticBoundaryCase {
            label: "three bytes from one byte before that section's last byte",
            va: SYNTHETIC_DATA_SECTION_LAST_VA - 1,
            len: 3,
            expected: SyntheticBoundaryOutcome::Refused(ExeError::OutsideFile {
                offset: SYNTHETIC_DATA_SECTION_LAST_OFFSET - 1,
                len: 3,
            }),
        },
        SyntheticBoundaryCase {
            label: "an address in the gap between two sections",
            va: SYNTHETIC_GAP_VA,
            len: 1,
            expected: SyntheticBoundaryOutcome::Refused(ExeError::Unmapped {
                va: SYNTHETIC_GAP_VA,
            }),
        },
        SyntheticBoundaryCase {
            label: "the first address of the uninitialised data section",
            va: SYNTHETIC_UNINIT_SECTION_FIRST_VA,
            len: 1,
            expected: SyntheticBoundaryOutcome::Refused(ExeError::Unmapped {
                va: SYNTHETIC_UNINIT_SECTION_FIRST_VA,
            }),
        },
        SyntheticBoundaryCase {
            label: "the first byte of the section that follows the tables",
            va: SYNTHETIC_TAIL_SECTION_FIRST_VA,
            len: 1,
            expected: SyntheticBoundaryOutcome::Accepted(vec![SYNTHETIC_NEXT_SECTION_FILLER]),
        },
    ]
}

// --- tests --------------------------------------------------------------------

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: every section declares a zero virtual size, and containment is decided by the raw fields)
#[test]
fn synthetic_every_section_declares_a_zero_virtual_size_and_containment_still_works() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let data = image
        .section_for(SYNTHETIC_DATA_SECTION_FIRST_VA)
        .expect("the section that holds the tables owns its first address");
    assert_eq!(
        data.name, *b"SYNTHD\0\0",
        "the data section carries its own name"
    );
    assert_eq!(
        data.virtual_size, SPEC_SECTION_VIRTUAL_SIZE,
        "the data section declares a zero virtual size"
    );
    assert_eq!(
        data.virtual_address, SYNTHETIC_DATA_SECTION_RVA,
        "the data section keeps its virtual address"
    );
    assert_eq!(
        data.raw_pointer, SYNTHETIC_DATA_SECTION_RAW_POINTER,
        "the data section keeps its file pointer"
    );
    assert_eq!(
        data.raw_size, SYNTHETIC_DATA_SECTION_RAW_SIZE,
        "the data section keeps its file extent"
    );

    let tail = image
        .section_for(SYNTHETIC_TAIL_SECTION_FIRST_VA)
        .expect("the trailing section owns its first address");
    assert_eq!(
        tail.virtual_size, SPEC_SECTION_VIRTUAL_SIZE,
        "the trailing section declares a zero virtual size"
    );
    assert_eq!(
        tail.raw_pointer, SYNTHETIC_TAIL_SECTION_RAW_POINTER,
        "the trailing section keeps its file pointer"
    );

    // Containment has to be decided by the raw fields. The last byte of the
    // data section is 0x183FF bytes into it, and a reader that asked the
    // virtual size would refuse every address in this build, because every
    // virtual size here is zero.
    let last_byte = image
        .read_va(SYNTHETIC_DATA_SECTION_LAST_VA, 1)
        .expect("the last byte of the data section is inside the section");
    assert_eq!(
        last_byte,
        &[SYNTHETIC_TAIL_FILLER],
        "the last byte of the data section reads back as the byte the fixture wrote"
    );

    let first_byte = image
        .read_va(SYNTHETIC_DATA_SECTION_FIRST_VA, 1)
        .expect("the first byte of the data section is inside the section");
    assert_eq!(
        first_byte,
        &[0u8],
        "the first byte of the data section is the padding before the tables"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: a section is file backed only when both raw fields are nonzero)
#[test]
fn synthetic_a_section_with_a_zero_raw_pointer_is_never_file_backed() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    // The good twin: a section with a nonzero file pointer owns its first
    // address.
    assert!(
        image.section_for(SYNTHETIC_DATA_SECTION_FIRST_VA).is_some(),
        "a section with a nonzero file pointer is file backed"
    );

    // The uninitialised data section declares a large nonzero file extent
    // beside a zero file pointer. Its first address is 0x00540000.
    assert_eq!(
        image.section_for(SYNTHETIC_UNINIT_SECTION_FIRST_VA),
        None,
        "a section with a zero file pointer owns no address"
    );

    let refused = image.read_va(SYNTHETIC_UNINIT_SECTION_FIRST_VA, 1);
    assert_eq!(
        refused,
        Err(ExeError::Unmapped {
            va: SYNTHETIC_UNINIT_SECTION_FIRST_VA
        }),
        "a read inside the uninitialised section is refused"
    );

    // The refusal is not vacuous: the DOS header really is at the front of the
    // file, so a reader that mapped the uninitialised section onto file offset
    // zero would hand these two bytes back as data.
    assert_eq!(
        &bytes[0..2],
        &[SYNTHETIC_DOS_FIRST_BYTE, SYNTHETIC_DOS_SECOND_BYTE],
        "the DOS header really is the first thing in the file"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: a read is bounded by the section that contains the address)
#[test]
fn synthetic_a_read_that_leaves_its_section_is_refused() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    // The good twin: the last byte of the data section reads.
    let last_byte = image
        .read_va(SYNTHETIC_DATA_SECTION_LAST_VA, 1)
        .expect("the last byte of the data section reads");
    assert_eq!(
        last_byte,
        &[SYNTHETIC_TAIL_FILLER],
        "the last byte of the data section reads back as the byte the fixture wrote"
    );

    // The bad twin: two bytes from the last byte would run into the section
    // that follows, so the reader must refuse rather than complete the read
    // from the next section's bytes.
    assert_eq!(
        image.read_va(SYNTHETIC_DATA_SECTION_LAST_VA, 2),
        Err(ExeError::OutsideFile {
            offset: SYNTHETIC_DATA_SECTION_LAST_OFFSET,
            len: 2,
        }),
        "two bytes from the last byte of the section are refused"
    );

    // The byte the refused read would have taken is the first byte of the
    // following section, so the refusal really stopped a plausible read.
    assert_eq!(
        bytes[SYNTHETIC_TAIL_SECTION_RAW_POINTER as usize], SYNTHETIC_NEXT_SECTION_FILLER,
        "the next section holds the byte the refused read would have taken"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: a read that falls in no file backed section is refused)
#[test]
fn synthetic_a_read_that_falls_in_no_section_is_refused() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    // The good twin: an address inside a file backed section reads.
    assert!(
        image.read_va(SYNTHETIC_DATA_SECTION_FIRST_VA, 1).is_ok(),
        "an address inside a file backed section reads"
    );

    // The bad twin: an address in the gap between the data section and the
    // uninitialised data section belongs to no section.
    assert_eq!(
        image.section_for(SYNTHETIC_GAP_VA),
        None,
        "an address in the gap between two sections belongs to no section"
    );
    assert_eq!(
        image.read_va(SYNTHETIC_GAP_VA, 1),
        Err(ExeError::Unmapped {
            va: SYNTHETIC_GAP_VA
        }),
        "a read of an unmapped address is refused"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: a virtual address is contained in a section only while the distance into the section is smaller than the file extent, so a section's end is exclusive)
#[test]
fn synthetic_the_address_at_the_end_of_a_section_belongs_to_no_section() {
    let bytes = synthetic_build_boundary_image();
    let image = PeImage::parse(&bytes).expect("the boundary image is a PE image");

    // The section that holds the table is file backed: its file extent and its
    // file pointer are both nonzero, so the rule admits the addresses inside
    // it.
    let section = image
        .section_for(SYNTHETIC_BOUNDARY_SECTION_FIRST_VA)
        .expect("the first address of the section is inside the section");
    assert_eq!(
        section.name, *b"SYNTHA\0\0",
        "the section that holds the table carries its own name"
    );
    assert_eq!(
        section.raw_size, SYNTHETIC_BOUNDARY_SECTION_RAW_SIZE,
        "the section that holds the table declares its file extent"
    );
    assert_eq!(
        section.raw_pointer, SYNTHETIC_BOUNDARY_SECTION_RAW_POINTER,
        "the section that holds the table declares its file pointer"
    );
    assert_ne!(
        section.raw_size, 0,
        "the section that holds the table declares a nonzero file extent"
    );
    assert_ne!(
        section.raw_pointer, 0,
        "the section that holds the table declares a nonzero file pointer"
    );

    // The good twin: the address just inside the section's end is inside the
    // section. The section's file extent is 0x40 bytes and its file pointer is
    // 0x400, so the address 0x0041003C is 0x3C bytes into the section and the
    // file offset 0x43C.
    let inside = image
        .read_va(SYNTHETIC_BOUNDARY_INSIDE_VA, 4)
        .expect("the address just inside the section's end is inside the section");
    assert_eq!(
        inside, &[SYNTHETIC_BOUNDARY_FILLER; 4],
        "the last four bytes of the section read back as the bytes the fixture wrote"
    );
    assert_eq!(
        &bytes[SYNTHETIC_BOUNDARY_INSIDE_OFFSET..SYNTHETIC_BOUNDARY_INSIDE_OFFSET + 4],
        &[SYNTHETIC_BOUNDARY_FILLER; 4],
        "the four bytes just inside the section's end are the bytes the fixture wrote"
    );

    // The bad twin: the address exactly at the section's end is 0x40 bytes
    // into the section, and the spec admits an address into a section only
    // while that distance is smaller than the file extent. The end is
    // exclusive, so no section holds that address, and the next section is
    // far enough away that the address is in no section at all.
    assert_eq!(
        SYNTHETIC_BOUNDARY_SECTION_LAST_VA + 1,
        SYNTHETIC_BOUNDARY_END_VA,
        "the boundary address is the address one past the section's last byte"
    );
    assert_eq!(
        image
            .section_for(SYNTHETIC_BOUNDARY_SECTION_LAST_VA)
            .map(|header| header.name),
        Some(*b"SYNTHA\0\0"),
        "the last byte of the section is inside the section"
    );
    assert_eq!(
        image.section_for(SYNTHETIC_BOUNDARY_END_VA),
        None,
        "the address exactly at the end of a section is in no section"
    );
    assert_eq!(
        image.read_va(SYNTHETIC_BOUNDARY_END_VA, 4),
        Err(ExeError::Unmapped {
            va: SYNTHETIC_BOUNDARY_END_VA,
        }),
        "a table read at the address exactly at the section's end is refused as unmapped"
    );

    // The refusal is not vacuous: the table's bytes really are in the file, at
    // the offset where the section's raw data ends, so a reader that admitted
    // the boundary address would have handed these bytes back as table data.
    assert_eq!(
        &bytes[SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_POINTER as usize
            ..SYNTHETIC_BOUNDARY_NEXT_SECTION_RAW_POINTER as usize + 4],
        &SYNTHETIC_BOUNDARY_TABLE,
        "the table's bytes are in the file, past the section's raw data"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: the seven tables live at the file offsets the spec records)
#[test]
fn synthetic_the_seven_tables_are_read_from_the_documented_file_offsets() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    // The first four bytes of each table, as the spec records them, read back
    // through the virtual address the spec records. A reader that mapped the
    // address anywhere else would return different bytes.
    assert_eq!(
        image
            .read_va(fo1_exe::HIT_LOCATION_PENALTY_VA, I32_WIDTH)
            .expect("the hit location penalty table is readable"),
        &bytes[SPEC_FILE_HIT_LOCATION_PENALTY..SPEC_FILE_HIT_LOCATION_PENALTY + I32_WIDTH],
        "the hit location penalty table is read from its documented file offset"
    );
    assert_eq!(
        image
            .read_va(fo1_exe::CRIT_SUCCESS_CRITTERS_VA, I32_WIDTH)
            .expect("the critter critical table is readable"),
        &bytes[SPEC_FILE_CRIT_SUCCESS_CRITTERS..SPEC_FILE_CRIT_SUCCESS_CRITTERS + I32_WIDTH],
        "the critter critical table is read from its documented file offset"
    );
    assert_eq!(
        image
            .read_va(fo1_exe::CRIT_SUCCESS_PLAYER_VA, I32_WIDTH)
            .expect("the player critical table is readable"),
        &bytes[SPEC_FILE_CRIT_SUCCESS_PLAYER..SPEC_FILE_CRIT_SUCCESS_PLAYER + I32_WIDTH],
        "the player critical table is read from its documented file offset"
    );
    assert_eq!(
        image
            .read_va(fo1_exe::CRIT_FAILURE_VA, I32_WIDTH)
            .expect("the critical failure table is readable"),
        &bytes[SPEC_FILE_CRIT_FAILURE..SPEC_FILE_CRIT_FAILURE + I32_WIDTH],
        "the critical failure table is read from its documented file offset"
    );
    assert_eq!(
        image
            .read_va(fo1_exe::STAT_LIMITS_VA, I32_WIDTH)
            .expect("the stat limits table is readable"),
        &bytes[SPEC_FILE_STAT_LIMITS..SPEC_FILE_STAT_LIMITS + I32_WIDTH],
        "the stat limits table is read from its documented file offset"
    );
    assert_eq!(
        image
            .read_va(fo1_exe::PLAYER_STAT_LIMITS_VA, I32_WIDTH)
            .expect("the player stat limits table is readable"),
        &bytes[SPEC_FILE_PLAYER_STAT_LIMITS..SPEC_FILE_PLAYER_STAT_LIMITS + I32_WIDTH],
        "the player stat limits table is read from its documented file offset"
    );
    assert_eq!(
        image
            .read_va(fo1_exe::XP_THRESHOLDS_VA, I32_WIDTH)
            .expect("the experience thresholds table is readable"),
        &bytes[SPEC_FILE_XP_THRESHOLDS..SPEC_FILE_XP_THRESHOLDS + I32_WIDTH],
        "the experience thresholds table is read from its documented file offset"
    );
}

/// claim: EXETBL-003
/// oracle: class 5 (hand computed vectors from the spec: -40, -30, -30, 0, -20, -20, -60, -30, 0)
#[test]
fn synthetic_the_hit_location_penalties_are_the_nine_documented_values() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let penalties =
        hit_location_penalties(&image).expect("the hit location penalties are readable");
    assert_eq!(
        penalties, SPEC_HIT_LOCATION_PENALTIES,
        "the nine hit location penalties are the nine documented values"
    );
    assert_eq!(
        penalties.len(),
        SPEC_HIT_LOCATION_PENALTY_COUNT,
        "the hit location penalty table holds nine values"
    );
    assert_eq!(
        SPEC_HIT_LOCATION_PENALTY_LEN,
        SPEC_HIT_LOCATION_PENALTY_COUNT * I32_WIDTH,
        "the hit location penalty table is nine values of four bytes"
    );

    // The raw bytes of the first value, as the spec records them.
    assert_eq!(
        &bytes[SPEC_FILE_HIT_LOCATION_PENALTY..SPEC_FILE_HIT_LOCATION_PENALTY + I32_WIDTH],
        &[0xD8, 0xFF, 0xFF, 0xFF],
        "the first hit location penalty is the little endian form of -40"
    );
}

/// claim: EXETBL-004
/// oracle: class 5 (hand computed vectors from the spec: 810 records of 28 bytes, first 4 0 -1 0 0 5001 5000, last 4 2048 -1 0 0 6402 5000)
#[test]
fn synthetic_the_critter_critical_table_is_810_records_of_28_bytes() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let records =
        critical_success_critters(&image).expect("the critter critical table is readable");
    assert_eq!(
        records.len(),
        SPEC_CRITTER_CRITICAL_COUNT,
        "the critter critical table holds 810 records"
    );
    assert_eq!(
        SPEC_CRIT_SUCCESS_CRITTERS_LEN,
        SPEC_CRITTER_CRITICAL_COUNT * CRITICAL_RECORD_SIZE,
        "810 records of 28 bytes are 22680 bytes"
    );

    let first: &CriticalRecord = &records[0];
    assert_eq!(first.multiplier(), 4, "the first record multiplies by four");
    assert_eq!(
        first.effect_flags(),
        0,
        "the first record carries no effect flags"
    );
    assert_eq!(
        first.massive_critical_stat(),
        -1,
        "the first record has no massive critical stat"
    );
    assert_eq!(
        first.massive_critical_modifier(),
        0,
        "the first record carries no massive critical modifier"
    );
    assert_eq!(
        first.massive_critical_flags(),
        0,
        "the first record carries no massive critical flags"
    );
    assert_eq!(
        first.message_id_1(),
        5001,
        "the first record names message 5001"
    );
    assert_eq!(
        first.message_id_2(),
        5000,
        "the first record names message 5000"
    );
    assert!(
        !first.has_massive_critical(),
        "the sentinel -1 is not a massive critical stat"
    );

    let last: &CriticalRecord = &records[SPEC_CRITTER_CRITICAL_COUNT - 1];
    assert_eq!(last.multiplier(), 4, "the last record multiplies by four");
    assert_eq!(
        last.effect_flags(),
        2048,
        "the last record carries effect flags 2048"
    );
    assert_eq!(
        last.massive_critical_stat(),
        -1,
        "the last record has no massive critical stat"
    );
    assert_eq!(
        last.massive_critical_modifier(),
        0,
        "the last record carries no massive critical modifier"
    );
    assert_eq!(
        last.massive_critical_flags(),
        0,
        "the last record carries no massive critical flags"
    );
    assert_eq!(
        last.message_id_1(),
        6402,
        "the last record names message 6402"
    );
    assert_eq!(
        last.message_id_2(),
        5000,
        "the last record names message 5000"
    );
    assert!(
        !last.has_massive_critical(),
        "the sentinel -1 is not a massive critical stat"
    );

    // The raw bytes of the first record, as the spec records them.
    assert_eq!(
        &bytes[SPEC_FILE_CRIT_SUCCESS_CRITTERS..SPEC_FILE_CRIT_SUCCESS_CRITTERS + I32_WIDTH],
        &[0x04, 0x00, 0x00, 0x00],
        "the first critter critical record starts with the multiplier four"
    );
}

/// claim: EXETBL-005
/// oracle: class 5 (hand computed vectors from the spec: 54 records of 28 bytes, first 3 0 -1 0 0 6500 5000, last 4 2050 6 2 128 6503 6513)
#[test]
fn synthetic_the_player_critical_table_is_54_records_of_28_bytes() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let records = critical_success_player(&image).expect("the player critical table is readable");
    assert_eq!(
        records.len(),
        SPEC_PLAYER_CRITICAL_COUNT,
        "the player critical table holds 54 records"
    );
    assert_eq!(
        SPEC_CRIT_SUCCESS_PLAYER_LEN,
        SPEC_PLAYER_CRITICAL_COUNT * CRITICAL_RECORD_SIZE,
        "54 records of 28 bytes are 1512 bytes"
    );

    let first: &CriticalRecord = &records[0];
    assert_eq!(
        first.multiplier(),
        3,
        "the first record multiplies by three"
    );
    assert_eq!(
        first.effect_flags(),
        0,
        "the first record carries no effect flags"
    );
    assert_eq!(
        first.massive_critical_stat(),
        -1,
        "the first record has no massive critical stat"
    );
    assert_eq!(
        first.massive_critical_modifier(),
        0,
        "the first record carries no massive critical modifier"
    );
    assert_eq!(
        first.massive_critical_flags(),
        0,
        "the first record carries no massive critical flags"
    );
    assert_eq!(
        first.message_id_1(),
        6500,
        "the first record names message 6500"
    );
    assert_eq!(
        first.message_id_2(),
        5000,
        "the first record names message 5000"
    );
    assert!(
        !first.has_massive_critical(),
        "the sentinel -1 is not a massive critical stat"
    );

    let last: &CriticalRecord = &records[SPEC_PLAYER_CRITICAL_COUNT - 1];
    assert_eq!(last.multiplier(), 4, "the last record multiplies by four");
    assert_eq!(
        last.effect_flags(),
        2050,
        "the last record carries effect flags 2050"
    );
    assert_eq!(
        last.massive_critical_stat(),
        6,
        "the last record names massive critical stat 6"
    );
    assert_eq!(
        last.massive_critical_modifier(),
        2,
        "the last record carries massive critical modifier 2"
    );
    assert_eq!(
        last.massive_critical_flags(),
        128,
        "the last record carries massive critical flags 128"
    );
    assert_eq!(
        last.message_id_1(),
        6503,
        "the last record names message 6503"
    );
    assert_eq!(
        last.message_id_2(),
        6513,
        "the last record names message 6513"
    );
    assert!(
        last.has_massive_critical(),
        "a stat of six is a massive critical stat"
    );

    // The raw bytes of the first record, as the spec records them.
    assert_eq!(
        &bytes[SPEC_FILE_CRIT_SUCCESS_PLAYER..SPEC_FILE_CRIT_SUCCESS_PLAYER + I32_WIDTH],
        &[0x03, 0x00, 0x00, 0x00],
        "the first player critical record starts with the multiplier three"
    );
}

/// claim: EXETBL-006
/// oracle: class 5 (hand computed vectors from the spec: seven rows of five values)
#[test]
fn synthetic_the_critical_failure_table_is_seven_rows_of_five_documented_values() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let rows = critical_failure(&image).expect("the critical failure table is readable");
    assert_eq!(
        rows.len(),
        SPEC_CRITICAL_FAILURE_ROW_COUNT,
        "the critical failure table holds seven rows"
    );
    assert_eq!(
        SPEC_CRIT_FAILURE_LEN,
        SPEC_CRITICAL_FAILURE_ROW_COUNT * SPEC_CRITICAL_FAILURE_COLUMNS * I32_WIDTH,
        "seven rows of five values are 140 bytes"
    );

    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            row.len(),
            SPEC_CRITICAL_FAILURE_COLUMNS,
            "row {} holds five values",
            index
        );
        assert_eq!(
            *row, SPEC_CRITICAL_FAILURE_ROWS[index],
            "row {} carries the five documented values",
            index
        );
    }

    // The raw bytes of the first row, as the spec records them.
    assert_eq!(
        &bytes[SPEC_FILE_CRIT_FAILURE..SPEC_FILE_CRIT_FAILURE + I32_WIDTH],
        &[0x00, 0x00, 0x00, 0x00],
        "the first critical failure row starts with a zero"
    );
}

/// claim: EXETBL-007
/// oracle: class 5 (hand computed vectors from the spec: 38 records of 24 bytes, the first seven with minimum 1, maximum 10, default 5)
#[test]
fn synthetic_the_stat_limits_are_38_records_of_six_fields() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let limits = stat_limits(&image).expect("the stat limits table is readable");
    assert_eq!(
        limits.len(),
        SPEC_STAT_LIMIT_COUNT,
        "the stat limits table holds 38 records"
    );
    assert_eq!(
        SPEC_STAT_LIMITS_LEN,
        SPEC_STAT_LIMIT_COUNT * STAT_LIMIT_SIZE,
        "38 records of 24 bytes are 912 bytes"
    );

    for (index, field2) in SPEC_STAT_LIMIT_CORE_FIELD2.iter().enumerate() {
        let limit: &StatLimit = &limits[index];
        assert_eq!(
            limit.field0(),
            0,
            "record {} carries a zero field zero",
            index
        );
        assert_eq!(
            limit.field1(),
            0,
            "record {} carries a zero field one",
            index
        );
        assert_eq!(
            limit.field2(),
            *field2,
            "record {} carries field two",
            index
        );
        assert_eq!(limit.minimum(), 1, "record {} has a minimum of one", index);
        assert_eq!(limit.maximum(), 10, "record {} has a maximum of ten", index);
        assert_eq!(limit.default(), 5, "record {} has a default of five", index);
    }

    let sixteenth: &StatLimit = &limits[16];
    assert_eq!(sixteenth.field0(), 0, "record 16 carries a zero field zero");
    assert_eq!(sixteenth.field1(), 0, "record 16 carries a zero field one");
    assert_eq!(sixteenth.field2(), 94, "record 16 carries field two 94");
    assert_eq!(sixteenth.minimum(), 0, "record 16 has a minimum of zero");
    assert_eq!(
        sixteenth.maximum(),
        100,
        "record 16 has a maximum of one hundred"
    );
    assert_eq!(sixteenth.default(), 0, "record 16 has a default of zero");

    // The raw bytes of the first minimum, as the spec records them.
    assert_eq!(
        &bytes[SPEC_FILE_STAT_LIMITS + 12..SPEC_FILE_STAT_LIMITS + 16],
        &[0x01, 0x00, 0x00, 0x00],
        "the minimum of the first record is the little endian form of one"
    );
}

/// claim: EXETBL-008
/// oracle: class 5 (hand computed vectors from the spec: five records, the second a level floor of one and the fourth a signed range)
#[test]
fn synthetic_the_player_stat_limits_are_five_records_with_the_documented_ranges() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let limits = player_stat_limits(&image).expect("the player stat limits table is readable");
    assert_eq!(
        limits.len(),
        SPEC_PLAYER_STAT_LIMIT_COUNT,
        "the player stat limits table holds five records"
    );
    assert_eq!(
        SPEC_PLAYER_STAT_LIMITS_LEN,
        SPEC_PLAYER_STAT_LIMIT_COUNT * STAT_LIMIT_SIZE,
        "five records of 24 bytes are 120 bytes"
    );

    // The spec states the minimum, the maximum and the default of all five
    // records, and that field 0 and field 1 are zero in all five. It states no
    // field 2 for these five records, so no field 2 is asserted here.
    for (index, expected) in SPEC_PLAYER_STAT_LIMITS.iter().enumerate() {
        let limit: &StatLimit = &limits[index];
        assert_eq!(
            limit.field0(),
            0,
            "record {} carries a zero field zero",
            index
        );
        assert_eq!(
            limit.field1(),
            0,
            "record {} carries a zero field one",
            index
        );
        assert_eq!(
            limit.minimum(),
            expected[0],
            "record {} carries its minimum",
            index
        );
        assert_eq!(
            limit.maximum(),
            expected[1],
            "record {} carries its maximum",
            index
        );
        assert_eq!(
            limit.default(),
            expected[2],
            "record {} carries its default",
            index
        );
    }

    // The level floor: the second record starts at one and defaults to one.
    let level: &StatLimit = &limits[1];
    assert_eq!(level.minimum(), 1, "the level floor is one");
    assert_eq!(level.default(), 1, "the level default is one");

    // The signed range: the fourth record runs from minus twenty to twenty.
    let signed: &StatLimit = &limits[3];
    assert_eq!(
        signed.minimum(),
        -20,
        "the signed range starts at minus twenty"
    );
    assert_eq!(signed.maximum(), 20, "the signed range ends at twenty");

    // The unbounded maximum, as the spec records it.
    let unbounded: &StatLimit = &limits[0];
    assert_eq!(
        unbounded.maximum(),
        2_147_483_647,
        "the unbounded maximum is the largest positive i32"
    );

    // The raw bytes of the second record's minimum, as the spec records them.
    assert_eq!(
        &bytes[SPEC_FILE_PLAYER_STAT_LIMITS + STAT_LIMIT_SIZE + 12
            ..SPEC_FILE_PLAYER_STAT_LIMITS + STAT_LIMIT_SIZE + 16],
        &[0x01, 0x00, 0x00, 0x00],
        "the level floor is the little endian form of one"
    );
}

/// claim: EXETBL-009
/// oracle: class 5 (hand computed vectors from the spec: the twenty one triangular numbers, indexed by level)
#[test]
fn synthetic_the_xp_thresholds_are_the_twenty_one_triangular_numbers_indexed_by_level() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let thresholds = xp_thresholds(&image).expect("the experience thresholds table is readable");
    assert_eq!(
        thresholds, SPEC_XP_THRESHOLDS,
        "the twenty one experience thresholds are the twenty one triangular numbers"
    );
    assert_eq!(
        thresholds.len(),
        SPEC_XP_THRESHOLD_COUNT,
        "the experience thresholds table holds twenty one values"
    );
    assert_eq!(
        SPEC_XP_THRESHOLDS_LEN,
        SPEC_XP_THRESHOLD_COUNT * I32_WIDTH,
        "twenty one values of four bytes are 84 bytes"
    );

    // The table is indexed by level, so element zero is level zero.
    assert_eq!(thresholds[0], 0, "element zero is level zero");
    assert_eq!(thresholds[1], 1, "element one is level one");
    assert_eq!(thresholds[20], 210, "element twenty is level twenty");

    // The raw bytes of the level one entry, as the spec records them.
    assert_eq!(
        &bytes[SPEC_FILE_XP_THRESHOLDS + I32_WIDTH..SPEC_FILE_XP_THRESHOLDS + 2 * I32_WIDTH],
        &[0x01, 0x00, 0x00, 0x00],
        "the level one threshold is the little endian form of one"
    );
}

/// claim: EXETBL-015
/// oracle: class 5 (hand computed vectors from the spec: the table is unscaled and the caller scales it by one thousand)
#[test]
fn synthetic_the_experience_of_a_level_is_the_threshold_scaled_by_one_thousand() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let thresholds = xp_thresholds(&image).expect("the experience thresholds table is readable");

    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, 0),
        Ok(0),
        "level zero needs no experience"
    );
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, 1),
        Ok(SPEC_EXPERIENCE_SCALE),
        "level one needs one thousand experience"
    );
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, 20),
        Ok(210_000),
        "level twenty needs two hundred and ten thousand experience"
    );
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, 21),
        Err(ExeError::FieldOutOfRange {
            offset: SPEC_XP_LEVEL_OUT_OF_RANGE_OFFSET,
            field: "level",
        }),
        "the table holds twenty one levels and refuses a level past them"
    );

    // The refused field sits one past the last value of the table, and the
    // offset is a byte offset: twenty one values of four bytes are eighty four
    // bytes, so eighty four is exactly one past the last value.
    assert_eq!(
        SPEC_XP_LEVEL_OUT_OF_RANGE_OFFSET,
        SPEC_XP_THRESHOLD_COUNT * I32_WIDTH,
        "the field one past the last threshold sits at the table's byte length"
    );
}

/// claim: EXETBL-015
/// oracle: class 5 (hand computed vectors from the spec: the array index is the level, so a negative level is not an index of the table at all)
#[test]
fn synthetic_a_negative_level_is_refused_before_the_table_is_indexed() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let thresholds = xp_thresholds(&image).expect("the experience thresholds table is readable");

    // The good twin: level zero and level twenty are indexes of the table.
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, 0),
        Ok(0),
        "level zero is inside the table"
    );
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, 20),
        Ok(210_000),
        "level twenty is inside the table"
    );

    // The bad twin: a negative level is not an index of the table, so the
    // conversion of the level to an index fails before the table is read.
    // `docs/spec/exe-tables.md:1050` states that refusal as
    // `ExeError::RangeOverflow { va: XP_THRESHOLDS_VA }`, and
    // `docs/spec/exe-tables.md:1056` is the arm that raises it.
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, -1),
        Err(ExeError::RangeOverflow {
            va: SPEC_XP_THRESHOLDS_VA,
        }),
        "a negative level is refused as an address overflow, not as a field offset"
    );
}

/// claim: EXETBL-010
/// oracle: class 6 (algebraic law: the critter table ends where the player table begins, in both address spaces)
#[test]
fn synthetic_the_critter_critical_table_ends_where_the_player_critical_table_begins() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    assert_eq!(
        fo1_exe::CRIT_SUCCESS_CRITTERS_VA + SPEC_CRITTER_CRITICAL_EXTENT,
        fo1_exe::CRIT_SUCCESS_PLAYER_VA,
        "the critter table ends where the player table begins"
    );
    assert_eq!(
        SPEC_FILE_CRIT_SUCCESS_CRITTERS + SPEC_CRIT_SUCCESS_CRITTERS_LEN,
        SPEC_FILE_CRIT_SUCCESS_PLAYER,
        "the same identity holds in the file domain"
    );

    // The two windows are adjacent in the fixture, with no gap and no
    // overlap: the last byte of the critter table and the first byte of the
    // player table are neighbours.
    let critter_window = image
        .read_va(
            fo1_exe::CRIT_SUCCESS_CRITTERS_VA,
            SPEC_CRIT_SUCCESS_CRITTERS_LEN,
        )
        .expect("the critter critical table is readable");
    let player_window = image
        .read_va(fo1_exe::CRIT_SUCCESS_PLAYER_VA, I32_WIDTH)
        .expect("the player critical table is readable");
    assert_eq!(
        critter_window.last(),
        Some(&0x00),
        "the last byte of the critter table is the high byte of its last message id"
    );
    assert_eq!(
        player_window.first(),
        Some(&0x03),
        "the first byte of the player table is the low byte of its first multiplier"
    );
}

/// claim: EXETBL-011
/// oracle: class 6 (algebraic law: the stat limits end where the player stat limits begin, in both address spaces)
#[test]
fn synthetic_the_stat_limits_end_where_the_player_stat_limits_begin() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    assert_eq!(
        fo1_exe::STAT_LIMITS_VA + SPEC_STAT_LIMITS_EXTENT,
        fo1_exe::PLAYER_STAT_LIMITS_VA,
        "the stat limits end where the player stat limits begin"
    );
    assert_eq!(
        SPEC_FILE_STAT_LIMITS + SPEC_STAT_LIMITS_LEN,
        SPEC_FILE_PLAYER_STAT_LIMITS,
        "the same identity holds in the file domain"
    );

    // The last record of the stat limits and the first record of the player
    // stat limits are different records, so a reader that slipped by one
    // record would show it.
    let limits = stat_limits(&image).expect("the stat limits table is readable");
    let player = player_stat_limits(&image).expect("the player stat limits table is readable");
    assert_eq!(
        limits[SPEC_STAT_LIMIT_COUNT - 1].default(),
        102,
        "the last stat limit record carries the pattern default"
    );
    assert_eq!(
        player[0].maximum(),
        2_147_483_647,
        "the first player stat limit record carries the unbounded maximum"
    );
}

/// claim: EXETBL-012
/// oracle: class 6 (algebraic law: the player stat limits end where the experience table begins, in both address spaces)
#[test]
fn synthetic_the_player_stat_limits_end_where_the_xp_table_begins() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    assert_eq!(
        fo1_exe::PLAYER_STAT_LIMITS_VA + SPEC_PLAYER_STAT_LIMITS_EXTENT,
        fo1_exe::XP_THRESHOLDS_VA,
        "the player stat limits end where the experience table begins"
    );
    assert_eq!(
        SPEC_FILE_PLAYER_STAT_LIMITS + SPEC_PLAYER_STAT_LIMITS_LEN,
        SPEC_FILE_XP_THRESHOLDS,
        "the same identity holds in the file domain"
    );

    // The last player stat limit record and the first experience threshold
    // are different data, so a reader that slipped by one record would show
    // it.
    let player = player_stat_limits(&image).expect("the player stat limits table is readable");
    let thresholds = xp_thresholds(&image).expect("the experience thresholds table is readable");
    assert_eq!(
        player[SPEC_PLAYER_STAT_LIMIT_COUNT - 1].maximum(),
        2_147_483_647,
        "the last player stat limit record carries the unbounded maximum"
    );
    assert_eq!(
        thresholds[0], 0,
        "the first experience threshold is level zero"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: an image truncated before the section table is refused)
#[test]
fn synthetic_an_image_truncated_before_the_section_table_is_refused() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    assert!(
        PeImage::parse(bytes).is_ok(),
        "the whole synthetic image is accepted"
    );

    let mut truncated = SYNTHETIC_IMAGE.clone();
    truncated.truncate(SYNTHETIC_TRUNCATED_LEN);

    let refused = match PeImage::parse(&truncated) {
        Ok(_) => panic!("an image without a section table must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::FieldOutOfRange {
            offset: SYNTHETIC_SECTION_TABLE_OFFSET,
            field: "sectionName",
        },
        "the truncation is reported where the section table starts"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: an image whose PE signature is wrong is refused)
#[test]
fn synthetic_an_image_with_a_bad_signature_is_refused() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    assert!(
        PeImage::parse(bytes).is_ok(),
        "the whole synthetic image is accepted"
    );

    let mut bad_signature = SYNTHETIC_IMAGE.clone();
    synthetic_put_u32(&mut bad_signature, 0x80, 0x0000_4551);

    let refused = match PeImage::parse(&bad_signature) {
        Ok(_) => panic!("an image with a wrong PE signature must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::Header {
            offset: 0x80,
            field: "signature",
        },
        "the bad signature is reported at the signature itself"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: an image with a PE32+ optional header is refused)
#[test]
fn synthetic_an_image_with_a_pe32_plus_optional_header_is_refused() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    assert!(
        PeImage::parse(bytes).is_ok(),
        "the whole synthetic image is accepted"
    );

    let mut pe32_plus = SYNTHETIC_IMAGE.clone();
    synthetic_put_u16(
        &mut pe32_plus,
        SYNTHETIC_OPTIONAL_HEADER_OFFSET,
        SYNTHETIC_PE32_PLUS_MAGIC,
    );

    let refused = match PeImage::parse(&pe32_plus) {
        Ok(_) => panic!("an image with a PE32+ optional header must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::Header {
            offset: SYNTHETIC_OPTIONAL_HEADER_OFFSET,
            field: "optionalMagic",
        },
        "the PE32+ optional header is reported at the optional magic"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: an image whose section count cannot fit in the file is refused)
#[test]
fn synthetic_an_image_with_an_impossible_section_count_is_refused() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    assert!(
        PeImage::parse(bytes).is_ok(),
        "the whole synthetic image is accepted"
    );

    let mut impossible = SYNTHETIC_IMAGE.clone();
    synthetic_put_u16(
        &mut impossible,
        SYNTHETIC_COFF_HEADER_OFFSET + 2,
        SYNTHETIC_IMPOSSIBLE_SECTION_COUNT,
    );
    impossible.truncate(SYNTHETIC_SHORT_SECTION_TABLE_LEN);

    let refused = match PeImage::parse(&impossible) {
        Ok(_) => panic!("an image with an impossible section count must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::FieldOutOfRange {
            offset: SYNTHETIC_SECOND_SECTION_HEADER_OFFSET,
            field: "sectionName",
        },
        "the impossible section count is reported where the second section header starts"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: the exact text of every error variant)
#[test]
fn synthetic_every_error_variant_prints_its_specified_text() {
    // Every expected string below is the spec's own text, taken from the
    // `impl fmt::Display for ExeError` block of `docs/spec/exe-tables.md`
    // (lines 550 to 583), one format string per variant:
    //   Header          line 555
    //   Unmapped        line 557
    //   RangeOverflow   line 559
    //   OutsideFile     line 564
    //   FieldOutOfRange line 569
    //   RecordCount     line 573
    //   BuildMismatch   line 577
    // The payloads are distinct and nonzero, so a variant that carried one
    // field's value in another's place would print differently.
    assert_eq!(
        ExeError::Header {
            offset: 0x3C,
            field: "e_lfanew"
        }
        .to_string(),
        "bad PE header field e_lfanew at file offset 0x3C",
        "the header variant names the field and its offset"
    );
    assert_eq!(
        ExeError::Unmapped { va: 0x0050_0000 }.to_string(),
        "virtual address 0x00500000 is in no section",
        "the unmapped variant names the address"
    );
    assert_eq!(
        ExeError::RangeOverflow { va: 0x004F_EC90 }.to_string(),
        "address arithmetic overflowed for 0x004FEC90",
        "the range overflow variant names the address"
    );
    assert_eq!(
        ExeError::OutsideFile {
            offset: 0x0001_2345,
            len: 0x2A
        }
        .to_string(),
        "read of 42 bytes at file offset 0x12345 runs past the end of its section or of the file",
        "the outside file variant names the offset and the length"
    );
    assert_eq!(
        ExeError::FieldOutOfRange {
            offset: 0x0000_002A,
            field: "minimum"
        }
        .to_string(),
        "field minimum at byte offset 0x2A is outside the record",
        "the field out of range variant names the field and its offset"
    );
    assert_eq!(
        ExeError::RecordCount {
            va: 0x0050_82D0,
            expected: 21,
            observed: 20
        }
        .to_string(),
        "table at 0x005082D0 declared 21 records but 20 were read",
        "the record count variant names the address and both counts"
    );
    assert_eq!(
        ExeError::BuildMismatch {
            expected_sha256: REFERENCE_SHA256,
            observed_sha256: SHA256_OF_ABC.to_string(),
        }
        .to_string(),
        "executable digest ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad \
         is not the reference build 444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272; \
         this build's table constants are unknown and are not guessed",
        "the build mismatch variant names both digests and says the constants are not guessed"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: every refusal carries the offset, the length, the address or the counts the spec names)
#[test]
fn synthetic_every_refusal_carries_the_payload_the_spec_names() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    // --- Header: the offset of the field that was refused, and its name ----
    //
    // The good twin: the whole synthetic image is a PE image.
    assert!(
        PeImage::parse(bytes).is_ok(),
        "the whole synthetic image is accepted, so a refusal names the field that was wrong"
    );
    let mut bad_signature = SYNTHETIC_IMAGE.clone();
    synthetic_put_u32(&mut bad_signature, 0x80, 0x0000_4551);
    let refused = match PeImage::parse(&bad_signature) {
        Ok(_) => panic!("an image with a wrong PE signature must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::Header {
            offset: 0x80,
            field: "signature",
        },
        "the header refusal carries the offset of the signature and the field's name"
    );

    let mut pe32_plus = SYNTHETIC_IMAGE.clone();
    synthetic_put_u16(
        &mut pe32_plus,
        SYNTHETIC_OPTIONAL_HEADER_OFFSET,
        SYNTHETIC_PE32_PLUS_MAGIC,
    );
    let refused = match PeImage::parse(&pe32_plus) {
        Ok(_) => panic!("an image with a PE32+ optional header must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::Header {
            offset: SYNTHETIC_OPTIONAL_HEADER_OFFSET,
            field: "optionalMagic",
        },
        "the header refusal carries the offset of the optional magic and the field's name"
    );

    // --- Unmapped: the address that no section covers ----------------------
    //
    // The good twin: an address inside a file backed section reads.
    assert!(
        image.read_va(SYNTHETIC_DATA_SECTION_FIRST_VA, 1).is_ok(),
        "an address inside a file backed section reads"
    );
    assert_eq!(
        image.read_va(SYNTHETIC_GAP_VA, 1),
        Err(ExeError::Unmapped {
            va: SYNTHETIC_GAP_VA
        }),
        "the unmapped refusal carries the address that no section covers"
    );
    assert_eq!(
        image.read_va(SYNTHETIC_UNINIT_SECTION_FIRST_VA, 1),
        Err(ExeError::Unmapped {
            va: SYNTHETIC_UNINIT_SECTION_FIRST_VA
        }),
        "the unmapped refusal carries the address inside the uninitialised section"
    );

    // --- RangeOverflow: the address whose arithmetic overflowed -------------
    //
    // The good twin: levels that are indexes of the table are read.
    let thresholds = xp_thresholds(&image).expect("the experience thresholds table is readable");
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, 20),
        Ok(210_000),
        "level twenty is inside the table"
    );
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, -1),
        Err(ExeError::RangeOverflow {
            va: SPEC_XP_THRESHOLDS_VA,
        }),
        "the range overflow refusal carries the address of the table"
    );

    // --- OutsideFile: the file offset the read started at, and its length ---
    //
    // The good twin: the last byte of the section reads.
    assert!(
        image.read_va(SYNTHETIC_DATA_SECTION_LAST_VA, 1).is_ok(),
        "the last byte of the data section reads"
    );
    assert_eq!(
        image.read_va(SYNTHETIC_DATA_SECTION_LAST_VA, 2),
        Err(ExeError::OutsideFile {
            offset: SYNTHETIC_DATA_SECTION_LAST_OFFSET,
            len: 2,
        }),
        "the outside file refusal carries the offset the read started at and the length"
    );

    // --- FieldOutOfRange: the offset of the field, and the field's name -----
    //
    // The good twin: the whole synthetic image is a PE image.
    let mut truncated = SYNTHETIC_IMAGE.clone();
    truncated.truncate(SYNTHETIC_TRUNCATED_LEN);
    let refused = match PeImage::parse(&truncated) {
        Ok(_) => panic!("an image without a section table must be refused"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::FieldOutOfRange {
            offset: SYNTHETIC_SECTION_TABLE_OFFSET,
            field: "sectionName",
        },
        "the field out of range refusal carries the offset of the field and the field's name"
    );
    assert_eq!(
        fo1_exe::experience_for_level(&thresholds, 21),
        Err(ExeError::FieldOutOfRange {
            offset: SPEC_XP_LEVEL_OUT_OF_RANGE_OFFSET,
            field: "level",
        }),
        "the field out of range refusal carries the byte offset of the refused level"
    );

    // --- BuildMismatch: the digest that was expected and the one observed ---
    //
    // The good twin: the whole synthetic image is a PE image.
    assert!(
        PeImage::parse(bytes).is_ok(),
        "the whole synthetic image is accepted, so the gate is what refuses it"
    );
    let refused = match PeImage::require_reference_build(b"abc") {
        Ok(_) => panic!("three bytes must never pass the reference build gate"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::BuildMismatch {
            expected_sha256: REFERENCE_SHA256,
            observed_sha256: SHA256_OF_ABC.to_string(),
        },
        "the build mismatch refusal carries the expected digest and the observed one"
    );

    // --- RecordCount: the one variant the reader cannot produce -----------
    //
    // Every table reader either fails or returns exactly the count it
    // declares, so the count is right by construction and the variant is
    // unreachable from outside the crate. What the gate can grade is the
    // condition the variant reports on: the count that comes back is the
    // count the table declares.
    let limits = stat_limits(&image).expect("the stat limits table is readable");
    assert_eq!(
        limits.len(),
        SPEC_STAT_LIMIT_COUNT,
        "the stat limits reader returns the count the table declares"
    );
    let thresholds = xp_thresholds(&image).expect("the experience thresholds table is readable");
    assert_eq!(
        thresholds.len(),
        SPEC_XP_THRESHOLD_COUNT,
        "the experience thresholds reader returns the count the table declares"
    );
}

/// claim: EXETBL-007
/// oracle: class 5 (hand computed vectors from the spec: six accessor values, each different from every other)
#[test]
fn synthetic_every_stat_limit_accessor_reads_its_own_distinct_field() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let limits = stat_limits(&image).expect("the stat limits table is readable");
    let limit: &StatLimit = &limits[7];

    assert_eq!(limit.field0(), 17, "field zero reads its own field");
    assert_eq!(limit.field1(), 34, "field one reads its own field");
    assert_eq!(limit.field2(), 51, "field two reads its own field");
    assert_eq!(limit.minimum(), 68, "the minimum reads its own field");
    assert_eq!(limit.maximum(), 85, "the maximum reads its own field");
    assert_eq!(limit.default(), 102, "the default reads its own field");

    synthetic_assert_all_distinct(&[
        ("field0", limit.field0()),
        ("field1", limit.field1()),
        ("field2", limit.field2()),
        ("minimum", limit.minimum()),
        ("maximum", limit.maximum()),
        ("default", limit.default()),
    ]);
}

/// claim: EXETBL-004
/// oracle: class 5 (hand computed vectors from the spec: seven accessor values, each different from every other)
#[test]
fn synthetic_every_critical_record_accessor_reads_its_own_distinct_field() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let records =
        critical_success_critters(&image).expect("the critter critical table is readable");
    let record: &CriticalRecord = &records[1];

    assert_eq!(
        record.multiplier(),
        17,
        "the multiplier reads its own field"
    );
    assert_eq!(
        record.effect_flags(),
        34,
        "the effect flags read their own field"
    );
    assert_eq!(
        record.massive_critical_stat(),
        51,
        "the massive critical stat reads its own field"
    );
    assert_eq!(
        record.massive_critical_modifier(),
        68,
        "the massive critical modifier reads its own field"
    );
    assert_eq!(
        record.massive_critical_flags(),
        85,
        "the massive critical flags read their own field"
    );
    assert_eq!(
        record.message_id_1(),
        102,
        "the first message id reads its own field"
    );
    assert_eq!(
        record.message_id_2(),
        119,
        "the second message id reads its own field"
    );

    synthetic_assert_all_distinct(&[
        ("multiplier", record.multiplier()),
        ("effect_flags", record.effect_flags()),
        ("massive_critical_stat", record.massive_critical_stat()),
        (
            "massive_critical_modifier",
            record.massive_critical_modifier(),
        ),
        ("massive_critical_flags", record.massive_critical_flags()),
        ("message_id_1", record.message_id_1()),
        ("message_id_2", record.message_id_2()),
    ]);

    // The player table carries the same pattern, so the same check holds for
    // the second table.
    let player = critical_success_player(&image).expect("the player critical table is readable");
    let player_record: &CriticalRecord = &player[1];
    synthetic_assert_all_distinct(&[
        ("multiplier", player_record.multiplier()),
        ("effect_flags", player_record.effect_flags()),
        (
            "massive_critical_stat",
            player_record.massive_critical_stat(),
        ),
        (
            "massive_critical_modifier",
            player_record.massive_critical_modifier(),
        ),
        (
            "massive_critical_flags",
            player_record.massive_critical_flags(),
        ),
        ("message_id_1", player_record.message_id_1()),
        ("message_id_2", player_record.message_id_2()),
    ]);
}

/// claim: EXETBL-001
/// oracle: class 5 (hand computed vectors from the spec: the digest gate refuses any build that is not the reference build)
#[test]
fn synthetic_the_build_gate_refuses_an_image_that_is_not_the_reference_build() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    assert!(
        PeImage::parse(bytes).is_ok(),
        "the synthetic image is a PE image, so the header parse accepts it"
    );

    let refused = match PeImage::require_reference_build(bytes) {
        Ok(_) => panic!("a synthetic image must never pass the reference build gate"),
        Err(error) => error,
    };
    match refused {
        ExeError::BuildMismatch {
            expected_sha256,
            observed_sha256,
        } => {
            assert_eq!(
                expected_sha256, REFERENCE_SHA256,
                "the gate names the reference digest it expected"
            );
            assert_eq!(
                observed_sha256.len(),
                64,
                "the observed digest is a whole sha256 in lowercase hex"
            );
            assert_ne!(
                observed_sha256, REFERENCE_SHA256,
                "the synthetic image is not the reference build"
            );
        }
        other => panic!(
            "the gate must refuse with a build mismatch, not with {:?}",
            other
        ),
    }
}

/// claim: EXETBL-001
/// oracle: class 5 (hand computed vectors from the spec: the digest is checked first, before any header parse and before any constant is used)
#[test]
fn synthetic_the_digest_is_checked_before_any_header_field_is_read() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();

    // The good twin: a well formed image parses.
    assert!(
        PeImage::parse(bytes).is_ok(),
        "the whole synthetic image is a PE image"
    );

    // The same three bytes, parsed without the gate, are a complaint about a
    // header field: the parse of a three byte buffer runs out of bytes at
    // `e_lfanew`, which sits at file offset 0x3C.
    let header_refusal = match PeImage::parse(b"abc") {
        Ok(_) => panic!("three bytes are not a PE image"),
        Err(error) => error,
    };
    assert_eq!(
        header_refusal,
        ExeError::FieldOutOfRange {
            offset: 0x3C,
            field: "e_lfanew",
        },
        "the header parse is what complains about a three byte buffer"
    );

    // Through the gate the same three bytes are refused with a build
    // mismatch, never with a header complaint, and the observed digest is the
    // digest of the bytes themselves. The gate therefore read the digest
    // before it read a header field.
    let refused = match PeImage::require_reference_build(b"abc") {
        Ok(_) => panic!("three bytes must never pass the reference build gate"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::BuildMismatch {
            expected_sha256: REFERENCE_SHA256,
            observed_sha256: SHA256_OF_ABC.to_string(),
        },
        "the gate refuses with a build mismatch and names the digest it observed"
    );

    // The same holds for a buffer that is too short to hold any header at all,
    // and for a buffer of the size the header parse would read first.
    let refused = match PeImage::require_reference_build(&[]) {
        Ok(_) => panic!("an empty buffer must never pass the reference build gate"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::BuildMismatch {
            expected_sha256: REFERENCE_SHA256,
            observed_sha256: SHA256_OF_EMPTY.to_string(),
        },
        "an empty buffer is refused as a build mismatch, not as a header complaint"
    );
    let refused = match PeImage::require_reference_build(&[0u8; 64]) {
        Ok(_) => panic!("sixty four zero bytes must never pass the reference build gate"),
        Err(error) => error,
    };
    assert_eq!(
        refused,
        ExeError::BuildMismatch {
            expected_sha256: REFERENCE_SHA256,
            observed_sha256: SHA256_OF_64_ZERO_BYTES.to_string(),
        },
        "sixty four zero bytes are refused as a build mismatch, not as a header complaint"
    );
}

/// claim: EXETBL-001
/// oracle: class 5 (hand computed vectors: the FIPS 180-4 digests, written out
/// here as public constants and not read out of the code under test)
#[test]
fn synthetic_the_crate_own_digest_is_the_published_digest_of_the_published_vectors() {
    // The crate hashes bytes with its own SHA-256 implementation, in
    // `crates/fo1-exe/src/sha256.rs`, and it keeps that implementation to
    // itself: the helper is private and the module is private. The one public
    // door to the digest is the build gate. A buffer that is not the reference
    // build is refused with a build mismatch, and that refusal carries the
    // digest the crate computed over the bytes it was given. So the refusal is
    // the observation, and it is the crate's own digest that is observed.
    //
    // The expected values are the published FIPS 180-4 vectors, so this is a
    // statement about the standard and not about the crate: a digest function
    // that agrees with the standard here agrees with it for the message the
    // build gate refuses, which is the only message the gate ever sees.
    for (message, published) in [
        (FIPS_EMPTY_MESSAGE, SHA256_OF_EMPTY),
        (FIPS_ABC_MESSAGE, SHA256_OF_ABC),
        (FIPS_448_BIT_MESSAGE, SHA256_OF_448_BIT_MESSAGE),
    ] {
        let refused = match PeImage::require_reference_build(message) {
            Ok(_) => panic!("a published message is not the reference build"),
            Err(error) => error,
        };
        match refused {
            ExeError::BuildMismatch {
                expected_sha256,
                observed_sha256,
            } => {
                assert_eq!(
                    expected_sha256, REFERENCE_SHA256,
                    "the gate names the reference digest it expected"
                );
                assert_eq!(
                    observed_sha256, published,
                    "the crate's own SHA-256 is the published digest of the message"
                );
            }
            other => panic!(
                "the gate must refuse with a build mismatch, not with {:?}",
                other
            ),
        }
    }
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: a read is bounded by the section that contains the address)
#[test]
fn synthetic_the_section_boundary_cases_are_refused_or_accepted_as_documented() {
    let bytes: &[u8] = SYNTHETIC_IMAGE.as_slice();
    let image = PeImage::parse(bytes).expect("the synthetic image is a PE image");

    let cases = synthetic_boundary_cases();

    // Every case is driven and its outcome recorded. The outcome is either
    // the bytes the read returned or the exact error it refused with, payload
    // included.
    let mut outcomes: Vec<(&'static str, SyntheticBoundaryOutcome)> = Vec::new();
    for case in &cases {
        let outcome = match image.read_va(case.va, case.len) {
            Ok(read) => SyntheticBoundaryOutcome::Accepted(read.to_vec()),
            Err(error) => SyntheticBoundaryOutcome::Refused(error),
        };
        outcomes.push((case.label, outcome));
    }

    // The expected set is built from the case table, so the assertion says
    // which input is refused with which exact error and which input is
    // accepted, not merely how many of each there were.
    let expected: Vec<(&'static str, SyntheticBoundaryOutcome)> = cases
        .iter()
        .map(|case| (case.label, case.expected.clone()))
        .collect();
    assert_eq!(
        outcomes, expected,
        "every boundary case has the outcome the section rule produces"
    );

    // The tally is measured from the outcomes above and compared with the
    // tally the case table declares. Neither number is written by hand.
    let refusals = outcomes
        .iter()
        .filter(|(_, outcome)| matches!(outcome, SyntheticBoundaryOutcome::Refused(_)))
        .count();
    let successes = outcomes
        .iter()
        .filter(|(_, outcome)| matches!(outcome, SyntheticBoundaryOutcome::Accepted(_)))
        .count();
    let declared_refusals = cases
        .iter()
        .filter(|case| matches!(case.expected, SyntheticBoundaryOutcome::Refused(_)))
        .count();
    let declared_successes = cases
        .iter()
        .filter(|case| matches!(case.expected, SyntheticBoundaryOutcome::Accepted(_)))
        .count();
    assert_eq!(
        refusals, declared_refusals,
        "the number of refusals is the number the case table declares"
    );
    assert_eq!(
        successes, declared_successes,
        "the number of successes is the number the case table declares"
    );
    assert_eq!(
        refusals + successes,
        cases.len(),
        "every case is either a refusal or a success"
    );
}

/// claim: EXETBL-002
/// oracle: class 5 (hand computed vectors from the spec: the addresses the crate publishes are the spec's addresses)
#[test]
fn synthetic_the_crate_constants_are_the_specified_addresses() {
    assert_eq!(
        fo1_exe::IMAGE_BASE,
        SPEC_IMAGE_BASE,
        "the image base is the one the spec records"
    );
    assert_eq!(
        fo1_exe::REFERENCE_SHA256,
        REFERENCE_SHA256,
        "the reference digest is the one the spec records"
    );
    assert_eq!(
        fo1_exe::HIT_LOCATION_PENALTY_VA,
        SPEC_HIT_LOCATION_PENALTY_VA,
        "the hit location penalty address is the one the spec records"
    );
    assert_eq!(
        fo1_exe::CRIT_SUCCESS_CRITTERS_VA,
        SPEC_CRIT_SUCCESS_CRITTERS_VA,
        "the critter critical address is the one the spec records"
    );
    assert_eq!(
        fo1_exe::CRIT_SUCCESS_PLAYER_VA,
        SPEC_CRIT_SUCCESS_PLAYER_VA,
        "the player critical address is the one the spec records"
    );
    assert_eq!(
        fo1_exe::CRIT_FAILURE_VA,
        SPEC_CRIT_FAILURE_VA,
        "the critical failure address is the one the spec records"
    );
    assert_eq!(
        fo1_exe::STAT_LIMITS_VA,
        SPEC_STAT_LIMITS_VA,
        "the stat limits address is the one the spec records"
    );
    assert_eq!(
        fo1_exe::PLAYER_STAT_LIMITS_VA,
        SPEC_PLAYER_STAT_LIMITS_VA,
        "the player stat limits address is the one the spec records"
    );
    assert_eq!(
        fo1_exe::XP_THRESHOLDS_VA,
        SPEC_XP_THRESHOLDS_VA,
        "the experience thresholds address is the one the spec records"
    );
    assert_eq!(
        SYNTHETIC_E_LFANEW, SPEC_E_LFANEW,
        "the fixture puts the PE signature where the reference build puts it"
    );
    assert_eq!(
        SYNTHETIC_SECTION_TABLE_OFFSET, SPEC_SECTION_TABLE_OFFSET,
        "the fixture puts the section table where the reference build puts it"
    );
}

// QUESTIONS FOR THE OWNER
//
// 1. The exact Display text of every ExeError variant is taken from the Rust
//    interface block of docs/spec/exe-tables.md, which the spec presents as the
//    spec author's decision rather than as a measurement. This file asserts
//    that text verbatim. Is that block normative for the error messages?
//
// 2. The error a truncated or malformed image produces is likewise taken from
//    that same block: a missing field is a FieldOutOfRange carrying the
//    offset and the field name. The spec does not state the malformed input
//    mapping outside that block. Should the field names "sectionName",
//    "virtualSize" and "signature" be fixed by the spec?
//
// 3. The seven tables' file offsets in this fixture are the reference build's
//    own offsets (EXETBL-C200 to EXETBL-C206), reproduced by giving the
//    synthetic section the reference's raw pointer. The test therefore asserts
//    the spec's recorded offsets. Is a synthetic fixture that reproduces the
//    reference's file layout acceptable for the mapping claims, or should the
//    mapping claims stay exclusively in the install gate?
//
// 4. The spec states the minimum, the maximum and the default of the five
//    player stat limit records, and says that field 0 and field 1 are zero in
//    all five, but it does not state field 2 for them. This fixture carries one
//    value for field 2 in all five records and **no test asserts it**: the
//    test asserts only the minimum, the maximum, the default and the two zero
//    fields the spec states. Should the spec pin that field as well?
//
// 5. The address exactly at a section's end is in no section, and this file
//    now says so. The spec states the rule (EXETBL-002: a virtual address is
//    contained in a section only while the distance into the section is
//    smaller than the file extent) but names no address that sits exactly at
//    that end, so the reference build's own end addresses are not in the
//    spec's tables. Should the spec name one end address of one section, so
//    the install gate can check the same boundary against the real file?
