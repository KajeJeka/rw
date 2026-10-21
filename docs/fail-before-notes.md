# Fail-before notes

The record for `tools/gate.sh` step 8, "fail before, pass after".

The guide: "The gate copies the test files from the head commit onto the base
tree and runs them. A new or changed test must fail on the base, and a
compile error counts as a failure. It must pass on the head. A test that
already passes on the base proves nothing about the change. It needs a
written note that explains why, and the owner reviews the note."

## What the gate judges

The gate judges each **test**, not each file. The test list of a target comes
from the test binary itself (`cargo test <selector> -- --list`), taken at
the base and at the head. A test is:

- **added** when the head lists it and the base does not;
- **changed** when both list it and its own source differs between the two.

Every added or changed test is run against the base implementation, in a
worktree of the base ref, with the head's test code copied in. A test of
new behaviour must **fail** there: that is what proves the change. A test
that already passes on the base cannot prove the change, so it must carry
**either** a recorded mutation kill **or** an exemption, and both live in this
file. A test that carries neither fails the step. Every added or changed test
must also **pass** on the head.

A file of a hundred tests is one line in a diff, so a file that changed once
and passed on the base would have hidden the fact that none of its hundred
tests was proved. That is why the unit of judgement here is the test.

### Every test target is judged

The gate reads the test targets of the workspace out of the root `Cargo.toml`
and the crate manifests:

- every `[[test]]` target (the gate test files of the root package, and any
  integration target a crate declares);
- the **unit test target** of every crate, which is the tests inside its
  `src/` tree;
- every auto-discovered integration target, a file directly under a crate's
  `tests/` directory.

The 34 tests of `crates/fo1-tests` and `crates/fo1-exe` are tests of a
target like any other. A test added or changed in one of them is a changed
test, and the gate judges it. A change that touches no test of any target is
exempt, which is the guide's own rule ("a pure refactor that changes no test
is exempt").

### The identity of a test

A test is identified by **the target it belongs to and its own name**. Two
tests that share a name in two targets are two tests, and a change to either
of them must be seen. The exemption and the record name the file the test
lives in, and a file belongs to exactly one target, so the file and the name
say the same thing.

### The base tree

One worktree of the base ref serves every file, and it is **restored to the
base ref before every file's run**, so one file's head copy can never reach
another file's base. What the tree holds for a file's run is:

- the **base** version of every file that carries the implementation under
  test. The base is the base: the base run must measure the base
  implementation, or the test proves nothing.
- the **head** version of the file under judgement, or of its `cfg(test)`
  region when the same file also carries the implementation. The test is the
  change under judgement. A file that holds its tests outside a `cfg(test)`
  region is judged whole, and the step says so, because then the base run
  measures the head's file.
- the **head** version of the changed files of the dev-only packages the
  judged package depends on, which are the harness the test needs to build.
  A file the test **needs** must not be left at the base, or a compile error
  in the base would be an artefact of the copy rather than the base's own
  fault. Everything else stays at the base, because a peer file's head copy
  is not part of the change under judgement.

The gate prints what the base tree holds for every run, so a reader can see
which version of each changed file the base run measured.

A base that does not build is **reported** and fails the step: the run
measured nothing. A compile error that comes from the head's own test code
is the guide's case ("a compile error counts as a failure"), and it is
reported as a verdict of its own, with the compiler's first lines, so that it
is never absorbed silently into "the test failed on the base".

This file is a PROTECTED file: a change to it is noticed by step 1, so the
owner reviews every record and every exemption.

### Entries: the exemptions

An exemption says why a test that passes on the base is acceptable anyway.
One entry per test, in this shape:

    <test file> :: <test name> :: <reason>

- `<test file>` — the file the test lives in, named exactly as it is named in
  the tree: a gate test file under `tests/gates/`, or a crate's source file
  when the test is one of that crate's unit tests.
- `<test name>` — the name of the test function in that file. The gate checks
  this field: an exemption that names a test the file does not hold is
  rejected, because an exemption that names the wrong test approves nothing.
  The name is the function's own name, not the `[[test]]` target's name: the
  gate judges tests, so an exemption covers one test.
