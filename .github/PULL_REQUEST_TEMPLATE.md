# Pull request

## Before you open this

Two rules. They are the reason this repository can carry an open source licence.

- [ ] I have not attached game files. No archive, no executable, no art, no map, no
      script, no message text, no screenshot of the original game. Not in the branch,
      not in the commit history.
- [ ] I copied no Community Edition source. No code, no comments, no identifier names,
      no structure, in whole or in part.
- [ ] I used my own legally obtained copy of the game, or I did not need one. This
      project does not ship game data and requires the player to supply their own
      install.

## If you wrote tests

- [ ] I did not see the implementation while writing these tests. I wrote them from the
      specification and its vectors, not from the code they are meant to prove. If I
      edited production code after writing them, I have said so below, because a test
      whose author has seen the implementation is worth less than it looks.

## Review checklist

Answer each as a question, honestly. "No" on the first two is a reason to stop, not a
reason to negotiate.

- [ ] Did a protected path change? A protected path is `tests/`, `crates/fo1-tests/`,
      `docs/spec/`, `spec/`, `tools/`, `.github/`, `clippy.toml`, `deny.toml`, the lint
      table in the root `Cargo.toml`, `expected_counts.toml` or `mutants-allow.md`. If
      one did, has the owner approved it?
- [ ] Did a test disappear, or did an assertion get weaker? A removed test needs owner
      approval. A loosened tolerance, a widened comparison or a changed expected value
      needs a new oracle, not a preference.
- [ ] Does each new test cite a claim and an independent oracle? The claim id goes in a
      comment on the test. The oracle class is one of: bytes of the original executable
      read by a separate extraction tool; closure on real data; known plaintext; an
      independent reference implementation; a hand-computed vector from the
      implementation guide; or an algebraic law. The output of the code under test, a
      snapshot of that output, and a value copied from a failure message are not oracles.
- [ ] Does any code branch on a literal that appears in a test vector? No input
      equality check against a known case, no test-mode environment check in production
      code.
- [ ] Does any decoder or reader cut, pad or default to meet a length? A decoder that
      stops at the declared output size passes a size check by construction. Declared
      sizes are for detecting overrun, never for reaching.
- [ ] Does the completion report match the gate report? The report quotes the last
      lines of the gate output exactly. It does not paraphrase a failure, does not count
      an ignored test as done, and does not claim a result it did not run.
- [ ] Did the hygiene script pass?
- [ ] Is every new deviation from the original listed, with the reason and the claim it
      touches?

## Completion report

Required. A claim of success without this section does not count.

- [ ] Commit hash.
- [ ] Path and SHA-256 of `target/gate/report.md`.
- [ ] Last lines of the gate output, copied exactly.
- [ ] Tests added. Tests changed. Tests removed -- which must be zero without approval.
- [ ] Every ignored test and the reason it is ignored.
- [ ] Claims touched and their evidence tags after the run.
- [ ] Deviations from the specification.
- [ ] What was not verified.
- [ ] Open questions, and anything that needs an owner decision rather than a review.

## Changes

Describe what changed and why. Name the specification claims this implements or
touches.

## Role separation

- [ ] I did not edit a gate test unless my role for this change was the test author.
- [ ] I did not edit a protected path without the owner's approval recorded above.
- [ ] I reported a suspected mistake in a protected file instead of fixing it. The
      correct response to a protected file that looks wrong is to write down the bytes,
      the offsets and the claim, and ask the owner.