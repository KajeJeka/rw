//! The `FO1_DATA` install harness.
//!
//! Resolves the player's install from an environment variable, fingerprints it
//! against the reference install, and hands gate tests byte slices and paths.
//!
//! # Memory
//!
//! [`read_archive`] reads a whole archive into memory. `MASTER.DAT` is
//! 339,379,746 bytes and `CRITTER.DAT` is 157,085,138 bytes, so a call holds
//! hundreds of megabytes. That is deliberate and acceptable **here** because
//! this crate is a dev-dependency gate harness that runs once on the owner's
//! machine against one known install, and because the whole-corpus closure
//! tests need random access across an archive. Production crates must never
//! take this approach and must not depend on this crate.
//!
//! # Never
//!
//! This module never spawns a process, never runs the game, a patcher or an
//! installer, and never writes to the install. The guide forbids all three
//! (guide: Rules for the agent, standing rules).

use std::fs;
use std::path::{Path, PathBuf};

use crate::sha256::{hex, sha256_of_file};

/// Environment variable naming the install root.
pub const ENV_INSTALL_ROOT: &str = "FO1_DATA";

/// SHA-256 of the reference `falloutw.exe` (guide: Source material and trust).
pub const REFERENCE_EXE_SHA256: &str =
    "444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272";

/// Byte length of `MASTER.DAT` in the reference install.
pub const REFERENCE_MASTER_DAT_BYTES: u64 = 339_379_746;

/// Byte length of `CRITTER.DAT` in the reference install.
pub const REFERENCE_CRITTER_DAT_BYTES: u64 = 157_085_138;

/// File name of the reference executable.
pub const EXE_FILE_NAME: &str = "falloutw.exe";

/// File name of the main archive.
pub const MASTER_DAT_FILE_NAME: &str = "MASTER.DAT";

/// File name of the critter archive.
pub const CRITTER_DAT_FILE_NAME: &str = "CRITTER.DAT";

/// Name of the loose patch folder inside the install.
pub const LOOSE_DATA_DIR_NAME: &str = "DATA";

/// Environment variable naming the private references directory.
///
/// The hold-out vectors live under it, outside the repository and outside the
/// working tree, so no crate in this workspace can read them by accident
/// (docs/spec/holdout.md, HOLD-001).
pub const ENV_REFERENCE_ROOT: &str = "FO1_REF";

/// Name of the hold-out vector folder inside the references directory.
pub const HOLDOUT_FOLDER_NAME: &str = "holdout";

/// Name of the hold-out manifest inside the hold-out folder.
pub const HOLDOUT_MANIFEST_NAME: &str = "manifest.txt";