- `<reason>` — why the test already passes on the base and therefore proves
  nothing about the change. The owner reads this before accepting it.

The entry is read as the first two `::` separators and the reason is
everything after the second, so a reason may hold a `::` of its own. The
entry is one line: a reason that runs over two lines is a malformed entry.

Only the "## Entries" section below is read. The prose above it, this format
example included, is not an entry.

A line in the section that is not in that shape is a malformed entry, and a
malformed entry fails step 8, the same way a key in `expected_counts.toml`
that the gate does not understand fails step 7: an approval nobody can read
is not an approval. "None yet." is the empty list, and it is removed when the
first entry is added.

#### How the entries below are ordered, and what the reasons say

The entries are grouped by the test file they belong to, in the order the
gate lists them, and a blank line separates one group from the next. Every
entry names its own test.

Two facts about this branch shape almost every one of these reasons, so they
are stated once here and each entry names the part that applies to it:

- The crates under test did not change. Between `m0-m1a-closed` and this
  head, the only implementation change in the whole workspace is the dev-only
  harness `crates/fo1-tests/src/harness.rs`, which gained the accessors that
  reach the owner's hold-out folder. `fo1-dat`, `fo1-exe`, `fo1-lzss` and
  `fo1-bigendian` are byte-identical to the base.
- The gate charges every test of a file when the code **outside every test**
  of that file changes, because a change in the shared code reaches tests the
  gate cannot attribute to one of them. Each of the three files below was
  changed that way: a new constant, a new column in a shared table, or two new
  fixture builders were added outside every test. That is why tests whose own
  source is byte-identical to the base are judged at all.

So a reason here says one of two things, and always which one applies: that
the test's own source did not change (a claim id in its doc attribute, or
nothing at all), or that the test's source did change and what it now pins
that the base already got right.

### Recorded mutation kills: the other way through

A recorded mutation kill says that the test still has teeth: it names a
mutation of the implementation, and the gate proves the claim by applying
that mutation and watching the test fail.

One entry per test, in this shape:

    <test file> :: <test name> :: <impl file> :: <from line> :: <to line> :: <reason>

- `<test file>` — the file the test lives in, named as it is named in the
  tree: a gate test file under `tests/gates/`, or a crate's source file when
  the test is one of that crate's unit tests.
- `<test name>` — the name of the test function that kills the mutation.
- `<impl file>` — the file the mutation is made in, named as it is named in
  the tree (`crates/fo1-lzss/src/lib.rs`).
- `<from line>` — the text of the ONE line of that file the mutation
  replaces. It must match exactly one line of the file, so the mutation is
  unambiguous; a line that matches none, or more than one, is rejected.
- `<to line>` — the text that replaces it. The indentation of the original
  line is kept.
- `<reason>` — why this test still has teeth, in prose.

The fields are split on the first five `" :: "` separators, with spaces around
them, because a Rust line often holds a `::` of its own. A field that holds
`" :: "` cannot be written down, and the gate says so instead of guessing.

The gate does not trust the record. It applies the mutation to the head
implementation, in a worktree of its own, and runs the named test. The record
is true only when the build succeeds and the test **fails**. A mutation that
does not compile is not a behaviour change a test can kill, and a test that
still passes with the mutation applied does not kill it: both are a FALSE
record, and a false record fails step 8. A record nobody checks is prose,
and prose is not a proof.

Only the "## Recorded mutation kills" section below is read.

## Entries

