# Fail-before notes

The record for `tools/gate.sh` step 8, "fail before, pass after".

The guide: "The gate copies the test files from the head commit onto the base
tree and runs them. A new or changed test must fail on the base, and a
compile error counts as a failure. It must pass on the head. A test that
already passes on the base proves nothing about the change. It needs a
written note that explains why, and the owner reviews the note."

## What the gate judges

The gate judges each **test**, not each file. The test list of a target comes
from the test binary itself (`cargo test --test <target> -- --list`), taken at
the base and at the head. A test is:

- **added** when the head lists it and the base does not;
- **changed** when both list it and its own source differs between the two.

Every added or changed test is run against the base implementation, in a
worktree of the base ref, with the head's test file copied over. A test of
new behaviour must **fail** there: that is what proves the change. A test
that already passes on the base cannot prove the change, so it must carry
**either** a recorded mutation kill **or** an exemption, and both live in this
file. A test that carries neither fails the step. Every added or changed test
must also **pass** on the head.

A file of a hundred tests is one line in a diff, so a file that changed once
and passed on the base would have hidden the fact that none of its hundred
tests was proved. That is why the unit of judgement here is the test.

This file is a PROTECTED file: a change to it is noticed by step 1, so the
owner reviews every record and every exemption.

## Entries: the exemptions

An exemption says why a test that passes on the base is acceptable anyway.
One entry per test, in this shape:

    tests/gates/<file>.rs :: <test name> :: <reason>

- `tests/gates/<file>.rs` — the gate test file, named exactly as it is named
  in the tree.
- `<test name>` — the name of the test function in that file. The gate checks
  this field: an exemption that names a test the file does not hold is
  rejected, because an exemption that names the wrong test approves nothing.
  The name is the function's own name, not the `[[test]]` target's name: the
  gate judges tests, so an exemption covers one test.
- `<reason>` — why the test already passes on the base and therefore proves
  nothing about the change. The owner reads this before accepting it.

The entry is read as the first two `::` separators and the reason is
everything after the second, so a reason may hold a `::` of its own.

Only the "## Entries" section below is read. The prose above it, this format
example included, is not an entry.

A line in the section that is not in that shape is a malformed entry, and a
malformed entry fails step 8, the same way a key in `expected_counts.toml`
that the gate does not understand fails step 7: an approval nobody can read
is not an approval. "None yet." is the empty list, and it is removed when the
first entry is added.

## Recorded mutation kills: the other way through

A recorded mutation kill says that the test still has teeth: it names a
mutation of the implementation, and the gate proves the claim by applying
that mutation and watching the test fail.

One entry per test, in this shape:

    tests/gates/<file>.rs :: <test name> :: <impl file> :: <from line> :: <to line> :: <reason>

- `tests/gates/<file>.rs` — the gate test file.
- `<test name>` — the name of the test function that kills the mutation.
- `<impl file>` — the file the mutation is made in, named as it is named in
  the tree (`crates/fo1-lzss/src/lib.rs`).
- `<from line>` — the text of the ONE line of that file the mutation
  replaces. It must match exactly one line of the file, so the mutation is
  unambiguous; a line that matches none, or more than one, is rejected.
- `<to line>` — the text that replaces it. The indentation of the original
  line is kept.
- `<reason>` — why this test still has teeth, in prose.

The fields are split on the first six `" :: "` separators, with spaces around
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

None yet.

## Recorded mutation kills

None yet.

## Historical file-level notes

These are the notes the M0 and M1a closure wrote, when the gate still judged
a changed test FILE. The gate does not read this section: the rule is now
per test, and a note that covers a whole file would let one line exempt a
hundred tests. They are kept because they say why those files were rewritten,
which is still true.

tests/gates/install_plaintext.rs :: install_plaintext :: the file was rewritten to strengthen its documentation: the doc comments now name the claim id (DAT-023, DAT-028) and spell out how each assertion relates to the candidate discovery the file pins. Not one of the six tests and not one assertion changed. The behaviour the file grades was already correct on the base, so the change under review is the strength of the proof, not a behaviour change.

tests/gates/synthetic_dat.rs :: synthetic_dat :: the file was rewritten to strengthen the proof: the class 4 oracle is now real (the tests import the reference tree walker and the reference LZSS decoder from fo1-tests and cross-walk or cross-decode the same bytes with them), the constants the spec's ledgers own are now read out of docs/spec/dat.constants.toml and docs/spec/lzss.constants.toml at run time instead of being literals, and two tests are new (the two-block LZSS chain fixture of DAT-019 and the DAT-021 stored-region bound). The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.

tests/gates/synthetic_endian.rs :: synthetic_endian :: the file was rewritten to strengthen the proof: the error texts are now pinned (both layers name the two variants Truncated and SeekOutOfRange and no third, the refused seek is exercised, and the rendered refusal is compared against the crate's frozen wording), and the reads that were discarded are now asserted to answer Ok before the counter counts them. The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.

tests/gates/synthetic_lzss.rs :: synthetic_lzss :: the file was rewritten to strengthen the proof: the ring bijection is now cross-walked through the decoder (every slot of the ring is read back and compared with the output position the shift gives it), all sixteen length biases are decoded instead of only the two ends of the range, and the refusals now pin the input offset and the crate's frozen wording. The behaviour these grade was already correct on the base, so the change under review is the added strength of the proof, not a behaviour change.
