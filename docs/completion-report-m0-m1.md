# Completion report: M0 and M1a

This report closes milestone 0 and milestone 1a of RUSTY WASTELAND. Milestone
1b, which is `fo1-path`, `fo1-assets` and the gate where 4,306 prototypes parse
with zero trailing bytes, is NOT closed here. Neither is anything from
milestone 2 to milestone 8.

## Commit and gate report

- The commit the gate measured: `94755be8fc7cddd10b2813507e8495393a63a594`
- The tree the gate measured: `a06c9573a35d41a484c91601f9b9b4351920f24f`
- The closure commit on `m0-m1-closure` that carries that exact tree:
  `2b785a1` - the tree hash above is the same in both, so the run measured
  the commit that landed.
- The gate report: `target/gate/report.md`, written by the run below, with the
  SHA-256 `c6b536a60996c8b3cbdd27ae2dcf02064345eb315cae290b3779e94dfbda92b2`.
- The gate was run with the mutation step enabled.

Two runs are quoted below, because the gate stops at the first failed step, as
the guide says it must.

- `bash tools/gate.sh origin/main` fails at step 1, by design: the step lists
  every PROTECTED path that changed since the base ref. That list is the
  approved set, and it is reproduced in full further down.
- `RW_MUTANTS=1 bash tools/gate.sh HEAD` runs all ten steps, and all ten pass.
  Step 1 has nothing to compare, because the base ref is the head itself.

The step that measures the change against the base - step 8, fail before and
pass after - therefore has no diff to work with in the run above, and reports
that no test changed. It was exercised against `origin/main` while the gate
was being fixed, where it reported that `tests/gates/synthetic_exe.rs`,
`tests/gates/install_archive.rs` and `tests/gates/install_exe_tables.rs` fail
on the base and that the four remaining changed files pass on the base and
carry a note in `docs/fail-before-notes.md`.

## The gate output, copied exactly

The last lines of the run, copied from the console:

```
=== gate result ===
report: target/gate/report.md
digest: c6b536a60996c8b3cbdd27ae2dcf02064345eb315cae290b3779e94dfbda92b2
  1 protected paths                      pass
  2 public hygiene                       pass
  3 formatting, lints, layering          pass
  4 workspace tests                      pass
  5 install fingerprint                  pass
  6 ignored tests                        pass
  7 count check                          pass
  8 fail before, pass after              pass
  9 traceability                         pass
  10 mutation testing                    pass
result: PASS
```

The step table in the gate's own report, copied from
`target/gate/report.md`:

```
| 1 | 1 protected paths | pass | no protected path changed since HEAD |
| 2 | 2 public hygiene | pass | ok |
| 3 | 3 formatting, lints, layering | pass | ok |
| 4 | 4 workspace tests | pass | ok |
| 5 | 5 install fingerprint | pass | FO1_DATA is set and the fingerprint matched |
| 6 | 6 ignored tests | pass | ok |
| 7 | 7 count check | pass | every floor in expected_counts.toml is met |
| 8 | 8 fail before, pass after | pass | no gate test changed since HEAD |
| 9 | 9 traceability | pass | ok |
| 10 | 10 mutation testing | pass | 336 of 374 mutants caught, 12 survivor(s) approved in mutants-allow.md |
```

The run against the base ref, whose step 1 lists the protected paths, failed
there and stopped, as the gate does:

```
=== step 1/10: 1 protected paths ===
Cargo.toml
crates/fo1-tests/src/harness.rs
docs/spec/dat.constants.toml
docs/spec/endian.md
docs/spec/exe-tables.constants.toml
docs/spec/exe-tables.md
docs/spec/lzss.md
docs/spec/trace.md
expected_counts.toml
mutants-allow.md
spec/constants.toml
tests/gates/install_archive.rs
tests/gates/install_exe_tables.rs
tests/gates/install_plaintext.rs
tests/gates/synthetic_dat.rs
tests/gates/synthetic_endian.rs
tests/gates/synthetic_exe.rs
tests/gates/synthetic_lzss.rs
tools/check_layers.sh
tools/gate.sh
FAIL these paths belong to another role or to the owner
```

## Every protected path that changed, and why