tests/gates/install_archive.rs :: dat_025_directory_and_entry_counts_are_exact :: The body did not change: it still reads the root header of both archives through the big-endian asset layer and pins the directory count and the entry count of each. Only the doc attribute above it changed, and it now names DAT-001, DAT-002, END-002 and END-035 beside DAT-025 and says which other test grades the little-endian fold. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: dat_026_payload_mode_counts_are_exact :: The body did not change: the payload mode counts of both archives and the mode arithmetic are still graded exactly as before. Only the wording of the oracle line in the doc attribute above it changed. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: dat_017_stored_and_chain_entries_split_on_their_compressed_size :: The body did not change: every one of the 25,243 entries is still split into stored and chain on its compressed size, exhaustively and not by sample. Only the wording of the oracle line in the doc attribute above it changed. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: dat_021_the_stored_extent_is_bounded_by_the_compressed_size :: The body did not change: the seven stored entries of the corpus are still cross-checked against the independent reference walk for the stored extent bound. Only the wording of the doc attribute above it changed, and it now states the oracle class in the shape its neighbours use. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: dat_022_every_block_chain_consumes_exactly_its_compressed_size :: The body did not change: every chain of the corpus is still walked as framed blocks and must consume exactly its compressed size. Only the doc attribute above it changed, and it now names DAT-016 and DAT-008 beside DAT-022 and DAT-020. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: dat_020_chain_stats_totals_equal_the_framing_census :: The body did not change: the production chain driver's totals are still compared with the framing census over all 20,456 chains. Only the doc attribute above it changed, and it now names DAT-018 and DAT-027 beside DAT-020. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: dat_023_every_entry_decodes_to_its_declared_uncompressed_size :: The body did not change: every entry of both archives is still decoded to its declared uncompressed size and the three size totals are still asserted. Only the doc attribute above it changed, and it now names DAT-009, DAT-015, DAT-019, DAT-030 and the four LZSS claims beside DAT-023 and DAT-024. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: dat_005_block_field_zero_is_the_entry_count_and_field_two_is_sixteen :: The body did change: the test now also reads block field 1 and block field 3 and asserts that block 0 of each archive carries the capacity and the dead pointer the spec names (DAT-007), so it now pins those two values as well as the entry count and the trailer width. It reads both values straight out of the archive bytes at the offsets the reference walker reports, so what it pins is a fact about the installed file and not about a changed implementation: the crates under test are byte-identical to the base and the base walker already reported those offsets, so the new assertions hold on the base and cannot fail there.

