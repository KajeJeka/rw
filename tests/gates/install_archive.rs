//! Archive layer gate tests, run against the reference install.
//!
//! **Oracle class 2 (closure on real data)** for every count below, with two
//! supporting oracles:
//!
//! - **oracle class 4**, an independent reference implementation: every walk of
//!   the archive tree, every block framing count and every LZSS decode compared
//!   against `fo1-lzss` comes from `fo1_tests::RefDatTree` and
//!   `fo1_tests::RefLzss`, which were written from `docs/spec/*.md` alone;
//! - **oracle class 5**, hand computed vectors: the `FONT0.AAF` block whose
//!   payload length and produced length are written out in `docs/spec/lzss.md`.
//!
//! Nothing here is a fixture. Every byte comes from the player's install, which
//! the harness fingerprints before any test body runs (guide: Fixtures and
//! golden files).
//!
//! # The two level assertion rule
//!
//! Every test below carries `#[ignore = "needs FO1_DATA"]`. That marker keeps
//! the default `cargo test` run fast and keeps CI away from game data; it is
//! **not** a way to pass. The gate runs these tests with `--ignored`, and the
//! harness call inside each test is unwrapped, so a missing or wrong install
//! fails loudly instead of skipping (guide: Exact assertions, "Missing data
//! fails").
//!
//! Every count is an `assert_eq!` against a named constant carrying the
//! `DAT-C0xx` id it came from. There is no "at least", no `continue`, no early
//! `return` and no `if let Ok` in a loop over cases. A case that is missing,
//! extra or duplicated fails.
//!
//! # Memory
//!
//! `MASTER.DAT` is 339,379,746 bytes and `CRITTER.DAT` is 157,085,138, so a test
//! that holds both holds about half a gigabyte. Tests that only inspect the tree
//! release the bytes again, through `trees`, because `DatArchive` and
//! `RefDatTree` own every string they report and borrow nothing. Only the tests
//! that decode payloads keep the bytes, through `Archive::load`.
//!
//! See `docs/spec/dat.md`, `docs/spec/lzss.md`.

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
use std::path::PathBuf;

use fo1_dat::{
    decode_entry, decode_entry_with_stats, payload_mode, DatArchive, DatError, EntryRecord,
    PayloadMode,
};
use fo1_lzss::decode_into;
use fo1_tests::{
    install_root_from_env, HarnessError, Install, RefDatTree, RefLzss, CRITTER_DAT_FILE_NAME,
    EXE_FILE_NAME, MASTER_DAT_FILE_NAME, REFERENCE_CRITTER_DAT_BYTES, REFERENCE_EXE_SHA256,
    REFERENCE_MASTER_DAT_BYTES,
};

// --- corpus counts, from docs/spec/dat.constants.toml -------------------------

/// MASTER.DAT directory count (`DAT-C010`, DAT-002).
const MASTER_DIRECTORIES: usize = 65;

/// CRITTER.DAT directory count (`DAT-C011`, DAT-002).
const CRITTER_DIRECTORIES: usize = 1;

/// Every block header across both archives, one per directory (DAT-004).
const CORPUS_BLOCK_HEADERS: usize = MASTER_DIRECTORIES + CRITTER_DIRECTORIES;

/// MASTER.DAT entry count (`DAT-C030`, DAT-025).
const MASTER_ENTRIES: usize = 19_784;

/// CRITTER.DAT entry count (`DAT-C031`, DAT-025).
const CRITTER_ENTRIES: usize = 5_459;

/// Corpus entry count (`DAT-C032`, DAT-025).
const CORPUS_ENTRIES: usize = 25_243;

/// Stored entries, mode `0x20` (`DAT-C033`, DAT-026). All in MASTER.DAT.
const STORED_ENTRIES: usize = 4_787;

/// Stored entries in MASTER.DAT, which is all of `STORED_ENTRIES`
/// (docs/spec/dat.md section 1.3).
const MASTER_STORED_ENTRIES: usize = 4_787;

/// Stored entries in CRITTER.DAT (docs/spec/dat.md section 1.3).
const CRITTER_STORED_ENTRIES: usize = 0;

/// Block chain entries, mode `0x40` (`DAT-C034`, DAT-026).
const CHAIN_ENTRIES: usize = 20_456;

/// Block chain entries in MASTER.DAT (DAT-016, docs/spec/dat.md section 1.3).
const MASTER_CHAIN_ENTRIES: usize = 14_997;

/// Block chain entries in CRITTER.DAT (`DAT-C034`, DAT-026).
const CRITTER_CHAIN_ENTRIES: usize = 5_459;

/// Entries in mode `0x10` in this corpus (`DAT-C035`, DAT-026). A measurement,
/// not a statement that the mode is dead (DAT-014).
const LZSS_STREAM_ENTRIES: usize = 0;

/// Stored entries with a nonzero compressed size (`DAT-C130`, DAT-017).
const STORED_WITH_NONZERO_COMPRESSED: usize = 0;

/// Chain entries with a zero compressed size (`DAT-C131`, DAT-017).
const CHAIN_WITH_ZERO_COMPRESSED: usize = 0;

/// Corpus block total (`DAT-C060`, DAT-020).
const CORPUS_BLOCKS: u64 = 67_663;

/// Corpus LZSS block total (`DAT-C061`, DAT-020).
const CORPUS_LZSS_BLOCKS: u64 = 61_070;

/// Corpus raw block total (`DAT-C062`, DAT-020).
const CORPUS_RAW_BLOCKS: u64 = 6_593;

/// MASTER.DAT block total (DAT-020).
const MASTER_BLOCKS: u64 = 38_020;

/// CRITTER.DAT block total (DAT-020).
const CRITTER_BLOCKS: u64 = 29_643;

/// MASTER.DAT LZSS block total (DAT-020).
const MASTER_LZSS_BLOCKS: u64 = 31_427;

/// CRITTER.DAT LZSS block total (DAT-020).
const CRITTER_LZSS_BLOCKS: u64 = 29_643;

/// MASTER.DAT raw block total (`DAT-C062`, DAT-020).
const MASTER_RAW_BLOCKS: u64 = 6_593;

/// CRITTER.DAT raw block total (`DAT-C062`, DAT-020). It holds none.
const CRITTER_RAW_BLOCKS: u64 = 0;

/// Output bytes the chain corpus decodes to (`LZSS-C021`, LZSS-022). The figure
/// covers the 20,456 chain entries only; a stored entry's output is its own
/// declared uncompressed size.
const CHAIN_OUTPUT_BYTES: u64 = 851_322_391;

/// Output bytes the 4,787 stored entries decode to.
///
/// A stored entry's output is its own declared uncompressed size, so this is
/// the sum of the stored half of the corpus. The ledger does not carry it yet;
/// the figure is the corpus measurement, confirmed by an independent walk over
/// the archive bytes that does not use the decoder under test.
const STORED_OUTPUT_BYTES: u64 = 66_934_665;

/// Output bytes the whole corpus decodes to, over all 25,243 entries.
///
/// The chain half is `LZSS-C021` and the stored half is
/// [`STORED_OUTPUT_BYTES`], and the two add up to this. The ledger does not
/// carry the total yet; the figure is the corpus measurement, confirmed by the
/// same independent walk.
const CORPUS_OUTPUT_BYTES: u64 = 918_257_056;

/// Chain entries that hold a raw block and an LZSS block both.
///
/// A chain's kind is read per block from the header's bit 15, so a chain can
/// mix the two kinds. The count is the corpus measurement, confirmed by the
/// same independent walk. A decoder that stopped testing the header bit per
/// block would report zero here.
const MIXED_CHAIN_COUNT: usize = 726;

/// Chain entries that hold a raw block and no LZSS block.
///
/// The corpus measurement, confirmed by the same independent walk: every raw
/// block in the corpus sits in a chain that also holds an LZSS block.
const RAW_ONLY_CHAIN_COUNT: usize = 0;

/// Block header field 2 at every one of the 66 measured block headers
/// (`DAT-C003`, DAT-006). The entry trailer width, not a record size.
const BLOCK_FIELD_2: u32 = 16;

