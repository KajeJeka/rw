# Phase 1 (M0+M1) — Archive Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: `dispatching-parallel-agents` for
> waves 1-3, `subagent-driven-development` inside each wave. Steps use checkbox
> (`- [ ]`) syntax for tracking.

**Goal:** All 25,243 DAT entries decode to their declared uncompressed size while
consuming exactly their compressed size; the 28 named known-plaintext pairs match
byte for byte; the executable tables read little-endian and satisfy three
adjacency identities — all behind a gate that cannot be faked.

**Architecture:** A Rust workspace of six crates in three dependency bands. Two
leaf reader crates carry the two byte-order layers separately so a wrong-endian
read cannot happen silently. One codec crate decodes LZSS. Two format crates
read the DAT container and the PE-hosted tables. One dev-only crate is the
`FO1_DATA` gate harness. The root package owns `tests/gates/` so the gate tests
are a protected path outside any subsystem.

**Tech Stack:** Rust 1.96 stable (edition 2021), zero third-party dependencies,
`clippy`/`rustfmt`, `cargo-deny`, `cargo-mutants`, Git bash for `tools/*.sh`.

**Spec:** `RUSTY WASTELAND Engine Implementation Guide, Revision 2.md` at the
repository root. That guide is the only bridge to the Community Edition: the
clean-room rule is that the spec is the only thing an implementer reads.

## Global Constraints

These apply to every task below.

- `unsafe` is forbidden in every crate (`unsafe_code = "forbid"`).
- The two byte-order readers have **no default order** and no conversion between them.
- Every reader returns `Result`. No panic, no `expect`, no indexing past the end.
- Rules arithmetic is `i32`. Where the original wraps, call `wrapping_*`; otherwise
  use checked operations and keep `overflow-checks = true` in release.
- No `as` to change width or sign. Division truncates toward zero; `div_euclid`
  only where a spec asks for floor division.
- No external dependency without a line in `docs/deps.md` and a `cargo deny` pass.
- **Protected paths** — a change needs the owner's approval: `tests/`,
  `crates/fo1-tests/`, `docs/spec/`, `spec/`, `tools/`, `.github/`, `clippy.toml`,
  `deny.toml`, the lint table in the root `Cargo.toml`, `expected_counts.toml`,
  `mutants-allow.md`.
- Never commit game data, Community Edition source, or a private path.
- Never run the game, a patcher or an installer.

## Environment Facts (verified, do not re-derive)

| Fact | Value |
| --- | --- |
| Reference install | `../_rw_private/analysis/target`, 169 files, 597,072,586 bytes |
| Executable SHA-256 | `444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272` |
| MASTER.DAT | 339,379,746 bytes |
| CRITTER.DAT | 157,085,138 bytes |
| Private references | `../_rw_private/analysis`, `../_rw_private/vendor` — outside the repo |
| Toolchain | rustc 1.96.0 stable MSVC; no nightly; no `cargo-deny`/`cargo-mutants` yet |
| Shell | Git for Windows' bundled `bash.exe`, invoked by absolute path from the tools scripts (not on `PATH`) |
| Remote | `KajeJeka/rw`, branch `phase/1-archive-foundation` -> PR into `main` |

---

## Wave 0 — Contracts and Scaffold (orchestrator only, sequential)

Done before any agent is dispatched. Every later wave treats these as frozen.

- [x] Move `analysis/` and `vendor/` to `../_rw_private/`, `git init` at the project dir
- [x] `.gitignore` (game extensions, `target/`, `analysis/`, `vendor/`) and `.gitattributes`
- [x] Root `Cargo.toml`: workspace, the guide's lint table verbatim, `rw-gate` root package
- [x] `clippy.toml`, `deny.toml`, `expected_counts.toml`, `mutants-allow.md`
- [x] Six crate manifests + stub `lib.rs` each carrying
      `#![deny(clippy::arithmetic_side_effects)]` and `[lints] workspace = true`
