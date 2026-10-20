# Handoff 2

State at the end of the gate-repair session. Written for the next agent
picking this up cold.

Branch `m0-m1-closure` at `4b22791`, pushed. `origin/main` untouched at
`fb2bef8`. Working tree clean. 64 tracked files. 124 tests, all green.

**PR #2 is open against `main` and carries everything below.** It is not
merged, and it should not be until the owner says so.

> This document supersedes `handoff.md` for the state of the tree. `handoff.md`
> describes the Phase 1 implementation session and is still accurate about the
> formats, the layering and the oracle discipline; it is **wrong** about the
> licence, the test count, the gate's speed and the mutation step.

---

## 1. What this session did

The owner asked for six things. Five were done. One is blocked by GitHub and
needs the owner's decision.

| # | The ask | State |
|---|---|---|
| 1 | `gate.sh` step 8: `--dry-run` → `--list` | done, was already in place |
| 2 | `gate.sh` step 10: select the gate tests, fail on zero caught, show it failing | done |
| 3 | Gate tests and mutation in release mode; `overflow-checks = true`; show an overflow still fails | done |
| 4 | `harness.rs`: hash only `falloutw.exe`, once per process; archives size-only; test the digest gate | done |
| 5 | List every protected file changed, with reasons, in the PR description | done |
| 6 | Set `FO1_PRIVATE_TERMS` and `FO1_REF` on this computer | done |
| 7 | Turn on branch protection for `main` in GitHub | **blocked, see section 6** |

### The two defects worth understanding

**The gate's mutation step graded nothing.** It ran
`cargo mutants --workspace`, which selects **no test target**. cargo-mutants
then runs only the mutated crate's own tests, and the gate tests live in the
`rw-gate` package. Every mutant therefore survived, and the step reported a
failure that said nothing about the suite.

The same mutant, two invocations:

| Invocation | Result |
|---|---|
| `cargo mutants -p fo1-lzss` | `1 mutant tested: 1 missed` |
| `cargo mutants -p fo1-lzss --test-package rw-gate` | `1 mutant tested: 1 caught` |

**`--test-package` replaces the mutated crate's own tests rather than adding
to them.** The first fix used `--test-package rw-gate`, and on `fo1-exe` that
left 148 survivors where there should have been 68, because `fo1-exe`'s 17
unit tests stopped running. `--test-workspace true` runs both:

| Crate | `--test-package rw-gate` | `--test-workspace true` |
|---|---:|---:|
| `fo1-exe` | 148 missed | **68 missed**, 80 caught |
| `fo1-lzss` | 2 missed | 2 missed, 45 caught |
| `fo1-dat` | 6 missed | 6 missed, 59 caught |
| `fo1-endian-host` | 5 missed | 5 missed, 38 caught |
| `fo1-bigendian` | 4 missed | 4 missed, 39 caught |

**The harness hashed 496 MB of archives per test to produce a value nothing
read.** `Install::at()` fingerprinted all three install files on every
construction, and the gate builds one `Install` per test. The archives'
digests were computed and thrown away; only their sizes were ever compared.

The fix hashes `falloutw.exe` — the file whose digest identifies the build —
once per process, and checks the archives by size alone.

| Measurement | Before | After |
|---|---:|---:|
| All 22 install tests, debug build | 822.5 s | **1.96 s** |
| All 22 install tests, release build | 40.1 s | 1.96 s |
| The whole gate, steps 1 to 9 | over 8 min | **72 s** |

The debug figure is now the same as the release figure, because the cost was
never the optimisation. It was hashing files no test reads.

### Three things the lints caught, worth knowing

**`tools/lint_rules.sh` bans `HashMap`, `HashSet`, `SystemTime`,
`std::thread::` and `Instant` in production sources.** The digest cache is a
`Vec` of `(path, digest)` pairs, not a `HashMap`, because the order a hash map
iterates in is not defined. A cached entry is reused only while the file's
**size** is unchanged, because size is the one file fact that needs no clock.