/// Everything the harness can fail at.
///
/// No variant carries file content: each reports a path, an offset or a length.
/// That is the guide's rule for errors (Engineering philosophy, "Errors that
/// help").
#[derive(Debug)]
pub enum HarnessError {
    /// `FO1_DATA` is not set in the environment.
    InstallRootUnset {
        /// The variable that was looked for.
        variable: &'static str,
    },
    /// `FO1_DATA` is set to something that is not a directory.
    InstallRootNotADirectory {
        /// The value that was found.
        value: String,
    },
    /// The private references directory is not set in the environment.
    ReferenceRootUnset {
        /// The variable that was looked for.
        variable: &'static str,
    },
    /// The hold-out vector folder does not exist.
    MissingHoldoutFolder {
        /// The path that was probed.
        path: PathBuf,
    },
    /// The hold-out manifest does not exist inside the hold-out folder.
    MissingHoldoutManifest {
        /// The path that was probed.
        path: PathBuf,
    },
    /// A file the harness needs is missing from the install.
    MissingFile {
        /// Role of the file, e.g. `MASTER.DAT`.
        role: &'static str,
        /// The path that was probed.
        path: PathBuf,
    },
    /// A file could not be read.
    ReadFailed {
        /// The path being read.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
    /// A file exists but is not a regular file.
    NotAFile {
        /// Role of the file.
        role: &'static str,
        /// The path that was probed.
        path: PathBuf,
    },
    /// The install does not match the reference fingerprint.
    ///
    /// This never relaxes a constant. Gate tests stop here (guide: Fixtures and
    /// golden files).
    UnsupportedInstall {
        /// Which fingerprint failed.
        what: &'static str,
        /// The path whose fingerprint or size was checked.
        path: PathBuf,
        /// What the reference install has.
        expected: String,
        /// What this install has.
        actual: String,
    },
    /// A length computation did not fit the destination type.
    ArithmeticOverflow {
        /// What was being counted.
        what: &'static str,
        /// The value that would have been produced.
        at: u64,
    },
    /// An internal invariant that the type system cannot express was violated.
    ///
    /// This exists so that a "cannot happen" branch reports itself instead of
    /// substituting a fallback value. The cheap version of the same idea,
    /// `slice.get(..).unwrap_or(&[])`, returns a shorter slice and produces a
    /// wrong digest with nothing to show for it.
    InternalInvariant {
        /// The invariant, phrased so a reader can check it.
        detail: &'static str,
    },
}

impl core::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InstallRootUnset { variable } => write!(
                f,
                "unsupported install: {variable} is not set. Export it and re-run, for example:\n  \
                 export {variable}='/path/to/Fallout'\nThe harness refuses to guess an install \
                 path and never relaxes a constant."
            ),
            Self::InstallRootNotADirectory { value } => write!(
                f,
                "unsupported install: {ENV_INSTALL_ROOT}={value} is not a directory."
            ),
            Self::ReferenceRootUnset { variable } => write!(
                f,
                "the hold-out vectors are unreachable: {variable} is not set. Export it and \
                 re-run, for example:\n  export {variable}='/path/to/private/references'\nThe \
                 harness never guesses a references path: the hold-out set is the owner's \
                 private folder and nothing in this repository can stand in for it."
            ),
            Self::MissingHoldoutFolder { path } => write!(
                f,
                "the hold-out vectors are unreachable: the hold-out folder {} does not exist. \
                 The hold-out set is the owner's private folder; the repository holds no \
                 vector data at all and cannot substitute for it.",
                path.display()
            ),
            Self::MissingHoldoutManifest { path } => write!(
                f,
                "the hold-out vectors are unreachable: the manifest {} does not exist.",
                path.display()
            ),
            Self::MissingFile { role, path } => {
                write!(
                    f,
                    "unsupported install: {role} is missing at {}",
                    path.display()
                )
            }
            Self::ReadFailed { path, source } => {
                write!(f, "read failed at {}: {source}", path.display())
            }
            Self::NotAFile { role, path } => write!(
                f,
                "unsupported install: {role} at {} is not a regular file",
                path.display()
            ),
            Self::UnsupportedInstall {
                what,
                path,
                expected,
                actual,
            } => write!(
                f,
                "unsupported install: {what} at {} does not match the reference install.\n  \
                 expected {expected}\n  actual   {actual}\nThe harness stops here. It does not \
                 relax a constant to fit a different install.",
                path.display()
            ),
            Self::ArithmeticOverflow { what, at } => {
                write!(f, "arithmetic overflow while counting {what} at {at}")
            }
            Self::InternalInvariant { detail } => {
                write!(f, "internal invariant violated: {detail}")
            }
        }
    }
}

impl std::error::Error for HarnessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// A fingerprint check result for one install file.
///
/// The digest is `None` for a file that is checked by size only. The two
/// archives are 496 MB between them and their digests are never compared
/// against anything, so hashing them cost the gate minutes per suite for no
/// result. The executable is the file whose digest identifies the build, so it
/// is the one that is hashed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    /// Role of the file, e.g. `falloutw.exe`.
    pub role: &'static str,
    /// Absolute path that was checked.
    pub path: PathBuf,
    /// Size on disk in bytes.
    pub bytes: u64,
    /// Lowercase hex SHA-256 of the whole file, when the file was hashed.
    pub sha256: Option<String>,
}

/// A resolved, fingerprinted install of the game.
///
/// Obtained only from [`Install::from_env`], which checks the whole fingerprint
/// before returning. There is no constructor that skips the check, so a test
/// cannot accidentally assert against an unknown install.
#[derive(Debug, Clone)]
pub struct Install {
    root: PathBuf,
    exe: Fingerprint,
    master_dat: Fingerprint,
    critter_dat: Fingerprint,
}

