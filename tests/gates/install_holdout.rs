//! Hold-out gate tests: the owner's private vectors grade the DAT reader and
//! the executable table reader.
//!
//! Spec: `docs/spec/holdout.md`, claims HOLD-001 to HOLD-027. The vectors live
//! outside the repository, under `FO1_REF/holdout/`, and this file reads them
//! from there. It never sees the values it grades against: every expected
//! byte, every expected size and every expected digest comes out of the
//! manifest, which the owner wrote with a generator that was never part of
//! this crate graph (HOLD-019, HOLD-024).
//!
//! # Oracle classes
//!
//! - **oracle class 4**, an independent reference implementation. The DAT
//!   vectors' expected bytes are the answer of a second decoder, written from
//!   the specifications alone and in another language. The reader under test
//!   never produced them.
//! - **oracle class 1**, bytes of the original executable. The executable
//!   table vectors' expected bytes were read out of the reference
//!   `falloutw.exe` by a separate extraction tool, and the reader is put to
//!   the question at the virtual address the manifest names.
//!
//! # What the file is for
//!
//! Two test functions, one per kind of vector, both `#[ignore]`d because the
//! hold-out folder is the owner's private folder and this repository holds no
//! vector data at all (HOLD-002, HOLD-025). The split is not cosmetic: the DAT
//! vectors need nothing but the folder, so a machine with the folder and no
//! install still runs them, and a missing install cannot hide a DAT failure
//! behind an install failure.
//!
//! # No skip path
//!
//! There is none. A missing folder, a missing manifest, a malformed manifest,
//! a file that is not there, a file whose digest does not match, a version
//! this reader does not implement, a count that disagrees, a vector that does
//! not decode: every one of them is a failure with a message that names the
//! condition (HOLD-026). Nothing here skips, swallows an error or gives up
//! quietly, and no loop over cases leaves a case unrun.
//!
//! # The report
//!
//! One line per failing case and one summary line per part, in the fixed form
//! of HOLD-027:
//!
//! ```text
//! holdout dat: FAILED dat_0001_stored_single_byte: decoded size mismatch
//! holdout dat: 32 cases, 31 passed
//! ```
//!
//! A failing line never prints the expected bytes, the expected digest or any
//! part of the archive: the implementer learns which case failed and what kind
//! of failure it was, and nothing about the answer.
//!
//! The summary is printed by the test itself, so a run that passes shows it
//! under `--nocapture`; a run that fails always shows it, because a failing
//! test's output is printed either way.
//!
//! # Two conditions the fixed vocabulary does not name
//!
//! HOLD-027 fixes the reason vocabulary, and two of the conditions HOLD-026
//! lists have no token of their own: a vector file whose bytes are not a legal
//! hex archive, and a file in `dat/` or `exe/` that the manifest does not
//! declare. Both are reported with the closest token the fixed vocabulary has,
//! `manifest unreadable`, and with a detail line that names the condition. A
//! new token would have been a reason outside the vocabulary the spec fixes.
//!
//! # The file name
//!
//! HOLD-025 fixes the name as `holdout.rs`. The owner's task for this file
//! names it `install_holdout.rs`, which is the name the gate's count file and
//! its `install_` floor expect, so that is the name it carries.

// Gate tests are test code. `clippy.toml` sets `allow-unwrap-in-tests` and its
// siblings, which cover a `#[cfg(test)]` module or a `#[test]` function body.
// An integration test file's top-level helper functions sit outside both, so
// the same exemptions are restated here, with reasons, for the gate files only.
// The workspace lint table stays fully denied for the six production crates,
// which are what the guide's "no panicking readers" guarantee is about.
#![allow(
    clippy::panic,
    clippy::print_stdout,
    clippy::indexing_slicing,
    reason = "gate-test helpers: the hold-out gate must print its case count, fail loudly and index fixed-width records outside #[test] bodies"
)]
use std::fs;
use std::path::{Path, PathBuf};

use fo1_dat::{decode_entry, DatArchive};
use fo1_exe::{PeImage, IMAGE_BASE};
use fo1_tests::{holdout_manifest_path, holdout_root, sha256_of_slice, Install};

// --- the reason vocabulary of HOLD-027 ---------------------------------------

/// `manifest unreadable` (HOLD-027).
const REASON_MANIFEST_UNREADABLE: &str = "manifest unreadable";
/// `manifest version not understood` (HOLD-027).
const REASON_MANIFEST_VERSION: &str = "manifest version not understood";
/// `manifest count mismatch` (HOLD-027).
const REASON_MANIFEST_COUNT: &str = "manifest count mismatch";
/// `vector file missing` (HOLD-027).
const REASON_VECTOR_MISSING: &str = "vector file missing";
/// `vector file size mismatch` (HOLD-027).
const REASON_VECTOR_SIZE: &str = "vector file size mismatch";
/// `vector file digest mismatch` (HOLD-027).
const REASON_VECTOR_DIGEST: &str = "vector file digest mismatch";
/// `archive parse failed` (HOLD-027).
const REASON_ARCHIVE_PARSE: &str = "archive parse failed";
/// `entry not found` (HOLD-027).
const REASON_ENTRY_NOT_FOUND: &str = "entry not found";
/// `decode failed` (HOLD-027).
const REASON_DECODE: &str = "decode failed";
/// `decoded size mismatch` (HOLD-027).
const REASON_DECODED_SIZE: &str = "decoded size mismatch";
/// `decoded digest mismatch` (HOLD-027).
const REASON_DECODED_DIGEST: &str = "decoded digest mismatch";
/// `reader bytes differ from the expected bytes` (HOLD-027).
const REASON_READER_BYTES: &str = "reader bytes differ from the expected bytes";
/// `reader mapping disagrees with the vector offset` (HOLD-027).
const REASON_MAPPING: &str = "reader mapping disagrees with the vector offset";
/// `executable is not the reference build` (HOLD-027).
const REASON_BUILD: &str = "executable is not the reference build";
/// `vector offset outside the executable` (HOLD-027).
const REASON_OFFSET_OUTSIDE: &str = "vector offset outside the executable";