/// MASTER.DAT entries held by the directory literally named `.`
/// (`DAT-C012`, DAT-005, DAT-011).
const ROOT_DIRECTORY_ENTRIES: usize = 11;

/// MASTER.DAT entries that compose with a directory prefix
/// (docs/spec/dat.md, DAT-011).
const MASTER_PREFIXED_ENTRIES: usize = 19_773;

/// The directory name that contributes no path separator (`DAT-C151`, DAT-011).
const ROOT_DIRECTORY_NAME: &str = ".";

/// CRITTER.DAT's one directory name (DAT-003, DAT-011).
const CRITTER_DIRECTORY_NAME: &str = "ART\\CRITTERS";

/// `COLOR.PAL` declared uncompressed size (`DAT-C080`, DAT-029). The palette
/// size the guide states, reached from a container field.
const COLOR_PAL_UNCOMPRESSED: u32 = 33_536;

/// `COLOR.PAL` stored offset (`DAT-C070`, DAT-012). Proof that tree order is not
/// physical order.
const COLOR_PAL_OFFSET: u32 = 211_060_669;

/// `COLOR.PAL` stored chain length (`DAT-C081`, DAT-029).
const COLOR_PAL_COMPRESSED: u32 = 9_292;

/// `ART\CRITTERS\CRITTERS.LST` stored offset (docs/spec/dat.md section 1.2).
const CRITTERS_LST_OFFSET: u32 = 158_356;

/// `ART\CRITTERS\CRITTERS.LST` declared uncompressed size
/// (docs/spec/dat.md section 1.2).
const CRITTERS_LST_UNCOMPRESSED: u32 = 646;

/// `ART\CRITTERS\CRITTERS.LST` stored chain length
/// (docs/spec/dat.md section 1.2).
const CRITTERS_LST_COMPRESSED: u32 = 405;

/// The chain entry the LZSS termination rules are measured on
/// (docs/spec/lzss.md, LZSS-014). It is a root level entry holding exactly one
/// LZSS block.
const LZSS_VECTOR_PATH: &str = "FONT0.AAF";

/// Declared uncompressed size of that entry, which is also the block's produced
/// size because the chain holds one block (LZSS-014).
const LZSS_VECTOR_PRODUCED: usize = 6_337;

/// The stored extent of that entry: a two byte block header plus the payload
/// (docs/spec/lzss.md, LZSS-014).
const LZSS_VECTOR_COMPRESSED: usize = 2_316;

/// The one LZSS block's compressed input length (docs/spec/lzss.md, LZSS-014).
const LZSS_VECTOR_BLOCK_PAYLOAD: usize = 2_314;

/// Bytes in a block header (DAT-018).
const BLOCK_HEADER_WIDTH: usize = 2;

/// Width of a tree or block header, the four big endian u32 it holds (DAT-004).
const TREE_HEADER_WIDTH: usize = 16;

/// Bit 15 of a block header, set on a raw block (`DAT-C050`, DAT-027).
const BLOCK_RAW_FLAG: u16 = 0x8000;

/// The three loose files whose path matches an archive entry but whose size
/// does not, so the size match drops them: the path, and `loose - declared`,
/// which is negative when the loose copy is smaller.
///
/// The guide's "Archive gate" section names the three and their exact byte
/// differences: `GABRIEL.INT` is 26 bytes larger, `KILLIAN.INT` is 14 bytes
/// smaller and `RAZOR.MSG` is 25 bytes smaller.
const SIZE_EXCLUSIONS: [(&str, i64); 3] = [
    ("SCRIPTS\\GABRIEL.INT", 26),
    ("SCRIPTS\\KILLIAN.INT", -14),
    ("TEXT\\ENGLISH\\DIALOG\\RAZOR.MSG", -25),
];

/// Number of names in [`SIZE_EXCLUSIONS`].
const SIZE_EXCLUSION_COUNT: usize = 3;

/// The five MASTER.DAT entries whose decoded extent runs past the end of the
/// archive, and the exact number of bytes each one overruns by.
///
/// The names and the numbers are `docs/spec/dat.md`'s, claim DAT-021: five
/// files of the `TEXT\SPANISH\GAME` directory, overrunning by 15228, 39038,
/// 508, 389 and 785 bytes.
const MASTER_UNCOMPRESSED_OVERRUNS: [(&str, u64); 5] = [
    ("TEXT\\SPANISH\\GAME\\PRO_SCEN.MSG", 15_228),
    ("TEXT\\SPANISH\\GAME\\SCRNAME.MSG", 39_038),
    ("TEXT\\SPANISH\\GAME\\STAT.MSG", 508),
    ("TEXT\\SPANISH\\GAME\\TRAIT.MSG", 389),
    ("TEXT\\SPANISH\\GAME\\WORLDMAP.MSG", 785),
];

/// The two CRITTER.DAT entries whose decoded extent runs past the end of the
/// archive, and the exact number of bytes each one overruns by.
///
/// `docs/spec/dat.md`, claim DAT-021: `NMPEASRP.FRM` by 7475 bytes and
/// `RESERVAA.FRM` by 2172. CRITTER.DAT holds one directory, `ART\CRITTERS`, and
/// every one of its entries keeps that prefix (DAT-003, DAT-011).
const CRITTER_UNCOMPRESSED_OVERRUNS: [(&str, u64); 2] = [
    ("ART\\CRITTERS\\NMPEASRP.FRM", 7_475),
    ("ART\\CRITTERS\\RESERVAA.FRM", 2_172),
];

/// The three files the harness fingerprints, named here for the scratch fixture
/// the refusal test builds.
const INSTALL_FILE_NAMES: [&str; 3] = [EXE_FILE_NAME, MASTER_DAT_FILE_NAME, CRITTER_DAT_FILE_NAME];

// --- harness plumbing --------------------------------------------------------

/// Resolves and fingerprints the install.
///
/// A missing or foreign install fails here, naming `FO1_DATA`, rather than
/// skipping: the harness's error message is the failure (guide: Exact
/// assertions, "Missing data fails").
fn install() -> Install {
    let root = install_root_from_env()
        .expect("this gate test needs the player's install: export FO1_DATA");
    step(
        Install::at(root),
        "the install at FO1_DATA is not the reference install",
    )
}

/// Unwraps one fallible step of a gate test, naming the step when it fails.
///
/// The gate's replacement for a bare `unwrap`: the failure message says which
/// step gave up, which is what a reader needs, instead of a `Debug` shape.
fn step<T, E: core::fmt::Debug>(result: Result<T, E>, what: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{what}: {error:?}"),
    }
}

/// As `step`, for an absent slice element or lookup.
fn present<T>(value: Option<T>, what: &str) -> T {
    match value {
        Some(inner) => inner,
        None => panic!("{what}: absent"),
    }
}

/// Panics when a decode returned `Ok`.
///
/// This is the "never a short buffer" half of the suite: a decoder that returns
/// a value here has truncated, padded or defaulted to reach the declared length
/// (guide: No truncation, and a consumption check).
fn must_fail<T, E: core::fmt::Debug>(result: Result<T, E>, case: &str) {
    if result.is_ok() {
        panic!("{case}: expected an error, the decoder returned a value");
    }
}

/// Name of a `DatError` variant. Exhaustive with no wildcard arm, so a new
/// variant is a compile error here rather than a silently accepted value.
fn dat_error_name(error: &DatError) -> &'static str {
    match error {
        DatError::Truncated { .. } => "Truncated",
        DatError::LengthOutOfRange { .. } => "LengthOutOfRange",
        DatError::OutOfBounds { .. } => "OutOfBounds",
        DatError::OffsetOverflow => "OffsetOverflow",
        DatError::UnknownPayloadMode { .. } => "UnknownPayloadMode",
        DatError::OutputOverrun { .. } => "OutputOverrun",
        DatError::OutputShort { .. } => "OutputShort",
        DatError::ConsumptionMismatch { .. } => "ConsumptionMismatch",
        DatError::EmptyBlock { .. } => "EmptyBlock",
    }
}

/// A u32 container field as a `usize`, without a widening cast.
fn as_usize(value: u32, what: &str) -> usize {
    match usize::try_from(value) {
        Ok(converted) => converted,
        Err(_) => panic!("{what} does not fit a usize: {value}"),
    }
}

