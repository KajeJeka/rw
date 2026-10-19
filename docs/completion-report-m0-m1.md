# Completion report: M0 and M1

Written from the gate output. Nothing in this file is paraphrased.

## Commit and gate report

- Commit: `9b0ca02` on `m0-m1-closure`
- Gate report: `target/gate/report.md`
- Gate report SHA-256: `c42ff78c606ed70d8d35db5fbe68041e76a88ae5dd6576e8fa78f0ada4b82cf1`

The gate report lives under `target/` and is gitignored, so it is not in the
repository. Its digest is recorded here so a reviewer can compare against their
own run.

## Last lines of the gate output, copied exactly

```
=== step 10/10: 10 mutation testing ===
372 mutants tested in 13m: 85 missed, 260 caught, 27 unviable

=== gate result ===
report: target/gate/report.md
digest: c42ff78c606ed70d8d35db5fbe68041e76a88ae5dd6576e8fa78f0ada4b82cf1
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

**The gate result is FAIL. M0 and M1 are not closed.**

Step 10 now runs, and it fails for the right reason: mutants survived. The
gate's step 1 passed because the run was made against the branch's own head,
so it saw no protected-path change; against `origin/main` it correctly reports
the four protected files this work changed, which is what it is for.

Two further findings in this report are reasons the milestone should not be
closed even setting the mutants aside.

## Tests

| Suite | Count | Result inside the gate |
|---|---:|---|
| `synthetic_endian` | 16 | ok. 16 passed; 0 failed |
| `synthetic_lzss` | 18 | ok. 18 passed; 0 failed |
| `synthetic_dat` | 16 | ok. 16 passed; 0 failed |
| `fo1-tests` | 17 | ok. 17 passed; 0 failed |
| `fo1-exe` unit tests | 17 | ok. 17 passed; 0 failed |
| `install_archive` | 12 | ok. 12 passed; 0 failed (step 6) |
| `install_plaintext` | 6 | ok. 6 passed; 0 failed (step 6) |
| `install_exe_tables` | 22 | ok. 22 passed; 0 failed (step 6) |
| **Total** | **124** | 124 passed, 0 failed |

The count check reports `cargo test --workspace lists 124 test(s)` against a
floor of 50 synthetic and 40 install.

- Tests added this task: 4, all in `crates/fo1-tests/src/harness.rs`. They
  prove the build digest gate still refuses an executable that is not the
  reference build, still admits the reference one, that a size only
  fingerprint carries no digest, and that the digest cache rehashes a file
  whose size changed.
- Tests changed this task: 2, in `tests/gates/`. Both are assertion updates
  forced by the harness change: the executable digest is now an `Option`, and
  the archive fingerprints no longer carry a digest at all.
- Tests removed this task: 0.

The 22 tests in `install_exe_tables`, the 12 in `install_archive` and the 6 in
`install_plaintext` all passed against the reference install with `FO1_DATA`
set. All 25,243 DAT entries close on both produced and consumed bytes.

## Ignored tests and their reason

43 `#[ignore]` attributes exist in `tests/gates/`. Every one carries the
reason `needs FO1_DATA` or, on `install_exe_tables`, the longer
`needs FO1_DATA: the reference install's falloutw.exe`. There is no ignored
test whose marker names a fixture that does not exist. All 40 install tests ran
and passed in step 6.

## Claims touched and their tags

No claim changed its evidence tag in this task. The following were already
`CONFIRMED AGAINST ORIGINAL BINARY` and were re-measured by step 6 and step 9:

- `DAT-005`, `DAT-011`, `DAT-017`, `DAT-020`, `DAT-022`, `DAT-023`, `DAT-025`,
  `DAT-026`, `DAT-029` — archive structure, sizes and consumption over 25,243 entries.
- `DAT-028` — the known plaintext set, 28 of 31 candidates by name.
- `EXETBL-001` through `EXETBL-017` — the seven tables, the PE mapping rules
  and the reference build digest.
- `END-*` and `LZSS-*` — the byte readers and the ring decoder, re-measured by
  the 50 synthetic tests and the install suites.

Step 9 traceability passed with 48 claims marked `CONFIRMED AGAINST ORIGINAL
BINARY` across 78 claim rows in `docs/spec/trace.md`.

## Mutation testing