Twenty protected paths changed. Every one of them is either approved by the
owner directly or is a change a reviewer marked as a required fix, which the
owner approved in advance.

| Protected path | Why it changed |
|---|---|
| `Cargo.toml` | The `[[test]]` entry for the new gate file `tests/gates/synthetic_exe.rs`. Approved by the owner. |
| `crates/fo1-tests/src/harness.rs` | The digest cache is gone. It was keyed on the file's size, so a file whose content changed without changing size was admitted with a stale digest, and `check_sha256` then passed a wrong executable. Reviewer finding, approved as a required fix. |
| `docs/spec/dat.constants.toml` | `DAT-C110` said 39256 where `docs/spec/dat.md` says 39038. The ledger now matches the spec. Reviewer finding, approved as a required fix. |
| `docs/spec/endian.md` | A new "## Error text" section fixes the exact wording of every endian error, so the tests that render those errors have a real oracle instead of the implementation's own wording. Reviewer finding, approved as a required fix. |
| `docs/spec/exe-tables.constants.toml` | The `EXETBL-C004` row held the digest's middle 64 bits and said they were the low 64. The value is now the low 64 bits, and the note that claimed two rows pin all 32 bytes is gone. Reviewer finding, approved as a required fix. |
| `docs/spec/exe-tables.md` | The Rust interface block did not compile: it wrote a `u32` virtual address into a `usize` offset field, and contradicted the field's own doc comment. The block now compiles, the offset is a byte offset, and the "## Error text" heading matches the two sibling specs. Reviewer finding, approved as a required fix. |
| `docs/spec/lzss.md` | The same "## Error text" section for the LZSS errors. Reviewer finding, approved as a required fix. |
| `docs/spec/trace.md` | The claim map named 117 of the gate tests; it now names all 137, and its counts and its open-gap table match the files. Reviewer finding, approved as a required fix. |
| `expected_counts.toml` | The count floors, raised to the measured counts. Approved by the owner. |
| `mutants-allow.md` | The twelve surviving mutants that two independent reviewers proved equivalent, each with the reason. Approved by the owner. |
| `spec/constants.toml` | The same `DAT-C110` correction as the DAT ledger. Reviewer finding, approved as a required fix. |
| `tests/gates/install_archive.rs` | The DAT-021 corpus test, the exact census counts, the three named size exclusions and the four claim ids that were missing. Reviewer findings, approved as required fixes. |
| `tests/gates/install_exe_tables.rs` | The error payloads and the exact error messages, the exact refusal tallies, the section-crossing refusals that now name their error, and the digest-first test that no longer needs an install. Reviewer findings, approved as required fixes. |
| `tests/gates/install_plaintext.rs` | The four claim ids that were missing. Reviewer finding, approved as a required fix. |
| `tests/gates/synthetic_dat.rs` | The class 4 oracle is now real, the two-block LZSS chain fixture, the DAT-021 stored-region bound, the error text test and the chain-block boundary test. Reviewer findings, approved as required fixes. |
| `tests/gates/synthetic_endian.rs` | The error texts, the reads that were discarded, and one test per inherent integer reader of both layers. Reviewer findings, approved as required fixes. |
| `tests/gates/synthetic_exe.rs` | NEW. The whole gate for the executable layer: the section map, the seven tables, the three adjacency spacings, the truncations, the error texts and the payloads. Approved by the owner. |
| `tests/gates/synthetic_lzss.rs` | The error texts, the decoder-driven sweeps that replaced two circular ones, and the vacuous counters. Reviewer findings, approved as required fixes. |
| `tools/check_layers.sh` | The dev-only band is read from `tools/layers.txt` instead of being hardcoded in two places. Reviewer finding, approved as a required fix. |
| `tools/gate.sh` | Step 7 reads every floor in `expected_counts.toml`, step 8 really implements fail-before and pass-after, step 10 parses `mutants-allow.md`, the mutation build is per mutant, the jobs are serial, and the gate refuses to run on a dirty tree. Reviewer findings, approved as required fixes. |

Three files that are NOT protected also changed: `docs/fail-before-notes.md` is
new and carries the notes the gate's step 8 asks for, `handoff2.md` has the
mutation invocation corrected, and `README.md` has the status table.

## Tests

