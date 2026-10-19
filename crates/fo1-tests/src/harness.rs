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
use std::sync::{LazyLock, Mutex, MutexGuard};

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
        // is the one that is hashed. The digest is taken once per process and
        // reused: the gate builds an `Install` per test, and hashing 1.2 MB
        // twenty two times is wasted work.
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

/// One cached digest, with the facts about the file that were true when the
/// digest was computed.
#[derive(Debug, Clone)]
struct CachedDigest {
    /// Size of the file when it was hashed.
    bytes: u64,
    /// Lowercase hex SHA-256 of the whole file.
    digest: String,
}

/// Digests already computed in this process, as a list of (path, digest).
///
/// The gate builds one `Install` per test, and every one of them would
/// otherwise hash the same executable. A file that does not change does not
/// change its digest either, so one hash per process is enough.
///
/// The list is a `Vec` and not a `HashMap`: the order a hash map iterates in
/// is not defined, and this crate's sources are held to determinism
/// (guide: Engineering philosophy, "Determinism"). There is one entry per
/// install file, so a linear scan is the right cost.
///
/// A cached entry is only reused while the file's **size** is unchanged. Size
/// is the one file fact that needs no clock: reading a modification time would
/// make the harness depend on the wall clock, which the determinism rule
/// forbids. A file rewritten in place with the same length is the one case a
/// size check cannot see, and nothing writes to the reference install while
/// the gate runs.
static DIGEST_CACHE: LazyLock<Mutex<Vec<(PathBuf, CachedDigest)>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// The digest cache's lock guard.
///
/// # Errors
///
/// [`HarnessError::InternalInvariant`] when the lock was poisoned, which
/// means another thread panicked while holding it. The poison is reported
/// rather than swallowed: a cache that cannot be locked has not been shown to
/// hold a correct digest.
fn digest_cache_guard() -> Result<MutexGuard<'static, Vec<(PathBuf, CachedDigest)>>, HarnessError> {
    DIGEST_CACHE
        .lock()
        .map_err(|_| HarnessError::InternalInvariant {
            detail: "the digest cache lock was poisoned by a panic in another thread",
        })
}

/// The digest of the file at `path`, hashed at most once per process.
///
/// # Errors
///
/// [`HarnessError::ReadFailed`] when the file cannot be read,
/// [`HarnessError::ArithmeticOverflow`] for a length that cannot be counted,
/// and [`HarnessError::InternalInvariant`] when the digest cache is poisoned.
fn cached_digest(path: &Path, bytes: u64) -> Result<String, HarnessError> {
    {
        let guard = digest_cache_guard()?;
        if let Some((_, cached)) = guard.iter().find(|(cached_path, _)| cached_path == path) {
            if cached.bytes == bytes {
                return Ok(cached.digest.clone());
            }
        }
    }
    // The hash happens outside the lock: it reads the whole file, and the
    // lock is only there to keep the cache consistent.
    let digest = sha256_of_file(path)?;
    let mut guard = digest_cache_guard()?;
    guard.retain(|(cached_path, _)| cached_path != path);
    guard.push((
        path.to_path_buf(),
        CachedDigest {
            bytes,
            digest: digest.clone(),
        },
    ));
    Ok(digest)
}

/// Fingerprints a file by hashing it and by measuring it.
fn fingerprint(path: &Path, role: &'static str) -> Result<Fingerprint, HarnessError> {
    let bytes = file_size(path, role)?;
    let sha256 = cached_digest(path, bytes)?;
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

        let observed = cached_digest(&path, foreign.len() as u64).expect("the fixture digests");
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

    /// The digest cache returns the digest it computed, and rehashes a file
    /// whose size moved.
    ///
    /// The cache is what makes the gate affordable, and a cache that answered
    /// with a stale digest would let a changed executable through the gate.
    /// A file whose size changed is rehashed, so the cache cannot launder a
    /// different file of the same name.
    #[test]
    fn the_digest_cache_rehashes_a_file_whose_size_changed() {
        let path = std::env::temp_dir().join(format!(
            "rw-harness-digest-cache-{}.bin",
            std::process::id()
        ));
        let first = b"the first contents of the fixture";
        fs::write(&path, first).expect("the fixture is written");

        let first_digest = cached_digest(&path, first.len() as u64).expect("the fixture digests");
        assert_eq!(
            first_digest,
            digest_of(first),
            "the cached digest is the digest of the file"
        );

        // The same path, a different file. The size differs, so the entry must
        // not be reused.
        let second = b"the second, longer contents of the fixture";
        fs::write(&path, second).expect("the fixture is rewritten");
        let second_digest =
            cached_digest(&path, second.len() as u64).expect("the rewritten fixture digests");
        assert_eq!(
            second_digest,
            digest_of(second),
            "a file whose size changed is rehashed, not answered from the cache"
        );
        assert_ne!(
            first_digest, second_digest,
            "the two contents must not hash to the same digest"
        );

        let _ = fs::remove_file(&path);
    }
}