/// A count as a `u64`, without a widening cast.
fn as_u64(value: usize, what: &str) -> u64 {
    match u64::try_from(value) {
        Ok(converted) => converted,
        Err(_) => panic!("{what} does not fit a u64: {value}"),
    }
}

/// A length as a signed `i64`, so a difference between two lengths can be
/// negative.
fn as_i64(value: usize, what: &str) -> i64 {
    match i64::try_from(value) {
        Ok(converted) => converted,
        Err(_) => panic!("{what} does not fit an i64: {value}"),
    }
}

/// A count as a `u32`, without a narrowing cast.
fn as_u32(value: usize, what: &str) -> u32 {
    match u32::try_from(value) {
        Ok(converted) => converted,
        Err(_) => panic!("{what} does not fit a u32: {value}"),
    }
}

/// The first four bytes of a header, big endian (DAT-001).
fn be_u32(header: &[u8]) -> u32 {
    let four = present(header.get(..4), "a header needs four bytes");
    u32::from_be_bytes([four[0], four[1], four[2], four[3]])
}

/// One archive with the bytes it was parsed from.
struct Archive {
    /// Diagnostic label the archive is parsed under (DAT-030).
    origin: &'static str,
    /// The whole archive file.
    bytes: Vec<u8>,
    /// The production parse of its two level tree.
    tree: DatArchive,
}

impl Archive {
    /// Reads and parses one archive, keeping its bytes.
    fn load(install: &Install, is_master: bool) -> Self {
        let origin = if is_master {
            MASTER_DAT_FILE_NAME
        } else {
            CRITTER_DAT_FILE_NAME
        };
        let outcome = if is_master {
            install.read_master_dat()
        } else {
            install.read_critter_dat()
        };
        let bytes = step(outcome, "an archive could not be read");
        let tree = match DatArchive::parse(&bytes, origin) {
            Ok(parsed) => parsed,
            Err(error) => panic!("{origin} refused to parse: {}", dat_error_name(&error)),
        };
        Self {
            origin,
            bytes,
            tree,
        }
    }

    /// The independent reference walk of the same bytes (oracle class 4).
    fn reference(&self) -> RefDatTree {
        step(RefDatTree::walk(&self.bytes), "a reference walk failed")
    }
}

/// Both archives parsed and walked, with the bytes released.
///
/// `DatArchive` and `RefDatTree` own every string they report, so a test that
/// only inspects the tree does not need half a gigabyte of bytes alive.
struct Trees {
    /// MASTER.DAT production parse.
    master: DatArchive,
    /// CRITTER.DAT production parse.
    critter: DatArchive,
    /// MASTER.DAT reference walk.
    master_ref: RefDatTree,
    /// CRITTER.DAT reference walk.
    critter_ref: RefDatTree,
}

/// Both trees, bytes dropped.
///
/// The harness digests the executable and both archives every time it resolves an
/// install, so a test that needs both the fingerprint and the trees takes the
/// install once and passes it in.
fn trees() -> Trees {
    let install = install();
    trees_from(&install)
}

/// Both trees from an install the caller already holds, bytes dropped.
fn trees_from(install: &Install) -> Trees {
    let master = Archive::load(install, true);
    let critter = Archive::load(install, false);
    let master_ref = master.reference();
    let critter_ref = critter.reference();
    Trees {
        master: master.tree,
        critter: critter.tree,
        master_ref,
        critter_ref,
    }
}

/// One chain's block census, counted by header arithmetic alone.
///
/// No payload is decompressed, so this half of the closure stands without the
/// codec and cannot be graded with the code under test (DAT-022).
#[derive(Clone, Copy)]
struct ChainCensus {
    /// Blocks framed, of either kind.
    blocks: u64,
    /// Blocks whose header bit 15 is clear.
    lzss_blocks: u64,
    /// Blocks whose header bit 15 is set.
    raw_blocks: u64,
    /// Bytes the block headers account for, header widths included.
    consumed: u64,
}

/// Walks one chain by block headers, starting at the entry's own offset.
///
/// This is the test author's own framing reader (oracle class 4): the header is
/// two big endian bytes, bit 15 picks the kind, and the low fifteen bits give
/// the payload length, raw or compressed (DAT-018, DAT-027). A zero length
/// payload is an error (DAT-018), so the walk cannot spin on one.
fn walk_chain(archive: &[u8], entry: &EntryRecord) -> ChainCensus {
    let start = as_usize(entry.offset, "an entry offset");
    let declared = as_usize(entry.compressed_size, "an entry compressed size");
    let end = start
        .checked_add(declared)
        .expect("an entry extent overflows");

    let mut census = ChainCensus {
        blocks: 0,
        lzss_blocks: 0,
        raw_blocks: 0,
        consumed: 0,
    };
    let mut cursor = start;
    while cursor < end {
        let header_end = cursor
            .checked_add(BLOCK_HEADER_WIDTH)
            .expect("a block header offset overflows");
        let header = present(
            archive.get(cursor..header_end),
            "a block header lies past the declared extent",
        );
        let value = u16::from_be_bytes([header[0], header[1]]);
        let is_raw = value & BLOCK_RAW_FLAG != 0;
        let payload = usize::from(value & !BLOCK_RAW_FLAG);
        assert_ne!(
            payload, 0,
            "{}: a zero length block payload is an error",
            entry.path
        );
        cursor = header_end
            .checked_add(payload)
            .expect("a block payload offset overflows");
        let overrun = cursor > end;
        assert!(
            !overrun,
            "{}: a block runs past the declared compressed size {end}",
            entry.path
        );
        census.blocks += 1;
        census.lzss_blocks += u64::from(!is_raw);
        census.raw_blocks += u64::from(is_raw);
        census.consumed += as_u64(BLOCK_HEADER_WIDTH + payload, "one framed block");
    }
    assert_eq!(
        cursor, end,
        "{}: the block headers land exactly on the compressed size",
        entry.path
    );
    census
}

/// Counts entries per payload mode, through the production `payload_mode`.
fn mode_counts(entries: &[EntryRecord]) -> (usize, usize, usize) {
    let mut stored = 0usize;
    let mut chain = 0usize;
    let mut stream = 0usize;
    for entry in entries {
        match payload_mode(entry.flags) {
            PayloadMode::Stored => stored += 1,
            PayloadMode::BlockChain => chain += 1,
            PayloadMode::LzssStream => stream += 1,
        }
    }
    (stored, chain, stream)
}

/// Counts entries whose composed path carries a directory prefix, and those that
/// do not.
fn prefix_counts(entries: &[EntryRecord]) -> (usize, usize) {
    let mut bare = 0usize;
    let mut prefixed = 0usize;
    for entry in entries {
        if entry.path.contains('\\') {
            prefixed += 1;
        } else {
            bare += 1;
        }
    }
    (bare, prefixed)
}

/// The production archive's composed paths, sorted, for a set comparison.
fn composed_paths(archive: &DatArchive) -> Vec<String> {
    let mut paths: Vec<String> = archive.entries().iter().map(|e| e.path.clone()).collect();
    paths.sort();
    paths
}

/// The entries of one archive whose decoded extent runs past the end of the
/// archive, with the exact number of bytes each one overruns by, and the number
/// of entries whose *stored* extent runs past the end (DAT-021).
///
/// The stored bound is `offset + compressed_size` and the decoded bound is
/// `offset + uncompressed_size`. Both are measured against the archive's own
/// length, so what is graded is the file, not the parser's opinion of it.
fn extent_bounds(entries: &[EntryRecord], length: u64) -> (usize, Vec<(String, u64)>) {
    let mut compressed_violations = 0usize;
    let mut overruns: Vec<(String, u64)> = Vec::new();
    for entry in entries {
        let offset = as_u64(as_usize(entry.offset, "an entry offset"), "an entry offset");
        let compressed = as_u64(
            as_usize(entry.compressed_size, "an entry compressed size"),
            "an entry compressed size",
        );
        let uncompressed = as_u64(
            as_usize(entry.uncompressed_size, "an entry uncompressed size"),
            "an entry uncompressed size",
        );
        if offset + compressed > length {
            compressed_violations += 1;
        }
        if offset + uncompressed > length {
            overruns.push((entry.path.clone(), offset + uncompressed - length));
        }
    }
    (compressed_violations, overruns)
}

