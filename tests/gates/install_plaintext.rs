//! Known plaintext gate for the archive layer, run against the reference install.
//!
//! **Oracle class 3 (known plaintext).** The bytes of a loose file in the
//! player's `DATA` patch folder are ground truth that no decoder produced, so
//! comparing an archive decode against them grades the decoder with something
//! outside it. The expected byte total is not taken from the decoder's own
//! output and not from a snapshot of one.
//!
//! Two supporting oracles:
//!
//! - **oracle class 4**, `fo1_tests::RefDatTree`, the independent reference tree
//!   walker, which locates every entry and every stored extent;
//! - **oracle class 6**, algebraic laws: the candidate set is the set that
//!   discovery finds, the two buckets partition it, and a decode length equals
//!   the declared uncompressed size.
//!
//! # Why loose files at all
//!
//! The engine reads loose files from a patch folder before the archives, and
//! the reference install's `DATA` folder was rewritten by a high resolution
//! patch. So a loose file is *not* proof on its own: three of the candidates
//! here are patched copies and three more are a different size. That is exactly
//! why the set is pinned by name, why 28 must match byte for byte, and why the
//! three patch divergences are asserted at their first differing byte instead
//! of being skipped (guide: Exact assertions; DAT-028).
//!
//! # The two level assertion rule
//!
//! Every test below carries `#[ignore = "needs FO1_DATA"]`. The marker keeps the
//! default `cargo test` run fast and keeps CI away from game data; it is not a
//! way to pass. The harness call inside each test is unwrapped with `.expect`,
//! so a missing install fails loudly instead of skipping.
//!
//! Every count is an `assert_eq!` against a named constant. There is no `>=`, no
//! "at least", no `continue`, no early `return` and no `if let Ok` in a loop over
//! cases, and every case is named: a candidate that disappears fails.
//!
//! # How the candidate set was obtained
//!
//! By measurement, not by guessing and not from the code under test. Both
//! archives were walked with an independent reader, the loose folder was walked,
//! and the two were joined on the uppercased composed path and the file size.
//! That join is 31 files wide, and 3 further loose files share a path with an
//! archive entry while differing in size.
//!
//! See `docs/spec/dat.md`, DAT-028 and DAT-029.

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

use fo1_dat::{decode_entry, payload_mode, DatArchive, DatError, PayloadMode};
use fo1_tests::{
    install_root_from_env, Install, RefDatTree, CRITTER_DAT_FILE_NAME, MASTER_DAT_FILE_NAME,
};

// --- the candidate set, by name -----------------------------------------------

/// Every archive path that has a loose file of the same size in the patch
/// folder. Ascending, so a set comparison is a straight equality.
const CANDIDATE_PATHS: [&str; 31] = [
    "ART\\INTRFACE\\MAINMENU.FRM",
    "ART\\TILES\\GRID000.FRM",
    "SCRIPTS\\BITSBOB.INT",
    "SCRIPTS\\COCDOOR.INT",
    "SCRIPTS\\GIZMO.INT",
    "SCRIPTS\\GLOWGEN.INT",
    "SCRIPTS\\HALLDED1.INT",
    "SCRIPTS\\HARRY.INT",
    "SCRIPTS\\HOTELMAP.INT",
    "SCRIPTS\\HUBDWNTN.INT",
    "SCRIPTS\\INBLADE.INT",
    "SCRIPTS\\JUNKCAS.INT",
    "SCRIPTS\\JUNKENT.INT",
    "SCRIPTS\\LARIPPER.INT",
    "SCRIPTS\\LARS.INT",
    "SCRIPTS\\MASTER1.INT",
    "SCRIPTS\\MASTER2.INT",
    "SCRIPTS\\MORPH.INT",
    "SCRIPTS\\MSTMERCH.INT",
    "SCRIPTS\\NH2OPUMP.INT",
    "SCRIPTS\\RAIDMAP.INT",
    "SCRIPTS\\RAZOR.INT",
    "SCRIPTS\\RHOMDOOR.INT",
    "SCRIPTS\\RNDMTN.INT",
    "SCRIPTS\\SLAPPY.INT",
    "SCRIPTS\\TANDI.INT",
    "SCRIPTS\\V13CAVE.INT",
    "TEXT\\ENGLISH\\GAME\\COMBAT.MSG",
    "TEXT\\ENGLISH\\GAME\\LSGAME.MSG",
    "TEXT\\ENGLISH\\GAME\\MAP.MSG",
    "TEXT\\ENGLISH\\GAME\\PROTO.MSG",
];

