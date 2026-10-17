//! Test author's reference DAT tree walker. **Oracle class 4.**
//!
//! Walks a DAT archive's two level tree and returns directory names and entry
//! records **without decompressing anything**, so the known plaintext test can
//! locate an archive entry by path and read its raw extent itself.
//!
//! Written from `docs/spec/dat.md` alone. It never becomes a production
//! reader: `fo1-tests` is a dev-dependency and no runtime crate may depend on
//! it (guide: Architecture; `tools/check_layers.sh` enforces it).
//!
//! Two rules from the spec are load bearing and are implemented literally:
//!
//! - Tree order is **not** physical order (DAT-012). An offset is read from the
//!   entry record and from nowhere else. Never from an index.
//! - The root directory of `MASTER.DAT` is named `.` and contributes no
//!   separator to a composed path (DAT-011). The rule keys on the **name**,
//!   not on the block index, because `CRITTER.DAT` has one directory named
//!   `ART\CRITTERS` whose entries all compose with the prefix.

use std::collections::BTreeMap;

/// Bytes in the root header and in every block header.
pub const TREE_HEADER_SIZE: usize = 16;

/// The same size as [`TREE_HEADER_SIZE`] in the width the header field uses, so
/// the comparison against a parsed field needs no narrowing conversion.
pub const TREE_HEADER_SIZE_U32: u32 = 16;

/// The directory name that contributes no path separator (DAT-011).
pub const ROOT_DIRECTORY_NAME: &str = ".";

/// Everything the reference walker can fail at. No variant carries file bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefDatError {
    /// The archive ended before a field or a record was complete.
    Truncated {
        /// What was being read.
        what: &'static str,
        /// Byte offset where the read started.
        at: usize,
        /// Bytes the archive had left.
        available: usize,
        /// Bytes the record needed.
        needed: usize,
    },
    /// A length byte or a declared count could not be used.
    BadLength {
        /// What was being read.
        what: &'static str,
        /// The offending value.
        value: usize,
        /// Byte offset where the value was read from.
        at: usize,
    },
    /// An index computation did not fit.
    ArithmeticOverflow {
        /// What was being counted.
        what: &'static str,
        /// The value that would have been produced.
        at: usize,
    },
    /// The archive is laid out differently from the spec.
    ///
    /// The spec fixes every block header's field 2 at the entry trailer width
    /// (DAT-006). A walker that only ever needs a tree lookup does not depend
    /// on that value, so this variant reports it rather than hiding it.
    UnexpectedBlockHeaderField {
        /// Index of the block.
        block: usize,
        /// Byte offset of the block header.
        at: usize,
        /// The value found in field 2.
        value: u32,
    },
}

impl core::fmt::Display for RefDatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Truncated {
                what,
                at,
                available,
                needed,
            } => write!(
                f,
                "reference dat: {what} truncated at offset {at}, {available} byte(s) available, \
                 {needed} needed"
            ),
            Self::BadLength { what, value, at } => {
                write!(f, "reference dat: bad {what} value {value} at offset {at}")
            }
            Self::ArithmeticOverflow { what, at } => {
                write!(
                    f,
                    "reference dat: arithmetic overflow counting {what} at {at}"
                )
            }
            Self::UnexpectedBlockHeaderField { block, at, value } => write!(
                f,
                "reference dat: block {block} header field 2 is {value} at offset {at}, the spec \
                 fixes it at {TREE_HEADER_SIZE}"
            ),
        }
    }
}

impl std::error::Error for RefDatError {}

/// One entry record as stored in the tree (DAT-009).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefEntry {
    /// Index of the owning directory in the tree.
    pub directory_index: usize,
    /// The entry's own name, with no path separator.
    pub name: String,
    /// The four u32 values: flags, absolute offset, uncompressed size,
    /// compressed size.
    pub flags: u32,
    /// Absolute byte offset into the archive file, not into the entry (DAT-012).
    pub offset: u32,
    /// Declared uncompressed size.
    pub uncompressed_size: u32,
    /// Declared compressed size. Zero for a stored entry, nonzero for a chain
    /// entry (DAT-017).
    pub compressed_size: u32,
}

