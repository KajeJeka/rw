# Handoff

State at the end of the Phase 1 implementation session. Written for the next
agent picking this up cold.

Last commit: `56e3b82` on `main`. Working tree clean. All 103 tests green.

---

## 1. What this session did

Three subagents were dispatched to write the runtime crates and fix the gate.
Two were cancelled mid-flight. They had left real code on disk, but the tree
did not compile and most of the gate was failing.

**The session was therefore mostly repair.** The shape of it:

1. Verified what actually landed on disk (`git status`, file sizes).
2. Ran `cargo fmt --check` and `cargo clippy --workspace --all-targets
   -- -D warnings`. 60+ errors, all confined to `tests/gates/*` and one
   `#[cfg(test)]` module.
3. Fixed those until clippy was silent workspace-wide.
4. Ran all six gate targets. 49 of 50 synthetic passed; one failed.
5. Ran the 40 install tests against the real corpus. First pass: 5 exe failures.
6. For each failure, read the implementation **and** the test before touching
   anything, then determined which one was wrong. All five were test bugs.
7. Re-ran everything, committed, pushed.

Net: 20 files changed, +15,607 / -137 lines. That line count is misleading —
most of it is the six new gate test files (~11,400 lines of tests for ~2,100
lines of implementation).

### Defects found in the gate, not in the implementation

These are worth understanding, because two of them are exactly the failure mode
this repo's rules exist to prevent.

**The digest-gate test was a fake pass.** In `install_exe_tables.rs`:

```rust
let flipped = *mutated.get_mut(last).expect("...") ^ FOREIGN_BUILD_BIT_MASK;
```

`^` is a value-producing operator, not a compound assignment. The read happened;
the **write never did**. `mutated` stayed byte-identical to the reference build,
so `reference_image(&mutated)` was handed the exact file the gate was supposed
to reject, returned `Ok`, and the test panicked — but for the wrong reason than
anyone would have guessed from reading it. Fixed by adding
`mutated[last] = flipped;` explicitly.

**The oracle contradicted the spec.** `RefLzss` prefilled the whole 4096-byte
ring with `0x20`. LZSS-002 says slots `0..=4077` hold `0x20` and slots
`4078..=4095` hold zero. The oracle is the thing that grades the implementation,
so an oracle that disagrees with the spec is worse than no oracle.

**Four lzss assertions contradicted the spec**, three of them unsatisfiable by
any implementation:

- `(ring_index_of(24) + 4096 - 6) % 4096 == 24` — both operands are test-local
  arithmetic, no decoder consulted. Left side is provably `0`.
- Two assertions expected `[0x00, 0x00, 0x00]` for `token_for(4095, 3)`. Per
  LZSS-011 the source cursor wraps at `RING_SIZE`, so that run reads slots
  `4095, 0, 1` — one zero plus two prefills. Correct answer is
  `[0x00, 0x20, 0x20]`.
- One asserted block B must observe block A's ring bytes. The spec mandates
  every LZSS block starts on a fresh ring, so it can never see them.

**Five more exe-gate test bugs:** two off-by-one adjacency comparisons, one
comparing two different record sizes (`CRITICAL_RECORD_SIZE` 28 vs
`STAT_LIMIT_SIZE` 24), one expecting a refusal for three bytes that sit exactly
inside the section, and one whose case tally said 6 cases / 4 refusals when the
walk drove 7 / 5.

**Ten swallowed errors** in `fo1-exe`'s unit tests (`.ok()`, `unwrap_or`) became
explicit `expect()` calls. The banned-pattern lint caught these; five of them
were `match PeImage::parse(&bytes).ok() { Some(i) => i, None => return }`,
which makes a whole test silently pass if its fixture stops parsing.

### Two structural problems worth knowing about

**`clippy.toml`'s test allowances do not reach integration test files.**
`allow-*-in-tests` covers `#[cfg(test)]` modules and `#[test]` bodies. An
integration test file's top-level helper functions are outside both, so 60
helpers were tripping `panic`/`expect_used`/`indexing_slicing`. The fix
restates the exemptions per gate file with `reason = "..."`, which the
workspace's own `allow_attributes_without_reason = "deny"` requires anyway. The
production lint table is untouched and still fully denied.

**PowerShell here does not support heredocs.** `git commit -F - <<'MSG'` fails.
Write the message to a file first, then `git commit -F <path>`.

---

## 2. How to run things

Everything from a PowerShell prompt in the repo root. Use Git Bash for the
`tools/*.sh` scripts.

```powershell
# keep build output out of git
$env:CARGO_TARGET_DIR = "target-agent-verify"

# the real install, needed for the 40 install tests
$env:FO1_DATA = "<path to the reference install>"
```