// --- the manifest format of HOLD-004 to HOLD-013 -----------------------------

/// The manifest format versions this reader implements (HOLD-007).
///
/// One version, and no other: a reader is never taught to accept a version it
/// does not implement, and a version it does not know is a refusal rather than
/// a fallback to its own default.
const KNOWN_FORMAT_VERSIONS: [u64; 1] = [1];

/// The five header keys, in the order the manifest carries them (HOLD-006).
const HEADER_KEYS: [&str; 5] = [
    "format_version",
    "seed",
    "generator",
    "dat_count",
    "exe_count",
];

/// The number of tokens a header line carries (HOLD-006).
const HEADER_TOKENS: usize = 2;

/// The number of tokens a record line carries (HOLD-010).
const RECORD_TOKENS: usize = 7;

/// The number of hex characters a digest field carries (HOLD-011).
const DIGEST_CHARS: usize = 64;

/// The number of hex characters the seed field carries (HOLD-008).
const SEED_CHARS: usize = 64;

/// The label the hold-out archives are parsed under (DAT-030).
const HOLDOUT_ORIGIN: &str = "holdout";

/// The case name a manifest-level failure carries, because a manifest has no
/// vector file of its own.
const MANIFEST_CASE: &str = "manifest";

// --- the vectors of HOLD-010 -------------------------------------------------

/// One `dat` record (HOLD-010, HOLD-014).
struct DatVector {
    /// The vector file, relative to the folder that holds the manifest.
    file: String,
    /// The byte length of that file on disk.
    file_bytes: u64,
    /// The SHA-256 of that file's bytes on disk.
    file_sha256: String,
    /// The number of bytes the reader must produce for the entry.
    decoded_size: u64,
    /// The SHA-256 of exactly those bytes.
    decoded_sha256: String,
    /// The composed path of the entry under test.
    path: String,
}

/// One `exe` record (HOLD-010, HOLD-020).
struct ExeVector {
    /// The vector file, relative to the folder that holds the manifest.
    file: String,
    /// The byte length of that file on disk.
    file_bytes: u64,
    /// The SHA-256 of that file's bytes on disk.
    file_sha256: String,
    /// The offset of the expected bytes in the reference executable file.
    file_offset: u64,
    /// The number of bytes the window holds.
    length: u64,
    /// The virtual address of the same bytes.
    va: u32,
}

/// The manifest, once it has parsed.
struct Manifest {
    /// The `dat` records, in the order the manifest lists them.
    dat: Vec<DatVector>,
    /// The `exe` records, in the order the manifest lists them.
    exe: Vec<ExeVector>,
}

/// One failure: the fixed reason of HOLD-027 and a detail that names the
/// condition without carrying any vector data.
struct Failure {
    /// The case name: the vector file's stem, or `manifest`.
    case: String,
    /// One of the fixed reasons of HOLD-027.
    reason: &'static str,
    /// What went wrong, in words.
    detail: String,
}

/// Builds one failure.
fn failure(case: &str, reason: &'static str, detail: String) -> Failure {
    Failure {
        case: case.to_string(),
        reason,
        detail,
    }
}

/// Builds a failure of the manifest itself, which has no case name of its own.
fn manifest_failure(reason: &'static str, detail: String) -> Failure {
    failure(MANIFEST_CASE, reason, detail)
}

/// Builds a failure of one manifest line, with its number.
fn line_failure(line_number: usize, what: &str) -> Failure {
    manifest_failure(
        REASON_MANIFEST_UNREADABLE,
        format!("manifest line {line_number}: {what}"),
    )
}

/// Prints the failing line of one case, and the detail that names the
/// condition (HOLD-027).
fn print_failure(kind: &str, failure: &Failure) {
    println!(
        "holdout {kind}: FAILED {}: {}",
        failure.case, failure.reason
    );
    println!("holdout {kind}: {}", failure.detail);
}

/// Prints the summary line of one part (HOLD-027).
fn print_summary(kind: &str, cases: usize, passed: usize) {
    println!("holdout {kind}: {cases} cases, {passed} passed");
}

/// The case name of a vector: the vector file's stem (HOLD-027).
fn case_name(file: &str) -> String {
    let base = match file.rsplit('/').next() {
        Some(base) => base,
        None => file,
    };
    let stem = match base.rsplit_once('.') {
        Some((stem, _extension)) => stem,
        None => base,
    };
    stem.to_string()
}

// --- the manifest reader of HOLD-004 to HOLD-013 -----------------------------