tests/gates/install_archive.rs :: dat_011_path_composition_keys_on_the_directory_name :: Neither its body nor its doc attribute changed: it still pins that the composed path keys on the directory name. The only change to the file is the four DAT-007 constants added outside every test, which dat_005 alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_archive.rs :: dat_029_real_lookups_return_the_measured_entries :: The body did not change: the real lookups still return the measured entries, and the offset of the palette entry still proves the tree order is not the physical order. Only the doc attribute above it changed, and it now names DAT-003 beside DAT-029, DAT-012 and DAT-010. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: dat_028_the_three_loose_size_exclusions_are_exact :: The body did not change: the three loose file size exclusions are still measured exactly against the byte differences the guide writes down. Only the wording of the oracle line in the doc attribute above it changed. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: lzss_014_a_stream_one_byte_short_and_one_byte_long_both_fail :: The body did not change: the one byte short and the one byte long stream of the real block are still both refused. Only the doc attribute above it changed, and it now names LZSS-013 and LZSS-014 and states the oracle classes in the shape its neighbours use. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: the_install_fingerprint_is_exact :: The body did not change: the install fingerprint is still compared against the exact sizes and digest the guide states. Only the wording of the oracle line in the doc attribute above it changed. The change outside every test that charges this test is the four DAT-007 constants, which dat_005 alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_archive.rs :: the_harness_rejects_a_foreign_install :: Neither its body nor its doc attribute changed: it still refuses an install that is not the reference install. The only change to the file is the four DAT-007 constants added outside every test, which dat_005 alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_reference_build_is_admitted_and_carries_the_expected_digest :: The body did not change: the reference build is still admitted only when it carries the expected digest. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_gate_rejects_an_executable_with_one_bit_flipped :: The body did not change: an executable with one bit flipped is still rejected. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_gate_refuses_two_short_buffers_before_touching_a_header :: The body did not change: the two short buffers are still refused before any header is read. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_pe32_plus_optional_header_is_refused :: The body did not change: a PE32+ optional header is still refused. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_critter_critical_table_ends_where_the_player_table_begins :: The body did not change: the critter critical table is still pinned to end exactly where the player table begins. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_stat_limits_end_where_the_player_stat_limits_begin :: The body did not change: the stat limits table is still pinned to end exactly where the player stat limits begin. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_player_stat_limits_end_where_the_xp_table_begins :: The body did not change: the player stat limits table is still pinned to end exactly where the XP table begins. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_xp_for_level_one_is_one_while_the_same_four_bytes_fold_big_endian_to_16777216 :: The body did not change: the XP for level one is still read as one, and the same four bytes still fold big-endian to 16,777,216. Only the doc attribute above it changed, and its claim list now names END-001, END-006, END-008 and END-015 beside EXETBL-013. The change outside every test that charges this test is the new PLAYER_STAT_LIMIT_FIELD2 constant, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_xp_table_holds_the_twenty_one_triangular_numbers_indexed_by_level :: The body did not change: the XP table is still pinned to the twenty-one triangular numbers, indexed by level. Only the doc attribute above it changed, and its claim list now names END-005 and END-007 beside EXETBL-009 and EXETBL-015. The change outside every test that charges this test is the new PLAYER_STAT_LIMIT_FIELD2 constant, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_stat_limit_record_sixteen_has_a_minimum_of_zero_and_not_minus_sixty :: The body did not change: record sixteen's minimum is still pinned to zero and not to minus sixty. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_stat_limits_are_thirty_eight_records_of_six_fields_with_a_zero_field_zero :: The body did not change: the stat limits are still pinned to thirty-eight records of six fields with a zero field zero. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_player_stat_limits_are_five_records_with_the_documented_ranges :: The body did change: the test now also asserts that field 2 is zero in all five player stat limit records, so it now pins that field 2 cannot be a per record index there. The value it asserts is read out of the reference executable through the reader that was already there, and the crates under test are byte-identical to the base, so the base already read field 2 as zero in all five records and the new assertion holds on the base and cannot fail there.

tests/gates/install_exe_tables.rs :: install_hit_location_penalties_are_the_nine_documented_values :: The body did not change: the nine hit location penalties are still pinned to the documented values. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_critical_success_critters_are_810_records_of_28_bytes :: The body did not change: the critter critical success table is still pinned to 810 records of 28 bytes. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_critical_success_player_is_54_records_of_28_bytes :: The body did not change: the player critical success table is still pinned to 54 records of 28 bytes. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_critical_failure_is_seven_rows_of_five_values :: The body did not change: the critical failure table is still pinned to seven rows of five values. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_critter_critical_first_and_last_records_carry_the_measured_bytes :: The body did not change: the first and the last critter critical records are still pinned to the measured bytes. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_player_critical_first_and_last_records_carry_the_measured_bytes :: The body did not change: the first and the last player critical records are still pinned to the measured bytes. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_read_va_returns_every_table_window_from_dgroup_raw_data :: The body did not change: every table window is still read out of the dgroup raw data at its own virtual address. Only the doc attribute above it changed, and its claim list now names END-004 beside EXETBL-002. The change outside every test that charges this test is the new PLAYER_STAT_LIMIT_FIELD2 constant, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so nothing this test grades changed and it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_read_va_is_total_over_length_and_over_address :: The body did not change: the read at a virtual address is still pinned to be total over both the length and the address. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_every_exe_error_carries_the_payload_and_the_message_the_spec_fixes :: The body did not change: every error variant is still pinned to carry its payload and the message the spec fixes. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_a_section_with_a_zero_raw_pointer_is_never_file_backed :: The body did not change: a section with a zero raw pointer is still refused as file backed. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_exe_tables.rs :: install_bit_flips_across_a_dgroup_slice_are_answered_never_panicked :: The body did not change: every bit flip across a dgroup slice is still answered and never panics. Neither its body nor its doc attribute changed. The only change to the file is the new PLAYER_STAT_LIMIT_FIELD2 constant added outside every test, which install_player_stat_limits_are_five_records_with_the_documented_ranges alone reads, so the gate charges this test for a change that does not reach it, and nothing this test grades changed, so it cannot fail on the base.