> Do not create a directory named `target/` or `target-verify/` at the repo
> root. A previous agent accidentally committed one; the history had to be
> force-purged. `.gitignore` covers `/target*/` but the mistake is worth not
> repeating.

```powershell
# the three fast suites, run in CI
cargo test --test synthetic_endian     # 16
cargo test --test synthetic_lzss       # 18
cargo test --test synthetic_dat        # 16
cargo test -p fo1-tests                # 13  harness + oracle canaries

# the real corpus: slow, and --ignored is required
cargo test --test install_archive  -- --ignored --test-threads=1   # 12, ~8 min
cargo test --test install_plaintext -- --ignored --test-threads=1   #  6, ~3 min
cargo test --test install_exe_tables -- --ignored --test-threads=1  # 22, ~14 min

# hygiene
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo doc --workspace --no-deps        # with RUSTDOCFLAGS="-D warnings"

# repo scripts, via Git Bash (substitute your bash path)
& "<path to bash.exe>" -lc "cd '$PWD' && bash tools/check_public.sh"
```

The install gate takes ~25 minutes total. `--test-threads=1` is required; the
archive tests hold hundreds of megabytes and race otherwise.

`tools/gate.sh [base-ref]` runs all ten steps and writes
`target/gate/report.md`. **It has not been run end to end this session.** It
defaults its base ref to `origin/main`, and step 1 checks protected paths
against it — which means it will now report the protected files this session
changed. That is expected, not a failure to fix. Step 10 needs `cargo mutants`
and will skip without it.

---

## 3. File tree

62 tracked files. Sizes are in bytes; the gate tests are large because each
assertion carries an inline explanation of why it holds.

```
.
├── RUSTY WASTELAND Engine Implementation Guide, Revision 2.md   105,888
├── README.md                                                       7,838
├── handoff.md                                                     13,326
├── LICENSE                                                          883
│     No licence has been chosen. Every manifest carries
│     LicenseRef-Proprietary, SPDX's marker for exactly that.
│
├── Cargo.toml                                                     3,447
│     Workspace root. Owns [workspace.lints] (the protected deny table),
│     the six [[test]] gate targets, and the rw-gate package.
├── Cargo.lock                                                       653
│     Frozen. Zero third-party dependencies, so it holds only the members.
├── clippy.toml                                                      454
│     allow-*-in-tests for the harness. Does NOT reach an integration
│     test file's top-level helpers; see section 1.
├── deny.toml                                                      1,378
├── expected_counts.toml                                           1,172
│     Test count floors. Protected; may only rise, with owner approval.
├── mutants-allow.md                                                 680
│     Approved surviving mutants. Currently empty.
├── .gitignore / .gitattributes                                   929/600
│     Ignores /target*/ — see the warning in section 2.
│
├── src/lib.rs                                                       480
│     rw-gate's library. Carries no code; exists so the root package is
│     a valid cargo member that owns tests/gates/.
│
├── crates/
│   ├── fo1-bigendian/          band 0
│   │   └── src/lib.rs          11,227   bounds-checked BE readers
│   ├── fo1-endian-host/        band 0
│   │   └── src/lib.rs          11,766   bounds-checked LE readers
│   ├── fo1-lzss/               band 0
│   │   └── src/lib.rs          21,972   4096-byte ring, decoder, free fns
│   ├── fo1-dat/                band 1   -> bigendian, lzss
│   │   └── src/lib.rs          30,784   archive container + entry lookup
│   ├── fo1-exe/                band 1   -> endian-host
│   │   ├── src/lib.rs          51,788   PE mapping, read_va, 7 tables
│   │   └── src/sha256.rs       13,788   private SHA-256 for the build
│   │                                  gate. Duplicated from fo1-tests on
│   │                                  purpose: a runtime crate may not
│   │                                  depend on band 9.
│   └── fo1-tests/              band 9   dev-only, no runtime crate may use
│       ├── src/lib.rs           2,460   crate doc + re-exports
│       ├── src/harness.rs      14,419   Install: resolves FO1_DATA,
│       │                                  fingerprints it, hands out slices
│       ├── src/ref_lzss.rs     13,204   oracle class 4: reference LZSS
│       ├── src/ref_dat.rs      17,436   oracle class 4: reference tree walk
│       ├── src/canary.rs       18,901   13 canary + vector tests
│       └── src/sha256.rs       12,296   reference SHA-256
│
├── tests/gates/                the six gate targets; see section 4
│   ├── synthetic_endian.rs     87,601   16 tests, runs in CI
│   ├── synthetic_lzss.rs       84,987   18 tests, runs in CI
│   ├── synthetic_dat.rs        82,377   16 tests, runs in CI
│   ├── install_archive.rs      50,015   12 tests, needs FO1_DATA
│   ├── install_plaintext.rs    24,299    6 tests, needs FO1_DATA
│   └── install_exe_tables.rs  114,144   22 tests, needs FO1_DATA
│
├── tools/
│   ├── gate.sh                 17,238   the 10-step pre-push gate
│   ├── check_public.sh         12,888   no absolute paths in tracked files
│   ├── check_trace.sh           9,148   claim -> test mapping
│   ├── check_layers.sh          8,993   reads tools/layers.txt
│   ├── lint_tests.sh            9,249   banned patterns in tests
│   ├── lint_rules.sh            8,357   banned patterns in production
│   └── layers.txt                 605   the banding contract
│
├── docs/
│   ├── deps.md                  2,471
│   ├── spec/
│   │   ├── endian.md           38,159   + endian.constants.toml
│   │   ├── lzss.md             31,003   + lzss.constants.toml
│   │   ├── dat.md              32,301   + dat.constants.toml
│   │   ├── exe-tables.md       49,098   + exe-tables.constants.toml
│   │   └── trace.md            24,730   claim -> test map
│   └── superpowers/plans/
│       └── 2026-10-17-phase1-archive-foundation.md   24,493
│
├── spec/
│   └── constants.toml          71,609   merged 251-row constant ledger,
│                                  the union of the four .constants.toml
│                                  fragments. Single source of truth.
│
└── .github/
    ├── CODEOWNERS                2,244
    ├── dependabot.yml              520
    ├── workflows/ci.yml          3,429
    ├── PULL_REQUEST_TEMPLATE.md  4,037
    └── ISSUE_TEMPLATE/
        ├── bug_report.yml        5,324
        └── feature_request.yml   5,285
```

