//! The data tables compiled into `falloutw.exe`, read through the PE section
//! headers and the little-endian executable layer.
//!
//! The stat limits, the critical hit tables and the XP table live in the
//! executable, not in the game data files. They are read from the player's own
//! copy at run time, or from a local cache under `target/`. They are never
//! copied out of the executable into this repository.
//!
//! Spec: `docs/spec/exe-tables.md`, claims EXETBL-001 to EXETBL-017.
//!
//! Three adjacency identities catch a wrong address or a wrong record size.
//! See `docs/spec/exe-tables.md` for EXETBL-010, EXETBL-011 and EXETBL-012.

#![deny(clippy::arithmetic_side_effects)]

mod sha256;

use core::fmt;

use fo1_endian_host::LittleEndian;

/// Digest of the reference build, checked before any header field is read
/// (EXETBL-001).
///
/// This crate's every constant is true only for this exact executable, so a
/// digest that differs is a hard stop and is never relaxed, defaulted or worked
/// around.
pub const REFERENCE_BUILD_SHA256: &str =
    "444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272";

/// The same digest under the name `docs/spec/exe-tables.md` gives it.
///
/// One value, two names: the spec's interface block calls it
/// `REFERENCE_SHA256` and the gate test calls it `REFERENCE_BUILD_SHA256`. They
/// are the same constant, so there is one definition and this is its other
/// name, not a second value that could drift.
pub const REFERENCE_SHA256: &str = REFERENCE_BUILD_SHA256;

/// `ImageBase` of the reference build, `0x00400000` (EXETBL-C010). `parse`
/// reads the real value out of the optional header; this is the value the spec
/// records, and the one every constant below is expressed against.
pub const IMAGE_BASE: u32 = 0x0040_0000;

/// Virtual address of the hit location penalty table (EXETBL-003).
pub const HIT_LOCATION_PENALTY_VA: u32 = 0x004F_EC90;

/// Virtual address of the critter critical success table (EXETBL-004).
pub const CRIT_SUCCESS_CRITTERS_VA: u32 = 0x004F_ECB4;

/// Virtual address of the player critical success table (EXETBL-005).
pub const CRIT_SUCCESS_PLAYER_VA: u32 = 0x0050_454C;

/// Virtual address of the critical failure table (EXETBL-006).
pub const CRIT_FAILURE_VA: u32 = 0x0050_4B3C;

/// Virtual address of the stat limits table (EXETBL-007).
pub const STAT_LIMITS_VA: u32 = 0x0050_7EC8;

/// Virtual address of the player stat limits table (EXETBL-008).
pub const PLAYER_STAT_LIMITS_VA: u32 = 0x0050_8258;

/// Virtual address of the XP thresholds table (EXETBL-009).
pub const XP_THRESHOLDS_VA: u32 = 0x0050_82D0;

/// Width of one little endian `i32`. Every field in the executable layer is one.
const I32_SIZE: usize = 4;

/// Offset of `e_lfanew` in the DOS header (EXETBL-C011).
const E_LFANEW_OFFSET: usize = 0x3C;

/// The PE signature `0x00004550` that sits at `e_lfanew` (EXETBL-002).
const PE_SIGNATURE: u32 = 0x0000_4550;

/// The PE32 optional header magic (EXETBL-002).
const PE32_MAGIC: u16 = 0x010B;

/// Width of a COFF `u16` field.
const U16_SIZE: usize = 2;

/// Width of a COFF `u32` field.
const U32_SIZE: usize = 4;

/// Bytes of one PE section header.
const SECTION_HEADER_SIZE: usize = 40;

/// Offset of `NumberOfSections` from the COFF header's first byte.
const COFF_SECTION_COUNT_AT: usize = 2;

/// Offset of `SizeOfOptionalHeader` from the COFF header's first byte.
const COFF_OPTIONAL_SIZE_AT: usize = 16;

/// Offset of the optional header from the COFF header's first byte.
const COFF_OPTIONAL_AT: usize = 20;

/// Offset of `ImageBase` from the first byte of a PE32 optional header. In a
/// PE32+ optional header the field moves to offset 24, which is why the magic is
/// refused rather than accommodated (EXETBL-002).
const OPTIONAL_IMAGE_BASE_AT: usize = 28;

/// Value count in the hit location penalty table (EXETBL-C400).
const HIT_LOCATION_VALUE_COUNT: usize = 9;

/// Critter critical record count, the flattening of `15 x 9 x 6`
/// (EXETBL-C404). The three nesting dimensions are not identified, so the
/// reader flattens them and preserves the order.
const CRITTER_CRIT_RECORD_COUNT: usize = 15 * 9 * 6;

/// Player critical record count, the flattening of `9 x 6` (EXETBL-C407).
const PLAYER_CRIT_RECORD_COUNT: usize = 9 * 6;

/// Row count of the critical failure table (EXETBL-C408).
const CRITICAL_FAILURE_GROUPS: usize = 7;

/// Column count of one critical failure row (EXETBL-C409).
const CRITICAL_FAILURE_WIDTH: usize = 5;

/// Record count of the stat limits table (EXETBL-C411).
const STAT_LIMIT_COUNT: usize = 38;

/// Record count of the player stat limits table (EXETBL-C412).
const PLAYER_STAT_LIMIT_COUNT: usize = 5;

/// Value count in the XP thresholds table, levels 0 through 20 (EXETBL-C413).
const XP_THRESHOLD_COUNT: usize = 21;

/// Byte offset one past the last XP threshold entry, which is where a level
/// beyond the table would have started (EXETBL-009).
const XP_THRESHOLDS_END_OFFSET: usize = XP_THRESHOLD_COUNT * I32_SIZE;

/// The factor the guide applies to an XP threshold. The bytes carry unscaled
/// triangular numbers, so the scaling belongs to the caller (EXETBL-009).
const XP_SCALE: i32 = 1_000;

/// Origin label the little-endian cursor carries, so a failure inside the
/// executable layer names the executable rather than some other buffer.
const ORIGIN_EXECUTABLE: &str = "falloutw.exe executable layer";