tests/gates/install_holdout.rs :: holdout_dat_vectors_decode_to_the_declared_size_and_digest :: A new test in a new file. It grades the DAT reader against the owner's hold-out set, which the manifest declares as 60 vectors, and for each of them it asserts the vector file's own length and SHA-256, that the archive parses, that the entry is found at the path the manifest names, that the entry decodes, and that the decoded bytes have the declared size and the declared digest. The crates under test are byte-identical to the base, and the base reader already produced those sizes and digests, so the test passes on the base and cannot fail there. What it adds is the strength of the proof: the reader is now graded on vectors it has never seen.

tests/gates/install_holdout.rs :: holdout_exe_table_vectors_match_the_reference_executable_bytes :: A new test in a new file. It grades the executable table reader against the owner's hold-out set, which the manifest declares as 12 vectors, and for each of them it asserts the vector file's own length and SHA-256, that the window it names lies inside the reference executable, that the reader's own mapping agrees with the vector's offset, and that the bytes the reader returns are the vector's bytes. The crates under test are byte-identical to the base, and the base reader already returned those bytes, so the test passes on the base and cannot fail there. What it adds is the strength of the proof: the tables are now graded on windows the reader has never been pointed at.

tests/gates/synthetic_dat.rs :: synthetic_round_trip_reads_every_field :: The body did not change: a synthetic archive is still built, parsed and compared field by field with the values it was built from. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_the_directory_list_is_the_root_header_names_in_file_order :: The body did not change: the directory list is still pinned to be the root header's names in file order. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_block_field_0_is_the_entry_count_not_the_directory_count :: The body did not change: a block's field 0 is still pinned to be that block's own entry count and not the archive's directory count. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_block_field_2_is_the_trailer_width_not_the_record_size :: The body did not change: a block's field 2 is still pinned to be the trailer width and not the record size. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_dot_directory_name_composes_no_prefix_and_a_named_one_does :: The body did not change: the dot directory name is still pinned to compose no prefix and a named one to compose one. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_payload_mode_is_the_flags_high_nibble :: The body did not change: the payload mode is still pinned to be the high nibble of the flags word. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_unknown_payload_nibble_is_an_error_not_a_default :: The body did not change: an unknown payload nibble is still pinned to be refused rather than defaulted. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_stored_entry_takes_the_uncompressed_size_and_rejects_a_nonzero_compressed_size :: The body did not change: a stored entry is still pinned to take its uncompressed size, and a nonzero compressed size is still refused. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_chain_mixes_one_raw_block_and_one_lzss_block_in_stored_order :: The body did not change: a chain that mixes one raw block and one LZSS block is still pinned to decode in stored order. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_a_chain_of_two_lzss_blocks_gives_each_block_a_fresh_ring :: The body did not change: each block of a two block LZSS chain is still pinned to start with a fresh ring. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_chain_block_headers_account_for_the_compressed_size_exactly :: The body did not change: the block headers of a chain are still pinned to account for its compressed size exactly. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_a_stored_region_is_bounded_by_the_compressed_size_not_the_uncompressed_one :: The body did not change: the stored region is still pinned to be bounded by the compressed size and not the uncompressed one. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_declared_output_larger_than_the_payload_produces_is_an_error :: The body did not change: a declared output larger than the payload is still pinned to be an error. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_stream_one_byte_short_and_one_byte_long_are_each_errors :: The body did not change: the one byte short and the one byte long streams are still pinned to be an error each. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_tree_order_is_not_physical_order :: The body did not change: the tree order is still pinned not to be the physical order. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_lookup_is_by_uppercased_path_and_an_absent_path_is_not_an_error :: The body did not change: the lookup is still pinned to be by uppercased path, and an absent path is still not an error. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_every_bit_flip_of_the_root_header_answers_without_panicking :: The body did not change: every bit of the root header is still flipped, and every answer is still an answer and not a panic. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_malformed_trees_are_errors :: The body did not change: the malformed trees it builds are still pinned to be errors. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_a_zero_entry_block_and_a_zero_length_name_are_accepted :: The body did not change: a zero entry block and a zero length name are still pinned to be accepted. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_a_truncated_tree_reports_the_byte_position_it_ran_out_at :: The body did change: the test now pins six more refusals, the fourth root header field cut short, a directory name missing its second byte, a stored name that is not UTF-8, an LZSS stream that runs out in the middle of a back reference, a chain frame that declares a zero length payload, and a frame whose payload runs past the chain, each with the byte offset the spec's byte offset section fixes, and its doc attribute now states the rule the spec states, that the offset is where the failed operation started. Every one of those offsets is what the DAT and LZSS crates already reported: the crates under test are byte-identical to the base and the base already refused each of those inputs with the same offset, so the new assertions hold on the base and cannot fail there.