### Reading order

If you are new to the repo, in this order:

1. `RUSTY WASTELAND Engine Implementation Guide...md` — the authority. It is
   103 KB, so search it rather than reading it straight through.
2. `docs/spec/<crate>.md` — the contract for the crate you are about to touch.
   Every claim id the gate tests cite is defined here.
3. `tools/layers.txt` — 18 lines, tells you what may depend on what.
4. The matching `tests/gates/*.rs` — what the crate has to satisfy.

### Where things are duplicated, and why

Two duplications look like mistakes and are not:

- **SHA-256 exists twice** (`crates/fo1-exe/src/sha256.rs` and
  `crates/fo1-tests/src/sha256.rs`, ~13 KB each). `fo1-tests` is band 9 and no
  runtime crate may depend on it. Sharing the code would mean letting a
  production crate depend on its own test oracle.
- **LZSS is implemented twice** (`crates/fo1-lzss/` and
  `crates/fo1-tests/src/ref_lzss.rs`). Same reason: the reference decoder is
  oracle class 4, written independently from the spec. It exists to disagree
  with the implementation if the implementation is wrong.

---

## 4. How the codebase works

### What is being built

Reverse-engineering Fallout 1's archive and data-table formats, in Rust, from
the binary alone. The source of truth is
`RUSTY WASTELAND Engine Implementation Guide, Revision 2.md` (103 KB).

Phase 1 (M0+M1) is now implemented: the endian readers, the LZSS decompressor,
the DAT archive container, and the seven data tables hosted in the executable.

### Layering

`tools/layers.txt` is the contract: **a crate may depend only on crates in a
strictly lower band.** `check_layers.sh` reads `cargo metadata` and fails the
gate otherwise.

```
band 0   fo1-bigendian     fo1-endian-host    fo1-lzss     (no deps)
band 1   fo1-dat  -> bigendian + lzss        fo1-exe -> endian-host
band 9   fo1-tests, rw-gate                 (dev-only, no runtime crate may use)
```

Band 9 is the important one. `fo1-tests` holds the independent reference
decoders (`RefLzss`, `RefDatTree`) that grade the implementations. If a runtime
crate could depend on them, the oracle would be grading itself. The layering
check is what makes that impossible.

### The oracle discipline

Every gate test carries two doc attributes:

```rust
/// claim: LZSS-011, oracle: class 5 (hand-computed vector from the spec)
```

Six oracle classes, in increasing strength:

| Class | Meaning |
|-------|---------|
| 1 | assertion against a stated constant |
| 2 | algebraic identity |
| 3 | known plaintext — ground truth no decoder produced |
| 4 | independent reference implementation |
| 5 | hand-computed vector |
| 6 | algebraic law (totality, partition) |

`docs/spec/trace.md` maps 78 claim rows to the tests that cover them.
`tools/check_trace.sh` verifies the mapping.

### The six gate targets

They live in `tests/gates/` and are declared as explicit `[[test]]` targets in
the root `Cargo.toml` — Cargo does not auto-discover subdirectories. Counts are
floored in `expected_counts.toml` (protected; may only rise, only with owner
approval).