impl Install {
    /// Resolves `FO1_DATA`, verifies the reference fingerprint and returns the
    /// install.
    ///
    /// # Errors
    ///
    /// [`HarnessError::InstallRootUnset`] when the variable is absent, and
    /// [`HarnessError::UnsupportedInstall`] on any fingerprint or size
    /// mismatch. A gate test unwraps this, so a missing or wrong install fails
    /// loudly with the message above instead of skipping.
    pub fn from_env() -> Result<Self, HarnessError> {
        let root = install_root_from_env()?;
        Self::at(root)
    }

    /// Fingerprints the install rooted at `root`.
    ///
    /// # Errors
    ///
    /// As [`Install::from_env`], plus the path errors for `root` itself.
    pub fn at(root: PathBuf) -> Result<Self, HarnessError> {
        if !root.is_dir() {
            return Err(HarnessError::InstallRootNotADirectory {
                value: root.display().to_string(),
            });
        }

        let exe_path = root.join(EXE_FILE_NAME);
        let master_path = root.join(MASTER_DAT_FILE_NAME);
        let critter_path = root.join(CRITTER_DAT_FILE_NAME);

        // The executable is the file whose digest identifies the build, so it
        // is the one that is hashed. The digest is read from the file on every
        // call and is never remembered: a digest carried over from an earlier
        // call goes stale the moment the file's content changes without its
        // size changing, and the gate would then admit a file it never read.
        let exe = fingerprint(&exe_path, EXE_FILE_NAME)?;
        let exe = check_sha256(exe, REFERENCE_EXE_SHA256)?;

        // The two archives are checked by size only. Their digests were never
        // compared against anything, and hashing 496 MB per test was most of
        // the gate's wall clock.
        let master_dat = size_fingerprint(&master_path, MASTER_DAT_FILE_NAME)?;
        let master_dat = check_size(master_dat, REFERENCE_MASTER_DAT_BYTES)?;
        let critter_dat = size_fingerprint(&critter_path, CRITTER_DAT_FILE_NAME)?;
        let critter_dat = check_size(critter_dat, REFERENCE_CRITTER_DAT_BYTES)?;

        Ok(Self {
            root,
            exe,
            master_dat,
            critter_dat,
        })
    }

    /// The install root as supplied.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Fingerprint of `falloutw.exe`.
    #[must_use]
    pub fn exe(&self) -> &Fingerprint {
        &self.exe
    }

    /// Fingerprint of `MASTER.DAT`.
    #[must_use]
    pub fn master_dat(&self) -> &Fingerprint {
        &self.master_dat
    }

    /// Fingerprint of `CRITTER.DAT`.
    #[must_use]
    pub fn critter_dat(&self) -> &Fingerprint {
        &self.critter_dat
    }

    /// Path of `falloutw.exe`.
    #[must_use]
    pub fn exe_path(&self) -> &Path {
        &self.exe.path
    }

    /// Path of `MASTER.DAT`.
    #[must_use]
    pub fn master_dat_path(&self) -> &Path {
        &self.master_dat.path
    }

    /// Path of `CRITTER.DAT`.
    #[must_use]
    pub fn critter_dat_path(&self) -> &Path {
        &self.critter_dat.path
    }

    /// Path of the loose `DATA` patch folder, which the High Resolution Patch
    /// changed. Known plaintext tests must read the archive, not this folder.
    #[must_use]
    pub fn loose_data_dir(&self) -> PathBuf {
        self.root.join(LOOSE_DATA_DIR_NAME)
    }

    /// True when the loose `DATA` folder exists.
    #[must_use]
    pub fn has_loose_data_dir(&self) -> bool {
        self.loose_data_dir().is_dir()
    }

    /// Reads a whole archive into memory. See the module note on memory.
    ///
    /// # Errors
    ///
    /// [`HarnessError::ReadFailed`] when the file cannot be read.
    pub fn read_archive(&self, path: &Path) -> Result<Vec<u8>, HarnessError> {
        fs::read(path).map_err(|source| HarnessError::ReadFailed {
            path: path.to_path_buf(),
            source,
        })
    }