/// The candidates whose loose copy equals the archive decode byte for byte.
/// Every other candidate is one of the three patched copies.
const EXACT_PATHS: [&str; 28] = [
    "SCRIPTS\\BITSBOB.INT",
    "SCRIPTS\\COCDOOR.INT",
    "SCRIPTS\\GIZMO.INT",
    "SCRIPTS\\GLOWGEN.INT",
    "SCRIPTS\\HALLDED1.INT",
    "SCRIPTS\\HARRY.INT",
    "SCRIPTS\\HOTELMAP.INT",
    "SCRIPTS\\HUBDWNTN.INT",
    "SCRIPTS\\INBLADE.INT",
    "SCRIPTS\\JUNKCAS.INT",
    "SCRIPTS\\JUNKENT.INT",
    "SCRIPTS\\LARIPPER.INT",
    "SCRIPTS\\LARS.INT",
    "SCRIPTS\\MASTER1.INT",
    "SCRIPTS\\MASTER2.INT",
    "SCRIPTS\\MORPH.INT",
    "SCRIPTS\\MSTMERCH.INT",
    "SCRIPTS\\NH2OPUMP.INT",
    "SCRIPTS\\RAIDMAP.INT",
    "SCRIPTS\\RAZOR.INT",
    "SCRIPTS\\RHOMDOOR.INT",
    "SCRIPTS\\RNDMTN.INT",
    "SCRIPTS\\SLAPPY.INT",
    "SCRIPTS\\TANDI.INT",
    "SCRIPTS\\V13CAVE.INT",
    "TEXT\\ENGLISH\\GAME\\COMBAT.MSG",
    "TEXT\\ENGLISH\\GAME\\LSGAME.MSG",
    "TEXT\\ENGLISH\\GAME\\MAP.MSG",
];

/// The three candidates the patch rewrote: the composed path, and the first
/// byte at which the loose copy differs from the archive decode.
///
/// These three offsets come from the guide's *Archive gate* section. The
/// archive spec records DAT-028 as UNVERIFIED and says explicitly that the spec
/// author did not verify the three divergences, so they are asserted as the
/// guide states them and are labelled as such here.
const PATCHED_FIRST_DIFFERENCE: [(&str, usize); 3] = [
    ("ART\\INTRFACE\\MAINMENU.FRM", 14_552),
    ("ART\\TILES\\GRID000.FRM", 117),
    ("TEXT\\ENGLISH\\GAME\\PROTO.MSG", 3_385),
];

/// The three loose files whose path matches an archive entry but whose size does
/// not, so the size match drops them: the path, and `loose - declared`, which is
/// negative when the loose copy is smaller.
const SIZE_EXCLUSIONS: [(&str, i64); 3] = [
    ("SCRIPTS\\GABRIEL.INT", 26),
    ("SCRIPTS\\KILLIAN.INT", -14),
    ("TEXT\\ENGLISH\\DIALOG\\RAZOR.MSG", -25),
];

/// Number of names in [`CANDIDATE_PATHS`].
const CANDIDATE_COUNT: usize = 31;

/// Candidates whose loose copy equals the archive decode byte for byte
/// (guide: Independent oracles, class 3).
const EXACT_COUNT: usize = 28;

/// Candidates whose loose copy the patch rewrote (DAT-028).
const PATCHED_COUNT: usize = 3;

/// Loose files excluded by the size match (guide: Archive gate).
const SIZE_EXCLUSION_COUNT: usize = 3;

/// A u32 container field as a `usize`, without a widening cast.
fn as_len(value: u32) -> usize {
    match usize::try_from(value) {
        Ok(converted) => converted,
        Err(_) => panic!("a container field does not fit a usize: {value}"),
    }
}

