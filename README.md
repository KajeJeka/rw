# RUSTY WASTELAND

RUSTY WASTELAND is an engine for Fallout 1, written in Rust. It reads the data files of a copy of the game that you own and runs the game on modern systems. It is not affiliated with or endorsed by Bethesda or any rights holder. Fallout is a trademark of its owner.

The first complete version will be called **0.9 ALL MUTANTS ALLOWED**. Its Cargo version is 0.9.0 and its tag is `v0.9.0`.

## Status

Milestones 0 and 1 are implemented. The engine reads the DAT archives, decompresses their entries and reads the seven data tables stored in the original executable. It cannot start the game yet. Milestones 2 to 8 are open. Version 0.9 is released when milestone 8 passes its gate.

| Milestone | Content | State |
|---|---|---|
| M0 | DAT archives and LZSS | Implemented |
| M1 | Byte readers, executable tables, assets | Implemented |
| M2 | World and renderer | Open |
| M3 | Combat | Open |
| M4 | Scripts | Open |
| M5 | Saves | Open |
| M6 | Pathfinding and world map | Open |
| M7 | Audio | Open |
| M8 | Assembly, one playable map | Open |

## What you need

You need your own legal copy of Fallout 1 for Windows, version 1.1. The repository contains no game data and never will. The engine checks the SHA-256 of `falloutw.exe` against the supported build and stops with an error on any other build.

You also need the current stable Rust toolchain from https://rustup.rs.

## Run the game

These steps apply once milestone 8 is complete.

1. Copy your game folder anywhere on your disk. It must contain `falloutw.exe`, `MASTER.DAT` and `CRITTER.DAT`.
2. Set `FO1_DATA` to that folder.
3. Build and start.

```
git clone https://github.com/KajeJeka/rusty-wasteland
cd rusty-wasteland
export FO1_DATA=/path/to/your/fallout
cargo run --release -p fo1-app
```

On Windows PowerShell, set the variable with `$env:FO1_DATA`, pointing it at that folder.

The engine reads your files and never writes to them. Save files go to the directory that the engine prints at startup.

## Run the tests

Three suites need no game files and finish in seconds.

```
cargo test --test synthetic_endian
cargo test --test synthetic_lzss
cargo test --test synthetic_dat
cargo test -p fo1-tests
```

Three suites check the code against your real game files. They are slow and need `--ignored`.

```
export FO1_DATA=/path/to/your/fallout
cargo test --test install_archive -- --ignored --test-threads=1
cargo test --test install_plaintext -- --ignored --test-threads=1
cargo test --test install_exe_tables -- --ignored --test-threads=1
```

All six take about 25 minutes. Keep `--test-threads=1`. The archive tests hold hundreds of megabytes each.

To run every check that a change must pass, use `tools/gate.sh`. It writes a report to `target/gate/report.md`.

## How the code is organized

The workspace has 18 crates in layers. A crate may depend only on crates in a lower layer. `tools/layers.txt` holds the rule and `tools/check_layers.sh` enforces it.

| Layer | Crates |
|---|---|
| Foundation | fo1-core, fo1-path, fo1-bigendian, fo1-endian-host, fo1-lzss |
| Data | fo1-dat, fo1-exe, fo1-assets |
| Rules and world | fo1-world, fo1-combat, fo1-pathfind, fo1-script, fo1-worldmap |
| Output and storage | fo1-render, fo1-audio, fo1-save |
| Assembly | fo1-game, fo1-app |
| Test only | fo1-tests |

Only `fo1-app` touches the platform. Every other crate takes bytes in and returns values or errors. This keeps the rules testable and deterministic.

The asset files use big endian numbers. The tables inside the executable use little endian. Two separate crates read them so that one cannot be used for the other by mistake.

Read these in order if you want to change something.

1. The Implementation Guide in the repository root. It holds every decision and the evidence behind it.
2. `docs/spec/<crate>.md`. It defines each claim that the tests cite by id.
3. `tools/layers.txt`. It shows what may depend on what.
4. The matching file in `tests/gates/`. It shows what the crate must satisfy.

## Code rules

The root `Cargo.toml` forbids `unsafe`, `unwrap`, `expect`, `panic`, direct indexing and lossy casts in runtime crates. Readers return errors for bad input. They never truncate. The workspace has no third party dependencies, so keep it that way.

Tests and code have separate authors. Do not edit these paths in a change that also changes the code they grade: `tests/`, `crates/fo1-tests/`, `docs/spec/`, `spec/`, `tools/`, `.github/`, `clippy.toml`, `deny.toml` and `expected_counts.toml`. If a test looks wrong, open an issue with the evidence.

Every test names its claim id and its oracle class. An oracle is the source of the expected value. A test whose expected value comes from the code under test does not count.

The script `tools/check_public.sh` rejects absolute paths and private names in tracked files. Run it before each commit.

## Contribute

Send an email to cttkajejeka@gmail.com with the title `I WANT TO HELP IN RUSTY WASTELAND`. Say what you want to work on. Reply time varies.

The address is public. Use it only for this purpose.

## Do not post game data

Do not add game files, hashes of your files, text extracted from them or code from other engines to issues or pull requests. See `CONTRIBUTING.md`.

## Licence

MIT. See `LICENSE`. You may use, copy, modify and redistribute the code, including in a repository you own. The licence covers this code only. It grants no rights to Fallout or its data.