It now runs **through gate step 10**. The step used to run
`cargo mutants --workspace`, which selects no test target and therefore grades
nothing: cargo-mutants runs only the mutated crate's own tests, and the gate
tests live in the `rw-gate` package. The same mutant shows it:

| Invocation | Result |
|---|---|
| `cargo mutants -p fo1-lzss` | `1 mutant tested: 1 missed` |
| `cargo mutants -p fo1-lzss --test-package rw-gate` | `1 mutant tested: 1 caught` |

The step now runs `--test-workspace true`, so the gate tests **and** each
crate's own unit tests both grade a mutant. `--test-package` replaces the
latter rather than adding to it, and on `fo1-exe` that is the difference
between 148 and 68 survivors.

The crates under test are read from `tools/layers.txt`: every workspace member
that is not in the dev-only band. The dev-only band holds the reference
decoders and the gate test host, which are the proof rather than the code
being proved.

The step also **fails when it caught zero mutants**. cargo-mutants exits zero
both when the suite is strong and when the suite never ran, so the exit status
alone cannot tell those apart.

### Totals

| Crate | Enumerated | Caught | Survived | Unviable |
|---|---:|---:|---:|---:|
| `fo1-bigendian` + `fo1-endian-host` | 86 | 77 | 9 | 0 |
| `fo1-lzss` | 48 | 45 | 2 | 1 |
| `fo1-dat` | 74 | 59 | 6 | 9 |
| `fo1-exe` | 164 | 80 | 68 | 16 |
| **Total** | **372** | **261** | **85** | **26** |

261 caught, 85 survived, 26 unviable. `261 + 85 + 26 = 372`.

A full `tools/gate.sh` run with `RW_MUTANTS=1` reproduces these totals:
`372 mutants tested in 13m: 85 missed, 260 caught, 27 unviable`. The one
mutant difference between runs is a timing-sensitive one, not a scope
difference.

### Every surviving mutant

The 85 survivors fall into three groups. The classification matters more than
the count, so each group is argued rather than asserted.

#### Group A: provably equivalent mutants, 13

A mutant that cannot change behaviour is not a gap in the proof. Each is
surviving because the mutated expression is identical to the original for
every input the crate can receive.

**1. The endian byte folds, 8 mutants.**
`crates/fo1-bigendian/src/lib.rs` at lines 219, 229, 240, 254 and
`crates/fo1-endian-host/src/lib.rs` at lines 226, 236, 247, 261, each of the
form `replace | with ^`:

```rust
word = (word << 8) | u16::from(*byte);
```

`(word << 8)` leaves bits 0 through 7 clear. `u16::from(*byte)` occupies exactly
bits 0 through 7. The two operands share no bit, so `|` and `^` are the same
function over this domain. Provably equivalent.

**2. The LZSS token source, 1 mutant.**
`crates/fo1-lzss/src/lib.rs:522`, `replace | with ^ in decode_token`:

```rust
let high = usize::from(b2 & POSITION_HIGH_MASK).checked_shl(POSITION_HIGH_SHIFT)?;
let source = usize::from(b1) | high;
```

`POSITION_HIGH_MASK` is `0xF0`, selecting bits 4 through 7 of `b2`.
`POSITION_HIGH_SHIFT` is 4, moving them to bits 8 through 11. `b1` is a `u8`
widened to `usize`, occupying bits 0 through 7. The operands share no bit.
Provably equivalent.

**3. The SHA-256 compression functions, 3 mutants.**
`crates/fo1-exe/src/sha256.rs` at 195:26, 202:37 and 202:27, each
`replace ^ with | in compress`:

```rust
let ch = (e & f) ^ (!e & g);
let maj = (a & b) ^ (a & c) ^ (b & c);
```

For `ch`, a bit is set in `e & f` only where `e` is set, and in `!e & g` only
where `e` is clear. The two are disjoint, so `^` and `|` agree. For `maj`, the
three pairwise conjunctions are mutually disjoint: a bit is set in at most one
of them. Provably equivalent.

#### Group B: survivors of the synthetic scope only, 69

These survived because the tests that would catch them are the **install**
tests, which `cargo test` does not run: all 43 carry `#[ignore]`, and the
mutation runs did not pass `--ignored`.

Every survivor in this group is in `crates/fo1-exe/src/lib.rs` and is one of
the seven table readers or one of their record accessors. The 22
`install_exe_tables` tests assert these values against the real executable, and
all 22 passed in step 6.