Added: 47 gate tests, 90 before this closure and 137 after it. Thirty-one of
them are the new gate for the executable layer, which did not exist.

| File | Before | After |
|---|---|---|
| `tests/gates/synthetic_endian.rs` | 16 | 22 |
| `tests/gates/synthetic_lzss.rs` | 18 | 18 |
| `tests/gates/synthetic_dat.rs` | 16 | 23 |
| `tests/gates/synthetic_exe.rs` | 0 (the file did not exist) | 31 |
| `tests/gates/install_archive.rs` | 12 | 14 |
| `tests/gates/install_plaintext.rs` | 6 | 6 |
| `tests/gates/install_exe_tables.rs` | 22 | 23 |

Changed: every test file above. The changes are stronger assertions, the error
texts, the real class 4 cross-walk, the two-block chain fixture and the new
readers' fold tests.

Removed: zero. No test was deleted, renamed or ignored that was not ignored
before. One test was rewritten in place: the harness test
`the_digest_cache_rehashes_a_file_whose_size_changed` became
`the_executable_digest_is_read_from_the_file_every_time`, because the cache it
tested is gone. It now proves the opposite: a file whose content changes with
its size unchanged produces a different digest, and the digest gate refuses it.

The workspace holds 137 gate tests, 17 harness tests and 17 unit tests in
`fo1-exe`.

## Ignored tests and their reason

- `tests/gates/install_archive.rs` - 14 tests, all `#[ignore = "needs FO1_DATA"]`.
  They walk the real MASTER.DAT and CRITTER.DAT of the player's copy.
- `tests/gates/install_plaintext.rs` - 6 tests, all `#[ignore = "needs FO1_DATA"]`.
  They compare the player's loose files with their archive entries.
- `tests/gates/install_exe_tables.rs` - 22 of its 23 tests are
  `#[ignore = "needs FO1_DATA: the reference install's falloutw.exe"]`. The one
  that is not ignored needs no install: it refuses two short buffers before it
  touches a header field, and it now runs in the default test run.

The gate runs the ignored tests with `--ignored` at step 6 and the count check
at step 7 covers them.

## Claims touched and their tags

No claim was re-tagged. No evidence tag changed. The following claims gained
a witness they did not have, and the claim map in `docs/spec/trace.md` now
names every one of the 137 gate tests.

- `DAT-021` - the on-disk extent of an entry is bounded by its compressed size.
  Now proved by a corpus test: 25,243 entries, 0 violations of the compressed
  bound, and exactly 7 named entries where the uncompressed bound runs past the
  end, by 15228, 39038, 508, 389, 785, 7475 and 2172 bytes.
- `DAT-018`, `DAT-024`, `DAT-030`, `LZSS-014`, `LZSS-015` - the chain block
  boundary, the error surface and the two LZSS refusals.
- `DAT-001` - the header layout, now with a witness for the position the
  reader reports when it runs out of input.
- `DAT-002`, `DAT-003` - the directory list.
- `END-014` to `END-022` - each inherent integer reader of both layers.
- `EXETBL-001`, `EXETBL-002` - the crate's own digest against the published
  FIPS 180-4 vectors, and the address that sits exactly at a section's end.
- `EXETBL-001` to `EXETBL-015` - every one of the executable layer's claims
  now has a fast synthetic witness, so the fo1-exe crate's mutants are graded
  without the install.

## Mutation testing

### The scope and the measurement

The gate mutates the five reader crates: `fo1-bigendian`, `fo1-endian-host`,
`fo1-lzss`, `fo1-dat` and `fo1-exe`. The dev-only band in `tools/layers.txt`
holds the reference decoders and the gate test host, which are the proof
rather than the code being proved.

Two defects in the measurement were found and fixed before the result was
believed, and both of them are worth knowing:

1. **The build was shared.** cargo-mutants copies the workspace for each
   mutant, and a shared target directory lets a worker be graded against a
   binary another worker built from a DIFFERENT mutation. The first sign was
   a mutated big-endian `u16` reader reported as caught by a test that never
   calls `u16`. The second sign was a mutant of the big-endian reader reported
   as caught by four DAT tests whose failures were all LZSS errors. The gate
   now lets cargo-mutants build inside the copy it makes for each mutant, so
   no artifact is ever shared.