**`tools/lint_tests.sh` bans `unwrap_or` as a substring**, so it also matches
`unwrap_or_else`. The digest cache reports a poisoned lock as a
`HarnessError::InternalInvariant` rather than swallowing the poison.

**The CE identifier rule caught this session's own code.** A helper in
`crates/fo1-tests/src/harness.rs` was named after a Community Edition
function, and the rule flagged it in this repository's own source. It was
renamed to something of our own. The clean-room rule covers this repository's
sources, not only the published ones.

### The private lists had to be narrowed before they were usable

`FO1_REF/ce-identifiers.txt` is generated from the Community Edition checkout
by a local script, and keeps only identifiers **distinctive to CE**. The first
version held 9,916 identifiers and reported 118 violations, nearly all of them
ordinary English words and short substrings — `name_len` firing inside
`name_length`, `d_name` inside `d_names`. A list like that trains people to
ignore the check.

The list now holds 6,350 identifiers, the check reports 0 violations on this
tree, and it still catches a real CE identifier and a real private term
(both verified with negative probes).

---

## 2. How to run things

PowerShell in the repo root. Git Bash for the `tools/*.sh` scripts.

```powershell
# keep build output out of git
$env:CARGO_TARGET_DIR = "target-agent-verify"

# the real install, needed for the 40 install tests
$env:FO1_DATA = "<path to the reference install>"

# the private lists. Both are set at User scope on this machine already, so a
# new session has them; set them here if you use a shell that does not read
# the registry.
$env:FO1_PRIVATE_TREMS = "<private dir>\private-terms.txt"
$env:FO1_REF = "<private dir>"
```

> Do not create a directory named `target/` or `target-verify/` at the repo
> root. A previous agent accidentally committed one; the history had to be
> force-purged. `.gitignore` covers `/target*/` but the mistake is worth not
> repeating.

### The gate

```powershell
# everything but the mutation step: 72 s
& "<path to bash.exe>" -lc "cd '$PWD' && bash tools/gate.sh"

# with the mutation step: about 15 min
$env:RW_MUTANTS = 1
& "<path to bash.exe>" -lc "cd '$PWD' && bash tools/gate.sh"

# against a base ref other than origin/main
bash tools/gate.sh origin/main
```

`tools/gate.sh [base-ref]` runs all ten steps and writes
`target/gate/report.md`. Step 1 checks protected paths against the base ref,
so **against `origin/main` it reports this session's four protected files and
fails.** That is the gate working. To exercise steps 2 to 10, run it against
the branch's own head: `bash tools/gate.sh HEAD`.

### The test suites

```powershell
# the three fast suites, run in CI: 50 tests, no game data
cargo test --release --test synthetic_endian     # 16
cargo test --release --test synthetic_lzss       # 18
cargo test --release --test synthetic_dat        # 16

# the harness and the oracle canaries: 17 tests
cargo test --release -p fo1-tests

# fo1-exe's own unit tests: 17
cargo test --release -p fo1-exe

# the real corpus: 40 tests, --ignored is required
cargo test --release --test install_archive  -- --ignored --test-threads=1   # 12
cargo test --release --test install_plaintext -- --ignored --test-threads=1   #  6
cargo test --release --test install_exe_tables -- --ignored --test-threads=1  # 22
```

The whole workspace is 124 tests. `--test-threads=1` is required for the
install suites: they hold hundreds of megabytes and race otherwise.

The install suites are now **fast** because the harness stopped hashing the
archives. Do not add the digest back.

### Hygiene

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo deny check
cargo doc --workspace --no-deps        # with RUSTDOCFLAGS="-D warnings"