/// A size difference, as a signed quantity, without a narrowing cast.
fn signed(value: usize, what: &str) -> i64 {
    match i64::try_from(value) {
        Ok(converted) => converted,
        Err(_) => panic!("{what} does not fit an i64: {value}"),
    }
}

// --- harness plumbing --------------------------------------------------------

/// Resolves and fingerprints the install.
///
/// A missing or foreign install fails here, naming `FO1_DATA`, rather than
/// skipping (guide: Exact assertions, "Missing data fails").
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
/// The gate''s replacement for a bare `unwrap`: the failure message says which
/// step gave up, which is what a reader needs, instead of a `Debug` shape.
fn step<T, E: core::fmt::Debug>(result: Result<T, E>, what: &str) -> T {
    match result {
        Ok(value) => value,
        Err(error) => panic!("{what}: {error:?}"),
    }
}

/// As `step`, for an absent lookup.
fn present<T>(value: Option<T>, what: &str) -> T {
    match value {
        Some(inner) => inner,
        None => panic!("{what}: absent"),
    }
}

/// One archive with the bytes a decode needs.
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
        step(
            RefDatTree::walk(&self.bytes),
            "a reference tree walk failed",
        )
    }
}

/// The install and both archives with their bytes, fingerprinted once.
///
/// The harness digests the executable and both archives every time it resolves an
/// install, so a test takes the install once and reuses it.
fn archives() -> (Install, Archive, Archive) {
    let install = install();
    let master = Archive::load(&install, true);
    let critter = Archive::load(&install, false);
    (install, master, critter)
}

