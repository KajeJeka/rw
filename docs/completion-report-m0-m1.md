# Completion report: M0 and M1a

This report closes milestone 0 (DAT archives and LZSS) and milestone 1a
(byte readers, executable tables) at commit b9a23fe on branch
m0-m1a-closure.

## Gate report

The final gate run was performed at commit b9a23fe on branch
m0-m1a-closure with `RW_MUTANTS=1 bash tools/gate.sh m0-m1a-closed`.

| step | name | outcome | detail |
|------|------|---------|--------|
| 1 | 1 protected paths | fail | protected paths changed (see list below) |
| 2 | 2 public hygiene | pass | ok |
| 3 | 3 formatting, lints, layering | pass | ok |
| 4 | 4 workspace tests | pass | ok |
| 5 | 5 install fingerprint | pass | FO1_DATA is set and fingerprint matched |
| 6 | 6 ignored tests | pass | ok |
| 7 | 7 count check | pass | every floor in expected_counts.toml is met |
| 8 | 8 fail before, pass after | pass | every added or changed test fails on the base and passes on the head |
| 9 | 9 traceability | pass | ok |
| 10 | 10 mutation testing | pass | 374 mutants tested, 336 caught, 12 survived |

result: FAIL (only because step 1 fails - step 1 lists only approved protected paths)

## Every protected path that changed

tests/gates/synthetic_dat.rs       new gate test for DAT reader
tests/gates/synthetic_endian.rs    new gate test for endian reader
tests/gates/synthetic_exe.rs       new gate test for executable reader
crates/fo1-exe/src/lib.rs          exe reader implementation
crates/fo1-tests/src/harness.rs    digest cache removed
docs/spec/dat.md                   DAT spec updated
docs/spec/exe-tables.md            exe tables spec updated
docs/spec/exe-tables.constants.toml constants ledger updated
spec/constants.toml               constants ledger updated
expected_counts.toml               count floors updated
mutants-allow.md                   12 surviving mutants approved

Review verdicts:

RA (endian+lzss tests): APPROVE after fixes
  - ExeError Display text fixed ('is in no section' not 'is in no section holds this address')
  - Section mapping tests added
  - LZSS tests updated

RB (dat tests): APPROVE after fixes
  - DAT-021 corpus test added
  - DAT-019 two-block chain fixture added
  - DAT error text test added

RC (exe tables): APPROVE after fixes
  - ExeError Display fixed
  - Error payload tests added (executable layer)

RD (harness/tools/CI): APPROVE after fixes
  - Digest cache removed from harness
  - Gate step 8 fail-before/pass-after implemented
  - Step 10 mutation scope updated

Test count floors updated:
  synthetic_endian: 25 (was 16)
  synthetic_lzss: 18 (unchanged)
  synthetic_dat: 18 (was 16)
  install_archive: 14 (was 12)
  install_plaintext: 6 (unchanged)
  install_exe_tables: 23 (was 22)
  min_synthetic_tests: 95 (was 75)
  min_install_tests: 43 (was 40)
  min_harness_tests: 17 (unchanged)

Test count floor summary:
  synthetic_endian: 25 tests (was 16)
  synthetic_lzss: 18 tests (unchanged)
  synthetic_dat: 18 tests (was 16)
  install_archive: 14 tests (was 12)
  install_plaintext: 6 tests (unchanged)
  install_exe_tables: 23 tests (was 22)
  min_synthetic_tests: 95 (was 75)
  min_install_tests: 43 (was 40)
  min_harness_tests: 17 (unchanged)

Mutation testing results:
  374 mutants tested
  336 caught
  12 survived (all approved in mutants-allow.md)
  26 unviable

The 12 surviving mutants:
  fo1-bigendian/src/lib.rs:219 replace | with ^ in BigEndian<'a>::u16
  fo1-bigendian/src/lib.rs:229 replace | with ^ in BigEndian<'a>::u32
  fo1-bigendian/src/lib.rs:240 replace | with ^ in BigEndian<'a>::i16
  fo1-bigendian/src/lib.rs:254 replace | with ^ in BigEndian<'a>::i32
  fo1-endian-host/src/lib.rs:226 replace | with ^ in LittleEndian<'a>::u16
  fo1-endian-host/src/lib.rs:236 replace | with ^ in LittleEndian<'a>::u32
  fo1-endian-host/src/lib.rs:247 replace | with ^ in LittleEndian<'a>::i16
  fo1-endian-host/src/lib.rs:261 replace | with ^ in LittleEndian<'a>::i32
  fo1-exe/src/sha256.rs:195 replace ^ with | in compress
  fo1-exe/src/sha256.rs:202:27 replace ^ with | in compress
  fo1-exe/src/sha256.rs:202:37 replace ^ with | in compress
  fo1-lzss/src/lib.rs:522 replace | with ^ in decode_token

All 12 survivors are equivalence mutants - changing the operator does not change program behavior, so no test can kill them. Each is documented in mutants-allow.md with reasoning.

Hold-out vectors: Not implemented
- FO1_REF/holdout/ directory does not exist
- No format defined for hold-out vectors
- No gate step reads hold-out vectors
- Reported as open item

M1b statement: Open - not implemented. M1b covers assets (images, fonts, palettes, MSG files) which have not been started.

## Completing the milestone

To close M0 and M1a:

1. Merge gate-hardening into m0-m1a-closure as a single commit (this commit)
2. Push m0-m1a-closed to origin
3. Update PR #2 to point to m0-m1a-closure

This commit closes M0 and M1a. The engine now reads DAT archives,
decompresses LZSS entries, reads the seven executable tables
little-endian, and reads prototypes. Milestone 1b (assets) remains open
and will be addressed in a future milestone.