- [x] `tools/layers.txt` — the band map `check_layers.sh` reads
- [x] `spec/constants.toml` and `docs/spec/trace.md` seeds
- [x] `cargo generate-lockfile`; `cargo fmt --check`, `cargo clippy -D warnings`,
      `RUSTDOCFLAGS="-D warnings" cargo doc` all exit 0

**Ruling.** Edition is 2021, not 2024. Nothing in the design needs 2024, and
2021 is what agents write by default. Recorded so it is not re-litigated.

**Ruling.** `license-file` is deliberately absent from every manifest. `publish =
false` means cargo never needs it, and declaring a licence nobody has chosen is
worse than declaring none. The licence stays an open owner decision.

**Ruling.** `arithmetic_side_effects` is applied to all five runtime crates, not
only the four the guide names. The guide applies it to "the rules and data
crates"; every Phase 1 runtime crate is a data crate.

**Ruling.** `fo1-tests` exposes only fallible functions. Gate tests unwrap inside
the test, where `clippy.toml` permits it. That keeps the harness fully lint-clean
while still failing loudly when `FO1_DATA` is unset.

---

## Wave 1 — Six parallel agents, disjoint files

All six are blocked on nothing but the frozen Wave 0 output.

| Agent | Owns exclusively | Role |
| --- | --- | --- |
| S1 | `docs/spec/endian.md`, `docs/spec/endian.constants.toml` | spec author |
| S2 | `docs/spec/lzss.md`, `docs/spec/lzss.constants.toml` | spec author |
| S3 | `docs/spec/dat.md`, `docs/spec/dat.constants.toml` | spec author |
| S4 | `docs/spec/exe-tables.md`, `docs/spec/exe-tables.constants.toml` | spec author |
| S5 | `crates/fo1-tests/**`, `tools/**`, `docs/deps.md` | harness + gate tooling |
| S6 | `.github/**`, `README.md`, `LICENSE`, `.gitattributes` | CI + docs |

### Clean-room rule for every agent in every wave

Do not open `../_rw_private/vendor/` or any `09_ce_*.md` / `0*_*.md` analysis
report. The permitted sources are the implementation guide, the bytes of
`../_rw_private/analysis/target/falloutw.exe`, and the bytes of the archives.
Those facts get written down; the Community Edition's expression does not.

### S1 — endian spec

Produce claims `END-001..`. Two reader types, no default order, no conversion
between them. Each error variant carries the byte offset, the field name and the
origin label, never the bytes. Give the interface the other waves implement:

```rust
impl<'a> BigEndian<'a> {          // crates/fo1-bigendian
    pub fn new(bytes: &'a [u8], origin: &'static str) -> Self;
    pub fn u8(&mut self)   -> Result<u8,   BigEndianError>;
    pub fn u16(&mut self)  -> Result<u16,  BigEndianError>;
    pub fn u32(&mut self)  -> Result<u32,  BigEndianError>;
    pub fn i16(&mut self)  -> Result<i16,  BigEndianError>;
    pub fn i32(&mut self)  -> Result<i32,  BigEndianError>;
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], BigEndianError>;
    pub fn skip(&mut self, n: usize)   -> Result<(), BigEndianError>;
    pub fn remaining(&self) -> usize;
    pub fn offset(&self)    -> usize;
    pub fn seek(&mut self, off: usize) -> Result<(), BigEndianError>;
}
```

`fo1-endian-host` gets the identical shape named `LittleEndian` / `LittleEndianError`,
deliberately duplicated rather than shared: a wrong endian read fails silently,
so the two layers must not share a code path.

### S2 — LZSS spec

Produce claims `LZSS-001..`. Ring 4,096 bytes; indices 0..=4077 prefilled `0x20`;
the rest zero; write index starts at **4078**. Control byte bits run 0 to 7. A set
bit is a literal. A clear bit is two bytes, `b1` then `b2`; source index
`b1 | ((b2 & 0xF0) << 4)`; length `(b2 & 0x0F) + 3`. Output position `p` lives at
ring index `(p + 4078) mod 4096`. Stop when the block input is used up, even
mid-control-byte. Never truncate. A back reference that starts with one input
byte left is an error.