2. **The jobs were parallel.** Even with a per-mutant build, parallel workers
   shared one target directory. The gate now runs the step with one job and
   REFUSES to run it with more, with a message that says why.

The measurement is also only sound when the tests can see the mutation. The
mutation step grades the fast synthetic scope, not the install suites, which
would add about an hour per run. That is a deliberate decision and it is
recorded in the gate's own output.

### The result

```
374 mutants tested, 336 caught, 12 survived, 26 could not be built
```

The 26 that could not be built are the changes that return
`Default::default()` from a function whose return type does not implement
`Default`. They cannot be built, so they cannot survive, and they are not
gaps.

All twelve that survived are equivalent, and all twelve are admitted in
`mutants-allow.md` with the reason and the evidence. Nothing is left
unadmitted, so the gate's step 10 passes.

### The accounting

| Stage | Survivors | What it means |
|---|---|---|
| The report this session replaced | 85 | Measured with the shared build and the parallel jobs. Wrong in both directions: real gaps looked like survivors and real kills looked like survivors. |
| Re-measured, serial, still shared build | 27 | The serial jobs alone do not fix the shared build. |
| Re-measured, serial, per-mutant build, before the last three tests | 15 | The sound number for the tree as it stood. Three of those fifteen were real gaps. |
| After the three new tests and the twelve admissions | 12, all admitted | The three gaps are caught by the new tests and the twelve equivalent ones are admitted with their proofs. Nothing is left unadmitted. |

The twelve admitted mutants are all one shape: a change of `|` to `^` in a
bit-disjoint fold, or a change of `^` to `|` in a SHA-256 step whose two
operands can never both be 1.

- The eight endian folds: the accumulator's low byte is zero and the byte
  being folded in has a zero high byte, so the two operands never share a bit.
  Swept over all 65,536 byte pairs and all 4,294,967,296 byte quads.
- The three SHA-256 steps: `ch` computes `(e & f) ^ (!e & g)`, whose operands
  need e's bit set and clear, and `maj` computes the majority per bit, which
  the change regroups into but does not alter. Swept over all 2^24 byte
  triples, over more than twenty billion word triples, and over the real
  digest of the whole corpus.
- The LZSS back reference: `b1 | ((b2 & 0xF0) << 4)` puts `b1` in bits 0 to 7
  and the shifted nibble in bits 8 to 11. Swept over all 65,536 byte pairs.

Every proof was made twice: once by reasoning and an exhaustive sweep, and
once by a second reviewer who re-derived the argument and ran an independent
sweep. Every sweep carried a control that is NOT equivalent, and every control
fired. The probes were built at `-C opt-level=0`, because at `-O3` the
compiler proves the identity and merges the two functions, and the sweep then
measures nothing.

The three gaps that were closed:

- The `Display` of the DAT error type was never rendered, so the change that
  makes it print nothing survived. A test now renders every variant and
  asserts the message names what went wrong.
- The guard that decides when a decoding run has produced too much output was
  never exercised at its boundary. A test now decodes a block that ends in the
  middle of a back reference, and pins the refusal.
- The comparison that decides which section holds a virtual address used a
  strict `<` at one boundary, and nothing tested the address that sits exactly
  at a section's end. A test now pins it, with the address just inside the
  section as its good-input twin.

## The hold-out vectors

The hold-out vectors were not run. `FO1_REF/holdout/` does not exist, and the
repository defines no format for it: no file naming, no field schema, and no
gate step or test reads one. The guide asks for more RNG seeds, more LZSS
streams and more damage cases in that folder, but nothing in the repository
consumes them, so there is nothing to run.

This is a real gap in the milestone's closure, not a formality: the guide says
a milestone closes only when the hold-out vectors pass on the owner's machine.
The report says so rather than claiming a pass that did not happen.

## The reviews

Six sessions reviewed the work, and none of them was the session that wrote
the code it reviewed.