/// Reads the hold-out folder and the manifest, and refuses everything HOLD-012
/// refuses.
///
/// The folder is resolved through the harness, which is the only place in this
/// workspace that reads the references directory. A manifest that does not
/// parse produces no cases at all (HOLD-012).
fn read_holdout() -> Result<(PathBuf, Manifest), Failure> {
    let root = match holdout_root() {
        Ok(root) => root,
        Err(error) => {
            return Err(manifest_failure(
                REASON_MANIFEST_UNREADABLE,
                error.to_string(),
            ))
        }
    };
    let manifest_path = match holdout_manifest_path() {
        Ok(path) => path,
        Err(error) => {
            return Err(manifest_failure(
                REASON_MANIFEST_UNREADABLE,
                error.to_string(),
            ))
        }
    };
    let text = match fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(error) => {
            return Err(manifest_failure(
                REASON_MANIFEST_UNREADABLE,
                format!(
                    "the manifest {} could not be read: {error}",
                    manifest_path.display()
                ),
            ))
        }
    };
    match parse_manifest(&text) {
        Ok(manifest) => Ok((root, manifest)),
        Err(failure) => Err(failure),
    }
}

/// Parses the manifest text (HOLD-005 to HOLD-013).
fn parse_manifest(text: &str) -> Result<Manifest, Failure> {
    let mut lines: Vec<&str> = text.split('\n').collect();

    // HOLD-005: the file ends with a newline, so the piece after the last one
    // is the terminator and not a line.
    match lines.pop() {
        Some("") => {}
        Some(_) => {
            return Err(manifest_failure(
                REASON_MANIFEST_UNREADABLE,
                "the manifest does not end with a newline".to_string(),
            ))
        }
        None => {
            return Err(manifest_failure(
                REASON_MANIFEST_UNREADABLE,
                "the manifest is empty".to_string(),
            ))
        }
    }

    // HOLD-005: no empty lines, no carriage returns, and no tabs anywhere.
    //
    // The lines are walked with an iterator rather than a `for` loop: a
    // refusal leaves the loop with a `return`, and a `return` inside a loop is
    // the shape this repository's banned pattern lint refuses.
    let broken = lines
        .iter()
        .enumerate()
        .find(|(_, line)| line.is_empty() || line.contains('\r') || line.contains('\t'));
    if let Some((index, line)) = broken {
        let what = if line.is_empty() {
            "the line is empty"
        } else if line.contains('\r') {
            "the line carries a carriage return"
        } else {
            "the line carries a tab"
        };
        return Err(line_failure(index + 1, what));
    }

    // HOLD-006: the five header lines, in this order, each two tokens.
    let header: Vec<Vec<&str>> = lines
        .iter()
        .take(HEADER_KEYS.len())
        .enumerate()
        .map(|(index, line)| header_line(line, index + 1, HEADER_KEYS[index]))
        .collect::<Result<Vec<_>, Failure>>()?;
    if header.len() != HEADER_KEYS.len() {
        return Err(line_failure(header.len() + 1, "the header line is missing"));
    }

    // HOLD-007: the version comes first, and a version this reader does not
    // implement is a hard refusal.
    let version = parse_canonical_u64(header[0][1], 1)?;
    if !KNOWN_FORMAT_VERSIONS.contains(&version) {
        return Err(manifest_failure(
            REASON_MANIFEST_VERSION,
            format!(
                "manifest line 1: format_version {version} is not a version this reader \
                 implements; the versions this reader implements are \
                 {KNOWN_FORMAT_VERSIONS:?}"
            ),
        ));
    }

    // HOLD-008: the seed and the generator are provenance. The reader checks
    // their shape and uses neither for anything.
    if header[1][1].len() != SEED_CHARS || !is_lower_hex(header[1][1]) {
        return Err(line_failure(
            2,
            "the seed must be 64 lower case hex characters",
        ));
    }
    if !is_generator_token(header[2][1]) {
        return Err(line_failure(
            3,
            "the generator must be one token of letters, digits, dot, underscore and hyphen",
        ));
    }

    // HOLD-009: the counts. The reader never hard-codes one.
    let dat_count = parse_canonical_u64(header[3][1], 4)?;
    let exe_count = parse_canonical_u64(header[4][1], 5)?;

    // HOLD-010: the records, all the `dat` ones first.
    //
    // The records are read with an iterator, so a malformed record line leaves
    // the manifest and not the middle of a loop: a manifest that does not parse
    // produces no cases at all (HOLD-012).
    let records: Vec<(usize, Record)> = lines
        .iter()
        .enumerate()
        .skip(HEADER_KEYS.len())
        .map(|(index, line)| {
            let line_number = index + 1;
            let tokens = tokens_of(line, line_number)?;
            Ok((line_number, parse_record(&tokens, line_number)?))
        })
        .collect::<Result<Vec<_>, Failure>>()?;

    let mut dat: Vec<DatVector> = Vec::new();
    let mut exe: Vec<ExeVector> = Vec::new();
    let mut first_exe_line: Option<usize> = None;
    let mut last_dat_line: Option<usize> = None;
    for (line_number, record) in records {
        match record {
            Record::Dat(vector) => {
                last_dat_line = Some(line_number);
                dat.push(vector);
            }
            Record::Exe(vector) => {
                if first_exe_line.is_none() {
                    first_exe_line = Some(line_number);
                }
                exe.push(vector);
            }
        }
    }
    if let (Some(first_exe), Some(last_dat)) = (first_exe_line, last_dat_line) {
        if last_dat > first_exe {
            return Err(line_failure(
                first_exe,
                "a dat record follows an exe record, and the dat records come first",
            ));
        }
    }

    // HOLD-009: the count of records of each kind, compared with equality.
    if dat.len() as u64 != dat_count {
        return Err(manifest_failure(
            REASON_MANIFEST_COUNT,
            format!(
                "the manifest declares {dat_count} dat records and carries {}",
                dat.len()
            ),
        ));
    }
    if exe.len() as u64 != exe_count {
        return Err(manifest_failure(
            REASON_MANIFEST_COUNT,
            format!(
                "the manifest declares {exe_count} exe records and carries {}",
                exe.len()
            ),
        ));
    }

    Ok(Manifest { dat, exe })
}