/// The expected overruns of one archive, as a sorted list of path and bytes.
fn expected_overruns(expected: &[(&str, u64)]) -> Vec<(String, u64)> {
    let mut sorted: Vec<(String, u64)> = expected
        .iter()
        .map(|(path, overrun)| ((*path).to_string(), *overrun))
        .collect();
    sorted.sort();
    sorted
}

/// Walks every chain entry of one archive.
///
/// Returns the chain count and the block census; `walk_chain` asserts the
/// consumption closure for each entry it walks.
fn chain_census(archive: &Archive) -> (usize, ChainCensus) {
    let mut chains = 0usize;
    let mut total = ChainCensus {
        blocks: 0,
        lzss_blocks: 0,
        raw_blocks: 0,
        consumed: 0,
    };
    for entry in archive.tree.entries() {
        if matches!(payload_mode(entry.flags), PayloadMode::BlockChain) {
            let census = walk_chain(&archive.bytes, entry);
            assert_eq!(
                census.consumed,
                u64::from(entry.compressed_size),
                "{}: the block headers account for exactly the compressed size",
                entry.path
            );
            total.blocks += census.blocks;
            total.lzss_blocks += census.lzss_blocks;
            total.raw_blocks += census.raw_blocks;
            total.consumed += census.consumed;
            chains += 1;
        }
    }
    (chains, total)
}

/// The loose bytes of one composed archive path, or `None` when the patch folder
/// holds no file at that path.
///
/// The composed path uses a backslash, which is a separator on Windows and a
/// legal character elsewhere, so the segments are split and rejoined with the
/// platform separator rather than handed over as one string.
fn loose_bytes(install: &Install, composed: &str) -> Option<Vec<u8>> {
    let relative: PathBuf = composed.split('\\').collect();
    step(
        install.read_loose_file(&relative),
        "a loose file could not be read",
    )
}

/// The declared uncompressed size of one composed path, from the archive that
/// holds it.
fn declared_size(path: &str, master: &DatArchive, critter: &DatArchive) -> u32 {
    match master.find(path) {
        Some(entry) => entry.uncompressed_size,
        None => match critter.find(path) {
            Some(entry) => entry.uncompressed_size,
            None => panic!("{path}: no archive holds an entry under that path"),
        },
    }
}

// --- DAT-025: the corpus the whole layer hangs from --------------------------

/// DAT-025: both archives parse, and their directory and entry counts are the
/// measured ones. Oracle class 2, cross-checked against oracle class 4.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_025_directory_and_entry_counts_are_exact() {
    let install = install();
    let trees = trees_from(&install);

    assert_eq!(
        trees.master.directories().len(),
        MASTER_DIRECTORIES,
        "MASTER.DAT directory count"
    );
    assert_eq!(
        trees.critter.directories().len(),
        CRITTER_DIRECTORIES,
        "CRITTER.DAT directory count"
    );
    assert_eq!(
        trees.master.entries().len(),
        MASTER_ENTRIES,
        "MASTER.DAT entry count"
    );
    assert_eq!(
        trees.critter.entries().len(),
        CRITTER_ENTRIES,
        "CRITTER.DAT entry count"
    );
    assert_eq!(
        MASTER_ENTRIES + CRITTER_ENTRIES,
        CORPUS_ENTRIES,
        "19784 plus 5459 is the corpus entry count"
    );
    assert_eq!(
        trees.master.directories().len() + trees.critter.directories().len(),
        CORPUS_BLOCK_HEADERS,
        "one block header per directory, across both archives"
    );

    // The same counts from the independent walker, which shares no code with
    // the production parser.
    assert_eq!(
        trees.master_ref.entries_total(),
        MASTER_ENTRIES,
        "MASTER.DAT entry count, reference walk"
    );
    assert_eq!(
        trees.critter_ref.entries_total(),
        CRITTER_ENTRIES,
        "CRITTER.DAT entry count, reference walk"
    );
    assert_eq!(
        trees.master_ref.directory_count(),
        MASTER_DIRECTORIES,
        "MASTER.DAT directory count, reference walk"
    );
    assert_eq!(
        trees.critter_ref.directory_count(),
        CRITTER_DIRECTORIES,
        "CRITTER.DAT directory count, reference walk"
    );
    assert_eq!(
        trees.master_ref.declared_directory_count(),
        as_u32(MASTER_DIRECTORIES, "the MASTER.DAT directory count"),
        "MASTER.DAT root field 0 is the directory count"
    );
    assert_eq!(
        trees.critter_ref.declared_directory_count(),
        as_u32(CRITTER_DIRECTORIES, "the CRITTER.DAT directory count"),
        "CRITTER.DAT root field 0 is the directory count"
    );
    assert_eq!(
        trees.master_ref.duplicate_paths().len(),
        0,
        "MASTER.DAT composes no duplicate path"
    );
    assert_eq!(
        trees.critter_ref.duplicate_paths().len(),
        0,
        "CRITTER.DAT composes no duplicate path"
    );

    // DAT-025 file sizes, which the harness already checked, asserted here
    // against the ledger so a relaxed harness fails too.
    assert_eq!(
        install.master_dat().bytes,
        REFERENCE_MASTER_DAT_BYTES,
        "MASTER.DAT size in bytes"
    );
    assert_eq!(
        install.critter_dat().bytes,
        REFERENCE_CRITTER_DAT_BYTES,
        "CRITTER.DAT size in bytes"
    );
}

/// DAT-026 and DAT-013: payload mode counts are exact per archive and corpus
/// wide, and the mode arithmetic closes. Oracle class 2.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_026_payload_mode_counts_are_exact() {
    let trees = trees();

    let (master_stored, master_chain, master_stream) = mode_counts(trees.master.entries());
    let (critter_stored, critter_chain, critter_stream) = mode_counts(trees.critter.entries());

    assert_eq!(
        master_stored, MASTER_STORED_ENTRIES,
        "MASTER.DAT stored entries"
    );
    assert_eq!(
        critter_stored, CRITTER_STORED_ENTRIES,
        "CRITTER.DAT stored entries"
    );
    assert_eq!(
        master_chain, MASTER_CHAIN_ENTRIES,
        "MASTER.DAT chain entries"
    );
    assert_eq!(
        critter_chain, CRITTER_CHAIN_ENTRIES,
        "CRITTER.DAT chain entries"
    );
    assert_eq!(
        master_stored + critter_stored,
        STORED_ENTRIES,
        "corpus stored entries"
    );
    assert_eq!(
        master_chain + critter_chain,
        CHAIN_ENTRIES,
        "corpus chain entries"
    );
    assert_eq!(
        master_stream + critter_stream,
        LZSS_STREAM_ENTRIES,
        "corpus entries in mode 0x10"
    );
    assert_eq!(
        STORED_ENTRIES + CHAIN_ENTRIES + LZSS_STREAM_ENTRIES,
        CORPUS_ENTRIES,
        "4787 plus 20456 plus 0 is the corpus entry count"
    );

    // The independent walker counts the same modes from the raw flags word.
    assert_eq!(
        trees.master_ref.count_by_mode(0x20),
        MASTER_STORED_ENTRIES,
        "MASTER.DAT stored entries, reference walk"
    );
    assert_eq!(
        trees.critter_ref.count_by_mode(0x20),
        CRITTER_STORED_ENTRIES,
        "CRITTER.DAT stored entries, reference walk"
    );
    assert_eq!(
        trees.master_ref.count_by_mode(0x40),
        MASTER_CHAIN_ENTRIES,
        "MASTER.DAT chain entries, reference walk"
    );
    assert_eq!(
        trees.critter_ref.count_by_mode(0x40),
        CRITTER_CHAIN_ENTRIES,
        "CRITTER.DAT chain entries, reference walk"
    );
    assert_eq!(
        trees.master_ref.count_by_mode(0x10),
        LZSS_STREAM_ENTRIES,
        "MASTER.DAT entries in mode 0x10, reference walk"
    );
    assert_eq!(
        trees.critter_ref.count_by_mode(0x10),
        LZSS_STREAM_ENTRIES,
        "CRITTER.DAT entries in mode 0x10, reference walk"
    );
}