# repo scripts, via Git Bash
& "<path to bash.exe>" -lc "cd '$PWD' && bash tools/check_public.sh"
& "<path to bash.exe>" -lc "cd '$PWD' && bash tools/check_layers.sh"
& "<path to bash.exe>" -lc "cd '$PWD' && bash tools/check_trace.sh"
& "<path to bash.exe>" -lc "cd '$PWD' && bash tools/lint_rules.sh"
& "<path to bash.exe>" -lc "cd '$PWD' && bash tools/lint_tests.sh"
```

`check_public.sh` runs **all eight rules** now that both private lists are
set. It reports `SKIP` for a rule whose input is missing, and a skip is not a
pass.

### Two environment facts

**PowerShell here does not support heredocs.** `git commit -F - <<'MSG'` fails.
Write the message to a file, then `git commit -F <path>`. The same applies to
`gh api --input - <<'EOF'`: write the JSON to a file.

**A detached background job does not survive the tool wrapper.** A `Start-Job`
dies with the shell that made it. To run something long, write a `.cmd` wrapper
that calls Git Bash and redirect its output, then start that with
`Start-Process -WindowStyle Hidden`.

---

## 3. What changed, and why each change was allowed

Four files changed. **All four are protected paths**, and the owner approved
each change for PR #2.

| File | Why |
|---|---|
| `tools/gate.sh` | Step 8's flag fix, step 10's test selection and zero-caught check, release mode for the test and mutation steps. |
| `crates/fo1-tests/src/harness.rs` | Hashes only `falloutw.exe`, caches the digest per process, archives are size-only. The four new tests live here because they test the harness. |
| `tests/gates/install_archive.rs` | Two assertions had to follow the harness change: the archive fingerprints no longer carry a digest, and the executable digest is now an `Option`. |
| `tests/gates/install_exe_tables.rs` | One assertion had to follow the harness change: the executable digest is now an `Option`. |

`docs/completion-report-m0-m1.md` also changed. It is not a protected path.

**The public API of the harness changed**: `Fingerprint::sha256` is now
`Option<String>`. `None` means the file was checked by size and never hashed.
Any new code that reads a fingerprint's digest must handle that.

The new harness tests:

- `the_build_digest_gate_rejects_an_executable_that_is_not_the_reference_build`
- `the_build_digest_gate_admits_the_reference_digest`
- `a_size_only_fingerprint_carries_no_digest`
- `the_digest_cache_rehashes_a_file_whose_size_changed`

The first two are a deliberate pair. A gate that refuses everything passes the
first; a gate that refuses nothing passes the second.

---

## 4. The mutation step, in detail

This is the part most likely to be got wrong next, so the reasoning is here.

```bash
# what the step runs
cargo mutants \
    -p fo1-bigendian -p fo1-endian-host -p fo1-lzss -p fo1-dat -p fo1-exe \
    --test-workspace true \
    --profile release \
    --no-shuffle \
    -j "${RW_MUTANTS_JOBS:-1}" \
    --timeout 1200