Pin the hand-computed vectors: token `4E 36` at output position 5000; stream
`00 00 0F` decodes to 18 spaces; token `75 3F` at write index 886 repeats the
previous byte 18 times.

Interface:

```rust
pub const RING_SIZE: usize;            // 4096
pub const RING_FILL: u8;               // 0x20
pub const RING_WRITE_START: usize;     // 4078
pub const MIN_MATCH: usize;            // 3
pub const MAX_MATCH: usize;            // 18
pub struct LzssDecoder;
impl LzssDecoder { pub fn new() -> Self; pub fn reset(&mut self); }
pub fn decode_into(input: &[u8], declared_out: usize) -> Result<Vec<u8>, LzssError>;
pub fn decode_block(input: &[u8], decoder: &mut LzssDecoder,
                    declared_out: usize) -> Result<(), LzssError>;
```

### S3 — DAT spec

Produce claims `DAT-001..`. Sixteen-byte root header, then one length-prefixed
directory name per directory, then per directory a sixteen-byte block header and
its entries. An entry is a length byte, the name, then four `u32`: flags,
absolute offset, uncompressed size, compressed size. Payload mode is
`flags & 0xF0`, where `flags == 0` means `0x10`. `0x20` is stored bytes, `0x40` a
block chain. Every raw entry has a compressed size of 0 and every chain entry a
nonzero one. Every LZSS block inside a chain starts with a fresh ring.

Interface:

```rust
pub struct DatArchive;
impl DatArchive {
    pub fn parse(bytes: &[u8], origin: &'static str) -> Result<Self, DatError>;
    pub fn directories(&self) -> &[DirectoryRecord];
    pub fn entries(&self) -> &[EntryRecord];
    pub fn find(&self, upper_path: &str) -> Option<&EntryRecord>;
}
pub struct EntryRecord { pub path: String, pub flags: u32, pub offset: u32,
                         pub uncompressed_size: u32, pub compressed_size: u32 }
pub enum PayloadMode { LzssStream, Stored, BlockChain }
pub fn payload_mode(flags: u32) -> PayloadMode;
pub fn decode_entry(entry: &EntryRecord, bytes: &[u8]) -> Result<Vec<u8>, DatError>;
pub struct ChainStats { pub blocks: u32, pub lzss_blocks: u32, pub raw_blocks: u32 }
```

**Known gap the spec author must close.** The guide says a block "has the same
16 byte header with the size field set to 16" and never says what field 0 means
once the block header replaces the root header. Measured against the real bytes:
MASTER.DAT root field 0 is 65 (directory count); MASTER.DAT block 0 field 0 is 11
with eleven entries; CRITTER.DAT block 0 field 0 is 5459, which is exactly the
CRITTER.DAT entry count. So field 0 changes meaning from directory count to entry
count, field 2 is the entry record size (16), and fields 1 and 3 are a capacity
and a dead pointer to be ignored, exactly as at the root. The spec must state this
and prove it with the all-entries closure, not by assertion.

### S4 — executable tables spec

Produce claims `EXETBL-001..`. Lookup by virtual address through the PE section
headers. Refuse any build whose SHA-256 is not the reference. Every `i32` is
little-endian.

| Table | Address | Layout |
| --- | --- | --- |
| Hit location penalty | `0x4FEC90` | 9 x `i32` |
| Critical success, critters | `0x4FECB4` | 15 x 9 x 6 records of 28 bytes |
| Critical success, player | `0x50454C` | 9 x 6 records of 28 bytes |
| Critical failure | `0x504B3C` | 7 x 5 x `i32` |
| Stat limits | `0x507EC8` | 38 records of 24 bytes |
| Player stat limits | `0x508258` | 5 records of 24 bytes |
| XP thresholds | `0x5082D0` | 21 x `i32` |

Three adjacency identities, each a class 1 oracle:
`0x4FECB4 + 22,680 == 0x50454C`, `0x507EC8 + 912 == 0x508258`,
`0x508258 + 120 == 0x5082D0`.