/// DAT-017 and DAT-013: the stored and chain split.
///
/// Every stored entry carries a zero compressed size and every chain entry a
/// nonzero one, and the count of each class is exact. This is an exhaustive
/// invariant over all 25,243 entries, not a sample. Oracle class 2.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_017_stored_and_chain_entries_split_on_their_compressed_size() {
    let trees = trees();
    let mut stored = 0usize;
    let mut chain = 0usize;
    let mut stream = 0usize;

    for entries in [trees.master.entries(), trees.critter.entries()] {
        for entry in entries {
            match payload_mode(entry.flags) {
                PayloadMode::Stored => {
                    assert_eq!(
                        entry.compressed_size, 0,
                        "{}: a stored entry has compressed size 0",
                        entry.path
                    );
                    stored += 1;
                }
                PayloadMode::BlockChain => {
                    assert_ne!(
                        entry.compressed_size, 0,
                        "{}: a chain entry has a nonzero compressed size",
                        entry.path
                    );
                    chain += 1;
                }
                PayloadMode::LzssStream => stream += 1,
            }
        }
    }

    assert_eq!(stored, STORED_ENTRIES, "corpus stored entries");
    assert_eq!(chain, CHAIN_ENTRIES, "corpus chain entries");
    assert_eq!(stream, LZSS_STREAM_ENTRIES, "corpus entries in mode 0x10");
    assert_eq!(
        stored - STORED_WITH_NONZERO_COMPRESSED,
        STORED_ENTRIES,
        "no stored entry has a nonzero compressed size"
    );
    assert_eq!(
        chain - CHAIN_WITH_ZERO_COMPRESSED,
        CHAIN_ENTRIES,
        "no chain entry has a zero compressed size"
    );
    assert_eq!(
        stored + chain + stream,
        CORPUS_ENTRIES,
        "every entry falls in exactly one mode class"
    );
}

/// DAT-021: the on-disk extent of an entry is bounded by its compressed size.
///
/// `offset + compressed_size` never runs past the end of the archive, for any of
/// the 25,243 entries in both archives. The decoded extent,
/// `offset + uncompressed_size`, does run past it for seven entries, and those
/// seven are exactly the files the spec names, with the exact number of bytes
/// each one overruns by. A reader that bounded a stored region by the
/// uncompressed size would reject all seven (`docs/spec/dat.md`, DAT-021).
///
/// The corpus walk is the production parse; the seven entries are cross-checked
/// against the independent reference walk (oracle class 4), so a parser that
/// reported a wrong offset fails here instead of passing a self-consistent but
/// wrong answer.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_021_the_stored_extent_is_bounded_by_the_compressed_size() {
    let install = install();
    let trees = trees_from(&install);

    let master_length = install.master_dat().bytes;
    let critter_length = install.critter_dat().bytes;

    let (master_violations, master_overruns) = extent_bounds(trees.master.entries(), master_length);
    let (critter_violations, critter_overruns) =
        extent_bounds(trees.critter.entries(), critter_length);

    assert_eq!(
        trees.master.entries().len() + trees.critter.entries().len(),
        CORPUS_ENTRIES,
        "entries walked across both archives"
    );
    assert_eq!(
        trees.master_ref.entries_total() + trees.critter_ref.entries_total(),
        CORPUS_ENTRIES,
        "entries walked, reference walk"
    );
    assert_eq!(
        master_violations + critter_violations,
        0,
        "entries whose stored extent runs past the end of their archive"
    );

    let mut measured_master = master_overruns;
    let mut measured_critter = critter_overruns;
    measured_master.sort();
    measured_critter.sort();

    assert_eq!(
        measured_master.len(),
        MASTER_UNCOMPRESSED_OVERRUNS.len(),
        "MASTER.DAT entries whose decoded extent runs past the end of the archive"
    );
    assert_eq!(
        measured_critter.len(),
        CRITTER_UNCOMPRESSED_OVERRUNS.len(),
        "CRITTER.DAT entries whose decoded extent runs past the end of the archive"
    );
    assert_eq!(
        measured_master,
        expected_overruns(&MASTER_UNCOMPRESSED_OVERRUNS),
        "the MASTER.DAT entries that overrun, and by how many bytes"
    );
    assert_eq!(
        measured_critter,
        expected_overruns(&CRITTER_UNCOMPRESSED_OVERRUNS),
        "the CRITTER.DAT entries that overrun, and by how many bytes"
    );

    // Oracle class 4: the independent reference walk puts the same seven entries
    // at the same offsets, and all seven are chain entries, so the overrun is a
    // fact about the file and not about one reader's opinion of it (DAT-016).
    for (path, overrun) in MASTER_UNCOMPRESSED_OVERRUNS {
        let entry = step(
            trees.master_ref.find(path),
            "the reference walk has no such MASTER.DAT path",
        );
        let offset = as_u64(as_usize(entry.offset, "an entry offset"), "an entry offset");
        let uncompressed = as_u64(
            as_usize(entry.uncompressed_size, "an entry uncompressed size"),
            "an entry uncompressed size",
        );
        assert_eq!(
            offset + uncompressed,
            master_length + overrun,
            "{path}: the reference walk puts the decoded extent exactly {overrun} bytes past the end of MASTER.DAT"
        );
        assert_eq!(entry.mode(), 0x40, "{path}: the entry is a block chain");
    }
    for (path, overrun) in CRITTER_UNCOMPRESSED_OVERRUNS {
        let entry = step(
            trees.critter_ref.find(path),
            "the reference walk has no such CRITTER.DAT path",
        );
        let offset = as_u64(as_usize(entry.offset, "an entry offset"), "an entry offset");
        let uncompressed = as_u64(
            as_usize(entry.uncompressed_size, "an entry uncompressed size"),
            "an entry uncompressed size",
        );
        assert_eq!(
            offset + uncompressed,
            critter_length + overrun,
            "{path}: the reference walk puts the decoded extent exactly {overrun} bytes past the end of CRITTER.DAT"
        );
        assert_eq!(entry.mode(), 0x40, "{path}: the entry is a block chain");
    }
}

/// DAT-022 and DAT-020: for all 20,456 chains the block headers account for the
/// compressed size exactly, and the block census is the measured one.
///
/// The walk is header arithmetic only, so this half of the closure is
/// established by structure and cannot be graded with the codec (oracle class 2,
/// worked by oracle class 4).
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_022_every_block_chain_consumes_exactly_its_compressed_size() {
    let install = install();
    let master = Archive::load(&install, true);
    let critter = Archive::load(&install, false);

    let (master_chains, master_census) = chain_census(&master);
    let (critter_chains, critter_census) = chain_census(&critter);

    assert_eq!(
        master_chains, MASTER_CHAIN_ENTRIES,
        "MASTER.DAT chain entries"
    );
    assert_eq!(
        critter_chains, CRITTER_CHAIN_ENTRIES,
        "CRITTER.DAT chain entries"
    );
    assert_eq!(
        master_chains + critter_chains,
        CHAIN_ENTRIES,
        "corpus chain entries"
    );

    assert_eq!(master_census.blocks, MASTER_BLOCKS, "MASTER.DAT blocks");
    assert_eq!(critter_census.blocks, CRITTER_BLOCKS, "CRITTER.DAT blocks");
    assert_eq!(
        master_census.blocks + critter_census.blocks,
        CORPUS_BLOCKS,
        "corpus blocks"
    );

    assert_eq!(
        master_census.lzss_blocks, MASTER_LZSS_BLOCKS,
        "MASTER.DAT LZSS blocks"
    );
    assert_eq!(
        critter_census.lzss_blocks, CRITTER_LZSS_BLOCKS,
        "CRITTER.DAT LZSS blocks"
    );
    assert_eq!(
        master_census.lzss_blocks + critter_census.lzss_blocks,
        CORPUS_LZSS_BLOCKS,
        "corpus LZSS blocks"
    );

    assert_eq!(
        master_census.raw_blocks, MASTER_RAW_BLOCKS,
        "MASTER.DAT raw blocks"
    );
    assert_eq!(
        critter_census.raw_blocks, CRITTER_RAW_BLOCKS,
        "CRITTER.DAT raw blocks, which are none"
    );
    assert_eq!(
        master_census.raw_blocks + critter_census.raw_blocks,
        CORPUS_RAW_BLOCKS,
        "corpus raw blocks"
    );
    assert_eq!(
        CORPUS_LZSS_BLOCKS + CORPUS_RAW_BLOCKS,
        CORPUS_BLOCKS,
        "61070 plus 6593 is the corpus block count"
    );

    // A raw block sits inside a chain entry, so the kind is read per block and
    // not per chain (DAT-020). The exact counts of the two kinds are asserted,
    // over the whole corpus, so a reader that stopped testing the header bit
    // per block fails on a number and not on a non-zero check.
    let mut mixed = 0usize;
    let mut raw_only = 0usize;
    for archive in [&master, &critter] {
        for entry in archive.tree.entries() {
            if matches!(payload_mode(entry.flags), PayloadMode::BlockChain) {
                let census = walk_chain(&archive.bytes, entry);
                if census.raw_blocks != 0 && census.lzss_blocks != 0 {
                    mixed += 1;
                } else if census.raw_blocks != 0 {
                    raw_only += 1;
                }
            }
        }
    }
    assert_eq!(
        mixed, MIXED_CHAIN_COUNT,
        "DAT-020: chain entries that hold a raw block and an LZSS block both, over \
         the whole corpus"
    );
    assert_eq!(
        raw_only, RAW_ONLY_CHAIN_COUNT,
        "DAT-020: chain entries that hold a raw block and no LZSS block, over the \
         whole corpus"
    );
}