/// One PE section header.
///
/// `virtual_size` is carried because the field exists in the file, but every
/// section of the reference build declares 0, so containment is decided by
/// `raw_size` (EXETBL-002). Containment additionally requires `raw_pointer != 0`,
/// because `.bss` declares a nonzero `raw_size` beside a zero `raw_pointer` and
/// a size-only test would accept it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectionHeader {
    /// The section's eight byte name, which carries the NUL padding the file
    /// stores. No accessor is offered for it because no rule here depends on a
    /// name; containment is decided entirely by the raw fields.
    pub name: [u8; 8],
    /// The section's address relative to `ImageBase`. `rva - virtual_address` is
    /// the distance of an address into the section.
    pub virtual_address: u32,
    /// `VirtualSize`, kept for the record and deliberately not used. Zero in
    /// every section of the reference build.
    pub virtual_size: u32,
    /// `SizeOfRawData`, the extent containment is decided by.
    pub raw_size: u32,
    /// `PointerToRawData`, which must also be nonzero before a section counts as
    /// file backed.
    pub raw_pointer: u32,
}

/// A failure that never carries table bytes or values.
///
/// Only offsets, counts, field labels and digests appear here, so a failing test
/// can name what was being read without leaking the data it misread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExeError {
    /// The file does not carry a parseable PE header where one was required.
    Header {
        /// File offset the reader was reading when it refused.
        offset: usize,
        /// Static label of the field it objected to, such as `optionalMagic`.
        field: &'static str,
    },
    /// No section holds this virtual address.
    Unmapped {
        /// The address that no section covers.
        va: u32,
    },
    /// The address arithmetic left the representable range.
    RangeOverflow {
        /// The address whose arithmetic overflowed.
        va: u32,
    },
    /// The address maps to a section, but the request does not fit inside the
    /// region that section owns: it runs past the end of the section's raw
    /// bytes, or past the end of the file. Both are one refusal, because a read
    /// that leaves its section must not return the next section's bytes under
    /// this section's address (EXETBL-002).
    OutsideFile {
        /// File offset the refused read started at.
        offset: usize,
        /// Bytes the refused read asked for.
        len: usize,
    },
    /// A record field fell outside the record that was read.
    FieldOutOfRange {
        /// Byte offset at which the refused field would have started.
        offset: usize,
        /// Static label of the field.
        field: &'static str,
    },
    /// A table held a different number of values than its declared shape.
    RecordCount {
        /// Virtual address of the table.
        va: u32,
        /// Record count the table's shape declares.
        expected: usize,
        /// Record count that was actually read.
        observed: usize,
    },
    /// The digest is not the reference digest. Never relaxed (EXETBL-001).
    BuildMismatch {
        /// The digest this crate requires.
        expected_sha256: &'static str,
        /// The digest the supplied bytes actually have.
        observed_sha256: String,
    },
}

impl fmt::Display for ExeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Header { offset, field } => {
                write!(
                    formatter,
                    "bad PE header field {field} at file offset 0x{offset:X}"
                )
            }
            Self::Unmapped { va } => {
                write!(formatter, "virtual address 0x{va:08X} is in no section")
            }
            Self::RangeOverflow { va } => {
                write!(formatter, "address arithmetic overflowed for 0x{va:08X}")
            }
            Self::OutsideFile { offset, len } => {
                write!(
                    formatter,
                    "read of {len} bytes at file offset 0x{offset:X} runs past the end \
                     of its section or of the file"
                )
            }
            Self::FieldOutOfRange { offset, field } => {
                write!(
                    formatter,
                    "field {field} at byte offset 0x{offset:X} is outside the record"
                )
            }
            Self::RecordCount {
                va,
                expected,
                observed,
            } => write!(
                formatter,
                "table at 0x{va:08X} declared {expected} records but {observed} were read"
            ),
            Self::BuildMismatch {
                expected_sha256,
                observed_sha256,
            } => write!(
                formatter,
                "executable digest {observed_sha256} is not the reference build \
                 {expected_sha256}; this build's table constants are unknown and are \
                 not guessed"
            ),
        }
    }
}

impl std::error::Error for ExeError {}

/// A parsed PE image that can translate a virtual address into a byte range.
///
/// The image borrows the caller's bytes, so `read_va` hands back a borrowed
/// slice and no table is ever copied into a long lived structure.
pub struct PeImage<'a> {
    bytes: &'a [u8],
    image_base: u32,
    sections: Vec<SectionHeader>,
}

impl<'a> PeImage<'a> {
    /// Parses enough of the PE headers to translate virtual addresses.
    ///
    /// Performs no digest check, so a caller that depends on the reference
    /// constants must reach this through [`PeImage::require_reference_build`].
    ///
    /// A PE32+ optional header is refused. `ImageBase` moves from offset 28 to
    /// offset 24 there, so accepting one would misread the base and every address
    /// derived from it (EXETBL-002).
    pub fn parse(bytes: &'a [u8]) -> Result<Self, ExeError> {
        let dos = read_u32(bytes, E_LFANEW_OFFSET, "e_lfanew")?;
        let dos_at = usize::try_from(dos).map_err(|_| ExeError::Header {
            offset: E_LFANEW_OFFSET,
            field: "e_lfanew",
        })?;
        let signature = read_u32(bytes, dos_at, "signature")?;
        if signature != PE_SIGNATURE {
            return Err(ExeError::Header {
                offset: dos_at,
                field: "signature",
            });
        }

        let coff = add(dos_at, U32_SIZE)?;
        let section_count = usize::from(read_u16(
            bytes,
            add(coff, COFF_SECTION_COUNT_AT)?,
            "numberOfSections",
        )?);
        let optional_size = usize::from(read_u16(
            bytes,
            add(coff, COFF_OPTIONAL_SIZE_AT)?,
            "sizeOfOptionalHeader",
        )?);
        let optional = add(coff, COFF_OPTIONAL_AT)?;

        if read_u16(bytes, optional, "optionalMagic")? != PE32_MAGIC {
            return Err(ExeError::Header {
                offset: optional,
                field: "optionalMagic",
            });
        }
        let image_base = read_u32(bytes, add(optional, OPTIONAL_IMAGE_BASE_AT)?, "imageBase")?;

        let table = add(optional, optional_size)?;
        let span = section_count
            .checked_mul(SECTION_HEADER_SIZE)
            .ok_or(ExeError::Header {
                offset: table,
                field: "sectionTable",
            })?;

        let mut sections = Vec::new();
        for index in 0..section_count {
            let at = add(
                table,
                index
                    .checked_mul(SECTION_HEADER_SIZE)
                    .ok_or(ExeError::Header {
                        offset: table,
                        field: "sectionTable",
                    })?,
            )?;
            sections.push(read_section_header(bytes, at)?);
        }
        // Touching the computed span keeps the whole table inside the parse, so a
        // truncated section table is refused here rather than mid read.
        let _ = add(table, span)?;

        Ok(Self {
            bytes,
            image_base,
            sections,
        })
    }