impl RefEntry {
    /// Payload mode, the high nibble of `flags` shifted into place (DAT-013).
    ///
    /// A `flags` value of 0 means `0x10`, per the spec.
    #[must_use]
    pub fn mode(&self) -> u8 {
        // The upper nibble of the flags word is the mode. Masking leaves
        // 0x00..=0xF0, which always fits in a byte; the conversion is written
        // out so that the impossible case says so rather than quietly reading
        // as a clear nibble.
        match u8::try_from(self.flags & 0xF0) {
            Ok(0x00) => 0x10,
            Ok(mode) => mode,
            Err(_) => 0x00,
        }
    }

    /// Directory name joined with the entry name by one backslash, except that
    /// the directory literally named `.` contributes no separator (DAT-011).
    #[must_use]
    pub fn path(&self, directory: &str) -> String {
        if directory == ROOT_DIRECTORY_NAME {
            self.name.clone()
        } else {
            format!("{directory}\\{}", self.name)
        }
    }
}

/// One directory block: a header and its entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefDirectory {
    /// Index of this directory in the tree.
    pub index: usize,
    /// The stored directory name, backslashes included.
    pub name: String,
    /// Byte offset of this directory's 16 byte block header.
    pub header_offset: usize,
    /// Byte offset of this directory's first entry record.
    pub entries_offset: usize,
    /// Entry records, in tree order.
    pub entries: Vec<RefEntry>,
}

/// A walked archive tree: names, directories and an uppercase path index.
#[derive(Debug, Clone)]
pub struct RefDatTree {
    directory_count: u32,
    root_fields: [u32; 4],
    directories: Vec<RefDirectory>,
    by_path: BTreeMap<String, (usize, usize)>,
    duplicate_paths: Vec<String>,
    entries_total: usize,
}

impl RefDatTree {
    /// Walks the whole tree of `archive`.
    ///
    /// Nothing is decompressed and nothing is read at any entry's offset, so
    /// this is cheap enough to run over both archives in every gate test.
    ///
    /// # Errors
    ///
    /// [`RefDatError`] on any layout that does not match
    /// `docs/spec/dat.md`: a truncated record, a length that does not fit the
    /// remaining bytes, or a block header whose field 2 is not the entry
    /// trailer width.
    pub fn walk(archive: &[u8]) -> Result<Self, RefDatError> {
        let root_fields = read_header(archive, 0, "root header")?;
        let directory_count = u32_to_usize(root_fields[0], "root directory count", 0)?;

        // Directory names come first, one length prefixed run each (DAT-003).
        let mut cursor = TREE_HEADER_SIZE;
        let mut names: Vec<String> = Vec::with_capacity(directory_count);
        for _ in 0..directory_count {
            let (name, next) = read_name(archive, cursor, "directory name")?;
            names.push(name);
            cursor = next;
        }

        // Then one block header per directory, in the same order (DAT-004), and
        // nothing separates the blocks (DAT-008).
        let mut directories: Vec<RefDirectory> = Vec::with_capacity(directory_count);
        for (index, name) in names.iter().enumerate() {
            let header_offset = cursor;
            let fields = read_header(archive, header_offset, "block header")?;
            if fields[2] != TREE_HEADER_SIZE_U32 {
                return Err(RefDatError::UnexpectedBlockHeaderField {
                    block: index,
                    at: header_offset,
                    value: fields[2],
                });
            }
            // Field 0 is the entry count at a block, not the directory count
            // that the same slot holds at the root (DAT-005).
            let entry_count = u32_to_usize(fields[0], "block entry count", header_offset)?;
            cursor = header_offset + TREE_HEADER_SIZE;

            let mut entries: Vec<RefEntry> = Vec::with_capacity(entry_count);
            for _ in 0..entry_count {
                let (name, next) = read_name(archive, cursor, "entry name")?;
                let tail = read_header(archive, next, "entry trailer")?;
                entries.push(RefEntry {
                    directory_index: index,
                    name,
                    flags: tail[0],
                    offset: tail[1],
                    uncompressed_size: tail[2],
                    compressed_size: tail[3],
                });
                cursor = next + TREE_HEADER_SIZE;
            }

            directories.push(RefDirectory {
                index,
                name: name.clone(),
                header_offset,
                entries_offset: header_offset + TREE_HEADER_SIZE,
                entries,
            });
        }

        let mut by_path = BTreeMap::new();
        let mut duplicate_paths: Vec<String> = Vec::new();
        let mut entries_total = 0usize;
        for dir in &directories {
            for (entry_index, entry) in dir.entries.iter().enumerate() {
                let path = entry.path(&dir.name);
                if by_path
                    .insert(path.clone(), (dir.index, entry_index))
                    .is_some()
                {
                    // A duplicate path would make a lookup ambiguous. DAT-011
                    // counts 25,243 composed paths with zero duplicates, so the
                    // walker records them instead of picking one.
                    duplicate_paths.push(path);
                }
                entries_total += 1;
            }
        }

        Ok(Self {
            // The walk advanced by one per name and never past `names`, whose
            // length came from a u32 field, so the count fits. The fallback
            // ceiling is unreachable, and it is a ceiling rather than zero
            // because zero would read as a valid "this tree has no
            // directories" finding.
            directory_count: u32::try_from(directory_count).unwrap_or(u32::MAX),
            root_fields,
            directories,
            by_path,
            duplicate_paths,
            entries_total,
        })
    }