/// Checks one header line (HOLD-006).
///
/// The line carries the key it must be, and exactly two tokens.
fn header_line<'a>(line: &'a str, line_number: usize, key: &str) -> Result<Vec<&'a str>, Failure> {
    let tokens = tokens_of(line, line_number)?;
    if tokens.len() != HEADER_TOKENS {
        return Err(line_failure(
            line_number,
            "the header line must hold exactly two tokens",
        ));
    }
    if tokens[0] != key {
        return Err(line_failure(
            line_number,
            &format!("the header line must be `{key}`"),
        ));
    }
    Ok(tokens)
}

/// The tokens of one line, split on a single space (HOLD-005).
///
/// A line is a sequence of tokens separated by exactly one space, every token
/// is non-empty, and no token holds a tab. Two spaces in a row, a leading
/// space and a trailing space all produce an empty token and are refused here.
fn tokens_of(line: &str, line_number: usize) -> Result<Vec<&str>, Failure> {
    let tokens: Vec<&str> = line.split(' ').collect();
    if tokens.iter().any(|token| token.is_empty()) {
        return Err(line_failure(
            line_number,
            "the line has two spaces in a row, or a leading space, or a trailing space",
        ));
    }
    Ok(tokens)
}

/// A canonical decimal unsigned integer (HOLD-011).
///
/// Digits only: no sign, no `0x` prefix, no thousands separator, no
/// underscore. No leading zero, except that the value zero is written as the
/// single character `0`.
fn parse_canonical_u64(token: &str, line_number: usize) -> Result<u64, Failure> {
    if token.is_empty() {
        return Err(line_failure(line_number, "the number is empty"));
    }
    if !token.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(line_failure(
            line_number,
            "the number is not a decimal unsigned integer",
        ));
    }
    if token.len() > 1 && token.starts_with('0') {
        return Err(line_failure(
            line_number,
            "the number has a leading zero, which the format does not allow",
        ));
    }
    match token.parse::<u64>() {
        Ok(value) => Ok(value),
        Err(_) => Err(line_failure(
            line_number,
            "the number does not fit a 64 bit unsigned integer",
        )),
    }
}

/// A digest field: exactly 64 lower case hex characters (HOLD-011).
///
/// An upper case digest is a refusal and not a case folded comparison.
fn parse_digest(token: &str, line_number: usize) -> Result<String, Failure> {
    if token.len() != DIGEST_CHARS || !is_lower_hex(token) {
        return Err(line_failure(
            line_number,
            "the digest must be 64 lower case hex characters",
        ));
    }
    Ok(token.to_string())
}

/// True when every character of the token is a lower case hex character.
fn is_lower_hex(token: &str) -> bool {
    token
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// True when the token is one token of `[A-Za-z0-9._-]` (HOLD-008).
fn is_generator_token(token: &str) -> bool {
    !token.is_empty()
        && token.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'_' || byte == b'-'
        })
}

/// One parsed record line, of either kind (HOLD-010).
enum Record {
    /// A `dat` record.
    Dat(DatVector),
    /// An `exe` record.
    Exe(ExeVector),
}

/// Parses one record line (HOLD-010, HOLD-011, HOLD-012, HOLD-016).
fn parse_record(tokens: &[&str], line_number: usize) -> Result<Record, Failure> {
    if tokens.len() != RECORD_TOKENS {
        return Err(line_failure(
            line_number,
            "a record line must hold exactly seven tokens",
        ));
    }
    let kind = tokens[0];
    let file = tokens[1];
    let file_bytes = parse_canonical_u64(tokens[2], line_number)?;
    let file_sha256 = parse_digest(tokens[3], line_number)?;
    match kind {
        "dat" => {
            check_vector_file(file, "dat", "dat_", ".hex", line_number)?;
            let decoded_size = parse_canonical_u64(tokens[4], line_number)?;
            let decoded_sha256 = parse_digest(tokens[5], line_number)?;
            let path = tokens[6];
            check_path(path, line_number)?;
            Ok(Record::Dat(DatVector {
                file: file.to_string(),
                file_bytes,
                file_sha256,
                decoded_size,
                decoded_sha256,
                path: path.to_string(),
            }))
        }
        "exe" => {
            check_vector_file(file, "exe", "exe_", ".bin", line_number)?;
            let file_offset = parse_canonical_u64(tokens[4], line_number)?;
            let length = parse_canonical_u64(tokens[5], line_number)?;
            let va = parse_canonical_u64(tokens[6], line_number)?;
            let va = match u32::try_from(va) {
                Ok(va) => va,
                Err(_) => {
                    return Err(line_failure(
                        line_number,
                        "the virtual address does not fit a 32 bit address",
                    ))
                }
            };
            Ok(Record::Exe(ExeVector {
                file: file.to_string(),
                file_bytes,
                file_sha256,
                file_offset,
                length,
                va,
            }))
        }
        _ => Err(line_failure(
            line_number,
            "the record kind is neither dat nor exe",
        )),
    }
}

