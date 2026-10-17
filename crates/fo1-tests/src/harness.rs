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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    /// Role of the file, e.g. `falloutw.exe`.
    pub role: &'static str,
    /// Absolute path that was checked.
    pub path: PathBuf,
    /// Size on disk in bytes.
    pub bytes: u64,
    /// Lowercase hex SHA-256 of the whole file.
    pub sha256: String,
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

        let exe = fingerprint(&exe_path, EXE_FILE_NAME)?;
        let master_dat = fingerprint(&master_path, MASTER_DAT_FILE_NAME)?;
        let critter_dat = fingerprint(&critter_path, CRITTER_DAT_FILE_NAME)?;

        let exe = check_sha256(exe, REFERENCE_EXE_SHA256)?;
        let master_dat = check_size(master_dat, REFERENCE_MASTER_DAT_BYTES)?;
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

fn fingerprint(path: &Path, role: &'static str) -> Result<Fingerprint, HarnessError> {
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
    let bytes = meta.len();
    let sha256 = sha256_of_file(path)?;
    Ok(Fingerprint {
        role,
        path: path.to_path_buf(),
        bytes,
        sha256,
    })
}

fn check_sha256(fp: Fingerprint, expected: &str) -> Result<Fingerprint, HarnessError> {
    if fp.sha256 != expected {
        return Err(HarnessError::UnsupportedInstall {
            what: "falloutw.exe sha256",
            path: fp.path,
            expected: expected.to_string(),
            actual: fp.sha256,
        });
    }
    Ok(fp)
}

fn check_size(fp: Fingerprint, expected: u64) -> Result<Fingerprint, HarnessError> {
    if fp.bytes != expected {
        return Err(HarnessError::UnsupportedInstall {
            what: "archive size",
            path: fp.path,
            expected: format!("{expected} bytes"),
            actual: format!("{} bytes, sha256 {}", fp.bytes, fp.sha256),
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