/// The archive a composed path belongs to, or the first when neither claims it.
///
/// Every candidate here lives in MASTER.DAT, so this only has to be right for the
/// corpus; the `assert!` inside says so rather than silently reading from the
/// wrong file.
fn owning_archive<'a>(path: &str, master: &'a Archive, critter: &'a Archive) -> &'a Archive {
    if master.tree.find(path).is_some() {
        master
    } else {
        assert!(
            critter.tree.find(path).is_some(),
            "{path} is in neither archive"
        );
        critter
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

/// Decodes one composed path out of whichever archive holds it, naming the path
/// when the decode refuses.
fn decoded_bytes(path: &str, master: &Archive, critter: &Archive) -> (Vec<u8>, &'static str, u32) {
    let archive = owning_archive(path, master, critter);
    let entry = present(archive.tree.find(path), "an archive lookup failed");
    let declared = entry.uncompressed_size;

    // Oracle class 4: the independent reference walker has to find the same
    // entry at the same offset with the same declared length, so a production
    // parse that disagreed with the file would be caught before any decode.
    let reference = archive.reference();
    let walked = step(reference.find(path), "the reference walk has no such path");
    assert_eq!(
        walked.offset, entry.offset,
        "{path}: the production and reference offsets agree"
    );
    assert_eq!(
        walked.uncompressed_size, declared,
        "{path}: the production and reference declared lengths agree"
    );
    let decoded = match decode_entry(entry, &archive.bytes) {
        Ok(bytes) => bytes,
        Err(error) => panic!(
            "{path}: decode refused the entry with {}",
            dat_error_name(&error)
        ),
    };
    (decoded, archive.origin, declared)
}

/// A composed archive path as a path relative to the loose `DATA` folder.
///
/// The composed path uses a backslash, which is a separator on Windows and a
/// legal character elsewhere, so the segments are split and rejoined with the
/// platform separator rather than handed over as one string.
fn loose_relative(composed: &str) -> PathBuf {
    composed.split('\\').collect()
}

/// The loose bytes of one composed path, or `None` when the patch folder holds
/// no file at that path.
fn loose_bytes(install: &Install, composed: &str) -> Option<Vec<u8>> {
    let relative = loose_relative(composed);
    step(
        install.read_loose_file(&relative),
        "a loose file could not be read",
    )
}

/// The declared uncompressed size of one composed path.
fn declared_size(path: &str, master: &Archive, critter: &Archive) -> u32 {
    let archive = owning_archive(path, master, critter);
    let entry = present(archive.tree.find(path), "an archive lookup failed");
    entry.uncompressed_size
}

/// Every archive path whose loose copy has the same size, ascending.
///
/// The join is on the uppercased composed path, which the archive already stores
/// in uppercase (DAT-010), and on the size: a stored entry's on disk length is
/// its declared uncompressed size (DAT-015), so a loose file of the same length
/// is a candidate for a byte comparison.
fn discovered_candidates(install: &Install, master: &Archive, critter: &Archive) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for archive in [master, critter] {
        for entry in archive.tree.entries() {
            if let Some(bytes) = loose_bytes(install, &entry.path) {
                let declared = match usize::try_from(entry.uncompressed_size) {
                    Ok(converted) => converted,
                    Err(_) => panic!("{}: the declared size does not fit", entry.path),
                };
                let same_size = bytes.len() == declared;
                if same_size {
                    found.push(entry.path.clone());
                }
            }
        }
    }
    found.sort();
    found
}

// --- the gate ----------------------------------------------------------------

/// DAT-028 and DAT-029: candidate discovery is exact. The set is 31 paths and the
/// test lists all 31 by name, so a candidate that disappears, appears or is
/// renamed fails (guide: Exact assertions, "A set of named cases is compared as
/// a set").
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_028_candidate_discovery_lists_all_thirty_one_names() {
    let (install, master, critter) = archives();

    let found = discovered_candidates(&install, &master, &critter);
    assert_eq!(
        found,
        CANDIDATE_PATHS.to_vec(),
        "the discovered candidate set must equal the listed 31"
    );
    assert_eq!(found.len(), CANDIDATE_COUNT, "candidate count");
    assert_eq!(
        CANDIDATE_PATHS.len(),
        CANDIDATE_COUNT,
        "the listed names are the whole candidate set"
    );
    assert_eq!(
        EXACT_PATHS.len() + PATCHED_FIRST_DIFFERENCE.len(),
        CANDIDATE_COUNT,
        "the two buckets together are the candidate set"
    );
}

/// DAT-029, oracle class 3: 28 loose files equal their archive entries byte for
/// byte.
///
/// The expected bytes are the file on disk in the patch folder. A decoder that
/// produced the declared size for every input, or that got the ring wrong, fails
/// here on the first differing byte with the pair's name in the message.
///
/// LZSS-001, LZSS-002 and LZSS-003: the ring is 4096 bytes, prefilled with
/// `0x20` in slots 0..=4077 and `0x00` in slots 4078..=4095, and the write head
/// starts at 4078. A wrong ring size, a wrong prefill or a wrong write head
/// changes the decoded bytes of the corpus.
///
/// LZSS-005, LZSS-006 and LZSS-007: a control byte carries eight tokens bit 0
/// first, a set bit is a literal and a clear bit is a two-byte back reference.
/// A decoder that read the tokens in the other order, or took the wrong branch,
/// produces different bytes.
///
/// LZSS-008, LZSS-009 and LZSS-010: the source index is an absolute ring
/// position, not a distance, the length is the other three bits, and the output
/// position maps to the ring slot at `(p + 4078) mod 4096`. A distance based
/// copy, or a slot off by the write head, produces different bytes.
///
/// LZSS-011: a copy reads each byte before it writes it and wraps at 4096, so an
/// overlapping run replicates its own output.
///
/// LZSS-012: no ring index above 4095 is addressable.
///
/// LZSS-016: every LZSS block inside a chain starts with a fresh ring buffer.
/// These candidates are chain entries, so a decoder that carried state across a
/// block boundary produces different bytes.
///
/// LZSS-017, LZSS-018 and LZSS-019: the three worked vectors. The corpus
/// contains the tokens they name, so the decoder that reproduces the corpus
/// byte for byte reproduces them.
///
/// The witness is joint: one byte-for-byte comparison over 28 real archive
/// entries constrains the rules above together. It is a class 3 oracle, known
/// plaintext, and it is a class 1, 2 or 3 witness for every rule it carries.
///
/// claim: DAT-029, LZSS-001, LZSS-002, LZSS-003, LZSS-005, LZSS-006, LZSS-007, LZSS-008, LZSS-009, LZSS-010, LZSS-011, LZSS-012, LZSS-016, LZSS-017, LZSS-018, LZSS-019
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_029_twenty_eight_candidates_match_the_archive_byte_for_byte() {
    let (install, master, critter) = archives();
    let mut compared = 0usize;

    for path in EXACT_PATHS {
        let loose = present(
            loose_bytes(&install, path),
            "a candidate has no loose copy in the patch folder",
        );
        let (decoded, origin, declared) = decoded_bytes(path, &master, &critter);
        assert_eq!(
            decoded.len(),
            as_len(declared),
            "{path}: the decode length equals the declared uncompressed size in {origin}"
        );
        assert_eq!(decoded, loose, "{path} in {origin}");
        compared += 1;
    }

    assert_eq!(compared, EXACT_COUNT, "pairs compared byte for byte");
    assert_eq!(
        EXACT_PATHS.len(),
        EXACT_COUNT,
        "the listed exact pairs are the whole bucket"
    );
    assert_eq!(
        PATCHED_FIRST_DIFFERENCE.len(),
        PATCHED_COUNT,
        "the listed patched copies are the whole bucket"
    );
}

/// DAT-028: exactly three candidates have a patched loose copy, and each one
/// first differs from its archive decode at the byte the guide records.
///
/// The offsets are the guide's (Archive gate section); `docs/spec/dat.md` marks
/// DAT-028 UNVERIFIED and says the spec author did not verify them. So this test
/// asserts the guide's numbers and labels them as the guide's, rather than
/// pretending to have measured them.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_028_three_patched_copies_differ_at_their_first_byte() {
    let (install, master, critter) = archives();
    let mut diverged = 0usize;

    for (path, expected_first_difference) in PATCHED_FIRST_DIFFERENCE {
        let loose = present(
            loose_bytes(&install, path),
            "a candidate has no loose copy in the patch folder",
        );
        let (decoded, origin, declared) = decoded_bytes(path, &master, &critter);
        assert_eq!(
            decoded.len(),
            loose.len(),
            "{path}: the patched copy still has the archive's length in {origin}"
        );
        assert_eq!(
            decoded.len(),
            as_len(declared),
            "{path}: the decode length equals the declared uncompressed size"
        );

        let first = present(
            first_difference(&decoded, &loose),
            "a patched loose copy is not patched at all",
        );
        assert_eq!(
            first, expected_first_difference,
            "{path}: the first byte at which the patched copy differs, in {origin}"
        );
        diverged += 1;
    }

    assert_eq!(diverged, PATCHED_COUNT, "patched copies located");
}