| Target | Count | Needs `FO1_DATA` |
|--------|------:|------------------|
| `synthetic_endian` | 16 | no |
| `synthetic_lzss` | 18 | no |
| `synthetic_dat` | 16 | no |
| `install_archive` | 12 | yes |
| `install_plaintext` | 6 | yes |
| `install_exe_tables` | 22 | yes |

Install tests are `#[ignore = "needs FO1_DATA"]` so the default run stays fast
and CI stays away from game data. A missing install **fails loudly**; it never
skips silently.

### The reference install

`FO1_DATA` points at a private directory, outside the repo:

- `falloutw.exe` — SHA-256 `444257e5…32272`, 1,243,136 bytes (a multiple of 64)
- `MASTER.DAT` — 339,379,746 bytes, 65 directories, 19,784 entries
- `CRITTER.DAT` — 157,085,138 bytes, 1 directory, 5,459 entries
- 25,243 entries total, 67,663 blocks

`check_public.sh` fails the build if any tracked file contains an absolute
path, a drive letter, a home directory, or a user download folder. This is why
the path above is written as `<path to the reference install>` — never commit
the real one. The rule matches backslashes, so a path in a doc must use forward
slashes even on Windows.

### The formats, briefly

**LZSS.** A 4096-byte ring, prefill `0x20` over slots `0..=4077`, zeros over
`4078..=4095`, write head starting at 4078. The source cursor wraps at 4096.
Every block starts on a fresh ring. A block header's bit 15 distinguishes raw
from LZSS.

**DAT.** A 16-byte root header, then one length-prefixed directory name and one
16-byte block header per directory, then that directory's entries. Three rulings
here are not in the guide and were established from the bytes:

- A block header's field 0 is the **entry count for that directory**, not the
  directory count.
- Field 2 is the width of the entry's four-`u32` trailer — **16 bytes** — not
  the entry record size, which varies with the name length.
- Path composition keys on the directory name being `"."`, not on being the
  first block. `CRITTER.DAT`'s only block is named `ART\CRITTERS` and all 5,459
  of its paths keep that prefix.

**PE tables.** Every section in the reference build declares
`VirtualSize == 0`, so the textbook mapping rule rejects all seven table
addresses; containment must use the raw fields. A section is file-backed only
when `PointerToRawData != 0` **and** `SizeOfRawData != 0` — `.bss` has a large
nonzero raw size beside a zero pointer, and a size-only test would hand back
the DOS header `"MZ"` as table data. `read_va` bounds to the containing section,
not merely the file.

### Lint posture

The root `Cargo.toml` denies, for the runtime crates: `unsafe_code`,
`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, the three cast
lints, `todo`, `unimplemented`, `dbg_macro`, `print_stdout`, `print_stderr`,
`missing_docs`, and `allow_attributes_without_reason`. Every offset
computation must be `checked_*`, `wrapping_*` or saturating.

Two scripts back this up, and they are **not** redundant with clippy:

- `tools/lint_rules.sh` — banned patterns in production code (13 files)
- `tools/lint_tests.sh` — banned patterns in tests (16 files), including
  swallowed errors, which would otherwise hide a fixture that stopped parsing

Zero third-party dependencies. `Cargo.lock` is frozen.

---

## 5. What is not done

- **Mutation testing never ran.** `cargo-mutants` re-runs the suite per mutant;
  at ~25 minutes per install-gate run this is not feasible in one session.
  `mutants-allow.md` is empty, so nothing is approved and nothing is known.
  Scoping it to the fast synthetic targets would be tractable if wanted.
- **`tools/gate.sh` has not run end to end.** Its step 1 diffs protected paths
  against `origin/main`, which now includes this session's changes.
- **Phase 2 has not started.** `docs/superpowers/plans/` holds the Phase 1 plan
  only.
- **Licence is undecided.** `LICENSE` and every manifest carry
  `LicenseRef-Proprietary` — SPDX's marker for "not chosen yet". This satisfies
  `cargo deny` honestly; it is an open owner decision.

## 6. Where to start

1. Read the guide's *Verification and anti faking* section. It is the reason
   the fake digest-gate test was worth hunting down.
2. Read `docs/spec/*.md` for the crate you are about to touch. They carry the
   claim ids the gate tests reference.
3. Run the three synthetic suites first — they are instant and need no data.
4. Before committing, run `check_public.sh`. It is the check most likely to
   catch a new mistake.

### Protected paths

Do not edit without owner approval:
`tests/`, `crates/fo1-tests/`, `docs/spec/`, `spec/`, `tools/`, `.github/`,
`clippy.toml`, `deny.toml`, the root `Cargo.toml` lint table,
`expected_counts.toml`, `mutants-allow.md`.

The role separation is real: implementers do not write the tests that grade
them, and when an implementer believes a protected file is wrong they stop and
report the evidence. That is not a formality — it is how the four bad lzss
assertions and the oracle bug were caught, and it is the reason the fake digest
test got found rather than shipped.