    /// Parses and refuses any build whose digest is not the reference digest
    /// (EXETBL-001).
    ///
    /// The digest is checked before the headers and before any constant is used.
    /// A mismatch is a hard stop: the constants in this module are true only for
    /// one exact build, so relaxing them would produce a silently wrong
    /// character instead of a visible failure. A buffer too short to hold any
    /// header at all is therefore refused as a build mismatch, not as a header
    /// complaint, because nothing was read to complain about.
    pub fn require_reference_build(bytes: &'a [u8]) -> Result<Self, ExeError> {
        let observed = sha256_hex(bytes);
        if observed != REFERENCE_BUILD_SHA256 {
            return Err(ExeError::BuildMismatch {
                expected_sha256: REFERENCE_BUILD_SHA256,
                observed_sha256: observed,
            });
        }
        Self::parse(bytes)
    }

    /// Borrows `len` bytes at virtual address `va`.
    ///
    /// Containment is tested against `raw_size`, because every section of the
    /// reference build declares `virtual_size == 0` (EXETBL-002). The read is
    /// then bounded by the containing section's raw extent, before the file is
    /// consulted at all: a length that would run past the section is an error,
    /// not a shorter read and not a read that carries on into the next section's
    /// bytes.
    pub fn read_va(&self, va: u32, len: usize) -> Result<&'a [u8], ExeError> {
        let (header, delta, start) = self.translate(va)?;
        let limit = usize::try_from(header.raw_size).map_err(|_| ExeError::RangeOverflow { va })?;
        let span = delta
            .checked_add(len)
            .ok_or(ExeError::RangeOverflow { va })?;
        if span > limit {
            return Err(ExeError::OutsideFile { offset: start, len });
        }
        let end = start
            .checked_add(len)
            .ok_or(ExeError::RangeOverflow { va })?;
        // A total `get` keeps a short file an error rather than a panic.
        self.bytes
            .get(start..end)
            .ok_or(ExeError::OutsideFile { offset: start, len })
    }

    /// The section header that owns `va`, if any (EXETBL-002).
    ///
    /// A section is file backed only when `raw_pointer != 0` **and**
    /// `raw_size != 0`. `.bss` declares a large nonzero `raw_size` beside a
    /// zero `raw_pointer`, so a size-only test would accept it, map any address
    /// in it to file offset `0 + delta`, and hand back the DOS header `MZ` as
    /// though it were `.bss` data.
    pub fn section_for(&self, va: u32) -> Option<&SectionHeader> {
        let rva = va.checked_sub(self.image_base)?;
        self.sections
            .iter()
            .find(|header| match rva.checked_sub(header.virtual_address) {
                None => false,
                Some(delta) => {
                    header.raw_pointer != 0 && header.raw_size != 0 && delta < header.raw_size
                }
            })
    }

    /// The section that owns `va`, the distance of `va` inside that section, and
    /// the file offset `va` maps to.
    fn translate(&self, va: u32) -> Result<(&SectionHeader, usize, usize), ExeError> {
        let header = self.section_for(va).ok_or(ExeError::Unmapped { va })?;
        let rva = va
            .checked_sub(self.image_base)
            .ok_or(ExeError::RangeOverflow { va })?;
        let delta = rva
            .checked_sub(header.virtual_address)
            .ok_or(ExeError::RangeOverflow { va })?;
        let raw =
            usize::try_from(header.raw_pointer).map_err(|_| ExeError::RangeOverflow { va })?;
        let inside = usize::try_from(delta).map_err(|_| ExeError::RangeOverflow { va })?;
        let start = raw
            .checked_add(inside)
            .ok_or(ExeError::RangeOverflow { va })?;
        Ok((header, inside, start))
    }
}

/// Byte offset of `field0` inside a stat limit record (EXETBL-C924).
const STAT_LIMIT_FIELD0_AT: usize = 0;
/// Byte offset of `field1` inside a stat limit record (EXETBL-C924).
const STAT_LIMIT_FIELD1_AT: usize = 4;
/// Byte offset of `field2` inside a stat limit record (EXETBL-017).
const STAT_LIMIT_FIELD2_AT: usize = 8;
/// Byte offset of the minimum inside a stat limit record (EXETBL-C306).
const STAT_LIMIT_MINIMUM_AT: usize = 12;
/// Byte offset of the maximum inside a stat limit record.
const STAT_LIMIT_MAXIMUM_AT: usize = 16;
/// Byte offset of the default inside a stat limit record.
const STAT_LIMIT_DEFAULT_AT: usize = 20;

/// Byte offset of `multiplier` inside a critical record.
const CRITICAL_MULTIPLIER_AT: usize = 0;
/// Byte offset of `effect_flags` inside a critical record.
const CRITICAL_EFFECT_FLAGS_AT: usize = 4;
/// Byte offset of `massive_critical_stat` inside a critical record (EXETBL-C307).
const CRITICAL_MASSIVE_STAT_AT: usize = 8;
/// Byte offset of `massive_critical_modifier` inside a critical record.
const CRITICAL_MASSIVE_MODIFIER_AT: usize = 12;
/// Byte offset of `massive_critical_flags` inside a critical record.
const CRITICAL_MASSIVE_FLAGS_AT: usize = 16;
/// Byte offset of `message_id_1` inside a critical record (EXETBL-C931).
const CRITICAL_MESSAGE_ID_1_AT: usize = 20;
/// Byte offset of `message_id_2` inside a critical record.
const CRITICAL_MESSAGE_ID_2_AT: usize = 24;

/// One 24-byte stat limit record (EXETBL-007).
///
/// Fields 0, 1 and 2 keep neutral names. Field 0 is `0` in every one of the 38
/// records of the reference build, so it is **not** named `stat_index` and no
/// stat is derived from it; addressing a limit is by record index only. See
/// EXETBL-016 and EXETBL-017.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatLimit {
    field0: i32,
    field1: i32,
    field2: i32,
    minimum: i32,
    maximum: i32,
    default: i32,
}

impl StatLimit {
    /// Bytes one record occupies (EXETBL-C301).
    pub const SIZE: usize = 24;

    /// Fields one record holds (EXETBL-C305).
    pub const FIELD_COUNT: usize = 6;