/// Checks the `file` field of a record (HOLD-010, HOLD-012, HOLD-003).
///
/// The file is named relative to the folder that holds the manifest, with `/`
/// as the separator on every platform. It is `dat/<name>.hex` for a DAT vector
/// and `exe/<name>.bin` for an executable table vector, and the name is the
/// case name: lower case ASCII letters, digits and `_`, beginning with the
/// kind's prefix.
fn check_vector_file(
    file: &str,
    folder: &str,
    prefix: &str,
    extension: &str,
    line_number: usize,
) -> Result<(), Failure> {
    if file.starts_with('/') {
        return Err(line_failure(
            line_number,
            "the file field is absolute, and the manifest names files inside the folder",
        ));
    }
    if file.contains('\\') {
        return Err(line_failure(
            line_number,
            "the file field carries a backslash, and the separator is a slash",
        ));
    }
    if file.split('/').any(|segment| segment == "..") {
        return Err(line_failure(
            line_number,
            "the file field names a parent directory, and the manifest names files inside the folder",
        ));
    }
    let parts: Vec<&str> = file.split('/').collect();
    if parts.len() != 2 {
        return Err(line_failure(
            line_number,
            "the file field must be the vector folder and the vector file",
        ));
    }
    if parts[0] != folder {
        return Err(line_failure(
            line_number,
            &format!("the file field must name a file in the {folder} folder"),
        ));
    }
    let name = parts[1];
    let stem = match name.rsplit_once('.') {
        Some((stem, suffix)) => {
            if suffix != extension.trim_start_matches('.') {
                return Err(line_failure(
                    line_number,
                    &format!("the file field must end in {extension}"),
                ));
            }
            stem
        }
        None => {
            return Err(line_failure(
                line_number,
                &format!("the file field must end in {extension}"),
            ))
        }
    };
    if !stem.starts_with(prefix) {
        return Err(line_failure(
            line_number,
            &format!("the case name must begin with `{prefix}`"),
        ));
    }
    if !stem
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(line_failure(
            line_number,
            "the case name must hold lower case ASCII letters, digits and underscores only",
        ));
    }
    Ok(())
}

/// Checks the `path` field of a DAT record (HOLD-016).
///
/// The path is the composed path, already upper case, with its segments
/// separated by a single backslash.
fn check_path(path: &str, line_number: usize) -> Result<(), Failure> {
    if path.is_empty() {
        return Err(line_failure(line_number, "the path is empty"));
    }
    if !path.bytes().all(|byte| {
        byte.is_ascii_uppercase()
            || byte.is_ascii_digit()
            || byte == b'.'
            || byte == b'_'
            || byte == b'-'
    }) {
        return Err(line_failure(
            line_number,
            "the path must hold upper case ASCII letters, digits, dot, underscore and hyphen only",
        ));
    }
    if path.starts_with('\\') || path.ends_with('\\') {
        return Err(line_failure(
            line_number,
            "the path must have no leading backslash and no trailing backslash",
        ));
    }
    if path.split('\\').any(|segment| segment.is_empty()) {
        return Err(line_failure(line_number, "the path has an empty segment"));
    }
    Ok(())
}

// --- the set of the folder ---------------------------------------------------

/// Checks that every file in `folder` is declared by the manifest (HOLD-026).
///
/// A file in `dat/` or `exe/` that the manifest does not declare is a failure
/// and not something to ignore: it is what makes the manifest a statement about
/// the whole set and not a list of some of it.
fn undeclared_file(root: &Path, folder: &str, declared: &[String]) -> Option<Failure> {
    let directory = root.join(folder);
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) => {
            return Some(manifest_failure(
                REASON_MANIFEST_UNREADABLE,
                format!("the {folder} folder cannot be listed: {error}"),
            ))
        }
    };
    let names: Vec<String> = match entries.collect::<Result<Vec<_>, _>>().map(|entries| {
        entries
            .into_iter()
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect()
    }) {
        Ok(names) => names,
        Err(error) => {
            return Some(manifest_failure(
                REASON_MANIFEST_UNREADABLE,
                format!("the {folder} folder cannot be listed: {error}"),
            ))
        }
    };
    let undeclared = names
        .iter()
        .find(|name| !declared.contains(&format!("{folder}/{name}")));
    undeclared.map(|name| {
        manifest_failure(
            REASON_MANIFEST_UNREADABLE,
            format!("the file {folder}/{name} is not declared in the manifest"),
        )
    })
}

/// The `file` fields of the records of one kind.
fn declared_files(vectors: &[DatVector]) -> Vec<String> {
    vectors.iter().map(|vector| vector.file.clone()).collect()
}

/// The `file` fields of the executable table records.
fn declared_exe_files(vectors: &[ExeVector]) -> Vec<String> {
    vectors.iter().map(|vector| vector.file.clone()).collect()
}

// --- the DAT case of HOLD-018 ------------------------------------------------