/// DAT-028: the three size exclusions, listed by name with their exact byte
/// difference.
///
/// A loose file whose path matches an archive entry but whose length does not is
/// dropped by the size match, so it never becomes a candidate and never reaches
/// a byte comparison. Dropping it silently would be a skip; listing it is the
/// alternative (guide: Archive gate).
///
/// The size match is the other half of the candidate discovery DAT-028's first
/// test pins: the 31 candidates are the files that match on path *and* size,
/// so the three that match on path alone are named here with the exact byte
/// difference that excluded them.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_028_three_loose_files_differ_in_size_and_are_excluded() {
    let (install, master, critter) = archives();
    let mut excluded = 0usize;

    for (path, expected_difference) in SIZE_EXCLUSIONS {
        let loose = present(
            loose_bytes(&install, path),
            "a candidate has no loose copy in the patch folder",
        );
        let declared = declared_size(path, &master, &critter);
        let declared_len = match usize::try_from(declared) {
            Ok(converted) => converted,
            Err(_) => panic!("{path}: the declared size does not fit"),
        };
        let difference = signed(loose.len(), path) - signed(declared_len, path);
        assert_eq!(
            difference, expected_difference,
            "{path}: loose length minus the archive's declared uncompressed size"
        );
        assert_ne!(
            loose.len(),
            declared_len,
            "{path}: a size exclusion must not match the archive's declared length"
        );
        excluded += 1;
    }

    assert_eq!(excluded, SIZE_EXCLUSION_COUNT, "size exclusions listed");
}