    /// Byte offset and name of each field, so the layout and the names a refusal
    /// carries cannot drift apart.
    pub const FIELDS: [(usize, &'static str); Self::FIELD_COUNT] = [
        (STAT_LIMIT_FIELD0_AT, "field0"),
        (STAT_LIMIT_FIELD1_AT, "field1"),
        (STAT_LIMIT_FIELD2_AT, "field2"),
        (STAT_LIMIT_MINIMUM_AT, "minimum"),
        (STAT_LIMIT_MAXIMUM_AT, "maximum"),
        (STAT_LIMIT_DEFAULT_AT, "default"),
    ];

    /// The first field, constant `0` in all 38 records and therefore not an
    /// index of anything (EXETBL-016).
    pub fn field0(&self) -> i32 {
        self.field0
    }

    /// The second field, constant `0` in all 38 records (EXETBL-017).
    pub fn field1(&self) -> i32 {
        self.field1
    }

    /// The third field, the only field that varies and the one with no
    /// established meaning (EXETBL-017).
    pub fn field2(&self) -> i32 {
        self.field2
    }

    /// The record's minimum, confirmed at byte 12 by the bytes themselves
    /// (EXETBL-007).
    pub fn minimum(&self) -> i32 {
        self.minimum
    }

    /// The record's maximum, at byte 16 (EXETBL-007).
    pub fn maximum(&self) -> i32 {
        self.maximum
    }

    /// The record's default, at byte 20 (EXETBL-007).
    pub fn default(&self) -> i32 {
        self.default
    }
}

/// One 28-byte critical record (EXETBL-004, EXETBL-005).
///
/// `massive_critical_stat` is `-1` when the record has no massive critical, so
/// it is a sentinel and must not be indexed without a check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CriticalRecord {
    multiplier: i32,
    effect_flags: i32,
    massive_critical_stat: i32,
    massive_critical_modifier: i32,
    massive_critical_flags: i32,
    message_id_1: i32,
    message_id_2: i32,
}

impl CriticalRecord {
    /// Bytes one record occupies (EXETBL-C300).
    pub const SIZE: usize = 28;

    /// Fields one record holds (EXETBL-C304).
    pub const FIELD_COUNT: usize = 7;

