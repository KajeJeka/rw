# Fail-before notes

The record for `tools/gate.sh` step 8, "fail before, pass after".

The guide: "A test that already passes on the base proves nothing about the
change. It needs a written note that explains why, and the owner reviews the
note."

Step 8 copies HEAD's version of every changed gate test file onto a worktree
of the base branch and runs it there. A changed test that FAILS on the base
proves the change: the test is new, or it pins behaviour the base does not
have. A changed test that PASSES on the base proves nothing about this
change, and step 8 fails until a note below says why that is acceptable.

The note is the owner's, not the gate's. The gate checks that one exists,
that it is in the shape the file documents, that it names a test the file
holds, prints the one it relied on, and leaves the reading of it to the
owner.

## Format

One entry per test file, in this shape:

    tests/gates/<file>.rs :: <test name> :: <reason>

The entry is read as the first two `::` separators and the reason is
everything after the second, so a reason may hold a `::` of its own.

- `tests/gates/<file>.rs` — the gate test file, named exactly as it is named
  in the tree. The step matches this field against the file it is reporting
  on.
- `<test name>` — the test the note covers: the `[[test]]` target name, or
  the name of a test function in that file. The step checks this field: a
  note that names a test the file does not hold is rejected, because a note
  that names the wrong test approves nothing. The step prints the note it
  relied on.
- `<reason>` — why the test already passes on the base and therefore proves
  nothing about the change. The owner reads this before accepting it.

Only the "Entries" section below is read. The prose above it, this format
example included, is not an entry.

A line in the section that is not in that shape is a malformed entry, and a
malformed entry fails step 8, the same way a key in `expected_counts.toml`
that the gate does not understand fails step 7: an approval nobody can read
is not an approval. "None yet." is the empty list, and it is removed when
the first entry is added.

## Entries

tests/gates/install_plaintext.rs :: install_plaintext :: the file was rewritten to strengthen its documentation: the doc comments now name the claim id (DAT-023, DAT-028) and spell out how each assertion relates to the candidate discovery the file pins. Not one of the six tests and not one assertion changed. The behaviour the file grades was already correct on the base, so the change under review is the strength of the proof, not a behaviour change.

tests/gates/synthetic_dat.rs :: synthetic_dat :: the file was rewritten to strengthen the proof: the class 4 oracle is now real (the tests import the reference tree walker and the reference LZSS decoder from fo1-tests and cross-walk or cross-decode the same bytes with them), the constants the spec's ledgers own are now read out of docs/spec/dat.constants.toml and docs/spec/lzss.constants.toml at run time instead of being literals, and two tests are new (the two-block LZSS chain fixture of DAT-019 and the DAT-021 stored-region bound). The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.

tests/gates/synthetic_endian.rs :: synthetic_endian :: the file was rewritten to strengthen the proof: the error texts are now pinned (both layers name the two variants Truncated and SeekOutOfRange and no third, the refused seek is exercised, and the rendered refusal is compared against the crate's frozen wording), and the reads that were discarded are now asserted to answer Ok before the counter counts them. The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.

tests/gates/synthetic_lzss.rs :: synthetic_lzss :: the file was rewritten to strengthen the proof: the ring bijection is now cross-walked through the decoder (every slot of the ring is read back and compared with the output position the shift gives it), all sixteen length biases are decoded instead of only the two ends of the range, and the refusals now pin the input offset and the crate's frozen wording. The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.