/// DAT-020: the `ChainStats` the production decoder reports, aggregated over all
/// 20,456 chains, equal the framing census.
///
/// Two independent passes have to agree: the test author's header walk and the
/// production chain driver. Oracle class 2, cross-checked with class 4.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_020_chain_stats_totals_equal_the_framing_census() {
    let install = install();
    let master = Archive::load(&install, true);
    let critter = Archive::load(&install, false);

    let mut chains = 0usize;
    let mut blocks = 0u64;
    let mut lzss_blocks = 0u64;
    let mut raw_blocks = 0u64;

    for archive in [&master, &critter] {
        for entry in archive.tree.entries() {
            if matches!(payload_mode(entry.flags), PayloadMode::BlockChain) {
                let reported = match decode_entry_with_stats(entry, &archive.bytes) {
                    Ok((_produced, stats)) => stats,
                    Err(error) => panic!(
                        "{}: decode with stats refused the entry with {}",
                        entry.path,
                        dat_error_name(&error)
                    ),
                };
                let walked = walk_chain(&archive.bytes, entry);
                assert_eq!(
                    u64::from(reported.blocks),
                    walked.blocks,
                    "{}: ChainStats block count equals the framing walk",
                    entry.path
                );
                assert_eq!(
                    u64::from(reported.lzss_blocks),
                    walked.lzss_blocks,
                    "{}: ChainStats LZSS block count equals the framing walk",
                    entry.path
                );
                assert_eq!(
                    u64::from(reported.raw_blocks),
                    walked.raw_blocks,
                    "{}: ChainStats raw block count equals the framing walk",
                    entry.path
                );
                blocks += u64::from(reported.blocks);
                lzss_blocks += u64::from(reported.lzss_blocks);
                raw_blocks += u64::from(reported.raw_blocks);
                chains += 1;
            }
        }
    }

    assert_eq!(chains, CHAIN_ENTRIES, "chain entries decoded with stats");
    assert_eq!(blocks, CORPUS_BLOCKS, "ChainStats corpus block total");
    assert_eq!(
        lzss_blocks, CORPUS_LZSS_BLOCKS,
        "ChainStats corpus LZSS block total"
    );
    assert_eq!(
        raw_blocks, CORPUS_RAW_BLOCKS,
        "ChainStats corpus raw block total"
    );
    assert_eq!(
        lzss_blocks + raw_blocks,
        blocks,
        "every framed block is one kind or the other"
    );
}

/// DAT-023, DAT-024 and LZSS-014: **every one of the 25,243 entries** decodes to
/// exactly its declared uncompressed size, and the chain corpus decodes to
/// exactly the byte total the spec measured.
///
/// This is the load bearing test of the archive layer. It is slow and it is not
/// to be weakened: a decoder that stops at the declared size produces the
/// declared size for every input, including garbage, and so passes a size gate
/// while being wrong (guide: No truncation, and a consumption check).
///
/// The other half of the closure, `consumed == compressed_size`, is asserted
/// structurally in `dat_022_every_block_chain_consumes_exactly_its_compressed_size`,
/// where every chain's block headers land exactly on its compressed size, and
/// `decode_entry` answers `ConsumptionMismatch` rather than handing back a
/// buffer when they do not.
///
/// Measured on the reference install: the 20,456 chain entries produce
/// 851,322,391 bytes and the 4,787 stored entries produce their own declared
/// sizes, for 918,257,056 bytes over all 25,243 entries. All three totals are
/// asserted here, so a decoder that produced a different number of bytes while
/// still producing the right length for every entry fails.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_023_every_entry_decodes_to_its_declared_uncompressed_size() {
    let install = install();
    let master = Archive::load(&install, true);
    let critter = Archive::load(&install, false);

    let mut checked = 0usize;
    let mut produced_total = 0u64;
    let mut chain_produced = 0u64;
    let mut stored_produced = 0u64;

    for archive in [&master, &critter] {
        for entry in archive.tree.entries() {
            let decoded = match decode_entry(entry, &archive.bytes) {
                Ok(bytes) => bytes,
                Err(error) => panic!(
                    "{}: decode refused the entry with {}",
                    entry.path,
                    dat_error_name(&error)
                ),
            };
            let produced = as_u64(decoded.len(), "a produced output length");
            assert_eq!(
                produced,
                u64::from(entry.uncompressed_size),
                "{}: produced output must equal the declared uncompressed size",
                entry.path
            );
            produced_total += produced;
            if matches!(payload_mode(entry.flags), PayloadMode::BlockChain) {
                chain_produced += produced;
            } else {
                stored_produced += produced;
            }
            checked += 1;
        }
    }

    assert_eq!(
        checked, CORPUS_ENTRIES,
        "entries decoded; they produced {produced_total} bytes in total"
    );
    assert_eq!(
        chain_produced, CHAIN_OUTPUT_BYTES,
        "chain output bytes, the figure docs/spec/lzss.md measured"
    );
    assert_eq!(
        stored_produced, STORED_OUTPUT_BYTES,
        "stored output bytes, the sum of the stored entries' declared sizes"
    );
    assert_eq!(
        produced_total, CORPUS_OUTPUT_BYTES,
        "output bytes over every entry of both archives, the chain half and the \
         stored half added together"
    );
    assert_eq!(
        chain_produced + stored_produced,
        produced_total,
        "the chain half and the stored half add up to the corpus total"
    );
    assert_eq!(
        STORED_ENTRIES + CHAIN_ENTRIES,
        checked,
        "every entry of both archives was decoded"
    );
}