    /// Byte offset and name of each field, so the layout and the names a refusal
    /// carries cannot drift apart.
    pub const FIELDS: [(usize, &'static str); Self::FIELD_COUNT] = [
        (CRITICAL_MULTIPLIER_AT, "multiplier"),
        (CRITICAL_EFFECT_FLAGS_AT, "effect_flags"),
        (CRITICAL_MASSIVE_STAT_AT, "massive_critical_stat"),
        (CRITICAL_MASSIVE_MODIFIER_AT, "massive_critical_modifier"),
        (CRITICAL_MASSIVE_FLAGS_AT, "massive_critical_flags"),
        (CRITICAL_MESSAGE_ID_1_AT, "message_id_1"),
        (CRITICAL_MESSAGE_ID_2_AT, "message_id_2"),
    ];

    /// The record's multiplier.
    pub fn multiplier(&self) -> i32 {
        self.multiplier
    }

    /// The record's effect flags.
    pub fn effect_flags(&self) -> i32 {
        self.effect_flags
    }

    /// The massive critical's stat, or the `-1` sentinel when the record has no
    /// massive critical (EXETBL-C307).
    pub fn massive_critical_stat(&self) -> i32 {
        self.massive_critical_stat
    }

    /// The massive critical's modifier.
    pub fn massive_critical_modifier(&self) -> i32 {
        self.massive_critical_modifier
    }

    /// The massive critical's flags.
    pub fn massive_critical_flags(&self) -> i32 {
        self.massive_critical_flags
    }

    /// The record's first message id.
    pub fn message_id_1(&self) -> i32 {
        self.message_id_1
    }

    /// The record's second message id.
    pub fn message_id_2(&self) -> i32 {
        self.message_id_2
    }

    /// `false` when the record has no massive critical, so a caller cannot use
    /// the `-1` sentinel as an index (EXETBL-C307).
    pub fn has_massive_critical(&self) -> bool {
        self.massive_critical_stat >= 0
    }
}

/// The nine hit location penalties, index 0 to 8 (EXETBL-003).
pub fn hit_location_penalties(image: &PeImage<'_>) -> Result<[i32; 9], ExeError> {
    let values = read_i32s(image, HIT_LOCATION_PENALTY_VA, HIT_LOCATION_VALUE_COUNT)?;
    // `try_into` rather than copying into a pre zeroed array, so a short read is
    // an error instead of a default that looks like real data.
    let exact: [i32; HIT_LOCATION_VALUE_COUNT] =
        values
            .try_into()
            .map_err(|short: Vec<i32>| ExeError::RecordCount {
                va: HIT_LOCATION_PENALTY_VA,
                expected: HIT_LOCATION_VALUE_COUNT,
                observed: short.len(),
            })?;
    Ok(exact)
}

/// The 810 critter critical records, flattened from the `15 x 9 x 6` nesting
/// (EXETBL-004).
pub fn critical_success_critters(image: &PeImage<'_>) -> Result<Vec<CriticalRecord>, ExeError> {
    read_criticals(image, CRIT_SUCCESS_CRITTERS_VA, CRITTER_CRIT_RECORD_COUNT)
}

/// The 54 player critical records, flattened from the `9 x 6` nesting
/// (EXETBL-005).
pub fn critical_success_player(image: &PeImage<'_>) -> Result<Vec<CriticalRecord>, ExeError> {
    read_criticals(image, CRIT_SUCCESS_PLAYER_VA, PLAYER_CRIT_RECORD_COUNT)
}

/// The 38 stat limits, addressed by record index (EXETBL-007).
pub fn stat_limits(image: &PeImage<'_>) -> Result<Vec<StatLimit>, ExeError> {
    read_limits(image, STAT_LIMITS_VA, STAT_LIMIT_COUNT)
}

/// The five player stat limits: unspent skill points, level, experience,
/// reputation and karma, in that record order (EXETBL-008).
///
/// The labels are the guide's. The ranges are consistent with that order and the
/// binary carries no label of its own, so record 3 being `-20 .. 20` is
/// evidence for "reputation", not proof of it.
pub fn player_stat_limits(image: &PeImage<'_>) -> Result<Vec<StatLimit>, ExeError> {
    read_limits(image, PLAYER_STAT_LIMITS_VA, PLAYER_STAT_LIMIT_COUNT)
}

/// The critical failure table as 7 rows of 5 (EXETBL-006).
///
/// The layout is fixed at `7 x 5 x i32`. What the five columns mean is not
/// established; the values are large power of two combinations, consistent with
/// flag masks rather than signed penalties.
pub fn critical_failure(image: &PeImage<'_>) -> Result<Vec<Vec<i32>>, ExeError> {
    let mut rows = Vec::new();
    for group in 0..CRITICAL_FAILURE_GROUPS {
        let stride = group
            .checked_mul(CRITICAL_FAILURE_WIDTH)
            .and_then(|width| width.checked_mul(I32_SIZE))
            .ok_or(ExeError::RangeOverflow {
                va: CRIT_FAILURE_VA,
            })?;
        let va = CRIT_FAILURE_VA
            .checked_add(u32::try_from(stride).map_err(|_| ExeError::RangeOverflow {
                va: CRIT_FAILURE_VA,
            })?)
            .ok_or(ExeError::RangeOverflow {
                va: CRIT_FAILURE_VA,
            })?;
        rows.push(read_i32s(image, va, CRITICAL_FAILURE_WIDTH)?);
    }
    Ok(rows)
}

/// The 21 XP thresholds (EXETBL-009, EXETBL-015).
///
/// The array index is the level: element `L` is `L * (L + 1) / 2` for `L` in
/// `0 ..= 20`, and element 0 is level 0's zero threshold. The values are not
/// scaled; the caller multiplies by 1,000. Indexing the table as if it began at
/// level 1 would put a zero threshold at level 1 and shift every level down by
/// one, so no off-by-one correction belongs in the reader.
pub fn xp_thresholds(image: &PeImage<'_>) -> Result<[i32; 21], ExeError> {
    let values = read_i32s(image, XP_THRESHOLDS_VA, XP_THRESHOLD_COUNT)?;
    let exact: [i32; XP_THRESHOLD_COUNT] =
        values
            .try_into()
            .map_err(|short: Vec<i32>| ExeError::RecordCount {
                va: XP_THRESHOLDS_VA,
                expected: XP_THRESHOLD_COUNT,
                observed: short.len(),
            })?;
    Ok(exact)
}

/// Experience needed to leave `level`, which is `thresholds[level]` scaled by
/// 1,000 (EXETBL-009).
///
/// `checked_mul` because the scaled value overflows `i32` for a high enough
/// level and a wrapped total would hand out free levels. A level outside the
/// table is an error rather than a clamped read. A negative level names no
/// entry and no offset, so it is refused as an overflow of the table's address;
/// a level past the end is refused with the byte offset its entry would have
/// started at.
pub fn experience_for_level(
    thresholds: &[i32; XP_THRESHOLD_COUNT],
    level: i32,
) -> Result<i32, ExeError> {
    let index = usize::try_from(level).map_err(|_| ExeError::RangeOverflow {
        va: XP_THRESHOLDS_VA,
    })?;
    let base = thresholds
        .get(index)
        .copied()
        .ok_or(ExeError::FieldOutOfRange {
            offset: XP_THRESHOLDS_END_OFFSET,
            field: "level",
        })?;
    base.checked_mul(XP_SCALE).ok_or(ExeError::RangeOverflow {
        va: XP_THRESHOLDS_VA,
    })
}

/// Lowercase hex SHA-256 of `bytes` (EXETBL-001).
fn sha256_hex(bytes: &[u8]) -> String {
    sha256::hex(&sha256::digest(bytes))
}

/// Reads `count` little endian `i32` values starting at `va`.
fn read_i32s(image: &PeImage<'_>, va: u32, count: usize) -> Result<Vec<i32>, ExeError> {
    let len = count
        .checked_mul(I32_SIZE)
        .ok_or(ExeError::RangeOverflow { va })?;
    let raw = image.read_va(va, len)?;
    let mut out = Vec::new();
    for index in 0..count {
        let at = index
            .checked_mul(I32_SIZE)
            .ok_or(ExeError::RangeOverflow { va })?;
        out.push(read_word(raw, at, "i32")?);
    }
    Ok(out)
}

/// Reads `count` critical records starting at `va`, flattening the nesting.
fn read_criticals(
    image: &PeImage<'_>,
    va: u32,
    count: usize,
) -> Result<Vec<CriticalRecord>, ExeError> {
    let len = count
        .checked_mul(CriticalRecord::SIZE)
        .ok_or(ExeError::RangeOverflow { va })?;
    let raw = image.read_va(va, len)?;
    let mut out = Vec::new();
    for index in 0..count {
        let start = index
            .checked_mul(CriticalRecord::SIZE)
            .ok_or(ExeError::RangeOverflow { va })?;
        out.push(read_critical_record(raw, start)?);
    }
    Ok(out)
}

/// Reads `count` stat limit records starting at `va`.
fn read_limits(image: &PeImage<'_>, va: u32, count: usize) -> Result<Vec<StatLimit>, ExeError> {
    let len = count
        .checked_mul(StatLimit::SIZE)
        .ok_or(ExeError::RangeOverflow { va })?;
    let raw = image.read_va(va, len)?;
    let mut out = Vec::new();
    for index in 0..count {
        let start = index
            .checked_mul(StatLimit::SIZE)
            .ok_or(ExeError::RangeOverflow { va })?;
        out.push(read_stat_limit(raw, start)?);
    }
    Ok(out)
}

/// Reads the little endian `i32` at `offset` inside `raw`.
///
/// The fold goes through [`fo1_endian_host::LittleEndian`], the one reader the
/// executable layer is allowed to use. `raw.get(..)` above has already proved
/// exactly `I32_SIZE` bytes are present, so the cursor cannot run out; the
/// refusal is named with this field's own label so an error names the field the
/// record declares rather than an offset inside a four byte window.
fn read_word(raw: &[u8], offset: usize, field: &'static str) -> Result<i32, ExeError> {
    let end = offset
        .checked_add(I32_SIZE)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?;
    let bytes = raw
        .get(offset..end)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?;
    match LittleEndian::new(bytes, ORIGIN_EXECUTABLE).i32() {
        Ok(value) => Ok(value),
        Err(_) => Err(ExeError::FieldOutOfRange { offset, field }),
    }
}

/// Reads the stat limit record at `start` inside a record buffer.
fn read_stat_limit(raw: &[u8], start: usize) -> Result<StatLimit, ExeError> {
    Ok(StatLimit {
        field0: read_word(raw, add(start, STAT_LIMIT_FIELD0_AT)?, "field0")?,
        field1: read_word(raw, add(start, STAT_LIMIT_FIELD1_AT)?, "field1")?,
        field2: read_word(raw, add(start, STAT_LIMIT_FIELD2_AT)?, "field2")?,
        minimum: read_word(raw, add(start, STAT_LIMIT_MINIMUM_AT)?, "minimum")?,
        maximum: read_word(raw, add(start, STAT_LIMIT_MAXIMUM_AT)?, "maximum")?,
        default: read_word(raw, add(start, STAT_LIMIT_DEFAULT_AT)?, "default")?,
    })
}

/// Reads the critical record at `start` inside a record buffer.
fn read_critical_record(raw: &[u8], start: usize) -> Result<CriticalRecord, ExeError> {
    Ok(CriticalRecord {
        multiplier: read_word(raw, add(start, CRITICAL_MULTIPLIER_AT)?, "multiplier")?,
        effect_flags: read_word(raw, add(start, CRITICAL_EFFECT_FLAGS_AT)?, "effect_flags")?,
        massive_critical_stat: read_word(
            raw,
            add(start, CRITICAL_MASSIVE_STAT_AT)?,
            "massive_critical_stat",
        )?,
        massive_critical_modifier: read_word(
            raw,
            add(start, CRITICAL_MASSIVE_MODIFIER_AT)?,
            "massive_critical_modifier",
        )?,
        massive_critical_flags: read_word(
            raw,
            add(start, CRITICAL_MASSIVE_FLAGS_AT)?,
            "massive_critical_flags",
        )?,
        message_id_1: read_word(raw, add(start, CRITICAL_MESSAGE_ID_1_AT)?, "message_id_1")?,
        message_id_2: read_word(raw, add(start, CRITICAL_MESSAGE_ID_2_AT)?, "message_id_2")?,
    })
}

/// Checked addition, so an offset that leaves the representable range is a
/// refusal rather than a wrapped address.
fn add(left: usize, right: usize) -> Result<usize, ExeError> {
    left.checked_add(right)
        .ok_or(ExeError::RangeOverflow { va: 0 })
}

/// Reads the little endian `u16` at `offset`.
fn read_u16(bytes: &[u8], offset: usize, field: &'static str) -> Result<u16, ExeError> {
    let end = offset
        .checked_add(U16_SIZE)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?;
    let pair: [u8; U16_SIZE] = bytes
        .get(offset..end)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?
        .try_into()
        .map_err(|_| ExeError::FieldOutOfRange { offset, field })?;
    Ok(u16::from_le_bytes(pair))
}

/// Reads the little endian `u32` at `offset`.
fn read_u32(bytes: &[u8], offset: usize, field: &'static str) -> Result<u32, ExeError> {
    let end = offset
        .checked_add(U32_SIZE)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?;
    let quad: [u8; U32_SIZE] = bytes
        .get(offset..end)
        .ok_or(ExeError::FieldOutOfRange { offset, field })?
        .try_into()
        .map_err(|_| ExeError::FieldOutOfRange { offset, field })?;
    Ok(u32::from_le_bytes(quad))
}

/// Reads one section header at `offset`.
fn read_section_header(bytes: &[u8], offset: usize) -> Result<SectionHeader, ExeError> {
    let name: [u8; 8] = bytes
        .get(offset..add(offset, 8)?)
        .ok_or(ExeError::FieldOutOfRange {
            offset,
            field: "sectionName",
        })?
        .try_into()
        .map_err(|_| ExeError::FieldOutOfRange {
            offset,
            field: "sectionName",
        })?;
    Ok(SectionHeader {
        name,
        virtual_size: read_u32(bytes, add(offset, 8)?, "virtualSize")?,
        virtual_address: read_u32(bytes, add(offset, 12)?, "virtualAddress")?,
        raw_size: read_u32(bytes, add(offset, 16)?, "sizeOfRawData")?,
        raw_pointer: read_u32(bytes, add(offset, 20)?, "pointerToRawData")?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        add, critical_failure, experience_for_level, read_u16, sha256_hex, CriticalRecord,
        ExeError, PeImage, StatLimit, E_LFANEW_OFFSET, I32_SIZE, IMAGE_BASE, PE32_MAGIC,
        PE_SIGNATURE, REFERENCE_BUILD_SHA256, REFERENCE_SHA256, SECTION_HEADER_SIZE,
    };

    // --- fixtures -----------------------------------------------------------------

    const SYNTH_PE_OFFSET: usize = 0x80;
    const SYNTH_PE_OFFSET_ADDRESS: u32 = 0x80;
    const SYNTH_OPTIONAL: usize = 0x98;
    const SYNTH_OPTIONAL_SIZE: u16 = 224;
    const SYNTH_SECTION_COUNT: u16 = 2;
    const SYNTH_SECTION_TABLE: usize = 0x178;
    /// The image is deliberately longer than any section's raw data, so a read
    /// that leaves its section still has file bytes behind it. That is what makes
    /// the section bound distinguishable from a file bound.
    const SYNTH_FILE_BYTES: usize = 0x400;

    const SYNTH_DGROUP_RVA: u32 = 0x000F_0000;
    const SYNTH_DGROUP_RAW_POINTER: u32 = 0x200;
    const SYNTH_DGROUP_RAW_SIZE: u32 = 0x100;
    const SYNTH_BSS_RVA: u32 = 0x0014_0000;
    const SYNTH_BSS_RAW_POINTER: u32 = 0;
    const SYNTH_BSS_RAW_SIZE: u32 = 0x0017_2C00;

    const SYNTH_DGROUP_VA: u32 = IMAGE_BASE + SYNTH_DGROUP_RVA;
    const SYNTH_DGROUP_LAST_VA: u32 = SYNTH_DGROUP_VA + SYNTH_DGROUP_RAW_SIZE - 1;
    const SYNTH_BSS_FIRST_VA: u32 = IMAGE_BASE + SYNTH_BSS_RVA;
    const SYNTH_BSS_LAST_VA: u32 = SYNTH_BSS_FIRST_VA + SYNTH_BSS_RAW_SIZE - 1;

    /// Name of an `ExeError` variant, matched exhaustively with no wildcard arm,
    /// so a new variant is a compile error here rather than a name that quietly
    /// stops matching.
    fn error_name(error: &ExeError) -> &'static str {
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

    /// Writes a little endian `u32` into a fixture.
    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        let end = offset.saturating_add(I32_SIZE);
        if let Some(slot) = bytes.get_mut(offset..end) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
    }

    /// Writes a little endian `u16` into a fixture.
    fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
        let end = offset.saturating_add(2);
        if let Some(slot) = bytes.get_mut(offset..end) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
    }