    /// Composed paths that appeared more than once. Must be empty: DAT-011
    /// counts 25,243 paths with zero duplicates across both archives.
    #[must_use]
    pub fn duplicate_paths(&self) -> &[String] {
        &self.duplicate_paths
    }

    /// The root header's four u32 values, exactly as stored.
    #[must_use]
    pub fn root_fields(&self) -> [u32; 4] {
        self.root_fields
    }

    /// Directory count from the root header.
    #[must_use]
    pub fn directory_count(&self) -> usize {
        self.directories.len()
    }

    /// Every directory block, in tree order.
    #[must_use]
    pub fn directories(&self) -> &[RefDirectory] {
        &self.directories
    }

    /// Directory names, in tree order.
    #[must_use]
    pub fn directory_names(&self) -> Vec<&str> {
        self.directories.iter().map(|d| d.name.as_str()).collect()
    }

    /// Total entry records across every directory.
    #[must_use]
    pub fn entries_total(&self) -> usize {
        self.entries_total
    }

    /// Every composed path, in ascending order.
    #[must_use]
    pub fn paths(&self) -> Vec<String> {
        self.by_path.keys().cloned().collect()
    }

    /// Looks up one entry by composed path.
    ///
    /// The path must already be uppercase (DAT-010). This walks the tree; it
    /// does not use the index, so it exercises the walker rather than trusting
    /// it.
    ///
    /// # Errors
    ///
    /// [`RefDatError::BadLength`] when no directory or entry matches.
    pub fn find(&self, upper_path: &str) -> Result<&RefEntry, RefDatError> {
        for dir in &self.directories {
            for entry in &dir.entries {
                if entry.path(&dir.name) == upper_path {
                    return Ok(entry);
                }
            }
        }
        Err(RefDatError::BadLength {
            what: "archive path",
            value: upper_path.len(),
            at: 0,
        })
    }