Interface:

```rust
pub struct PeImage;
impl PeImage {
    pub fn parse(bytes: &[u8]) -> Result<Self, ExeError>;
    pub fn require_reference_build(bytes: &[u8]) -> Result<Self, ExeError>;
    pub fn read_va(&self, va: u32, len: usize) -> Result<&[u8], ExeError>;
}
pub fn xp_thresholds(image: &PeImage) -> Result<[i32; 21], ExeError>;
pub fn stat_limits(image: &PeImage) -> Result<Vec<StatLimit>, ExeError>;
pub fn player_stat_limits(image: &PeImage) -> Result<Vec<StatLimit>, ExeError>;
pub fn hit_location_penalties(image: &PeImage) -> Result<[i32; 9], ExeError>;
pub fn critical_success_critters(image: &PeImage) -> Result<Vec<CriticalRecord>, ExeError>;
pub fn critical_success_player(image: &PeImage) -> Result<Vec<CriticalRecord>, ExeError>;
pub fn critical_failure(image: &PeImage) -> Result<Vec<Vec<i32>>, ExeError>;
```

### S5 — harness and gate tooling

`crates/fo1-tests` resolves `FO1_DATA`, fingerprints the install against the
reference, and hands out byte slices. Every function is fallible. It contains the
test-author's **independent** reference decoders — never imported by production
code. It never spawns a process and never runs the game.

`tools/` scripts, all executable and all runnable from `tools/gate.sh`:

| Script | Fails on |
| --- | --- |
| `check_public.sh` | a game extension, a file over 256 KB, a drive-letter / `/Users/` / `/home/` / `Downloads` path, a term from `$FO1_PRIVATE_TERMS`, a Community Edition identifier from `$FO1_REF`, an integer literal array over 64 elements, a tracked file `.gitignore` should have excluded |
| `check_layers.sh` | a crate depending on a crate in the same or a higher band, or a crate missing from `tools/layers.txt` |
| `lint_rules.sh` | `SystemTime`, `Instant`, `thread::spawn`, `HashMap`, `HashSet` in a rules crate |
| `lint_tests.sh` | `assert!(true)`, an assertion-less test, a non-equality comparison on a ledger count, `#[ignore]` without a reason, `continue` or early `return` in a loop over cases, `.ok()`, `unwrap_or`, `unwrap_or_default`, `if let Ok`, `catch_unwind`, `Command::new` in a test, a read of `FO1_DATA` outside the harness; and in production: `cfg(test)` outside a test module, a read of `FO1_DATA`/`FO1_REF`, an unapproved `#[allow(`, `std::process::exit` in a library |
| `check_trace.sh` | a test with no claim id, a `CONFIRMED` claim with no class 1/2/3 test, or a drop in the number of tests for a claim |
| `gate.sh` | runs the ten steps in the guide's order, stops at the first failure, writes `target/gate/report.md` ending in its own SHA-256 |

Failure messages print offsets, lengths and hashes. They never print more than
16 bytes of file content.

### S6 — CI and docs

`.github/workflows/ci.yml` runs only the synthetic steps and never holds game
data. `.github/CODEOWNERS` marks every protected path. Issue and pull-request
templates tell contributors never to attach game files and to confirm they copied
no Community Edition source. `README.md` states the project is unofficial, has no
link to the rights holders, and needs the player's own copy. `LICENSE` records
that the licence is an open owner decision rather than picking one.

---

## Wave 2 — Six parallel gate-test authors

Gate tests are written from the specs and their vectors, never from code. They
are expected to fail now: a compile error counts as a failure.