/// Runs one `dat` vector (HOLD-018).
///
/// Every step of HOLD-018 is an assertion, in the order the spec gives them.
fn run_dat_case(root: &Path, vector: &DatVector) -> Option<Failure> {
    let case = case_name(&vector.file);

    // Step 1: the file exists, its byte length on disk is `file_bytes`, and the
    // SHA-256 of its bytes is `file_sha256`.
    let path = root.join(&vector.file);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Some(failure(
                &case,
                REASON_VECTOR_MISSING,
                format!("the vector file {} could not be read: {error}", vector.file),
            ))
        }
    };
    if bytes.len() as u64 != vector.file_bytes {
        return Some(failure(
            &case,
            REASON_VECTOR_SIZE,
            format!(
                "the vector file {} is {} bytes on disk and the manifest declares {}",
                vector.file,
                bytes.len(),
                vector.file_bytes
            ),
        ));
    }
    let digest = match sha256_of_slice(&bytes) {
        Ok(digest) => digest,
        Err(error) => {
            return Some(failure(
                &case,
                REASON_VECTOR_DIGEST,
                format!(
                    "the vector file {} could not be digested: {error}",
                    vector.file
                ),
            ))
        }
    };
    if digest != vector.file_sha256 {
        return Some(failure(
            &case,
            REASON_VECTOR_DIGEST,
            format!(
                "the vector file {} does not have the digest the manifest declares",
                vector.file
            ),
        ));
    }

    // Step 2: the file's bytes are decoded into the archive bytes.
    let archive_bytes = match decode_hex_file(&bytes) {
        Ok(archive_bytes) => archive_bytes,
        Err(detail) => {
            return Some(failure(
                &case,
                REASON_ARCHIVE_PARSE,
                format!(
                    "the vector file {} does not hold a hex archive: {detail}",
                    vector.file
                ),
            ))
        }
    };

    // Step 3: the archive parses.
    let archive = match DatArchive::parse(&archive_bytes, HOLDOUT_ORIGIN) {
        Ok(archive) => archive,
        Err(error) => {
            return Some(failure(
                &case,
                REASON_ARCHIVE_PARSE,
                format!("the archive in {} does not parse: {error}", vector.file),
            ))
        }
    };

    // Step 4: the archive holds an entry whose composed path is `path`.
    //
    // The lookup is an exact match on the composed path (DAT-031, DAT-032), so
    // finding the entry IS the assertion that the vector's path is the path
    // the reader composes.
    let entry = match archive.find(&vector.path) {
        Some(entry) => entry,
        None => {
            return Some(failure(
                &case,
                REASON_ENTRY_NOT_FOUND,
                format!(
                    "the archive in {} holds no entry at the path the manifest names",
                    vector.file
                ),
            ))
        }
    };

    // Step 5: that entry decodes.
    let decoded = match decode_entry(entry, &archive_bytes) {
        Ok(decoded) => decoded,
        Err(error) => {
            return Some(failure(
                &case,
                REASON_DECODE,
                format!("the entry in {} does not decode: {error}", vector.file),
            ))
        }
    };

    // Step 6: the decoded bytes are exactly `decoded_size` bytes long.
    if decoded.len() as u64 != vector.decoded_size {
        return Some(failure(
            &case,
            REASON_DECODED_SIZE,
            format!(
                "the entry in {} decoded to {} bytes and the manifest declares {}",
                vector.file,
                decoded.len(),
                vector.decoded_size
            ),
        ));
    }

    // Step 7: the SHA-256 of the decoded bytes is `decoded_sha256`.
    let decoded_digest = match sha256_of_slice(&decoded) {
        Ok(decoded_digest) => decoded_digest,
        Err(error) => {
            return Some(failure(
                &case,
                REASON_DECODED_DIGEST,
                format!(
                    "the decoded bytes of {} could not be digested: {error}",
                    vector.file
                ),
            ))
        }
    };
    if decoded_digest != vector.decoded_sha256 {
        return Some(failure(
            &case,
            REASON_DECODED_DIGEST,
            format!(
                "the entry in {} decoded to bytes the manifest does not name",
                vector.file
            ),
        ));
    }

    // Step 8: the entry's own declared uncompressed size equals
    // `decoded_size`. Redundant with step 6 for a correct reader, and kept
    // because it catches a vector whose own numbers disagree.
    if u64::from(entry.uncompressed_size) != vector.decoded_size {
        return Some(failure(
            &case,
            REASON_DECODED_SIZE,
            format!(
                "the entry in {} declares an uncompressed size the manifest does not name",
                vector.file
            ),
        ));
    }

    None
}

/// Decodes a vector file's bytes into the archive bytes (HOLD-015).
///
/// The file holds the archive as lower case hex, two characters per byte, with
/// no whitespace inside it and a single trailing newline. The reader removes
/// that one newline and requires the rest to be hex characters of even length.
fn decode_hex_file(bytes: &[u8]) -> Result<Vec<u8>, &'static str> {
    let body = match bytes.split_last() {
        Some((b'\n', body)) => body,
        _ => return Err("the file does not end with a newline"),
    };
    if body.is_empty() {
        return Err("the file is empty");
    }
    if body.len() % 2 != 0 {
        return Err("the file holds an odd number of hex characters");
    }
    let decoded: Result<Vec<u8>, &'static str> = body
        .chunks(2)
        .map(|pair| {
            let high = hex_value(pair[0])
                .ok_or("the file holds a character that is not a hex character")?;
            let low = hex_value(pair[1])
                .ok_or("the file holds a character that is not a hex character")?;
            Ok((high << 4) | low)
        })
        .collect();
    match decoded {
        Ok(archive) => Ok(archive),
        Err(detail) => Err(detail),
    }
}

/// The value of one lower case hex character.
fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