    /// Writes one section header and returns the offset just past it.
    fn put_section(
        bytes: &mut [u8],
        at: usize,
        rva: u32,
        raw_pointer: u32,
        raw_size: u32,
    ) -> usize {
        if let Some(slot) = bytes.get_mut(at..at.saturating_add(8)) {
            slot.copy_from_slice(b"DGROUP\0\0");
        }
        put_u32(bytes, at.saturating_add(8), 0);
        put_u32(bytes, at.saturating_add(12), rva);
        put_u32(bytes, at.saturating_add(16), raw_size);
        put_u32(bytes, at.saturating_add(20), raw_pointer);
        add(at, SECTION_HEADER_SIZE).expect("a synthetic PE offset stays in a usize")
    }

    /// A two section synthetic PE32 image: one ordinary file backed section and
    /// one with a zero `PointerToRawData` beside a large nonzero
    /// `SizeOfRawData`, which is exactly the `.bss` shape of the reference build
    /// (EXETBL-002).
    ///
    /// Nothing here comes from the reference executable, so it supports no
    /// CONFIRMED tag. It exists so the containment rules can be proved without an
    /// install: a reader that decides containment with `virtual_size`, or with
    /// `raw_size` alone, fails here.
    fn synthetic_image() -> Vec<u8> {
        let mut bytes = vec![0_u8; SYNTH_FILE_BYTES];
        if let Some(slot) = bytes.get_mut(..2) {
            slot.copy_from_slice(b"MZ");
        }
        put_u32(&mut bytes, E_LFANEW_OFFSET, SYNTH_PE_OFFSET_ADDRESS);
        put_u32(&mut bytes, SYNTH_PE_OFFSET, PE_SIGNATURE);
        // `NumberOfSections` sits six bytes past the signature and
        // `SizeOfOptionalHeader` twenty, both measured from it, which is the
        // COFF header's own layout.
        put_u16(
            &mut bytes,
            SYNTH_PE_OFFSET.saturating_add(6),
            SYNTH_SECTION_COUNT,
        );
        put_u16(
            &mut bytes,
            SYNTH_PE_OFFSET.saturating_add(20),
            SYNTH_OPTIONAL_SIZE,
        );
        put_u16(&mut bytes, SYNTH_OPTIONAL, PE32_MAGIC);
        put_u32(&mut bytes, SYNTH_OPTIONAL.saturating_add(28), IMAGE_BASE);

        let after_first = put_section(
            &mut bytes,
            SYNTH_SECTION_TABLE,
            SYNTH_DGROUP_RVA,
            SYNTH_DGROUP_RAW_POINTER,
            SYNTH_DGROUP_RAW_SIZE,
        );
        put_section(
            &mut bytes,
            after_first,
            SYNTH_BSS_RVA,
            SYNTH_BSS_RAW_POINTER,
            SYNTH_BSS_RAW_SIZE,
        );
        bytes
    }