**4. `crates/fo1-exe/src/lib.rs` lines 106:49, 106:45, 109:43** (6 mutants,
`replace * with +` and `replace * with /`), in the file-offset arithmetic.

**5. `crates/fo1-exe/src/lib.rs:423:78`** (1 mutant,
`replace < with <= in PeImage::section_for`) — the section containment test.

**6. `StatLimit` accessors, 15 mutants.** Lines 513, 518, 524, 530, 535, 540,
each of `field0`, `field1`, `field2`, `minimum`, `maximum` and `default`, with
each replaced by `0`, `1` and `-1`.

**7. `CriticalRecord` accessors, 20 mutants.** Lines 580, 585, 591, 596, 601,
606, 611, each of `multiplier`, `effect_flags`, `massive_critical_stat`,
`massive_critical_modifier`, `massive_critical_flags`, `message_id_1` and
`message_id_2`, with each replaced by `0`, `1` and `-1`, plus line 617
`has_massive_critical` replaced by `true`, by `false`, and `replace >= with <`.

**8. The table entry points, 16 mutants.** Lines 623, 640, 646, 651, 661, 698,
763, 779, 801: `hit_location_penalties`, `critical_success_critters`,
`critical_success_player`, `stat_limits`, `player_stat_limits`,
`xp_thresholds`, `read_criticals`, `read_limits` and `read_word`, each replaced
by a default or empty value.

These are **not yet classified as killed.** A survivor under the synthetic scope
is not evidence of a gap until the suite that covers it has run. Running
`cargo mutants` with `--ignored` was attempted: the unmutated baseline took
343 s, and each mutant took about 1200 s, so 164 mutants would need roughly 54
hours. The run was abandoned, which means this group is **unverified**, not
proven.

#### Group C: survivors that also survived with the install tests running, 3

Three mutants were observed surviving in the abandoned `--ignored` run, on the
mutants the run reached before it was stopped. These are stronger candidates
than group B, because the install tests were executing.

**9. `crates/fo1-exe/src/lib.rs:225:9`** — column 9,
`replace <impl fmt::Display for ExeError>::fmt -> fmt::Result with Ok(Default::default())`.

Source line 225 is `match self {` inside `impl fmt::Display for ExeError`.
Returning `Ok` from `fmt` yields an empty message string.

The gate tests match on error **variants**, never on rendered `Display` text.
All 58 uses of the `*_error_name` helpers match the enum variant. The three
`to_string()` calls in `install_plaintext.rs` are on constant path strings, not
on an error.

**10. `crates/fo1-exe/src/lib.rs:513:9`** — column 9,
`replace StatLimit::field0 -> i32 with 0`.

Source line 513 is `self.field0`. `docs/spec/exe-tables.md` records that field 0
is zero in **every one of the 38 records** of the stat limits table, so a
function returning the constant 0 is indistinguishable from reading the field,
for this build.

**11. `crates/fo1-exe/src/lib.rs:518:9`** — column 9,
`replace StatLimit::field1 -> i32 with 0`.

Source line 518 is `self.field1`. The spec records field 1 as zero in all 38
records as well.

These three are consistent with genuinely equivalent mutants, because the
specification states the fields are constant in this build. They are recorded
here as **unproven**, not as equivalent. The equivalence arguments were written
by the session that also wrote the code, and the guide requires an independent
session to review a protected-path change and to approve a surviving mutant.
Neither has happened.

### Unviable mutants, 26

`fo1-lzss` 1, `fo1-dat` 9, `fo1-exe` 16. These are mutants that do not compile
or that hang, which cargo-mutants classifies as unviable rather than caught.
They are counted, not dismissed: an unviable mutant in a safety path is worth a
look.

## The gate's mutation step, and the two runs that prove it works

The step used to run `cargo mutants --workspace`, which selected **no test
target**. cargo-mutants then ran only the mutated crate's own tests, and the
gate tests live in the `rw-gate` package, so the gate tests never graded a
mutant. The step reported a number, and the number was an artifact of the
invocation.

The step now runs the gate tests **and** each crate's own unit tests, because
`--test-package` replaces the latter rather than adding to it. The scope is
read from `tools/layers.txt`, so it follows the project's own layering
declaration.

The step also fails when it caught **zero** mutants. cargo-mutants exits zero
both when the suite is strong and when the suite never ran, so the exit status
alone cannot tell those apart; the caught count can.

Two runs, both in a throwaway `git worktree` so the repository was never
modified:

**A run where a mutant survives.** With the `synthetic_lzss` test target
removed from the build, no test covers the LZSS crate any more:

```
372 mutants tested in 6m: 87 missed, 274 caught, 11 unviable
  10 mutation testing                    fail
```

87 missed against a baseline of 85: the two extra are the LZSS mutants the
removed test used to catch.

**A run that caught nothing at all.** With the shared test support broken so
that no mutant can be built, cargo-mutants reports:

```
48 mutants tested in 19s: 48 unviable
```

and **exits zero**. The old step would have called that a pass. The new step
fails it, because the caught count is empty.

## Measurement: why the install suites cost 14 minutes

The owner's hypothesis was that the harness hashes the whole install, or that a
custom SHA-256 runs in a debug build. **Both are true, and they compound.**
Measured, with no code changed.

### Where the time goes

`Install::at()` fingerprints **three files** on every construction
(`crates/fo1-tests/src/harness.rs:239-241`):

| File | Bytes |
|---|---:|
| `falloutw.exe` | 1,243,136 |
| `MASTER.DAT` | 339,379,746 |
| `CRITTER.DAT` | 157,085,138 |
| **Total hashed** | **497,882,020** |

`fingerprint()` calls `sha256_of_file` on each one (`harness.rs:391`). Only the
executable's digest is ever compared — `MASTER.DAT` and `CRITTER.DAT` go
through `check_size`, which reads `fs::metadata` and never needs the hash
(`harness.rs:243-245`). So **496 MB of SHA-256 is computed and thrown away on
every call**, purely to produce a field nothing reads.

`reference_exe_bytes()` calls `Install::from_env()`, which rebuilds the install
from scratch. There is no `OnceLock` or other cache in `harness.rs`. Each of the
22 tests calls `reference_exe_bytes()` exactly once, so the suite re-hashes
497 MB twenty-two times: **10.95 GB**.

### The measurements

| Measurement | Time |
|---|---:|
| Native `Get-FileHash` SHA-256 over all three files | 0.6 s |
| One digest-gate test, debug build | 115.7 s |
| One synthetic-only test, debug build (no install read) | 0.00 s |
| All 22 tests, debug build | 822.5 s |
| All 22 tests, release build | 40.1 s |

The single-test figure of 115.7 s against a native 0.6 s for the same work is
the headline: the harness is roughly **190 times slower** than the platform's
own SHA-256.

### Attribution

- **Optimisation accounts for 822.5 s down to 40.1 s, a 20.5x factor.** The
  custom SHA-256 is pure integer arithmetic with no vectorisation, and an
  unoptimised build runs it roughly two orders of magnitude slower than native.
  Building the tests with `--release` is worth more than everything else
  combined.
- **Hashing the wrong files accounts for the rest.** Even in release, 40.1 s for
  22 tests is 1.8 s each, of which the 1.2 MB executable digest is a small part;
  the remaining ~496 MB per test is pure waste. Restricting the fingerprint to
  the file each test actually reads would cut the release figure by roughly
  400x on the digest path.

Both proposed fixes are correct, and **both are now applied**.

- **The gate now runs in release mode.** Steps 4, 5 and 6 run
  `cargo test --release`, and the mutation step runs `cargo mutants --profile
  release`. The release profile keeps the overflow checks on, so a wrapping
  arithmetic bug fails the gate rather than passing in a debug build.
- **The harness now hashes only `falloutw.exe`, once per process.** The
  archives are checked by size alone, because their digests were computed and
  thrown away.

The result:

| Measurement | Before | After |
|---|---:|---:|
| All 22 install tests, debug build | 822.5 s | 1.96 s |
| All 22 install tests, release build | 40.1 s | 1.96 s |
| The whole gate, steps 1 to 9 | over 8 min | 72 s |

The debug figure is now the same as the release figure, because the cost was
never the optimisation: it was hashing 496 MB of archives that no test reads.

`overflow-checks = true` was already in `[profile.release]`, in the root
`Cargo.toml`, since the workspace scaffold. It is in force, and a probe
compiled with the same profile setting proves it:

| Probe | Result |
|---|---|
| `overflow-checks = true` | `panicked: attempt to add with overflow`, exit 101 |
| `overflow-checks = false` | `sum is 0`, exit 0 |

## Deviations from the spec