// --- the executable table case of HOLD-023 -----------------------------------

/// Runs one executable table vector (HOLD-023).
///
/// Every step of HOLD-023 is an assertion, in the order the spec gives them.
fn run_exe_case(
    path: &Path,
    exe_bytes: &[u8],
    image: &PeImage<'_>,
    vector: &ExeVector,
) -> Option<Failure> {
    let case = case_name(&vector.file);

    // Step 1: the vector file exists, its byte length on disk is `file_bytes`
    // and is equal to `length`, and the SHA-256 of its bytes is `file_sha256`.
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            return Some(failure(
                &case,
                REASON_VECTOR_MISSING,
                format!("the vector file {} could not be read: {error}", vector.file),
            ))
        }
    };
    if bytes.len() as u64 != vector.file_bytes || bytes.len() as u64 != vector.length {
        return Some(failure(
            &case,
            REASON_VECTOR_SIZE,
            format!(
                "the vector file {} is {} bytes on disk, and the manifest declares {} and {}",
                vector.file,
                bytes.len(),
                vector.file_bytes,
                vector.length
            ),
        ));
    }
    let digest = match sha256_of_slice(&bytes) {
        Ok(digest) => digest,
        Err(error) => {
            return Some(failure(
                &case,
                REASON_VECTOR_DIGEST,
                format!(
                    "the vector file {} could not be digested: {error}",
                    vector.file
                ),
            ))
        }
    };
    if digest != vector.file_sha256 {
        return Some(failure(
            &case,
            REASON_VECTOR_DIGEST,
            format!(
                "the vector file {} does not have the digest the manifest declares",
                vector.file
            ),
        ));
    }

    // Step 2: `length` is at least one, and the window does not pass the end
    // of the executable.
    if vector.length < 1 {
        return Some(failure(
            &case,
            REASON_OFFSET_OUTSIDE,
            format!(
                "the vector {} declares a window of {} bytes, and a window is never empty",
                vector.file, vector.length
            ),
        ));
    }
    let end = match vector.file_offset.checked_add(vector.length) {
        Some(end) => end,
        None => {
            return Some(failure(
                &case,
                REASON_OFFSET_OUTSIDE,
                format!(
                    "the window of {} does not fit the executable at all",
                    vector.file
                ),
            ))
        }
    };
    if end > exe_bytes.len() as u64 {
        return Some(failure(
            &case,
            REASON_OFFSET_OUTSIDE,
            format!(
                "the window of {} runs past the end of the executable",
                vector.file
            ),
        ));
    }

    // Step 3: the reader, asked for `length` bytes at `va`, returns exactly
    // the vector file's bytes.
    let length = match usize::try_from(vector.length) {
        Ok(length) => length,
        Err(_) => {
            return Some(failure(
                &case,
                REASON_OFFSET_OUTSIDE,
                format!("the window of {} does not fit a usize", vector.file),
            ))
        }
    };
    let read = match image.read_va(vector.va, length) {
        Ok(read) => read,
        Err(error) => {
            return Some(failure(
                &case,
                REASON_READER_BYTES,
                format!(
                    "the reader refused the window of {} at its virtual address: {error}",
                    vector.file
                ),
            ))
        }
    };
    if read != bytes.as_slice() {
        return Some(failure(
            &case,
            REASON_READER_BYTES,
            format!(
                "the reader returned bytes at the virtual address of {} that are not the \
                 vector file's bytes",
                vector.file
            ),
        ));
    }

    // Step 4: the reader's own section table maps `va` to `file_offset`.
    let section = match image.section_for(vector.va) {
        Some(section) => section,
        None => {
            return Some(failure(
                &case,
                REASON_MAPPING,
                format!("no section owns the virtual address of {}", vector.file),
            ))
        }
    };
    let mapped = match mapped_offset(vector.va, section.raw_pointer, section.virtual_address) {
        Some(mapped) => mapped,
        None => {
            return Some(failure(
                &case,
                REASON_MAPPING,
                format!(
                    "the virtual address of {} does not map into its section",
                    vector.file
                ),
            ))
        }
    };
    if mapped != vector.file_offset {
        return Some(failure(
            &case,
            REASON_MAPPING,
            format!(
                "the reader maps the virtual address of {} to a file offset the manifest \
                 does not name",
                vector.file
            ),
        ));
    }

    // Step 5: the bytes at `file_offset` in the executable file are the vector
    // file's bytes.
    let start = match usize::try_from(vector.file_offset) {
        Ok(start) => start,
        Err(_) => {
            return Some(failure(
                &case,
                REASON_OFFSET_OUTSIDE,
                format!("the offset of {} does not fit a usize", vector.file),
            ))
        }
    };
    let end = match usize::try_from(end) {
        Ok(end) => end,
        Err(_) => {
            return Some(failure(
                &case,
                REASON_OFFSET_OUTSIDE,
                format!("the window end of {} does not fit a usize", vector.file),
            ))
        }
    };
    let at_offset = match exe_bytes.get(start..end) {
        Some(at_offset) => at_offset,
        None => {
            return Some(failure(
                &case,
                REASON_OFFSET_OUTSIDE,
                format!(
                    "the window of {} does not fit the executable file",
                    vector.file
                ),
            ))
        }
    };
    if at_offset != bytes.as_slice() {
        return Some(failure(
            &case,
            REASON_READER_BYTES,
            format!(
                "the bytes at the file offset of {} are not the vector file's bytes",
                vector.file
            ),
        ));
    }

    None
}