```

**`-j 1`, and the gate refuses anything above 1.** The job count is not a
performance setting, it is what makes the survivor list a measurement.
cargo-mutants copies the workspace into a temporary directory for each mutant,
and every parallel worker shares the ONE target directory. cargo's fingerprint
for a path dependency does not include the path, so a worker can be graded
against a binary another worker built from a DIFFERENT mutation, and the list
is wrong in both directions: a real gap looks like a survivor and a real kill
looks like a survivor. Measured: with `-j 2` the five reader crates reported 27
survivors, twelve of them changes to the endian readers that a hand
re-measurement showed are killed by this gate; with `-j 1` the same crates
report 2 for fo1-dat and 3 for the SHA-256 module, and every one of those is a
genuinely equivalent change. `RW_MUTANTS_JOBS` overrides the count, and an
override above 1 is REFUSED rather than honoured.

**The package list is read from `tools/layers.txt`**, not hard-coded. Every
workspace member that is not in the dev-only band is mutated. The dev-only
band holds the reference decoders and the gate test host: those are the proof,
not the code being proved, and mutating them measures the proof rather than
the code. The guide says to run mutation on the readers.

**`--test-workspace true`, not `--test-package rw-gate`.** Both the gate tests
and each crate's own unit tests must grade a mutant. `--test-package` drops
the latter.

**`--profile release`.** The release profile keeps the overflow checks on, so
a wrapping arithmetic bug fails the gate rather than passing in a debug build.
`overflow-checks = true` was already in the root `Cargo.toml`, in
`[profile.release]`, since the workspace scaffold. A probe compiled with the
same setting proves it is in force:

| Probe | Result |
|---|---|
| `overflow-checks = true` | `panicked: attempt to add with overflow`, exit 101 |
| `overflow-checks = false` | `sum is 0`, exit 0 |

**The step fails when it caught zero mutants.** cargo-mutants exits zero both
when the suite is strong and when the suite never ran, so the exit status
alone cannot tell those apart. The caught count can.

### The measurement, and why the scope is right

| Run | Mutants | Caught | Missed |
|---|---:|---:|---:|
| The old step, `--workspace` | 667 | 228 | 388 |
| The new step | **372** | **261** | **85** |

The old step's 667 mutants included the reference decoders, and the gate
tests never graded any of them, so its numbers were an artifact of the
invocation. The new step reproduces the per-crate baseline exactly, which is
the check that the scope is right.

### The two runs that prove the step works

Both in a throwaway `git worktree`, so the repository is never modified.

**A run where a mutant survives.** With the `synthetic_lzss` test target
removed from the build, no test covers the LZSS crate any more:

```
372 mutants tested in 6m: 87 missed, 274 caught, 11 unviable
  10 mutation testing                    fail
```

87 missed against a baseline of 85: the two extra are the LZSS mutants the
removed test used to catch.

**A run that caught nothing at all.** With the shared test support broken so
that no mutant can be built:

```
48 mutants tested in 19s: 48 unviable
```

and **cargo-mutants exits zero**. The old step would have called that a pass.
The new step fails it, because the caught count is empty.

---

## 5. The state of the milestone

**M0 and M1 are open.** The gate reports FAIL, and now it fails for the right
reason: step 10 runs and reports the 85 surviving mutants, rather than being
skipped.

A full run with `RW_MUTANTS=1`:

```
  1 protected paths                      pass
  2 public hygiene                       pass
  3 formatting, lints, layering          pass
  4 workspace tests                      pass
  5 install fingerprint                  pass
  6 ignored tests                        pass
  7 count check                          pass
  8 fail before, pass after              pass
  9 traceability                         pass
  10 mutation testing                    fail