| Agent | Owns | Proves |
| --- | --- | --- |
| T1 | `tests/gates/synthetic_endian.rs` | both readers, no default order, truncation errors, endian bit-flip |
| T2 | `tests/gates/synthetic_lzss.rs` | guide vectors, overlap replication, three write-index canaries, mid-pair end error |
| T3 | `tests/gates/synthetic_dat.rs` | tree walk, payload-mode table, chain headers summing exactly, one-byte-short and one-byte-long streams |
| T4 | `tests/gates/install_archive.rs`, `install_plaintext.rs` | 25,243 entries `produced == declared && consumed == compressed`; 4,787 raw / 20,456 chain / 67,663 blocks; **31 named candidates, 28 exact, 3 first differing at bytes 117 / 14,552 / 3,385**; three named size exclusions `GABRIEL.INT`, `KILLIAN.INT`, `RAZOR.MSG` |
| T5 | `tests/gates/install_exe_tables.rs` | three adjacency identities; XP level 1 is **1**, not 16,777,216; stat 16 minimum is **0**, not -60 |
| T6 | `spec/constants.toml`, `docs/spec/trace.md`, `expected_counts.toml` | merges the four `.constants.toml`; claim-to-test map with an oracle class for every `CONFIRMED` claim |

T2's canaries are the load-bearing ones. The reference decoder in the harness
takes its ring parameters as arguments; the canary runs it with a write index of
4095 and with a `0x00` prefill and asserts the known-plaintext pairs stop
matching. If a wrong variant also passes, the suite is too weak.

T4 must compare **sets by name**. A loop over cases has no `continue`, no early
`return` and no `if let Ok`. The test lists all 31 candidate names, so a missing
case fails.

---

## Wave 3 — Four parallel implementers

| Agent | Owns exclusively |
| --- | --- |
| I1 | `crates/fo1-bigendian/src/**`, `crates/fo1-endian-host/src/**` |
| I2 | `crates/fo1-lzss/src/**` |
| I3 | `crates/fo1-dat/src/**` |
| I4 | `crates/fo1-exe/src/**` |

Hard rules for all four: do not edit any `Cargo.toml`, any gate test, any
protected path, or another crate's source. If a new dependency or manifest field
seems necessary, report `BLOCKED` instead of editing. Each sets its own
`CARGO_TARGET_DIR` so four concurrent builds never share a target directory.
Because the workspace has zero third-party dependencies, `Cargo.lock` is written
once in Wave 0 and never changes, so concurrent cargo runs cannot contend on it.

---

## Wave 4 — Adversarial reviewer

One agent, in a scratch worktree **outside** the repository, sees only the specs
and the diff. Its job is to write plausible wrong implementations — `SWAP` as a
no-op, the XP table read big-endian, a decoder that truncates to the declared
size, an error clamped instead of returned — and check the suite rejects each
one. A surviving mutant means the suite is too weak and the task returns to the
owning **test author**, never to the implementer.

## Wave 5 — Integration and gate

`tools/gate.sh` in full with `FO1_DATA` set, `cargo deny check`, mutation testing
on `fo1-lzss` / `fo1-dat` / `fo1-exe`, the completion report the guide requires,
then a pull request into `main`.

---

## Agent Handoff Protocol

Applies to every dispatch in every wave.

1. **Assignment.** The orchestrator gives each agent its objective, the files it
   owns, the dependencies it may rely on, the interface contract, and its
   acceptance criteria. A task with overlapping file ownership is not dispatched.
2. **Completion report.** Before handing off, each agent reports:
   - `COMPLETE`, `BLOCKED`, or `FAILED`;
   - files created or modified;
   - interfaces, contracts and decisions established;
   - tests and commands executed, with actual output;
   - remaining issues, assumptions and unresolved dependencies.
3. **Receiver validation.** The orchestrator inspects the deliverables on disk and
   checks them against the agreed contract. A reported success is not accepted as
   evidence.
4. **Dependency control.** A dependent task starts only after the required handoff
   is accepted. Independent tasks run concurrently.
5. **Role separation.** Specification authors, gate-test authors, implementers and
   the reviewer are separate sessions with separate contexts. An implementer never
   edits a gate test or a protected path.
6. **Integration.** The orchestrator integrates accepted changes, resolves
   conflicts, runs the checks, and records verified results. A failed validation
   returns the task to its owner.