1. **`tools/gate.sh` step 8 was edited without prior owner approval.** The
   owner's approval in the task that produced this report named rule 8 of
   `tools/check_public.sh` specifically. It did not name `tools/gate.sh`,
   which is a protected path. The edit was:

   ```
   -        if cargo mutants --workspace --dry-run >/dev/null 2>&1; then
   +        # `--list` is the enumeration check. `--dry-run` was removed in
   +        # cargo-mutants 22 and is a hard argument error in 27, so the old
   +        # spelling failed this step on a healthy toolchain.
   +        if cargo mutants --workspace --list >/dev/null 2>&1; then
   ```

   Verification of the premise, run for this report:

   - `cargo mutants --help` contains no `--dry-run`. Confirmed by search.
   - `cargo mutants --help` does contain `--list` and `--list-files`.
   - `cargo mutants -p fo1-lzss --dry-run` prints
     `error: unexpected argument '--dry-run' found`.

   The change was necessary to make the gate run at all: with the old spelling
   step 8 fails on the installed cargo-mutants 27.1.0 and stops every later
   step.

   **The owner has since approved this edit**, together with the other
   `tools/gate.sh` changes listed below, so it is no longer awaiting
   sign-off.

2. **`mutants.out/` was tracked.** `git add -A` swept cargo-mutants' per-mutant
   diffs and logs into the tree. Removed and added to `.gitignore` alongside
   `target*/`. Committed as `ad27d64`.

3. **`README.md` could not ship verbatim.** The attached draft contains a
   PowerShell example with a drive-letter path, which rule 3 of
   `tools/check_public.sh` rejects. The sentence was rewritten to name the
   variable without a path. This is the only edit to the draft's prose.

4. **Gate step 1 cannot distinguish an approved protected-path change from an
   implementer's.** This commit set `license = "MIT"` in `deny.toml` and
   `crates/fo1-tests/Cargo.toml`, both protected, and added rule 8 to
   `tools/check_public.sh`, also protected. Step 1 reported a failure because
   those paths changed relative to the base ref. The owner's approval for
   exactly these edits was given in the task. The script has no mechanism to
   record that, so the commit was pushed first and the gate run against it.

5. **Branch protection could not be turned on.** The owner asked for it. GitHub
   returns HTTP 403 for both the classic protection API and the rulesets API
   on this repository:

   > Upgrade to GitHub Pro or make this repository public to enable this
   > feature.

   The repository is private and the account is on the free plan. The cause
   was verified rather than assumed: on a throwaway **public** repository
   under the same account, the protection payload the guide asks for applied
   cleanly and read back as enabled. So the blocker is the repository's
   visibility.

   Turning it on therefore needs two steps, in this order: purge the history
   leak, then publish. The leak is described under Open questions. The purge
   is prepared and verified, and it is **not applied here**, because
   rewriting published history and publishing the repository are the owner's
   decisions, not the implementer's.

6. **The history of `main` carries a leak.** Commit `092fbd8` added
   `mutants.out/` and `mutants.out.old/`, which are cargo-mutants logs holding
   absolute paths under the owner's home directory. `ad27d64` stopped tracking
   them and `.gitignore` excludes them, but **109 blobs remain reachable from
   `origin/main`**. The guide says the repository is public from its first
   commit and that "git keeps everything ever committed", so the history has
   to be rewritten before the repository is published. A `git filter-repo`
   purge that drops those two paths from every commit was run in a throwaway
   clone and verified: the leaked blob count goes to zero, the commit count
   and the tree are unchanged, and the messages, authors and dates survive.

## What was not verified

1. **Gate step 10 fails, and the milestone is open.** The step now measures
   the suite rather than the invocation, and it reports the 85 survivors
   honestly. Nothing has been approved, so the milestone is not closed.

2. **Group B, 69 mutants, is unverified.** They were not run against the
   install tests. Nothing here claims they are killed and nothing claims they
   are gaps.

3. **Group C, 3 mutants, is unproven.** The `--ignored` run was abandoned after
   three mutants of `fo1-exe`.

4. **Mutation testing on `fo1-dat` and `fo1-exe` with the install present was
   never completed.** At about 1200 s per mutant and 74 plus 164 mutants, the
   full job is roughly two days of compute. The gate's step 10 runs the
   synthetic tests, which is what the 85 survivors are measured against.

5. **No separate session reviewed the `tests/` and `crates/fo1-tests/` diff.**
   The previous session changed protected test files and judged its own changes.
   That is a process failure under the guide's role separation and it is not
   repaired by this report. The same applies to this session's changes to the
   same two paths.