/// DAT-005 and DAT-006: block header field 0 is the entry count at all 66 block
/// headers, the per archive sums equal the archive entry totals, and field 2 is
/// 16 at every one of them.
///
/// The fields are read straight out of the bytes at the offsets the reference
/// walker reports, so this measures the file rather than restating the parser's
/// own view (oracle class 2, offsets from class 4).
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_005_block_field_zero_is_the_entry_count_and_field_two_is_sixteen() {
    let install = install();
    let master = Archive::load(&install, true);
    let critter = Archive::load(&install, false);
    let master_ref = master.reference();
    let critter_ref = critter.reference();

    let mut headers = 0usize;
    let mut master_sum = 0u64;
    let mut critter_sum = 0u64;

    for (archive, reference, is_master) in [
        (&master, &master_ref, true),
        (&critter, &critter_ref, false),
    ] {
        for directory in reference.directories() {
            let at = directory.header_offset;
            let end = at
                .checked_add(TREE_HEADER_WIDTH)
                .expect("a block header offset overflows");
            let raw = present(
                archive.bytes.get(at..end),
                "a block header lies past the archive",
            );

            let field0 = be_u32(raw);
            let field2 = be_u32(present(raw.get(8..12), "a block header needs twelve bytes"));

            assert_eq!(
                as_usize(field0, "a block field 0"),
                directory.entries.len(),
                "{} block {}: field 0 is this block's own entry count",
                archive.origin,
                directory.index
            );
            assert_eq!(
                field2, BLOCK_FIELD_2,
                "{} block {}: field 2 is the entry trailer width",
                archive.origin, directory.index
            );

            if is_master {
                master_sum += u64::from(field0);
            } else {
                critter_sum += u64::from(field0);
            }
            headers += 1;
        }
    }

    assert_eq!(headers, CORPUS_BLOCK_HEADERS, "block headers read");
    assert_eq!(
        master_sum,
        as_u64(MASTER_ENTRIES, "the MASTER.DAT entry count"),
        "MASTER.DAT: block field 0 sums to the archive entry count"
    );
    assert_eq!(
        critter_sum,
        as_u64(CRITTER_ENTRIES, "the CRITTER.DAT entry count"),
        "CRITTER.DAT: block field 0 sums to the archive entry count"
    );
    assert_eq!(
        master.tree.directories()[0].entry_count,
        as_u32(ROOT_DIRECTORY_ENTRIES, "the root directory entry count"),
        "MASTER.DAT block 0 declares 11 entries, which is not its directory count"
    );
    assert_eq!(
        critter.tree.directories()[0].entry_count,
        as_u32(CRITTER_ENTRIES, "the CRITTER.DAT block entry count"),
        "CRITTER.DAT block 0 declares 5459 entries"
    );
}

/// DAT-011: path composition keys on the directory name, not on the block index.
///
/// MASTER.DAT's block 0 is the directory literally named `.` and yields 11 bare
/// names; CRITTER.DAT's only block is named `ART\CRITTERS` and all 5,459 of its
/// entries keep the prefix. A rule keyed on block index would strip the prefix
/// from every CRITTER path, which is the disproof DAT-011 records.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_011_path_composition_keys_on_the_directory_name() {
    let trees = trees();

    assert_eq!(
        trees.master.directories()[0].name,
        ROOT_DIRECTORY_NAME,
        "MASTER.DAT block 0 is the directory named ."
    );
    assert_eq!(
        trees.master.directories()[0].entry_count,
        as_u32(ROOT_DIRECTORY_ENTRIES, "the root directory entry count"),
        "the . directory holds 11 entries"
    );
    assert_eq!(
        trees.critter.directories()[0].name,
        CRITTER_DIRECTORY_NAME,
        "CRITTER.DAT block 0 is ART\\CRITTERS"
    );
    assert_eq!(
        trees.critter.directories()[0].entry_count,
        as_u32(CRITTER_ENTRIES, "the CRITTER.DAT block entry count"),
        "CRITTER.DAT has one directory holding 5459 entries"
    );

    let (master_bare, master_prefixed) = prefix_counts(trees.master.entries());
    assert_eq!(master_bare, ROOT_DIRECTORY_ENTRIES, "MASTER.DAT bare names");
    assert_eq!(
        master_prefixed, MASTER_PREFIXED_ENTRIES,
        "MASTER.DAT prefixed names"
    );
    assert_eq!(
        master_bare + master_prefixed,
        MASTER_ENTRIES,
        "every MASTER.DAT entry is bare or prefixed"
    );

    let (critter_bare, critter_prefixed) = prefix_counts(trees.critter.entries());
    assert_eq!(critter_bare, 0, "CRITTER.DAT has no bare names");
    assert_eq!(
        critter_prefixed, CRITTER_ENTRIES,
        "all 5459 CRITTER.DAT paths carry the ART\\CRITTERS prefix"
    );
    assert_eq!(
        critter_bare + critter_prefixed,
        CRITTER_ENTRIES,
        "every CRITTER.DAT entry is bare or prefixed"
    );

    // The production composition and the reference composition are the same set
    // of paths, which is what makes a lookup table sound.
    assert_eq!(
        composed_paths(&trees.master),
        trees.master_ref.paths(),
        "MASTER.DAT composed paths, production against reference"
    );
    assert_eq!(
        composed_paths(&trees.critter),
        trees.critter_ref.paths(),
        "CRITTER.DAT composed paths, production against reference"
    );
}

/// DAT-029, DAT-012 and DAT-010: a real lookup on both archives, and the
/// uppercase rule.
///
/// `COLOR.PAL` is the first entry of MASTER.DAT and declares 33,536 bytes, the
/// palette size the guide states from a completely different direction
/// (DAT-029). Its offset of 211,060,669 is the proof that tree order is not
/// physical order (DAT-012).
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_029_real_lookups_return_the_measured_entries() {
    let trees = trees();

    let color = present(trees.master.find("COLOR.PAL"), "COLOR.PAL in MASTER.DAT");
    assert_eq!(color.path, "COLOR.PAL", "the found entry's path");
    assert_eq!(
        color.uncompressed_size, COLOR_PAL_UNCOMPRESSED,
        "COLOR.PAL declares 33536 uncompressed bytes"
    );
    assert_eq!(
        color.offset, COLOR_PAL_OFFSET,
        "COLOR.PAL sits at offset 211060669"
    );
    assert_eq!(
        color.compressed_size, COLOR_PAL_COMPRESSED,
        "COLOR.PAL stores 9292 bytes"
    );
    assert!(
        matches!(payload_mode(color.flags), PayloadMode::BlockChain),
        "COLOR.PAL is a chain, so its decoded form is the palette"
    );

    let critters = present(
        trees.critter.find("ART\\CRITTERS\\CRITTERS.LST"),
        "CRITTERS.LST in CRITTER.DAT",
    );
    assert_eq!(
        critters.path, "ART\\CRITTERS\\CRITTERS.LST",
        "the found entry's composed path"
    );
    assert_eq!(
        critters.offset, CRITTERS_LST_OFFSET,
        "CRITTERS.LST sits at offset 158356"
    );
    assert_eq!(
        critters.uncompressed_size, CRITTERS_LST_UNCOMPRESSED,
        "CRITTERS.LST declares 646 uncompressed bytes"
    );
    assert_eq!(
        critters.compressed_size, CRITTERS_LST_COMPRESSED,
        "CRITTERS.LST stores 405 bytes"
    );

    // DAT-010: the caller uppercases, so a lowercased path is not a second
    // lookup and no locale is consulted.
    assert!(
        trees.master.find("color.pal").is_none(),
        "lookup is on the uppercase form only"
    );
    assert!(
        trees.master.find("FONT0.AAF").is_some(),
        "a bare root level name resolves against the . directory"
    );
}

/// The three loose files whose size does not match their archive entry, listed
/// by name with their exact byte difference.
///
/// The guide's "Archive gate" section names the three: `GABRIEL.INT` is 26
/// bytes larger, `KILLIAN.INT` is 14 bytes smaller and `RAZOR.MSG` is 25 bytes
/// smaller. A loose file whose path matches an archive entry but whose length
/// does not is dropped by the size match, so it never becomes a candidate;
/// listing it with its exact difference is the alternative to a silent skip.
///
/// The three names and the three differences are asserted one by one, so a set
/// that grew, shrank or carried the wrong sign fails.
///
/// claim: DAT-028
/// oracle: class 2 (closure on real data: the loose file and the archive entry
/// are read from the install) and class 5 (the exact byte differences the
/// guide's "Archive gate" section writes down)
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_028_the_three_loose_size_exclusions_are_exact() {
    let install = install();
    let trees = trees_from(&install);

    let mut excluded = 0usize;
    for (path, expected_difference) in SIZE_EXCLUSIONS {
        let loose = present(
            loose_bytes(&install, path),
            "a size exclusion has no loose copy in the patch folder",
        );
        let declared = declared_size(path, &trees.master, &trees.critter);
        let difference = as_i64(loose.len(), "a loose file's length")
            - as_i64(
                as_usize(declared, "a declared uncompressed size"),
                "a declared uncompressed size",
            );
        assert_eq!(
            difference, expected_difference,
            "{path}: the loose file's length minus the archive entry's declared \
             uncompressed size"
        );
        excluded += 1;
    }
    assert_eq!(excluded, SIZE_EXCLUSION_COUNT, "size exclusions listed");
}