| Review | Scope | Verdict | What happened |
|---|---|---|---|
| RA | `synthetic_endian.rs`, `synthetic_lzss.rs`, the endian and LZSS equivalence arguments | CHANGES NEEDED | 3 high, 5 medium, 6 low. All high and medium findings were applied; the fixed files were then approved. |
| RB | `synthetic_dat.rs`, `install_archive.rs`, `install_plaintext.rs` | CHANGES NEEDED | 4 high, 8 medium, 5 low. All high and medium findings were applied. |
| RC | `install_exe_tables.rs`, the SHA-256 module, the SHA-256 and unproven equivalence arguments | CHANGES NEEDED | 2 high, 6 medium, 12 low. All high and medium findings were applied. |
| RD | `harness.rs`, the gate scripts, the lint and profile tables, `ci.yml` | CHANGES NEEDED | 3 high, 7 medium, 11 low. All high findings were applied. |
| RA, second round | the fixes to the endian and LZSS files | APPROVE | 0 high, 0 medium, 6 low. |
| RB, RC, RD, second round | the fixes to the DAT, exe, harness and gate files | CHANGES NEEDED | The findings were applied and re-measured; see the sections above. |

Two findings were not applied, and both are stated here rather than hidden.

- The traceability script's rule that a CONFIRMED claim needs a test in oracle
  class 1, 2 or 3 does not substitute the claim id into its pattern, so the
  rule cannot fire. Enforcing it as written would fail 52 of the 82 CONFIRMED
  claims: 12 need a map edit, 29 need a claim citation in a test that already
  has the right class, and 11 have no class 1, 2 or 3 oracle at all, which
  would mean re-tagging evidence the binary confirms. That is an owner's
  decision about the evidence tags, not a change to make inside this closure.
  The rule's own reading is recorded in the open questions.
- The gate's step 8 judges the changed test FILE, not each changed test. A
  changed file's target must fail on the base, and a file whose target passes
  on the base needs a written note. Judging each test separately would need a
  note for every test that grades behaviour the base already had right, which
  is a hundred and twenty notes that say the same thing, and a list like that
  trains people to ignore the check.

## Deviations from the spec

- The `FieldOutOfRange` error's `offset` carries a BYTE OFFSET. The spec's
  Rust interface block wrote the table's virtual address into a `usize` field
  and contradicted the field's own doc comment, and the block did not compile.
  The byte offset is what the guide requires of an error variant, what every
  other error in the workspace carries, and what the repaired spec now says.
- The specification of the DAT-021 overrun was 39038 and the two ledgers said
  39256. The ledgers were wrong and now match the specification.
- The endian and LZSS specs did not fix the wording of their error messages, so
  the tests that render those errors had no oracle. The specs now fix the
  wording, which is our own choice for our own messages.

## What was not verified

- The hold-out vectors, because there are none and no format is defined.
- The claim map's rule for a CONFIRMED claim with no class 1, 2 or 3 test, as
  described above.
- The `fo1-exe` crate's table readers against the real executable, except by
  the install suite, which the mutation step does not grade.
- The CI workflow still runs only the synthetic suites and never runs
  `lint_rules.sh`, `lint_tests.sh`, `check_trace.sh` or `cargo deny`. That is
  hygiene work and it was left alone.
- The history of `main` still carries the 109 blobs of the previous session,
  and branch protection on `main` is still off. Both are the owner's decisions
  and both are outside this closure.

## Open questions

- The hold-out vector format, and where the gate step that runs it goes.
- Whether the traceability rule should be enforced as the guide writes it,
  which needs a decision on the eleven CONFIRMED claims that have no class 1,
  2 or 3 oracle.
- The `Truncated` error's offset: the specification says "at which the failure
  was detected" and the crate says "the region that ran out", and the two are
  one byte apart for a pair that is missing its second byte.
- Whether the DAT error messages should be pinned word for word, as the
  executable layer's are.
- The second column of the five player stat limit records, which the
  specification does not state and which no test asserts.
- The README's crate table still describes the target layout rather than the
  seven crates that exist; the report says so rather than pretending.

## Milestone state

- **M0** - DAT archives and LZSS: closed.
- **M1a** - byte readers, the DAT container and the executable tables: closed.
- **M1b** - `fo1-path`, `fo1-assets`, and the gate where 4,306 prototypes
  parse with zero trailing bytes: **open. Not implemented. No crate, no spec
  and no test for it exists in this tree.**
- **M2** to **M8**: open.
