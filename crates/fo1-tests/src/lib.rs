//! The `FO1_DATA` gate harness and the test author's reference decoders.
//!
//! A dev-dependency only. No runtime crate depends on it, and
//! `tools/check_layers.sh` fails the gate if one ever does.
//!
//! # Why this crate exists
//!
//! Two separated jobs, deliberately in one dev-only crate:
//!
//! 1. [`Install`] resolves the player's install from `FO1_DATA`, fingerprints it
//!    against the reference install, and hands gate tests byte slices and paths.
//! 2. [`RefLzss`] and [`RefDatTree`] are **oracle class 4**: independent reference
//!    implementations written by the test author from the spec alone, never
//!    imported by production code. They are the oracle the gate grades the
//!    production decoders with.
//!
//! # Fallibility
//!
//! Every function that reads data returns a `Result`. A gate test that needs the
//! install unwraps the result *inside the test*, where `clippy.toml` permits it,
//! so a missing or wrong install fails loudly with a clear message instead of
//! silently skipping (guide: Exact assertions, "Missing data fails").
//!
//! # Never
//!
//! Nothing in this crate spawns a process, runs the game, a patcher or an
//! installer, or writes to the install.
//!
//! This path is PROTECTED (guide: Verification and anti faking).
//!
//! See `docs/spec/lzss.md`, `docs/spec/dat.md`.
//!
//! No crate level `#![deny(clippy::arithmetic_side_effects)]` here: the guide
//! puts that lint on the rules and data crates (`fo1-dat`, `fo1-script`,
//! `fo1-combat`, `fo1-save`) because game rule arithmetic must be checked or
//! explicitly wrapping. This crate is a harness and a set of reference decoders
//! written to be obviously correct rather than fast, and every arithmetic step
//! that can wrap already says `wrapping_*` on purpose.

mod harness;
mod ref_dat;
mod ref_lzss;
mod sha256;

pub use harness::{
    describe, hex_of, holdout_manifest_path, holdout_root, install_root_from_env, Fingerprint,
    HarnessError, Install, CRITTER_DAT_FILE_NAME, ENV_INSTALL_ROOT, ENV_REFERENCE_ROOT,
    EXE_FILE_NAME, HOLDOUT_FOLDER_NAME, HOLDOUT_MANIFEST_NAME, LOOSE_DATA_DIR_NAME,
    MASTER_DAT_FILE_NAME, REFERENCE_CRITTER_DAT_BYTES, REFERENCE_EXE_SHA256,
    REFERENCE_MASTER_DAT_BYTES,
};
pub use ref_dat::{
    RefDatError, RefDatTree, RefDirectory, RefEntry, ROOT_DIRECTORY_NAME, TREE_HEADER_SIZE,
};
pub use ref_lzss::{
    RefError, RefLzss, CANONICAL_PREFILL, CANONICAL_RING_SIZE, CANONICAL_WRITE_START, MAX_MATCH,
    MIN_MATCH,
};
pub use sha256::{sha256_of_file, sha256_of_slice};

#[cfg(test)]
mod canary;