/// LZSS-013 and LZSS-014: a stream one byte short and a stream one byte long
/// each return an error.
///
/// The block is real: `FONT0.AAF`'s single LZSS block, 2,314 compressed input
/// bytes producing 6,337 (oracle class 5, written out in `docs/spec/lzss.md`).
/// The production decoder is graded against `fo1_tests::RefLzss`, the
/// independent reference decoder (oracle class 4), on the exact input first.
#[test]
#[ignore = "needs FO1_DATA"]
fn lzss_014_a_stream_one_byte_short_and_one_byte_long_both_fail() {
    let install = install();
    let master = Archive::load(&install, true);
    let reference = master.reference();

    let entry = step(
        reference.find(LZSS_VECTOR_PATH),
        "FONT0.AAF is not in the reference walk",
    );
    assert_eq!(
        as_usize(entry.compressed_size, "a compressed size"),
        LZSS_VECTOR_COMPRESSED,
        "FONT0.AAF stores 2316 framed bytes"
    );
    assert_eq!(
        as_usize(entry.uncompressed_size, "an uncompressed size"),
        LZSS_VECTOR_PRODUCED,
        "FONT0.AAF declares 6337 uncompressed bytes"
    );

    let extent = step(
        reference.raw_extent(&master.bytes, LZSS_VECTOR_PATH),
        "the FONT0.AAF raw extent failed",
    );
    assert_eq!(extent.len(), LZSS_VECTOR_COMPRESSED, "raw extent length");

    let production_entry = present(
        master.tree.find(LZSS_VECTOR_PATH),
        "FONT0.AAF in the production parse",
    );
    let census = walk_chain(&master.bytes, production_entry);
    assert_eq!(census.blocks, 1, "FONT0.AAF is one framed block");
    assert_eq!(census.lzss_blocks, 1, "that block is LZSS, not raw");
    assert_eq!(census.raw_blocks, 0, "FONT0.AAF holds no raw block");
    assert_eq!(
        census.consumed,
        as_u64(LZSS_VECTOR_COMPRESSED, "the stored extent"),
        "the framing walk accounts for all 2316 stored bytes"
    );

    let payload = present(
        extent.get(BLOCK_HEADER_WIDTH..),
        "FONT0.AAF has no block payload",
    );
    assert_eq!(
        payload.len(),
        LZSS_VECTOR_BLOCK_PAYLOAD,
        "the block payload is 2314 compressed bytes"
    );

    // Oracle class 4: the independent reference decoder's produced length is
    // what the declared size is checked against.
    let expected = step(
        RefLzss::canonical().decode(payload, LZSS_VECTOR_PRODUCED),
        "the reference decoder refused the exact block",
    );
    assert_eq!(
        expected.len(),
        LZSS_VECTOR_PRODUCED,
        "the reference decoder produces exactly the declared size"
    );

    let exact = step(
        decode_into(payload, LZSS_VECTOR_PRODUCED),
        "fo1-lzss refused the exact block",
    );
    assert_eq!(
        exact, expected,
        "fo1-lzss and the independent reference decoder agree byte for byte"
    );

    // One byte short. The stream ends before the declared size is reached, which
    // is an error and never a padded output (LZSS-014).
    let short = present(
        payload.get(..payload.len() - 1),
        "the block payload is empty",
    );
    assert_eq!(
        short.len(),
        LZSS_VECTOR_BLOCK_PAYLOAD - 1,
        "the short input is one byte shorter"
    );
    must_fail(
        decode_into(short, LZSS_VECTOR_PRODUCED),
        "the FONT0.AAF block one byte short",
    );

    // One byte long. The extra byte becomes a control byte with no tokens behind
    // it, which is an error and never a silently discarded tail (LZSS-014).
    let mut long_input = Vec::with_capacity(payload.len() + 1);
    long_input.extend_from_slice(payload);
    long_input.push(payload[0]);
    assert_eq!(
        long_input.len(),
        LZSS_VECTOR_BLOCK_PAYLOAD + 1,
        "the long input is one byte longer"
    );
    must_fail(
        decode_into(&long_input, LZSS_VECTOR_PRODUCED),
        "the FONT0.AAF block one byte long",
    );
}

/// DAT-025 and the guide's Fixtures section: the install fingerprint is exact.
/// Oracle class 2.
#[test]
#[ignore = "needs FO1_DATA"]
fn the_install_fingerprint_is_exact() {
    let install = install();

    assert_eq!(
        install.exe().sha256.as_deref(),
        Some(REFERENCE_EXE_SHA256),
        "falloutw.exe sha256"
    );
    assert_eq!(
        install.master_dat().bytes,
        REFERENCE_MASTER_DAT_BYTES,
        "MASTER.DAT size in bytes"
    );
    assert_eq!(
        install.critter_dat().bytes,
        REFERENCE_CRITTER_DAT_BYTES,
        "CRITTER.DAT size in bytes"
    );
    assert_eq!(
        install
            .exe_path()
            .file_name()
            .and_then(|name| name.to_str()),
        Some(EXE_FILE_NAME),
        "the fingerprinted executable is falloutw.exe"
    );
    assert_eq!(
        install
            .master_dat_path()
            .file_name()
            .and_then(|name| name.to_str()),
        Some(MASTER_DAT_FILE_NAME),
        "the fingerprinted main archive is MASTER.DAT"
    );
    assert_eq!(
        install
            .critter_dat_path()
            .file_name()
            .and_then(|name| name.to_str()),
        Some(CRITTER_DAT_FILE_NAME),
        "the fingerprinted critter archive is CRITTER.DAT"
    );
    assert_eq!(
        install.exe().role,
        EXE_FILE_NAME,
        "the executable fingerprint names its own role"
    );
    // The archives are checked by size alone. Their fingerprint therefore
    // carries no digest: hashing 496 MB of archive per test bought nothing,
    // because the digests were never compared against anything. The size is
    // the whole check.
    assert_eq!(
        install.master_dat().sha256,
        None,
        "MASTER.DAT is checked by size, not by digest"
    );
    assert_eq!(
        install.critter_dat().sha256,
        None,
        "CRITTER.DAT is checked by size, not by digest"
    );
    assert!(
        install.has_loose_data_dir(),
        "the reference install carries the loose DATA folder the patch left"
    );
}

/// DAT-025: the gate is only as good as its refusal. This proves the harness
/// rejects an install whose executable is not the reference one.
///
/// DAT-025 pins the counts and the sizes for this install and says what
/// happens on any other one: the gate reports `unsupported install` and does
/// not relax them. The fingerprint test above pins the accepted side; this
/// test pins the refused side, so a harness that stopped checking would fail
/// here instead of quietly grading a foreign install.
///
/// The fixture is a directory of deliberately small files named after the three
/// install files, created under the platform temp directory and removed again.
/// It is not game data and it is not committed; it exists so that a harness
/// which stopped checking would fail here (guide: Fixtures and golden files,
/// "unsupported install").
#[test]
#[ignore = "needs FO1_DATA"]
fn the_harness_rejects_a_foreign_install() {
    let scratch: PathBuf = std::env::temp_dir().join("synthetic_foreign_install");
    if scratch.is_dir() {
        step(
            std::fs::remove_dir_all(&scratch),
            "could not clear the scratch install dir",
        );
    }
    step(
        std::fs::create_dir_all(&scratch),
        "could not create the scratch install dir",
    );
    for name in INSTALL_FILE_NAMES {
        step(
            std::fs::write(scratch.join(name), b"synthetic"),
            "could not write a scratch install file",
        );
    }

    let outcome = Install::at(scratch.clone());
    let cleaned = std::fs::remove_dir_all(&scratch);
    assert!(cleaned.is_ok(), "the scratch install dir was removed again");

    match outcome {
        Err(HarnessError::UnsupportedInstall { what, .. }) => {
            assert!(
                what.contains("exe"),
                "the refusal names the executable digest, not the archive size: {what}"
            );
        }
        Err(other) => {
            panic!("the harness refused the foreign install for the wrong reason: {other}")
        }
        Ok(_) => {
            panic!("the harness accepted an install whose executable is not the reference one")
        }
    }
}