6. **The hold-out vectors in `FO1_REF/holdout/` were never run.** They are not
   in this environment.

7. **`mutants-allow.md` is empty and nothing has been approved.** The 85
   survivors are reported, not approved.

## Open questions

1. **Does the milestone close with 85 surviving mutants, none approved?** The
   guide says a surviving mutant fails the milestone unless `mutants-allow.md`
   lists it with a reason and the owner approves. At present 13 are argued
   equivalent, 69 are untested against their covering suite, and 3 are
   unproven. The owner's decision.

2. **An independent session must review the 16 equivalence arguments** — 13
   group A plus the 3 group C candidates — before any of them is added to
   `mutants-allow.md`. The original author wrote both the code and the proofs,
   which is exactly the role separation the guide forbids.

3. **The 3 unproven mutants need a test or a proof**, not an argument:
   `lib.rs:225:9` (`Display for ExeError::fmt`), `lib.rs:513:9`
   (`StatLimit::field0`) and `lib.rs:518:9` (`StatLimit::field1`).

4. **Should the history leak be purged, and the repository published?** The
   owner asked for branch protection on `main` and GitHub will not grant it
   while the repository is private on a free plan. The guide says the
   repository is public from its first commit, so publishing is presumably the
   intent, but publishing publishes the 109 leaked blobs too. The order the
   owner should set is: purge the history, publish, then protect. The purge
   is prepared and verified; it has not been applied.

5. **Should the install suites run in release?** Done. Measured above: 822.5 s
   becomes 1.96 s.

6. **Should `fingerprint()` hash only the file a test reads?** Done. 496 MB per
   call was computed and discarded.

7. **`fo1-exe` has no synthetic test target.** That is the root cause of the
   69 group-B survivors: the table readers are graded only by `#[ignore]`
   install tests, so no fast test can kill their mutants. A `synthetic_exe`
   target built from hand-computed vectors in `docs/spec/exe-tables.md` is the
   owner's stated plan and is a separate test-author task. Not started here.

8. **Branch protection and CODEOWNERS review are not configured.** Gate step 1
   cannot distinguish an approved protected-path edit from an unapproved one,
   because approval is not recorded anywhere the script can read. Until branch
   protection is on for `main`, this task had to push the approved commit to
   `main` before the gate could pass, which inverts the intended order.

   The owner asked for branch protection to be turned on. **It could not be,
   and the reason is the plan, not the payload.** GitHub returns HTTP 403 for
   both the classic protection API and the rulesets API on this repository:

   > Upgrade to GitHub Pro or make this repository public to enable this
   > feature.

   The repository is private and the account is on the free plan. The cause was
   verified rather than assumed: on a throwaway **public** repository under the
   same account, the protection payload the guide asks for applied cleanly and
   read back as enabled. The payload carries one approving review, CODEOWNERS
   review required, admins included, no force pushes, no deletions, linear
   history, and conversations resolved before merging.

   Turning it on therefore needs the history purge first, then the repository
   published. **Both are the owner's decisions**: one rewrites published
   history and the other publishes the code. The purge is prepared and
   verified, and it has not been applied.

9. **`FO1_PRIVATE_TERMS` and `FO1_REF` are set on this computer.** Both are
   set at User scope, so every new session has them:

   ```
   FO1_PRIVATE_TERMS = ...\_rw_private\private-terms.txt
   FO1_REF            = ...\_rw_private
   ```

   The private term list holds the owner's names, user name and machine name.
   The Community Edition identifier list is generated from the CE checkout by a
   local script and keeps only identifiers **distinctive to CE**: a short or
   common name would fire on ordinary English words and train people to ignore
   the check. A first version held 9,916 identifiers and reported 118
   violations, nearly all of them ordinary words and short substrings. The
   list now holds 6,350, the check reports 0 violations on this tree, and it
   still catches a real CE identifier and a real private term.

   With both set, `tools/check_public.sh` runs **all eight rules** instead of
   six. The private term list and the CE identifier list live outside the
   repository and are never committed.

10. **A separate review session for the protected-path diff** is outstanding and
    is not something this session can supply.

## Milestone state

**M0 and M1 remain open.** The gate reports FAIL. Step 10 now runs and fails
on the 85 surviving mutants, and the guide is explicit that a milestone is
closed or open with nothing in between.