# Contributing to RUSTY WASTELAND

RUSTY WASTELAND is a Rust engine that runs the data of Fallout 1 version 1.1
with the behaviour of the original program. It is unofficial and is not
affiliated with or endorsed by Bethesda or any rights holder. Fallout is a
trademark of its owner.

## Before you start

Read the Implementation Guide in the repository root. It holds every decision
and the evidence behind it. Then read `docs/spec/<crate>.md` for the crate you
want to change, and the matching file in `tests/gates/`.

The same inputs must always give the same outputs. If your change makes a
vanilla behaviour differ for the player's benefit, stop and open an issue
instead of committing it.

## Do not post game data

This repository holds no game data and never will. The player supplies a legal
copy of the game at run time.

Never add to an issue, a pull request or a commit:

- game files, archives, art, audio, maps or scripts
- `falloutw.exe`, patch files or installers
- hashes of your own game files, unless the owner approved that manifest
- text, tables or numbers read out of your game files
- screenshots of the original game
- source, comments, test data or a checkout of another engine

The engine reads the player's copy of the game at run time and never writes to
it. Anything the engine needs from the game is measured from the player's own
files, or committed as a fact with an evidence tag in a spec.

A private research note that quotes another engine's source must stay outside
the working tree, next to your install rather than inside the repository.

## Tests and code have separate authors

The party that writes the code never controls the proof that the code is
correct. That rule is what makes the test suite trustworthy.

Do not edit these paths in a change that also changes the code they grade:

- `tests/`
- `crates/fo1-tests/`
- `docs/spec/`
- `spec/`
- `tools/`
- `.github/`
- `clippy.toml`
- `deny.toml`
- `expected_counts.toml`
- the lint table in the root `Cargo.toml`
- `mutants-allow.md`

A change to one of those needs the owner's approval. Commit the tests and the
code separately, tests first.

If a test looks wrong, do not edit it to match your code. Open an issue with
the bytes, the offsets and the spec claim id that show what is wrong. An agent
that edits a protected file and then judges its own edit has broken the rule
even when the edit was correct.

## What makes a good test

- Name it after the claim it proves and cite the claim id in a doc comment.
- State the oracle class. The oracle is where the expected value comes from. A
  test whose expected value comes from the code under test does not count.
- Compare exactly. Use `assert_eq!` against a named constant. Never `>=` and
  never "at least".
- Cover the whole set. A test that loops over cases may not `continue`, may not
  return early and may not skip a case. A case passes or the test fails and
  prints the case name.
- Test the failure, not just the success. A reader must be proved to return an
  error on bad input and to never panic.

## Before you open a pull request

Run the gate. It runs every check a change must pass and writes
`target/gate/report.md`.

```
tools/gate.sh
```

If the gate needs your game files, point `FO1_DATA` at your install first. The
gate checks the SHA-256 of the executable and the sizes of the two archives and
stops on any other install rather than relaxing a constant.

Run `tools/check_public.sh` before every commit. It rejects absolute paths,
private names, game files and anything else that must not be published. Git
keeps everything ever committed, so a later delete does not undo a mistake.

## Licence

Contributions are accepted under the MIT licence. See `LICENSE`. The licence
covers this code only. It grants no rights to Fallout, to its data or to any
other engine.

By opening a pull request you confirm that your change contains no game data and
no source copied from another engine.

## Contact

Send an email to cttkajejeka@gmail.com with the title
`I WANT TO HELP IN RUSTY WASTELAND`. Say what you want to work on. Reply time
varies.

The address is public. Use it only for this purpose.