    /// Reads `MASTER.DAT` into memory.
    ///
    /// # Errors
    ///
    /// As [`Install::read_archive`].
    pub fn read_master_dat(&self) -> Result<Vec<u8>, HarnessError> {
        self.read_archive(self.master_dat_path())
    }

    /// Reads `CRITTER.DAT` into memory.
    ///
    /// # Errors
    ///
    /// As [`Install::read_archive`].
    pub fn read_critter_dat(&self) -> Result<Vec<u8>, HarnessError> {
        self.read_archive(self.critter_dat_path())
    }

    /// Reads the loose file at `relative`, if the loose folder holds one.
    ///
    /// The High Resolution Patch rewrote some of these, so this is never proof
    /// of anything on its own (guide: DAT archives, patch folder rule).
    ///
    /// # Errors
    ///
    /// [`HarnessError::ReadFailed`] when the file exists but cannot be read.
    pub fn read_loose_file(&self, relative: &Path) -> Result<Option<Vec<u8>>, HarnessError> {
        let path = self.loose_data_dir().join(relative);
        if !path.is_file() {
            return Ok(None);
        }
        fs::read(&path)
            .map(Some)
            .map_err(|source| HarnessError::ReadFailed { path, source })
    }
}

/// Reads `FO1_DATA` from the environment.
///
/// # Errors
///
/// [`HarnessError::InstallRootUnset`] when it is absent.
pub fn install_root_from_env() -> Result<PathBuf, HarnessError> {
    match std::env::var(ENV_INSTALL_ROOT) {
        Ok(value) if !value.is_empty() => Ok(PathBuf::from(value)),
        _ => Err(HarnessError::InstallRootUnset {
            variable: ENV_INSTALL_ROOT,
        }),
    }
}

/// The hold-out vector folder, `FO1_REF/holdout/`.
///
/// The hold-out set is the owner's private folder (docs/spec/holdout.md,
/// HOLD-001). It is reached through this function and through no other route:
/// the banned pattern rule forbids a read of the references directory outside
/// the harness, so a gate test asks the harness where the folder is and never
/// looks the variable up itself.
///
/// # Errors
///
/// [`HarnessError::ReferenceRootUnset`] when `FO1_REF` is absent or empty, and
/// [`HarnessError::MissingHoldoutFolder`] when the folder does not exist.
/// Neither is a skip: a missing hold-out set fails the gate loudly.
pub fn holdout_root() -> Result<PathBuf, HarnessError> {
    let reference = match std::env::var(ENV_REFERENCE_ROOT) {
        Ok(value) if !value.is_empty() => PathBuf::from(value),
        _ => {
            return Err(HarnessError::ReferenceRootUnset {
                variable: ENV_REFERENCE_ROOT,
            })
        }
    };
    let folder = reference.join(HOLDOUT_FOLDER_NAME);
    if !folder.is_dir() {
        return Err(HarnessError::MissingHoldoutFolder { path: folder });
    }
    Ok(folder)
}

/// The hold-out manifest, `FO1_REF/holdout/manifest.txt`.
///
/// # Errors
///
/// As [`holdout_root`], plus [`HarnessError::MissingHoldoutManifest`] when the
/// manifest is not there.
pub fn holdout_manifest_path() -> Result<PathBuf, HarnessError> {
    let folder = holdout_root()?;
    let manifest = folder.join(HOLDOUT_MANIFEST_NAME);
    if !manifest.is_file() {
        return Err(HarnessError::MissingHoldoutManifest { path: manifest });
    }
    Ok(manifest)
}

/// Fingerprints a file by hashing it and by measuring it.
fn fingerprint(path: &Path, role: &'static str) -> Result<Fingerprint, HarnessError> {
    let bytes = file_size(path, role)?;
    let sha256 = sha256_of_file(path)?;
    Ok(Fingerprint {
        role,
        path: path.to_path_buf(),
        bytes,
        sha256: Some(sha256),
    })
}

/// Fingerprints a file by its size alone, without reading it.
///
/// Used for the archives, whose digests are never compared against anything.
/// The file is still measured and still has to exist.
fn size_fingerprint(path: &Path, role: &'static str) -> Result<Fingerprint, HarnessError> {
    let bytes = file_size(path, role)?;
    Ok(Fingerprint {
        role,
        path: path.to_path_buf(),
        bytes,
        sha256: None,
    })
}