result: FAIL
```

`mutants-allow.md` has no approved survivors. The 85 are reported in
`docs/completion-report-m0-m1.md`, in three groups: 13 argued equivalent, 69
that no fast test can kill, and 3 unproven.

**The 13 equivalence arguments and the 3 unproven mutants were written by the
session that wrote the code.** The guide requires an independent session to
review a protected-path change and to approve a surviving mutant. Neither has
happened. A review session is the next gate on this work.

---

## 6. Blocked: branch protection, and the history leak

**The owner asked for branch protection on `main`. It could not be turned on,
and the reason is the plan, not the payload.** GitHub returns HTTP 403 for
both the classic protection API and the rulesets API on this repository:

> Upgrade to GitHub Pro or make this repository public to enable this feature.

The repository is private and the account is on the free plan. The cause was
verified rather than assumed: on a throwaway **public** repository under the
same account, the protection payload applied cleanly and read back as
enabled. The blocker is the repository's visibility.

The payload is prepared and verified. It carries:

- one approving review, stale reviews dismissed
- CODEOWNERS review required
- admins included
- no force pushes, no deletions
- linear history
- conversations resolved before merging

### Publishing needs the history purged first

Commit `092fbd8` added `mutants.out/` and `mutants.out.old/` — cargo-mutants
build logs that hold absolute paths under the owner's home directory.
`ad27d64` stopped tracking them and `.gitignore` excludes them, but **109
blobs remain reachable from `origin/main`**.

The guide says the repository is public from its first commit, and that "git
keeps everything ever committed, so a later delete does not undo a mistake".
So the history has to be rewritten before the repository is published.

A `git filter-repo` purge was run in a throwaway clone and verified:

- the leaked blob count goes to zero on every branch
- the commit count and the trees are unchanged
- the messages, the authors and the dates survive

**It has not been applied.** Rewriting published history and publishing the
repository are the owner's decisions, and the order matters:

1. purge the history
2. publish the repository
3. apply the protection

Steps 2 and 3 are one action, and step 1 is a force-push to `main`. Both
need the owner.

### Two loose ends from this session

- The throwaway probe repository `rw-protection-probe-1492549452` is
  **archived but not deleted**. Deleting it needs the `delete_repo` scope,
  which `gh auth refresh -h github.com -s delete_repo` obtains through a
  browser. It is harmless and clearly named.
- `git filter-repo` was installed into the machine's Python. It is a
  development tool, not a project dependency.

---

## 7. The tree

64 tracked files. Sizes are in bytes; the gate tests are large because each
assertion carries an inline explanation of why it holds.

```
.
├── RUSTY WASTELAND Engine Implementation Guide, Revision 2.md   110,595
├── README.md                                                       5,450
├── handoff.md                                                     20,150
├── handoff2.md                                               (this file)
├── LICENSE                                                        1,085
│     MIT. The licence was changed from LicenseRef-Proprietary in this
│     series of sessions; see the completion report.
│
├── Cargo.toml                                                     3,409
│     Workspace root. Owns [workspace.lints] (the protected deny table),
│     [profile.release] with overflow-checks = true, the six [[test]]
│     gate targets, and the rw-gate package.
├── Cargo.lock                                                       653
│     Frozen. Zero third-party dependencies, so it holds only the members.
├── clippy.toml                                                      454
│     allow-*-in-tests for the harness. Does NOT reach an integration
│     test file's top-level helpers; see handoff.md section 1.
├── deny.toml                                                      1,433
├── expected_counts.toml                                           1,172
│     Test count floors. Protected; may only rise, with owner approval.
├── mutants-allow.md                                                 680
│     Approved surviving mutants. The file carries the template and the
│     scope notes, and its "Approved survivors" section says "None yet".
│     Nothing is approved.
├── .gitignore / .gitattributes                                   1,100/600
│     Ignores /target*/ — see the warning in section 2.
│
├── src/lib.rs                                                       480
│     rw-gate's library. Carries no code; exists so the root package is
│     a valid cargo member that owns tests/gates/.
│
├── crates/
│   ├── fo1-bigendian/src/lib.rs        11,227   band 0
│   ├── fo1-endian-host/src/lib.rs      11,766   band 0
│   ├── fo1-lzss/src/lib.rs             21,972   band 0
│   ├── fo1-dat/src/lib.rs              30,784   band 1
│   ├── fo1-exe/src/lib.rs              51,788   band 1
│   │     └── sha256.rs                 13,788   private copy
│   └── fo1-tests/                                 band 9, dev only
│       ├── src/canary.rs               18,901   the LZSS canaries
│       ├── src/harness.rs              24,963   the install harness
│       ├── src/ref_dat.rs              17,436   the DAT oracle
│       ├── src/ref_lzss.rs             13,204   the LZSS oracle
│       ├── src/sha256.rs               12,296   the harness SHA-256
│       └── src/lib.rs                   2,460
│
├── tests/gates/                       (protected: the test author's)
│   ├── synthetic_endian.rs             87,601   16 tests
│   ├── synthetic_lzss.rs               84,987   18 tests
│   ├── synthetic_dat.rs                82,377   16 tests
│   ├── install_archive.rs              50,261   12 tests
│   ├── install_plaintext.rs            24,299    6 tests
│   └── install_exe_tables.rs          114,152   22 tests
│
├── tools/                             (protected: the gate tooling)
│   ├── gate.sh                         21,634   the ten-step gate
│   ├── check_public.sh                 14,542   the hygiene check
│   ├── check_layers.sh                  8,993
│   ├── check_trace.sh                   9,148
│   ├── lint_rules.sh                    8,357   determinism lint
│   ├── lint_tests.sh                    9,249   banned-pattern lint
│   └── layers.txt                        605   the band table
│
├── docs/
│   ├── completion-report-m0-m1.md      27,342   the M0/M1 report
│   ├── deps.md                          2,471
│   ├── spec/                           (protected: the spec author's)
│   │   ├── dat.md                      32,301
│   │   ├── endian.md                   38,159
│   │   ├── exe-tables.md               49,098
│   │   ├── lzss.md                     31,003
│   │   ├── trace.md                    24,730
│   │   └── *.constants.toml                        the constants ledgers
│   └── superpowers/plans/
│       └── 2026-10-17-phase1-archive-foundation.md  24,493
│
├── spec/constants.toml                71,609   (protected) the constants ledger
│
└── .github/                           (protected: the repository governance)
    ├── CODEOWNERS                       2,244
    ├── workflows/ci.yml                 3,429
    ├── PULL_REQUEST_TEMPLATE.md         4,037
    ├── dependabot.yml                     520
    └── ISSUE_TEMPLATE/                 10,609
```

### Reading order

1. The guide, sections *Publication rules*, *Verification and anti faking*,
   and *Engineering philosophy*.
2. `docs/spec/trace.md` — the claim ids every gate test cites.
3. `docs/spec/<format>.md` for the crate you are about to touch.
4. `crates/fo1-tests/src/harness.rs` — how the install is resolved.
5. `crates/fo1-tests/src/ref_lzss.rs` and `ref_dat.rs` — the oracles.
6. `tools/gate.sh` — what the gate actually does, in order.

---

## 8. What is not done

- **Branch protection is off.** Blocked on the plan, see section 6.
- **The history of `main` carries a leak.** 109 blobs, see section 6.
- **`mutants-allow.md` has no approved survivors.** The 85 are reported, not
  approved.
- **The 16 equivalence arguments need an independent reviewer.** They were
  written by the session that wrote the code.
- **`synthetic_exe` does not exist.** That is the root cause of the 69
  group-B survivors: the PE table readers are graded only by `#[ignore]`
  install tests, so no fast test can kill their mutants. It is the
  test-author role's task, and the owner has stated the plan: a
  `synthetic_exe` target built from hand-computed vectors in
  `docs/spec/exe-tables.md`. Not started.
- **No separate session reviewed this session's `tests/` and
  `crates/fo1-tests/` diff.** The role separation is real, and this session
  changed both paths under owner approval. A reviewer still has to look.
- **The hold-out vectors in `FO1_REF/holdout/` were never run.** They are not
  in this environment.
- **Phase 2 has not started.** `docs/superpowers/plans/` holds the Phase 1
  plan only.

## 9. Where to start

1. Read the guide's *Verification and anti faking* section. It is the reason
   the fake digest-gate test was worth hunting down, and the reason the
   mutation step's scope mattered.
2. Run the three synthetic suites. They are instant and need no game data.
3. Read `docs/completion-report-m0-m1.md` before touching anything that
   could change a test count, a claim or a mutant.
4. Before committing, run `check_public.sh`. It is the check most likely to
   catch a new mistake, and now that both private lists are set, all eight
   rules run.

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

**This session changed four protected files under explicit owner approval, and
recorded the reason for each in the PR description.** That is the pattern:
approval, then the change, then the disclosure.