tests/gates/synthetic_dat.rs :: synthetic_the_archive_reports_the_label_it_was_parsed_under :: The body did not change: the archive is still pinned to report the label it was parsed under. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

tests/gates/synthetic_dat.rs :: synthetic_every_error_variant_names_what_went_wrong :: The body did change: the test now asserts the rendered text of all nine variants word for word against the strings the spec's new Error text section fixes, where before it only asserted that a message is not empty, that it names its quantity, and that no two variants render the same. The wording it now pins is the wording the DAT crate already printed, because that crate is byte-identical to the base, so the base renders the same nine texts and the new assertion holds on the base and cannot fail there.

tests/gates/synthetic_dat.rs :: synthetic_a_chain_block_that_ends_mid_back_reference_says_whether_the_declaration_was_met :: The body did not change: a chain block that ends in the middle of a back reference is still pinned to say whether the declaration was met. Neither its body nor its doc attribute changed. The changes outside every test that charge it are the new DAT-030 offset constants, the fourth column the shared error case table gained, and the two new fixture builders, and none of them reaches this test, so nothing this test grades changed and it cannot fail on the base.

## Recorded mutation kills

None yet.

### Historical file-level notes

These are the notes the M0 and M1a closure wrote, when the gate still judged
a changed test FILE. The gate does not read this section: the rule is now
per test, and a note that covers a whole file would let one line exempt a
hundred tests. They are kept because they say why those files were rewritten,
which is still true.

tests/gates/install_plaintext.rs :: install_plaintext :: the file was rewritten to strengthen its documentation: the doc comments now name the claim id (DAT-023, DAT-028) and spell out how each assertion relates to the candidate discovery the file pins. Not one of the six tests and not one assertion changed. The behaviour the file grades was already correct on the base, so the change under review is the strength of the proof, not a behaviour change.

tests/gates/synthetic_dat.rs :: synthetic_dat :: the file was rewritten to strengthen the proof: the class 4 oracle is now real (the tests import the reference tree walker and the reference LZSS decoder from fo1-tests and cross-walk or cross-decode the same bytes with them), the constants the spec's ledgers own are now read out of docs/spec/dat.constants.toml and docs/spec/lzss.constants.toml at run time instead of being literals, and two tests are new (the two-block LZSS chain fixture of DAT-019 and the DAT-021 stored-region bound). The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.

tests/gates/synthetic_endian.rs :: synthetic_endian :: the file was rewritten to strengthen the proof: the error texts are now pinned (both layers name the two variants Truncated and SeekOutOfRange and no third, the refused seek is exercised, and the rendered refusal is compared against the crate's frozen wording), and the reads that were discarded are now asserted to answer Ok before the counter counts them. The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.

tests/gates/synthetic_lzss.rs :: synthetic_lzss :: the file was rewritten to strengthen the proof: the ring bijection is now cross-walked through the decoder (every slot of the ring is read back and compared with the output position the shift gives it), all sixteen length biases are decoded instead of only the two ends of the range, and the refusals now pin the input offset and the crate's frozen wording. The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.