/// The size of a file, after checking that it exists and is a regular file.
fn file_size(path: &Path, role: &'static str) -> Result<u64, HarnessError> {
    if !path.exists() {
        return Err(HarnessError::MissingFile {
            role,
            path: path.to_path_buf(),
        });
    }
    if !path.is_file() {
        return Err(HarnessError::NotAFile {
            role,
            path: path.to_path_buf(),
        });
    }
    let meta = fs::metadata(path).map_err(|source| HarnessError::ReadFailed {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(meta.len())
}

fn check_sha256(fp: Fingerprint, expected: &str) -> Result<Fingerprint, HarnessError> {
    let actual = fp.sha256.clone();
    match actual {
        Some(ref digest) if digest == expected => Ok(fp),
        Some(digest) => Err(HarnessError::UnsupportedInstall {
            what: "falloutw.exe sha256",
            path: fp.path,
            expected: expected.to_string(),
            actual: digest,
        }),
        None => Err(HarnessError::InternalInvariant {
            detail: "the executable fingerprint carries no digest, so it cannot be compared",
        }),
    }
}

fn check_size(fp: Fingerprint, expected: u64) -> Result<Fingerprint, HarnessError> {
    if fp.bytes != expected {
        return Err(HarnessError::UnsupportedInstall {
            what: "archive size",
            path: fp.path,
            expected: format!("{expected} bytes"),
            actual: format!("{} bytes", fp.bytes),
        });
    }
    Ok(fp)
}

/// A one line, content free summary of an install, for a test failure message.
#[must_use]
pub fn describe(install: &Install) -> String {
    format!(
        "install root {} | {} {} bytes | {} {} bytes | {} {} bytes",
        install.root().display(),
        EXE_FILE_NAME,
        install.exe().bytes,
        MASTER_DAT_FILE_NAME,
        install.master_dat().bytes,
        CRITTER_DAT_FILE_NAME,
        install.critter_dat().bytes,
    )
}

/// Hex of a byte slice, re-exported so gate tests can print a short digest of a
/// failing entry without adding a dependency of their own.
#[must_use]
pub fn hex_of(bytes: &[u8]) -> String {
    hex(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A digest of a byte slice, computed the same way the harness computes
    /// the digest of a file.
    fn digest_of(bytes: &[u8]) -> String {
        crate::sha256::sha256_of_slice(bytes).expect("a byte slice digests")
    }

    /// The build digest gate rejects an executable that is not the reference
    /// build.
    ///
    /// The gate is what stands between the suite and an install the constants
    /// do not describe, so it is checked here rather than only through the
    /// reference install, which CI does not have. A digest that is not the
    /// reference digest must be refused, and the refusal must name the digest
    /// rather than a size or a missing file.
    #[test]
    fn the_build_digest_gate_rejects_an_executable_that_is_not_the_reference_build() {
        // A short file that is certainly not the reference build: the reference
        // executable is 1,243,136 bytes.
        let foreign = b"a build that is not the reference build";
        assert_ne!(
            digest_of(foreign),
            REFERENCE_EXE_SHA256,
            "the fixture must not be the reference build, or the test proves nothing"
        );

        let path =
            std::env::temp_dir().join(format!("rw-harness-build-gate-{}.bin", std::process::id()));
        fs::write(&path, foreign).expect("the fixture is written");

        let observed = sha256_of_file(&path).expect("the fixture digests");
        let fp = Fingerprint {
            role: EXE_FILE_NAME,
            path: path.clone(),
            bytes: foreign.len() as u64,
            sha256: Some(observed.clone()),
        };

        let error =
            check_sha256(fp, REFERENCE_EXE_SHA256).expect_err("a foreign build must be refused");
        match error {
            HarnessError::UnsupportedInstall { what, actual, .. } => {
                assert_eq!(what, "falloutw.exe sha256", "the refusal names the digest");
                assert_eq!(actual, observed, "the refusal reports the digest it saw");
            }
            other => panic!("expected an UnsupportedInstall, got {other:?}"),
        }

        let _ = fs::remove_file(&path);
    }

    /// The digest gate admits the reference digest.
    ///
    /// The refusal above would also be produced by a gate that refuses
    /// everything, so the admitting half is checked too.
    #[test]
    fn the_build_digest_gate_admits_the_reference_digest() {
        let fp = Fingerprint {
            role: EXE_FILE_NAME,
            path: PathBuf::from(EXE_FILE_NAME),
            bytes: 1_243_136,
            sha256: Some(REFERENCE_EXE_SHA256.to_string()),
        };
        let admitted =
            check_sha256(fp, REFERENCE_EXE_SHA256).expect("the reference digest must be admitted");
        assert_eq!(
            admitted.sha256.as_deref(),
            Some(REFERENCE_EXE_SHA256),
            "the admitted fingerprint carries the reference digest"
        );
    }

    /// A size only fingerprint carries no digest.
    ///
    /// The archives are checked by size alone, so their fingerprint must not
    /// claim a digest it never computed. A fingerprint that carried a digest
    /// anyway would be a value no test could check, which is the same defect
    /// as a wrong constant.
    #[test]
    fn a_size_only_fingerprint_carries_no_digest() {
        let path =
            std::env::temp_dir().join(format!("rw-harness-size-only-{}.bin", std::process::id()));
        fs::write(&path, b"an archive that is only measured").expect("the fixture is written");

        let fp = size_fingerprint(&path, MASTER_DAT_FILE_NAME).expect("the fixture is measured");
        assert_eq!(fp.bytes, 32, "the size is measured");
        assert_eq!(
            fp.sha256, None,
            "a size only fingerprint must not carry a digest it never computed"
        );

        let _ = fs::remove_file(&path);
    }

    /// The executable digest is read from the file every time, so a file whose
    /// content changed without changing its size cannot keep its old digest.
    ///
    /// The harness used to remember the digest of the executable per process
    /// and reuse it while the file's size was unchanged. That made the gate
    /// blind to the one change a size check cannot see: a file rewritten in
    /// place with the same length. The stale digest still matched the
    /// reference digest, so the gate admitted a file it had never read. The
    /// digest is therefore taken from the file on every call.
    ///
    /// The fingerprint goes through the same path the gate uses, so the test
    /// fails against a harness that remembers digests instead of reading them.
    #[test]
    fn the_executable_digest_is_read_from_the_file_every_time() {
        let path = std::env::temp_dir().join(format!(
            "rw-harness-fresh-digest-{}.bin",
            std::process::id()
        ));
        // The two contents differ, and their lengths do not. That is the case
        // a size keyed cache cannot see.
        let first = b"the first contents of the file";
        let second = b"the other contents of the file";
        assert_eq!(
            first.len(),
            second.len(),
            "the fixture must change its content without changing its size"
        );

        fs::write(&path, first).expect("the fixture is written");
        let first_expected = digest_of(first);
        let first_fp = fingerprint(&path, EXE_FILE_NAME).expect("the fixture fingerprints");
        assert_eq!(
            first_fp.sha256.as_deref(),
            Some(first_expected.as_str()),
            "the fingerprint carries the digest of the bytes on disk"
        );

        fs::write(&path, second).expect("the fixture is rewritten");
        let second_expected = digest_of(second);
        let second_fp =
            fingerprint(&path, EXE_FILE_NAME).expect("the rewritten fixture fingerprints");
        assert_eq!(
            second_fp.sha256.as_deref(),
            Some(second_expected.as_str()),
            "the fingerprint is taken again, so it carries the digest of the bytes now on disk"
        );
        assert_ne!(
            first_fp.sha256, second_fp.sha256,
            "a file whose content changed must not keep the digest it had before"
        );

        // And the gate built from the changed file must refuse it, rather than
        // admit it on the strength of the digest taken before the change.
        let error = check_sha256(second_fp, REFERENCE_EXE_SHA256)
            .expect_err("a changed file must be refused");
        match error {
            HarnessError::UnsupportedInstall { what, actual, .. } => {
                assert_eq!(what, "falloutw.exe sha256", "the refusal names the digest");
                assert_eq!(
                    actual, second_expected,
                    "the refusal reports the digest of the changed file, not the earlier one"
                );
            }
            other => panic!("expected an UnsupportedInstall, got {other:?}"),
        }

        let _ = fs::remove_file(&path);
    }
}
