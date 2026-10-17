# PROTECTED file. Approved surviving mutants.

`tools/gate.sh` step 10 runs `cargo mutants`. A surviving mutant marks either a
gap in the proof or dead code, and fails the milestone.

This file is the only place a surviving mutant may be recorded. Every entry
needs a reason and the owner's approval. An empty list is the normal state.

## Mutation testing runs

- `fo1-lzss` — decoder and ring buffer. Target: every surviving mutant must be
  explained by the write-index 4078 ruling, which three canary tests pin.
- `fo1-dat` — tree walk, payload-mode table, chain accounting.
- `fo1-exe` — section lookup, record sizes, table addressing.

## Approved survivors

None yet.