    // --- the build gate, EXETBL-001 -----------------------------------------------

    #[test]
    fn the_digest_is_checked_before_any_header_field() {
        // Neither buffer holds a DOS header, let alone a PE signature, so a
        // reader that parsed first would report a header complaint. The spec
        // orders the digest first, so both report a build mismatch.
        for buffer in [vec![0_u8; 64], Vec::new()] {
            let refused = PeImage::require_reference_build(&buffer).err();
            assert_eq!(
                refused.as_ref().map(error_name),
                Some("BuildMismatch"),
                "EXETBL-001: a buffer with no headers at all is refused for its digest"
            );
            let digests = match refused {
                Some(ExeError::BuildMismatch {
                    expected_sha256,
                    observed_sha256,
                }) => Some((expected_sha256, observed_sha256)),
                _ => None,
            };
            let (expected, observed) = digests.expect("the refusal is a build mismatch");
            assert_eq!(
                expected, REFERENCE_BUILD_SHA256,
                "EXETBL-001: the refusal names the required digest"
            );
            assert_eq!(
                observed.len(),
                64,
                "EXETBL-001: and the observed digest is 64 hex characters"
            );
        }
    }

    #[test]
    fn the_two_digest_names_are_one_constant() {
        assert_eq!(
            REFERENCE_SHA256, REFERENCE_BUILD_SHA256,
            "EXETBL-001: the spec's name and the gate's name are one value"
        );
        assert_eq!(
            REFERENCE_BUILD_SHA256,
            "444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272",
            "EXETBL-001: which is the digest measured over the whole reference build"
        );
    }

