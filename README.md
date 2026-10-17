# RUSTY WASTELAND

An unofficial, clean-room reimplementation engine for Fallout 1 version 1.1, in Rust.

**This project is unofficial. It has no link to, and no association with, the rights
holders of Fallout. It is not endorsed by them. You need your own copy of the game to
use it.**

## What is in this repository

No game data. No Community Edition source. No private paths. No original art, logos,
icons or audio. The engine reads the player's own install at run time; the repository
ships nothing that came off a game disc.

This is enforced, not promised. `tools/check_public.sh` runs inside the gate and in
continuous integration and fails the build on a file with a game extension, a file over
256 KB, a drive-letter path, a name from the owner's private term list, an identifier
from the Community Edition identifier list, an integer literal array long enough to be
a pasted table dump, or any tracked file that `.gitignore` should have excluded.

## What this is

A Rust engine that runs the **data** of the game with the **behaviour** of the original
program.

Those are two different commitments and the difference is deliberate.

Same data:

- Bit-exact reading of the two archives and every asset format inside them.
- The compiled script files run with the original instruction semantics.
- The tables compiled into the reference executable are read from the player's own
  copy, in the correct byte order.

Same rules, same script results:

- The random number generator, the stat model, the damage pipeline, the critical hit
  and critical failure tables, experience and levels, and the pathfinder reproduce the
  verified behaviour.

Not the same bugs:

- The original's buffer defects -- unbounded buffers, a path helper with no length
  parameter, unclamped configuration values and an unframed save stream -- all share
  one root cause. Reproducing them is one decision, not four, and this project does not
  make it.
- A quirk that the game's rules or its own scripts depend on is reproduced and pinned by
  a test. A defect that only wastes memory or corrupts a file is not.

"Fidelity" here means the rules decide behaviour. Where the engine deliberately differs
from the original, the difference is on a deviation list with an owner decision behind
it.

## Status

Phase 1 is the archive foundation. **It is not finished and the engine does not run.**

What exists today:

- The DAT container is specified in `docs/spec/dat.md`.
- The LZSS decoder is specified in `docs/spec/lzss.md`, with the ring geometry, the
  termination rule and the hand-computed vectors pinned.
- The two byte-order layers are specified in `docs/spec/endian.md`.
- The tables compiled into the executable are specified in `docs/spec/exe-tables.md`.

What does not exist yet:

- The implementations. The specs are written; the crates that satisfy them have not
  landed. There are no working readers.
- The gate tests. They are being written now, and until they exist there is no proof of
  anything.
- Everything past the archive layer: sprites, prototypes, maps, the script virtual
  machine, rules, rendering, audio, saves, input.

Do not read the existence of a specification as a claim that the corresponding code
works. It does not yet.

## Repository layout

Six crates exist. The full design has sixteen crates and one test harness; the rest do
not exist yet and are not stubbed into pretending otherwise.

| Crate | Band | Owns |
| --- | --- | --- |
| `fo1-bigendian` | 0 | The big-endian byte order layer used by the asset formats. A total reader: returns `Result`, never panics, never reads past the end. |
| `fo1-endian-host` | 0 | The little-endian byte order layer used by the tables inside the reference executable. Deliberately a separate crate from the above, so a wrong-endian read cannot happen silently through a shared code path. |
| `fo1-lzss` | 0 | The LZSS decoder: the 4096 byte ring, the token loop, the termination and overrun rules. Never truncates, never pads, never defaults to a length. |
| `fo1-dat` | 1 | The DAT archive container: the directory tree, the entry records, the payload mode table, and block chains. |
| `fo1-exe` | 1 | The PE section reader for the reference executable, and the data tables compiled into it. Refuses any build whose digest is not the reference. |
| `fo1-tests` | 9 | The `FO1_DATA` gate harness and the test author's independent reference decoders. Dev dependency only. No runtime crate depends on it. |

The root package, `rw-gate`, carries no library code. It exists to own `tests/gates/`,
where the gate tests live, so that the proof is outside every subsystem's own tree.

`tools/layers.txt` declares the dependency bands. Dependencies point down only, and
`tools/check_layers.sh` fails the build on a crate that reaches sideways or upward.

## Building

```sh
cargo build --workspace
cargo test --workspace
```

Point the engine and the gate harness at your own install with the `FO1_DATA`
environment variable, naming the directory that contains `falloutw.exe`,
`MASTER.DAT` and `CRITTER.DAT`. The harness fingerprints that install against the
reference build and refuses anything else rather than relaxing a constant.

Continuous integration runs **only the synthetic tests**. It never holds game data, it
never downloads game data, and it has no network access. The tests that need a real
install are marked `#[ignore]` and are run by the owner on their own machine through
`tools/gate.sh`, which is the only entry point that counts as a full gate.

The toolchain is pinned. Use the Rust version named in `Cargo.toml`
(`rust-version`) or newer with the same toolchain generation; the CI workflow pins one
exact version.

## Contributing

Two rules, and they are not stylistic.

1. **Never attach game files.** Not to an issue, not to a pull request, not as a
   fixture, not as a hex dump of an asset you decoded. The hygiene script fails the
   build if a tracked file looks like game data, and a leak into the git history is
   permanent -- git keeps everything ever committed, so a later delete does not undo
   it.
2. **Confirm in your pull request that you copied no Community Edition source.** The
   project's licence position depends on it. The Community Edition is licensed under a
   licence that is not open source and that restricts distribution of derivative work,
   so a single copied function could change what this repository is allowed to be.

Beyond that: write the specification before the code, give every claim an id and an
evidence tag, and make every expected value come from somewhere other than the code
under test.

## Licence

**No licence has been selected.** That is a deliberate open decision and it is not an
oversight.

The reason it is open is the clean-room rule. The Community Edition is a decompilation
of the reference build and is published under a licence that is not an open source
licence and that limits distribution of derivative work to free non-commercial use. If
any part of its code, comments, names or structure reached this repository, the project
could not carry an ordinary open source licence. The clean-room rule exists to keep
that from happening, which means the licence has to be chosen on the basis of what this
repository actually contains.

Until a licence is chosen, nothing in this repository is granted any rights. See
`LICENSE`.

The owner should take legal advice on the repository licence, and separately on
trademark wording, before the first public push. Nothing in this file is legal advice.

## Trademarks

Fallout is a trademark of its rights holders. This repository is not affiliated with,
endorsed by, sponsored by or approved by those rights holders. It is a fan project. Any
reference to the original game is nominative: it identifies what this engine is
compatible with, and nothing more.