/// DAT-028: every candidate lands in exactly one bucket: 28 exact and 3 patched,
/// as a set comparison, so a name appearing in both buckets fails.
///
/// The partition is the accounting half of the candidate discovery: the set of
/// 31 names DAT-028's first test pins is exactly the union of these two
/// buckets, so a candidate that is neither exact nor patched, or one that is
/// both, fails here.
#[test]
#[ignore = "needs FO1_DATA"]
fn dat_028_every_candidate_is_accounted_for_in_exactly_one_bucket() {
    let mut bucketed: Vec<String> = EXACT_PATHS.iter().map(|p| (*p).to_string()).collect();
    for (path, _first) in PATCHED_FIRST_DIFFERENCE {
        bucketed.push((*path).to_string());
    }
    bucketed.sort();

    let mut expected: Vec<String> = CANDIDATE_PATHS.iter().map(|p| (*p).to_string()).collect();
    expected.sort();

    assert_eq!(
        bucketed, expected,
        "the exact and patched buckets partition the candidate set"
    );
    assert_eq!(bucketed.len(), CANDIDATE_COUNT, "bucket membership count");
    assert_eq!(
        EXACT_COUNT + PATCHED_COUNT,
        CANDIDATE_COUNT,
        "28 exact plus 3 patched is 31 candidates"
    );

    // No name may sit in both buckets. The set comparison above would allow a
    // duplicate and a missing name to cancel out, so the overlap is counted.
    let mut overlap = 0usize;
    for path in EXACT_PATHS {
        for (patched, _first) in PATCHED_FIRST_DIFFERENCE {
            if path == patched {
                overlap += 1;
            }
        }
    }
    assert_eq!(overlap, 0, "no candidate is in both buckets");
}

/// DAT-023 and DAT-028, oracle class 6: every candidate decodes to exactly its
/// declared uncompressed size, whatever its payload mode, and its loose copy
/// has that same length.
///
/// DAT-023 is the produced-size law: a decoded entry must produce exactly the
/// uncompressed size its record declares.
///
/// DAT-028 is the rule the loose copy's length is checked against: the
/// candidates are the loose files that match an archive entry on path and on
/// size, so a loose copy that did not carry the decoded length would not be a
/// candidate at all.
///
/// This is the algebraic law under the two buckets above: a patched copy differs
/// in bytes, never in length, which is why the size match finds all 31.
#[test]
#[ignore = "needs FO1_DATA"]
fn every_candidate_decodes_to_its_declared_uncompressed_size() {
    let (install, master, critter) = archives();
    let mut checked = 0usize;
    let mut stored = 0usize;
    let mut chained = 0usize;

    for path in CANDIDATE_PATHS {
        let loose = present(
            loose_bytes(&install, path),
            "a candidate has no loose copy in the patch folder",
        );
        let (decoded, origin, declared) = decoded_bytes(path, &master, &critter);
        let declared_len = match usize::try_from(declared) {
            Ok(converted) => converted,
            Err(_) => panic!("{path}: the declared size does not fit"),
        };
        assert_eq!(
            decoded.len(),
            declared_len,
            "{path}: the decode length equals the declared uncompressed size in {origin}"
        );
        assert_eq!(
            decoded.len(),
            loose.len(),
            "{path}: the loose copy has the decoded length, which is why it was a candidate"
        );

        let archive = owning_archive(path, &master, &critter);
        let entry = present(archive.tree.find(path), "a candidate vanished");
        match payload_mode(entry.flags) {
            PayloadMode::Stored => stored += 1,
            PayloadMode::BlockChain => chained += 1,
            PayloadMode::LzssStream => {
                panic!("{path}: mode 0x10 is not exercised by this corpus")
            }
        }
        checked += 1;
    }

    assert_eq!(checked, CANDIDATE_COUNT, "candidates decoded");
    assert_eq!(
        stored + chained,
        CANDIDATE_COUNT,
        "every candidate is stored or a chain"
    );
}

/// The first index at which two buffers differ, or `None` when they are equal.
fn first_difference(left: &[u8], right: &[u8]) -> Option<usize> {
    if left.len() != right.len() {
        panic!("the buffers differ in length, so there is no first differing byte");
    }
    left.iter().zip(right.iter()).position(|(a, b)| a != b)
}