    #[test]
    fn the_digest_helper_renders_the_published_empty_digest() {
        // The gate hashes a 1.2 MB file; this pins the same helper on the one
        // digest anyone can check by hand.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "EXETBL-001: the digest of an empty message is the published value"
        );
    }

    // --- header refusals, EXETBL-002 ----------------------------------------------

    #[test]
    fn a_buffer_with_no_pe_signature_is_a_header_refusal() {
        // The same buffer the gate refuses as a digest, once the gate is bypassed,
        // produces the header complaint. That is what makes the BuildMismatch
        // above meaningful rather than an accident of length.
        let short = vec![0_u8; 64];
        let refused = PeImage::parse(&short).err();
        assert_eq!(
            refused.as_ref().map(error_name),
            Some("Header"),
            "EXETBL-002: a 64 byte zero buffer is a header refusal"
        );
        let field = match refused.as_ref() {
            Some(ExeError::Header { field, .. }) => Some(*field),
            _ => None,
        };
        assert_eq!(
            field,
            Some("signature"),
            "EXETBL-002: and it names the signature field"
        );
    }

    #[test]
    fn a_pe32_plus_optional_header_is_refused_by_name() {
        let mut bytes = synthetic_image();
        put_u16(&mut bytes, SYNTH_OPTIONAL, 0x020B);
        let refused = PeImage::parse(&bytes).err();
        let refusal = match refused.as_ref() {
            Some(ExeError::Header { field, offset }) => Some((*field, *offset)),
            _ => None,
        };
        assert_eq!(
            refusal,
            Some(("optionalMagic", SYNTH_OPTIONAL)),
            "EXETBL-002: the refusal names the optional header magic and its offset, \
             so the reader says which field it objected to"
        );
    }

    #[test]
    fn the_synthetic_image_parses_and_reports_its_own_header_facts() {
        let bytes = synthetic_image();
        let image = PeImage::parse(&bytes).expect("the synthetic PE fixture parses");
        assert_eq!(
            read_u16(&bytes, SYNTH_OPTIONAL, "optionalMagic")
                .expect("the optional header magic reads"),
            PE32_MAGIC,
            "EXETBL-002: the fixture really is PE32"
        );
        assert!(
            image.section_for(SYNTH_DGROUP_VA).is_some(),
            "EXETBL-002: an address inside the file backed section is owned"
        );
        assert!(
            image.section_for(SYNTH_BSS_FIRST_VA).is_none(),
            "EXETBL-002: an address inside the zero raw pointer section is not"
        );
    }

    // --- the two conditions a file backed section must satisfy, EXETBL-002 ---------

    #[test]
    fn a_section_with_a_zero_raw_pointer_is_never_file_backed() {
        let bytes = synthetic_image();
        let image = PeImage::parse(&bytes).expect("the synthetic PE fixture parses");

        // A size only test accepts this section, maps `file_offset = 0 + delta`,
        // and hands back the DOS header as though it were data. Both ends are
        // checked, so this is not one hand picked address.
        for address in [SYNTH_BSS_FIRST_VA, SYNTH_BSS_LAST_VA] {
            let refused = image.read_va(address, 1).err();
            assert!(
                refused.is_some(),
                "EXETBL-002: a read inside a zero raw pointer section is refused, \
                 not answered with file bytes"
            );
            assert!(
                image.section_for(address).is_none(),
                "EXETBL-002: and no section claims that address"
            );
        }

        // The neighbouring real section still reads, so the refusal above is about
        // the zero raw pointer and not a blanket refusal.
        assert_eq!(
            image
                .read_va(SYNTH_DGROUP_VA, 1)
                .expect("the file backed section reads")
                .len(),
            1,
            "EXETBL-002: the file backed section next to it reads"
        );
    }

    #[test]
    fn a_read_is_bounded_by_its_section_and_not_only_by_the_file() {
        let bytes = synthetic_image();
        let image = PeImage::parse(&bytes).expect("the synthetic PE fixture parses");

        // The last byte of the section's raw extent reads.
        assert_eq!(
            image
                .read_va(SYNTH_DGROUP_LAST_VA, 1)
                .expect("the last byte of the section reads")
                .len(),
            1,
            "EXETBL-002: the last byte of the section reads as one byte"
        );

        // Two bytes from there runs past the section, and the file still holds
        // bytes behind it, so a file only bound would hand those bytes back. This
        // is the read that proves the bound is per section.
        let crossing = image.read_va(SYNTH_DGROUP_LAST_VA, 2).err();
        assert_eq!(
            crossing.as_ref().map(error_name),
            Some("OutsideFile"),
            "EXETBL-002: a two byte read at the section's last byte is refused as \
             leaving its section, even though the file continues"
        );

        // And an address in no section at all is a different refusal.
        let gap = image.read_va(SYNTH_BSS_LAST_VA.saturating_add(1), 1).err();
        assert_eq!(
            gap.as_ref().map(error_name),
            Some("Unmapped"),
            "EXETBL-002: an address past the last section is in no section"
        );
    }

    #[test]
    fn an_address_below_the_image_base_is_in_no_section() {
        let bytes = synthetic_image();
        let image = PeImage::parse(&bytes).expect("the synthetic PE fixture parses");
        let below = image.read_va(IMAGE_BASE.saturating_sub(1), 1).err();
        assert_eq!(
            below.as_ref().map(error_name),
            Some("Unmapped"),
            "EXETBL-002: an address one below the image base is in no section"
        );
    }

    #[test]
    fn reading_a_table_the_image_does_not_contain_is_an_error_not_a_value() {
        let bytes = synthetic_image();
        let image = PeImage::parse(&bytes).expect("the synthetic PE fixture parses");
        assert!(
            critical_failure(&image).is_err(),
            "EXETBL-002: reading a table this image does not contain is an error, \
             not a fabricated value"
        );
    }

    // --- record shape, checked without an install ---------------------------------

    #[test]
    fn record_strides_follow_from_the_field_counts() {
        assert_eq!(
            StatLimit::SIZE,
            StatLimit::FIELD_COUNT.saturating_mul(I32_SIZE),
            "EXETBL-007: six i32 are the 24 bytes one stat limit occupies"
        );
        assert_eq!(
            CriticalRecord::SIZE,
            CriticalRecord::FIELD_COUNT.saturating_mul(I32_SIZE),
            "EXETBL-004: seven i32 are the 28 bytes one critical record occupies"
        );
        assert_eq!(
            StatLimit::FIELDS.len(),
            StatLimit::FIELD_COUNT,
            "EXETBL-007: the field table lists every field exactly once"
        );
        assert_eq!(
            CriticalRecord::FIELDS.len(),
            CriticalRecord::FIELD_COUNT,
            "EXETBL-004: the field table lists every field exactly once"
        );
    }

    #[test]
    fn the_minimum_sits_twelve_bytes_into_a_stat_limit_record() {
        // The byte offsets the spec fixes, written out so a reader that shifted
        // the minimum by one field cannot pass by reading the maximum.
        let offsets = [
            0_usize,
            I32_SIZE,
            I32_SIZE.saturating_mul(2),
            I32_SIZE.saturating_mul(3),
            I32_SIZE.saturating_mul(4),
            I32_SIZE.saturating_mul(5),
        ];
        for (expected, entry) in offsets.iter().zip(StatLimit::FIELDS.iter()) {
            assert_eq!(
                entry.0, *expected,
                "EXETBL-007: the minimum is at byte 12, the maximum at 16 and the \
                 default at 20"
            );
        }
    }

    #[test]
    fn experience_for_level_scales_and_refuses() {
        // The 21 unscaled triangular numbers, which is what the executable's bytes
        // read as (EXETBL-009). Written out rather than computed, so the test
        // cannot agree with a broken fold.
        let thresholds: [i32; 21] = [
            0, 1, 3, 6, 10, 15, 21, 28, 36, 45, 55, 66, 78, 91, 105, 120, 136, 153, 171, 190, 210,
        ];
        assert_eq!(
            experience_for_level(&thresholds, 1),
            Ok(1_000),
            "EXETBL-009: level 1 needs 1 before scaling and 1000 after"
        );
        assert_eq!(
            experience_for_level(&thresholds, 20),
            Ok(210_000),
            "EXETBL-009: level 20 needs 210 before scaling"
        );
        // The index is the level, so element 0 is level 0's zero threshold.
        assert_eq!(
            experience_for_level(&thresholds, 0),
            Ok(0),
            "EXETBL-015: level 0 has a zero threshold, so indexing does not begin at \
             level 1"
        );
        assert!(
            experience_for_level(&thresholds, 21).is_err(),
            "EXETBL-015: a level past the table is an error, not a clamped read"
        );
        assert!(
            experience_for_level(&thresholds, i32::MIN).is_err(),
            "EXETBL-015: a negative level is an error"
        );
        // The scale is checked, so a high enough level cannot wrap into free
        // experience.
        let huge = [i32::MAX; 21];
        assert!(
            experience_for_level(&huge, 20).is_err(),
            "EXETBL-009: the checked multiply surfaces the overflow instead of \
             wrapping it"
        );
    }
}