/// `raw_pointer + (va - IMAGE_BASE - virtual_address)`, the mapping EXETBL-002
/// states (HOLD-023 step 4).
fn mapped_offset(va: u32, raw_pointer: u32, virtual_address: u32) -> Option<u64> {
    let rva = va.checked_sub(IMAGE_BASE)?;
    let delta = rva.checked_sub(virtual_address)?;
    let inside = u64::from(delta);
    let pointer = u64::from(raw_pointer);
    pointer.checked_add(inside)
}

// --- the DAT part ------------------------------------------------------------

/// claim: HOLD-001, HOLD-003, HOLD-004, HOLD-005, HOLD-006, HOLD-007, HOLD-008,
///        HOLD-009, HOLD-010, HOLD-011, HOLD-012, HOLD-013, HOLD-014, HOLD-015,
///        HOLD-016, HOLD-017, HOLD-018, HOLD-025, HOLD-026, HOLD-027
/// oracle: class 4 (an independent reference implementation)
#[test]
#[ignore = "needs the hold-out folder: FO1_REF/holdout/ with the owner's vectors"]
fn holdout_dat_vectors_decode_to_the_declared_size_and_digest() {
    let mut cases = 0usize;
    let mut failed = 0usize;

    let (root, manifest) = match read_holdout() {
        Ok(holdout) => holdout,
        Err(failure) => {
            print_failure("dat", &failure);
            print_summary("dat", 0, 0);
            panic!(
                "holdout dat: the hold-out set could not be read: {}",
                failure.detail
            );
        }
    };

    let declared = declared_files(&manifest.dat);
    if let Some(failure) = undeclared_file(&root, "dat", &declared) {
        print_failure("dat", &failure);
        print_summary("dat", 0, 0);
        panic!(
            "holdout dat: the hold-out set is not what the manifest describes: {}",
            failure.detail
        );
    }

    for vector in manifest.dat.iter() {
        cases += 1;
        if let Some(failure) = run_dat_case(&root, vector) {
            print_failure("dat", &failure);
            failed += 1;
        }
    }

    let passed = cases - failed;
    print_summary("dat", cases, passed);
    assert_eq!(
        failed, 0,
        "holdout dat: {failed} of {cases} cases failed, and the hold-out set grades the \
         reader with vectors the reader has never seen"
    );
}

// --- the executable table part -----------------------------------------------

/// claim: HOLD-001, HOLD-003, HOLD-004, HOLD-005, HOLD-006, HOLD-007, HOLD-008,
///        HOLD-009, HOLD-010, HOLD-011, HOLD-012, HOLD-020, HOLD-021, HOLD-022,
///        HOLD-023, HOLD-025, HOLD-026, HOLD-027
/// oracle: class 1 (bytes of the original executable)
#[test]
#[ignore = "needs the hold-out folder: FO1_REF/holdout/ and the reference install: FO1_DATA"]
fn holdout_exe_table_vectors_match_the_reference_executable_bytes() {
    let mut cases = 0usize;
    let mut failed = 0usize;

    let (root, manifest) = match read_holdout() {
        Ok(holdout) => holdout,
        Err(failure) => {
            print_failure("exe", &failure);
            print_summary("exe", 0, 0);
            panic!(
                "holdout exe: the hold-out set could not be read: {}",
                failure.detail
            );
        }
    };

    let declared = declared_exe_files(&manifest.exe);
    if let Some(failure) = undeclared_file(&root, "exe", &declared) {
        print_failure("exe", &failure);
        print_summary("exe", 0, 0);
        panic!(
            "holdout exe: the hold-out set is not what the manifest describes: {}",
            failure.detail
        );
    }

    // HOLD-022: the reader reads the player's own copy of the reference
    // executable, reached through the install harness.
    let install = match Install::from_env() {
        Ok(install) => install,
        Err(error) => {
            print_summary("exe", 0, 0);
            panic!("holdout exe: the reference install is unreachable: {error}");
        }
    };
    let exe_bytes = match install.read_archive(install.exe_path()) {
        Ok(exe_bytes) => exe_bytes,
        Err(error) => {
            print_summary("exe", 0, 0);
            panic!("holdout exe: the reference executable could not be read: {error}");
        }
    };

    // HOLD-022: the digest gate is the reader's own entry point, and it is
    // checked before any vector is applied. A build that is not the reference
    // build fails the whole part, because every constant in the executable
    // layer is true for one exact build only.
    let image = match PeImage::require_reference_build(&exe_bytes) {
        Ok(image) => image,
        Err(error) => {
            let failure = failure(
                "executable",
                REASON_BUILD,
                format!("the executable at run time is not the reference build: {error}"),
            );
            print_failure("exe", &failure);
            print_summary("exe", 0, 0);
            panic!("holdout exe: the executable is not the reference build: {error}");
        }
    };

    for vector in manifest.exe.iter() {
        cases += 1;
        let path = root.join(&vector.file);
        if let Some(failure) = run_exe_case(&path, &exe_bytes, &image, vector) {
            print_failure("exe", &failure);
            failed += 1;
        }
    }

    let passed = cases - failed;
    print_summary("exe", cases, passed);
    assert_eq!(
        failed, 0,
        "holdout exe: {failed} of {cases} cases failed, and the hold-out set grades the \
         reader with vectors the reader has never seen"
    );
}