    /// The raw on-disk extent of `upper_path`, exactly as many bytes as its
    /// compressed size, without decompressing anything.
    ///
    /// # Errors
    ///
    /// [`RefDatError`] when the path is unknown, when the extent runs past the
    /// archive (DAT-021) or when the bytes could not be read.
    pub fn raw_extent<'a>(
        &self,
        archive: &'a [u8],
        upper_path: &str,
    ) -> Result<&'a [u8], RefDatError> {
        let entry = self.find(upper_path)?;
        let start = u32_to_usize(entry.offset, "entry offset", 0)?;
        let len = u32_to_usize(entry.compressed_size, "entry compressed size", 0)?;
        let end = start
            .checked_add(len)
            .ok_or(RefDatError::ArithmeticOverflow {
                what: "entry extent end",
                at: start,
            })?;
        if end > archive.len() {
            return Err(RefDatError::Truncated {
                what: "entry extent",
                at: start,
                available: archive.len().saturating_sub(start),
                needed: len,
            });
        }
        match archive.get(start..end) {
            Some(slice) => Ok(slice),
            None => Err(RefDatError::Truncated {
                what: "entry extent",
                at: start,
                available: archive.len().saturating_sub(start),
                needed: len,
            }),
        }
    }

    /// Total count of entries whose payload mode is `mode`, counted from the
    /// stored `flags` and nowhere else (DAT-013).
    #[must_use]
    pub fn count_by_mode(&self, mode: u8) -> usize {
        self.directories
            .iter()
            .flat_map(|d| d.entries.iter())
            .filter(|e| e.mode() == mode)
            .count()
    }

    /// Total compressed size of every entry whose payload mode is `mode`.
    #[must_use]
    pub fn compressed_bytes_by_mode(&self, mode: u8) -> u64 {
        self.directories
            .iter()
            .flat_map(|d| d.entries.iter())
            .filter(|e| e.mode() == mode)
            .map(|e| u64::from(e.compressed_size))
            .sum()
    }

    /// `directory_count` as the root header stored it, kept separate from the
    /// walked length so a test can compare the two.
    #[must_use]
    pub fn declared_directory_count(&self) -> u32 {
        self.directory_count
    }
}

fn read_header(buf: &[u8], at: usize, what: &'static str) -> Result<[u32; 4], RefDatError> {
    let end = at
        .checked_add(TREE_HEADER_SIZE)
        .ok_or(RefDatError::ArithmeticOverflow { what, at })?;
    let slice = match buf.get(at..end) {
        Some(s) => s,
        None => {
            return Err(RefDatError::Truncated {
                what,
                at,
                available: buf.len().saturating_sub(at),
                needed: TREE_HEADER_SIZE,
            })
        }
    };
    let mut out = [0u32; 4];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut b = [0u8; 4];
        if let Some(raw) = slice.get(i * 4..i * 4 + 4) {
            b.copy_from_slice(raw);
        }
        *slot = u32::from_be_bytes(b);
    }
    Ok(out)
}

fn read_name(buf: &[u8], at: usize, what: &'static str) -> Result<(String, usize), RefDatError> {
    let len = match buf.get(at) {
        Some(b) => usize::from(*b),
        None => {
            return Err(RefDatError::Truncated {
                what,
                at,
                available: buf.len().saturating_sub(at),
                needed: 1,
            })
        }
    };
    let end = at.checked_add(1).and_then(|v| v.checked_add(len)).ok_or(
        RefDatError::ArithmeticOverflow {
            what: "name end",
            at,
        },
    )?;
    let bytes = match buf.get(at + 1..end) {
        Some(s) => s,
        None => {
            return Err(RefDatError::Truncated {
                what,
                at,
                available: buf.len().saturating_sub(at),
                needed: len + 1,
            })
        }
    };
    // The spec says every stored name is ASCII and carries no separator, so a
    // lossy decode would hide a layout change rather than report it.
    let name = String::from_utf8(bytes.to_vec()).map_err(|_| RefDatError::BadLength {
        what: "non-ascii name length",
        value: len,
        at,
    })?;
    Ok((name, end))
}

fn u32_to_usize(value: u32, what: &'static str, at: usize) -> Result<usize, RefDatError> {
    usize::try_from(value).map_err(|_| RefDatError::ArithmeticOverflow { what, at })
}
