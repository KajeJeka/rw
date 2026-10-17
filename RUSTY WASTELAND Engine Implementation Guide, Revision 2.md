# RUSTY WASTELAND: Engine Implementation Guide, Revision 2

Oct 16, 2026

This guide tells an engineering agent how to build RUSTY WASTELAND, a Rust engine that runs the data of Fallout 1 version 1.1 with the behaviour of the original program. It is written for a public repository, so it holds no game data, no third party source code and no private paths. Revision 2 merges the corrections found while planning the build and adds the engineering and verification rules.

## Purpose and scope

The target is the vanilla 1.1 executable with build stamp "Nov 11 1997 14:59:39". Fallout 1 Community Edition (CE) is a decompilation of this exact build. Use CE as a reading aid for behaviour. It is not a specification, because CE changed some behaviour and its licence limits what you may copy. See Publication rules.

The engine must:

- Read MASTER.DAT and CRITTER.DAT and every asset format inside them, bit exact.
- Run the 952 compiled .INT scripts with the original instruction semantics.
- Reproduce the verified rules: RNG, stats, damage, critical hits, XP and pathfinding.
- Draw the isometric map with the verified projection.
- Save and load games.

The engine must not:

- Ship any game data. The player supplies a legal copy.
- Contain code or text copied or translated from CE source.
- Inherit the original's defect classes: unbounded buffers, a path helper with no length parameter, unclamped config values and an unframed save stream.

### Decisions

The project owner can overturn any decision below. Until then the agent follows it.

| Topic | Decision | Reason |
| --- | --- | --- |
| Fidelity target | Vanilla falloutw.exe behaviour at 640 by 480. Widescreen is a later render option. | GOG ships the High Resolution Patch runtime. Nobody has disassembled it, so its behaviour is unknown. |
| Meaning of 1:1 | Same data, same rules, same script results. Not the same bugs. | The original's buffer defects share one root. Quirks that rules or scripts depend on stay. |
| Save format | Write a new self contained versioned format with a CRC32. Read only the original header. | No real original save has been parsed. The original has no integrity check. |
| Audio | Separate crate. The game runs silent until the ACM codec is solved. | The ACM container is solved. The codec is not. |
| Fallout 2 and CE features | Out of scope. | CE added Fallout 2 rules. A vanilla port must not inherit them. |
| Repository | Public on GitHub. It holds no game data, no CE source and no private paths. | See Publication rules. |
| Unsafe code | Forbidden in every crate. | No part of the design needs it. The LZSS ring buffer is plain indexed code. |
| Milestone closure | A milestone closes only when the gate script passes with the player's install present. | See Verification and anti faking. |

## Publication rules

The repository is public from its first commit. Git keeps everything ever committed, so a later delete does not undo a mistake. Check before every commit and never after.

### What may be committed

- Our own source code and tests.
- Specs written in our own words, each claim carrying an evidence tag.
- Facts about the formats: offsets, field layouts, counts, constants and addresses in the executable.
- Small test vectors that we computed ourselves, such as the RNG sequence and the LZSS token in this guide.
- Synthetic test inputs that the tests build in code.
- SHA-256 hashes and size manifests of the player's files, after the owner approves each manifest.

### What must never be committed

- Any file or fragment taken from the game: archives, art, sound, maps, scripts, message text, tables read from the executable, thumbnails and screenshots of the original.
- `falloutw.exe`, patch files and installers.
- CE source, CE comments, CE test data, a CE checkout, a CE submodule or a vendor directory holding it.
- Private research notes: raw analysis reports, disassembly listings, hex dumps of the executable and any note that quotes CE source.
- Local paths, user names, machine names and personal email addresses.
- Tokens and secrets.

Keep private material outside the working tree. Use one sibling directory for the player's install, set by `FO1_DATA`, and one for private references, set by `FO1_REF`. The references hold the CE checkout and the research notes. Neither sits inside the repository, so `git add` cannot reach them. Any plan that places `vendor/` or `analysis/` inside the repository must move them out before the first push.

### Clean room rules

Separate facts from expression. A fact is an offset, a field layout, a constant or the outcome of a rule. You may use facts. Expression is CE's code, comments, names and structure. You may not copy it.

- Write each spec in your own words from the binary, the real data and observed behaviour. Implement from the spec only. Keep CE closed while you write code, so the spec is the only bridge.
- Choose your own names for functions, types and modules. Use a format's field names only where the spec defines them.
- Do not port CE table initialisers, comments or message strings. Message text comes from the player's MSG files at run time.
- Tables that live in the executable come from the player's own copy at run time, or from a local cache under `target/`.
- Every spec file starts with a provenance line that names its sources: binary, game data, or CE for behaviour. It quotes nothing.

The reason is the licence. CE uses the Sustainable Use License 1.0. It is not an open source licence and it limits distribution of derivative work to free non commercial use. A project that copies CE could not carry an ordinary open source licence. This guide is not legal advice. The owner should take advice on the repository licence and on trademarks before the first push.

### Naming

The README says the project is unofficial, has no link to the rights holders and needs the player's own copy of the game. The repository holds no original art, logos, icons or audio. Issue and pull request templates tell contributors never to attach game files and to confirm that they copied no CE source.

### Hygiene checks

The script `tools/check_public.sh` runs inside the gate and in CI. It fails the build on any of these.

- A file with a game extension: `.dat`, `.exe`, `.frm`, `.fr0` to `.fr5`, `.pal`, `.pro`, `.map`, `.int`, `.acm`, `.mve`, `.aaf`, `.gam`, `.msg`, `.sav`. Synthetic fixtures live in code or in `.hex` text files.
- A file larger than 256 KB.
- A private path pattern: a drive letter path, `/Users/`, `/home/` or `Downloads`.
- A term from the owner's private list. The list lives outside the repository and the script reads it from `FO1_PRIVATE_TERMS`. It holds names, user names and machine names.
- An identifier from the CE identifier list. The owner generates that list locally from the CE checkout and keeps it in `FO1_REF`. A hit means CE naming leaked into our code.
- An integer literal array of more than 64 elements. It suggests a pasted table dump.
- A tracked file that `.gitignore` should have excluded.

Before the first push, run the same script over the whole history with `git log --all` and rewrite history if anything appears. After a push, treat any leak as permanent and rotate or remove the exposed item at its source.

Failure messages print offsets, lengths and hashes. They never print file contents beyond 16 bytes. CI runs only the synthetic tests and never holds game data.

## Source material and trust

The facts here come from analysis of the reference executable, the game data and a read of CE. The raw research notes stay private. This guide restates the findings in its own words. Rank every claim by its evidence tag and let byte evidence overrule prose.

The reference build has SHA-256 `444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272`. The build stamp "Nov 11 1997 14:59:39" sits at file offset 0xD270C.

When sources disagree, the higher source wins. The order is the original binary, then real game data, then CE, then the notes. Do not average two sources. Write down why you chose one and add the ruling to the table below.

| Tag | Meaning | How you use it |
| --- | --- | --- |
| CONFIRMED AGAINST ORIGINAL BINARY | Checked against falloutw.exe bytes or real game data | Implement it. Pin it with a test. |
| CONFIRMED IN COMMUNITY EDITION ONLY | CE source says so. Not checked against the binary. | Implement it behind a test. Queue a byte check. |
| VERSION DIFFERENCE IDENTIFIED | CE differs from vanilla | Follow vanilla. Add an anti deviation test. |
| UNVERIFIED | Not enough evidence | Do not guess. Isolate it and list it as open. |

Addresses in the notes drift from the bytes. One instruction sat 2 bytes from its stated address and one function was 17 bytes shorter than stated. Read the bytes before you trust an address.

### What is settled

- CE decompiles the same build as the target.
- The DAT container, the LZSS codec and the three payload modes.
- All 4,306 .PRO files parse with zero trailing bytes. All 7,524 .FRM frame regions and all 2,862 .FR0 to .FR5 frame regions walk exactly. All 72 .MAP files are version 19 with a 236 byte header.
- The script VM: 342 dispatch slots, 340 handlers, and 197 opcodes used by 669,318 decoded words in 950 scripts, with zero unexplained words.
- The projection basis (48, −12) and (32, +24), confirmed three ways.
- The RNG, the XP table, the stat limits table, the damage pipeline, the critical hit tables and the A star pathfinder.

### Corrections in Revision 2

Each row is confirmed against the original binary or real data. It overrides Revision 1 and anything older.

| ID | Revision 1 said | Correct rule |
| --- | --- | --- |
| C1 | Every integer is big endian. | Two byte order layers exist. The asset layer is big endian. The data tables inside falloutw.exe are little endian. Reading the XP table big endian returns 16,777,216 for level 1. See Asset formats. |
| C2 | `.PAL` files share one depth and the first bytes are unchecked. | Depth is per file. 8 bit: `color.pal`, `subtitle.pal`, `death.pal`, `helpscrn.pal`. 6 bit: `dp.pal` and 14 `seq*.pal` files. Detect it from the data. |
| C3 | `.FRM` version 4 appears in 4,927 files. | 7,523 of 7,524 files are version 4. One is version 3. |
| C4 | `.FR0` to `.FR5` frame regions do not walk. | All 2,862 walk with the same 12 byte frame record rule as `.FRM`. Only the mapping from extension to direction is open. |
| C5 | 28 known plaintext pairs were verified. | No such suite existed. A discovery script found 31 candidates and 3 have patched loose copies, which leaves 28. The suite must be built and must assert exactly 28 by name. |
| C6 | 1,950 `.MSG` files and 132,744 non blank lines. | The measurement over `TEXT\**` gives 1,935 files and 132,623 lines. Reconcile the directory scope before either pair becomes a test constant. |

These items were checked again and stay as written. Do not change them.

- The floor flag is bit 12, not bit 15. The bytes at 0x49F518 are `81 E2 00 F0 00 00` (`and edx, 0xF000`) then `C1 EA 0C` (`shr edx, 12`). The roof flag is bit 28.
- The map header is 236 bytes and every map is version 19.
- The `.FRM` header is 62 bytes (0x3E) and `0x3E` plus the frame region size equals the file size for 7,524 of 7,524 files.
- `GRID000.FRM` is 80 by 36 with a frame size of 2,880.
- `SWAP` and `SWAPA` exchange the top two entries.
- The age divisor is 0x12CC0300, found as `mov ecx, 0x12CC0300` at 0x49C578.

### Rulings where sources disagreed

Each ruling ends in a test, so the agent can overturn it with data.

| Topic | What earlier notes said | Ruling |
| --- | --- | --- |
| LZSS decoder works | One analysis decoded all 25,243 entries to exact size. Another said a block length defect remained and a third could not reproduce the result. | Trust the first. Prototypes and sprites parsed to exact consumption through that decoder, which a wrong decode cannot do. The failures came from an older Python tool. A decoder that used one control byte per token contradicts the first. The gate in Build order proves it. |
| SWAP and SWAPA | CE's versions do nothing. | CE pops the top value, then the second, then pushes the top value and then the second. The two entries trade places. The real epilogue needs this. See Script virtual machine. |
| Tile gate bits | Bit 15 gates the floor and bit 31 the roof. | The instructions test mask 0x1000, which is bit 12. The roof equivalent is bit 28. Decode real maps to settle the roof direction. |
| Save location | The path is relative to the working directory, or it resolves through the database layer. | Keep the logical path `SAVEGAME\SLOTnn\SAVE.DAT`. Make the save root configurable. Default to `<game>/DATA/SAVEGAME`. |
| Version triple in saves | Written and never read, or read and never compared. | Store it. Never branch on it. Offsets 24, 26 and 28. |
| Save header size | 30,051 is exact, or it is a sum of CE field sizes. | The constant appears nowhere in code. Parse by field. Do not hard code the total until a real save exists. |
| Map header size | 240 or 236. | 236 bytes (0xEC). Five maps closed exactly with it. |
| Pathfinder constants | A backtrack cap of 800, or no such cap. | Use 2,000 nodes, a step cost of 50 and a turn cost of 10. The 800 cap exists only in CE source. |
| Viewport | 480 by 400, or 640 by 380. | Those two numbers are scroll limits. Use a 640 by 480 logical surface with a 640 by 380 map view until the binary says otherwise. |
| FRM header | A 48 byte table at 0x04, or named fields. | Use the named fields in Asset formats. They add up to 62 bytes. |
| `.LIP` layout | CE's reader matches the shipped files. | It diverges at offset 0x14. Do not port CE's reader. It is an open item. |

### Older claims to discard

| Older claim | Use instead |
| --- | --- |
| 0x413044 is the damage formula | It computes explosion damage only. Weapon damage is at 0x422118. |
| Dividing by 10 scales damage | It is the knockback distance. |
| No critical hits exist | A critical is a multiplier from tables at 0x4FECB4 and 0x50454C. |
| The pathfinder was not found | A star runs at 0x4159E8. The old search looked for symbol names in a stripped binary. |
| Stats live at object+0x24 | Base stats sit at proto+0x24 in memory. Objects hold only the pid. |
| Saves have no version field | A version triple exists. |
| SAVE.DAT holds a map dump | The 29,792 bytes are a 224 by 133 thumbnail. Maps live in the `MAPS\*.SAV` files. SAVE.DAT alone is not a loadable original save. |
| Bit 28 gates objects | No tile bit gates objects. Objects come from per tile lists. |
| `.INT` files are little endian | They are big endian. |
| `.EDG` files exist | None exist. |
| Tree order is physical order | MASTER.DAT has 15 descents. Sort by offset before any tiling check. |
| XP is L(L−1)/2 | It is L(L+1)/2 times 1000, minus one at level 21. |
| NumPathNodes is an engine setting | 2,000 is hard coded. The key belongs to the High Resolution Patch. |

## Architecture

The engine is a Rust workspace of 16 crates and one test harness. Each crate owns one subsystem and one test surface.

&#91;embedded content: workspace layers · 16 crates and a test harness\]

Dependencies point down. A crate never uses a crate in a higher band. `tools/check_layers.sh` reads `cargo metadata` and fails the gate on a violation.

- Only `fo1-app` touches the window, the audio device, the clock and the file system roots. Every other crate takes a byte slice or a trait object.
- Two endian crates exist because a wrong endian read fails silently. `fo1-bigendian` serves the asset layer. `fo1-endian-host` serves the tables inside the executable. Every integer read in the other crates goes through one of them.
- `fo1-assets` reads prototypes field by field in the on disk order. Do not read them by casting bytes to a `#[repr(C)]` struct, because the disk order differs from the memory order.
- `fo1-path` builds paths in fixed capacity buffers and returns an error on overflow.
- `fo1-pathfind` and `fo1-worldmap` stay apart. The local A star search and the world map walkmask share nothing.
- `fo1-audio` is isolated so the codec blocker stays visible and the rest of the game runs silent.
- `fo1-save` writes the port format and reads the original header. It embeds map state in one self contained file and does not write `MAPS\*.SAV` beside `SAVE.DAT`.
- `fo1-game` owns the loop. The order of one tick is not specified. Two parts are known. Each program runs a burst of 10 script instructions per tick, and a move replays one step per tick. Derive the rest from CE's main loop behaviour and write the spec before milestone M8.
- `fo1-tests` holds the `FO1_DATA` harness and the test author's independent reference decoders. It is a dev dependency only and no runtime crate depends on it. Its path is protected.
- Fallout 2 and CE additions stay out of every crate.

## Engineering philosophy

Code quality here has one meaning. The engine does what the original does, and a reader can see why. Every principle below has an automated check in the table after it. A rule with no check is advice, and an agent under pressure drops advice.

1. **Fidelity over taste.** The spec decides behaviour. Do not improve a rule, rename a quirk away or fix a defect that the rules depend on. Every difference from the original goes on the deviation list and needs an owner decision.
2. **Spec, then tests, then code.** Each subsystem has `docs/spec/<subsystem>.md`. Each claim has an id such as `DAT-007` and an evidence tag. A claim with no tag counts as UNVERIFIED.
3. **Total readers.** A function that parses bytes returns `Result`. It never panics, never indexes past the end, never allocates more than the remaining input can fill and bounds its recursion depth.
4. **Types carry meaning.** Use newtypes for pids, frame ids, hex indexes, square indexes, elevations, stat ids and ticks. The two byte order readers have no default order. Match closed sets exhaustively with no wildcard arm, so a new variant forces a decision.
5. **Explicit arithmetic.** Rules use `i32` and named operations. Where the original wraps, call `wrapping_*`. Elsewhere use checked operations and keep overflow checks on. Do not use `as` to change width or sign. Division truncates toward zero as in the original. Floor division appears only where the spec asks for it.
6. **Determinism.** The same inputs give the same outputs. Rules code takes the RNG, the data and the time as parameters. It reads no wall clock, starts no thread, holds no global state and never depends on hash map order.
7. **No unsafe code.** Every crate forbids it.
8. **Few dependencies.** The foundation and data crates use the standard library only. Any other crate needs a line in `docs/deps.md` with its licence and the reason, and must pass the licence check.
9. **Plain code.** Functions are short and do one job. Public items carry a doc comment that names the spec claim. Comments say why, not what. Add an abstraction at the second use, not before. Optimise only after a measurement.
10. **Errors that help.** Each crate has one error enum. Variants carry the byte offset and the field name. They never carry file content.
11. **Own words and names.** Specs, comments and identifiers are written by us. See Publication rules.

### How each principle is checked

| Principle | Check | Command |
| --- | --- | --- |
| Formatting | rustfmt | `cargo fmt --all --check` |
| Lints | Clippy with the workspace lint table. Any warning fails. | `cargo clippy --workspace --all-targets -- -D warnings` |
| No unsafe | `unsafe_code = "forbid"` stops the compile | `cargo build --workspace` |
| Total readers | Deny `unwrap_used`, `expect_used`, `panic` and `indexing_slicing` outside tests. Bit flip tests and fuzz targets for every reader. | Clippy, `cargo test`, `cargo fuzz run <target>` |
| Explicit arithmetic | Deny the three cast lints. Deny `arithmetic_side_effects` in the rules and data crates. Overflow checks stay on in release builds. | Clippy, the Cargo profile |
| Documentation | `missing_docs` and rustdoc warnings fail | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` |
| Dependencies | Licence, ban and advisory checks. The lock file is committed. | `cargo deny check` |
| Layering | A script reads `cargo metadata` and fails when a crate depends on a higher band | `tools/check_layers.sh` |
| Determinism | A script bans `SystemTime`, `Instant`, `thread::spawn`, `HashMap` and `HashSet` in the rules crates | `tools/lint_rules.sh` |
| Spec first | Every test cites a claim id and every claim id has a test | `tools/check_trace.sh` |
| Public repository | The hygiene script from Publication rules | `tools/check_public.sh` |

The workspace `Cargo.toml` carries the lint table. Each crate opts in with `[lints] workspace = true`.

```toml
[workspace.lints.rust]
unsafe_code = "forbid"
missing_docs = "warn"
unused_must_use = "deny"

[workspace.lints.clippy]
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
indexing_slicing = "deny"
todo = "deny"
unimplemented = "deny"
dbg_macro = "deny"
print_stdout = "deny"
print_stderr = "deny"
cast_possible_truncation = "deny"
cast_sign_loss = "deny"
cast_possible_wrap = "deny"
allow_attributes_without_reason = "deny"

[profile.release]
overflow-checks = true
```

The file `clippy.toml` relaxes the panic lints inside tests only.

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
allow-indexing-slicing-in-tests = true
allow-panic-in-tests = true
```

The crates `fo1-dat`, `fo1-script`, `fo1-combat` and `fo1-save` also put `#![deny(clippy::arithmetic_side_effects)]` at the top of their `lib.rs`, because a crate that opts into the workspace table cannot add lints of its own. The lint table and `clippy.toml` are protected files. See Verification and anti faking.

## Verification and anti faking

An agent told to make tests pass takes the cheapest path. The cheapest path is often to change the test, the data or the report. This section removes those options. It rests on one rule: the party that writes the code never controls the proof.

### How an agent fakes a pass

| Move | Example in this project | Control |
| --- | --- | --- |
| Weaken the assertion | `matched >= 25` where the exact count is 28 | Exact assertions |
| Skip without a word | `continue` on a mismatch, `#[ignore]`, a test that returns early when `FO1_DATA` is unset | Exact assertions, the gate counts |
| Fit the code to the test | `if input == known_case { return expected }` | Hold out vectors, mutation testing |
| Grade with the code under test | The expected bytes come from the same decoder | Independent oracles |
| Truncate or pad to the gate | A decoder that stops at the declared size passes a size check by construction | No truncation, consumption check |
| Fabricate a fixture | A synthetic file described as original data | Fixture rules |
| Edit the gate | Change the lint table, an expected count or the CI file | Protected paths |
| Regenerate the golden file | An update switch that rewrites the expected output | Fixture rules |
| Branch on test mode | `cfg(test)` or an environment check in production code | Banned patterns |
| Misreport | "All tests pass" with no run, paraphrased output, an ignored test counted as done | Completion report |

### Separate the roles

Three roles exist. The spec author writes `docs/spec`. The test author writes the gate tests from the spec and its vectors. The implementer writes the code. Each role is a separate agent session with its own context. The test author never sees the implementation. The implementer never edits a gate test.

- The implementer may add new tests under `src/`. It may not edit or delete an existing test. Gate tests live in `tests/gates/` and belong to the test author.
- A reviewer session sees only the diff and the spec. Its job is to write a wrong implementation that still passes the suite. If it succeeds, the suite is too weak.
- These paths are protected: `tests/`, `crates/fo1-tests/`, `docs/spec/`, `spec/`, `tools/`, `.github/`, `clippy.toml`, `deny.toml`, the lint table in the root `Cargo.toml`, `expected_counts.toml` and `mutants-allow.md`. A change to a protected path needs the owner's approval.
- Enforce this with CODEOWNERS and branch protection. The agent has no push rights to `main`. CI runs the gate from the base branch, never from the pull request branch.
- An agent that thinks a protected file is wrong stops. It writes the evidence (bytes, offsets, the spec claim) and asks the owner.

### Independent oracles

Every expected value comes from outside the code under test.

| Class | Source | Example |
| --- | --- | --- |
| 1 | Bytes of the original executable, read by a separate extraction tool | The XP table, the stat limits, the adjacency proofs |
| 2 | Closure on real data | Exact decoded size, exact consumption, zero trailing bytes, a frame walk that ends at the file size |
| 3 | Known plaintext | 28 loose files that equal their archive entries |
| 4 | An independent reference implementation | A simple decoder written by the test author, never imported by production code |
| 5 | Hand computed vectors in this guide | The RNG sequences, the LZSS token `4E 36`, the explosion damage table |
| 6 | Algebraic laws | Projection and its inverse, round trips, table adjacency |

These are not oracles: the output of the code under test, a snapshot of that output, CE's output, and a value copied from a failure message. A claim tagged CONFIRMED AGAINST ORIGINAL BINARY needs a test in class 1, 2 or 3.

### Exact assertions

- A count is compared with `assert_eq!` against a named constant from `spec/constants.toml`. Never use `>=` or "at least".
- A set of named cases is compared as a set. The test lists the expected names, so a missing case fails.
- A loop over cases has no `continue`, no early `return` and no `if let Ok`. A case passes or the test fails and prints the case name.
- Missing data fails. A gate test that needs `FO1_DATA` and cannot find it panics with a clear message. The `#[ignore]` marker only keeps the default `cargo test` run fast. The full gate runs the ignored tests.
- Every test compares a value from the code under test with an independent value.

```rust
// Weak: passes with 25 matches and skips a mismatch without a word.
for pair in &pairs {
    if pair.loose.is_none() { continue; }
    check(pair);
}
assert!(matched >= 25);

// Strong: the exact set, by name, with no skip path.
const PLAINTEXT_PAIRS: usize = 28;
let found = known_pairs(&install, &loose_dir);
assert_eq!(found.names(), expected_pair_names());
assert_eq!(found.len(), PLAINTEXT_PAIRS);
for pair in &found {
    assert_eq!(decode(pair), pair.loose_bytes, "{}", pair.name);
}
```

### No truncation, and a consumption check

A decoder receives the declared output size only to detect overrun. It returns an error when the output would exceed that size or when the input ends early. It never cuts, pads or defaults to reach a length. A decoder that stops at the declared size passes a size gate for any input.

The archive gate asserts both `produced == declared` and `consumed == compressed` for all 25,243 entries. For a block chain it asserts that the block headers account for the compressed size exactly. A stream one byte short and a stream one byte long must each return an error.

### Hold out vectors and canaries

- **Hold out vectors.** The owner keeps extra vectors for each milestone in `FO1_REF/holdout/`: more RNG seeds, more LZSS streams, more damage cases. The gate runs them on the owner's machine only. The implementer sees a pass or fail and a case count, so it cannot special case them.
- **Canaries.** For each fixed constant, a test proves that the suite rejects the wrong value. The test author's reference decoder takes its ring parameters as arguments. The canary runs it with a write index of 4095 and with a prefill of 0x00 and asserts that the known plaintext pairs no longer match. If a wrong variant also passes, the tests are too weak.

### Mutation testing

`cargo mutants` changes the code in small ways: it flips a comparison, swaps a constant or returns a default. A sound suite fails on every change. A mutant that survives marks a gap in the proof or dead code.

- Run it on `fo1-dat`, `fo1-world`, `fo1-combat`, `fo1-script` and the `fo1-save` readers with `FO1_DATA` set, so the data tests count.
- A surviving mutant fails the milestone unless `mutants-allow.md` lists it with a reason and the owner approved it.
- Run the full job on the owner's machine before a milestone closes. Run `--in-diff` on pull requests for crates that need no game data.

### Fail before, pass after

The gate copies the test files from the head commit onto the base tree and runs them. A new or changed test must fail on the base, and a compile error counts as a failure. It must pass on the head. A test that already passes on the base proves nothing about the change. It needs a written note that explains why, and the owner reviews the note. A pure refactor that changes no test is exempt.

### Constants ledger and traceability

- `spec/constants.toml` holds every number the spec fixes: its id, value, evidence tag and claim id. A test compares each public constant in the code with the ledger.
- `docs/spec/trace.md` maps each claim id to its tests and the oracle class of each test. `tools/check_trace.sh` fails when a test has no claim id, when a CONFIRMED claim has no test in oracle class 1, 2 or 3, or when the number of tests for a claim has dropped.
- Both files are protected. The ledger changes only with new evidence and an owner decision.

### Banned patterns

`tools/lint_tests.sh` searches the source and fails on any of these.

In tests:

- `assert!(true)` and any test function with no assertion and no `?`.
- A comparison operator other than equality in an assertion about a count that the ledger fixes.
- `#[ignore]` without a `reason` that names the missing fixture.
- `continue` or an early `return` inside a loop over cases.
- `.ok()`, `unwrap_or`, `unwrap_or_default`, `if let Ok` and `let Ok(..) = .. else { continue }` applied to a result under test.
- `catch_unwind`, and `Command::new` that runs the game or a patcher.
- A read of `FO1_DATA` outside the `fo1-tests` harness.

In production code:

- `cfg(test)` outside a test module, and `cfg(debug_assertions)` that changes a result.
- Any read of `FO1_DATA` or `FO1_REF`.
- `#[allow(` and `#![allow(` outside a list that the owner approved.
- `std::process::exit` in a library crate.

### Fixtures and golden files

- Golden data comes only from the original: bytes of the player's install, or a capture of the original game that the owner made.
- Our own output may be stored as a snapshot labelled `regression_only`. It detects change. It never counts as proof and never closes a gate.
- No update switch exists. Code never writes to a golden or snapshot path. Changing one needs the owner's approval and a recorded reason.
- Synthetic fixtures are built in code and named `synthetic_*`. A test on synthetic data cannot support a CONFIRMED tag. A synthetic file is never called original data.
- Gate tests belong to one reference install. The gate checks the executable's SHA-256 and the sizes of the two archives, 339,379,746 bytes for MASTER.DAT and 157,085,138 bytes for CRITTER.DAT. On any other install it stops with `unsupported install` and does not relax a constant.

### The gate script

`tools/gate.sh` is the single entry point. CI runs its synthetic steps. The owner runs every step. It runs these in order and stops at the first failure.

1. Protected paths against the base branch.
2. Public hygiene.
3. Format, Clippy, rustdoc, layering, determinism lint, banned pattern lint and `cargo deny`.
4. `cargo test --workspace` on synthetic data.
5. Install check: `FO1_DATA` is set and the fingerprint matches.
6. `cargo test --workspace -- --ignored` against the install.
7. Count check against `expected_counts.toml`: tests run, ignored tests run and cases per gate test. A lower number fails. A crate marked as tested that runs zero tests fails.
8. Fail before, pass after.
9. Traceability.
10. Mutation testing, before a milestone closes.

The script writes `target/gate/report.md` with the commit hash, tool versions, the count of every step, and any step that was skipped. A skipped required step fails the full gate. The counts in `expected_counts.toml` can only rise without the owner's approval. The report ends with its own SHA-256.

### When a test fails

Change the code. These moves are never allowed to turn a test green:

- Weakening an assertion, widening a tolerance or changing an expected value without a new oracle.
- Deleting, ignoring or renaming a test.
- A special case keyed to an input that appears in a test vector.
- Catching, swallowing or defaulting an error.
- Clamping, padding or truncating output to the expected length.
- Reading the expected value from the code under test or from CE.
- Regenerating a golden file or snapshot.
- Adding a test mode branch to production code.
- Editing a protected path.

An agent that believes the test is wrong stops. It reports the bytes, the offsets and the spec claim, and it waits for the owner.

### Completion report

Every finished task ends with a report. A claim of success without it does not count.

- The commit hash and the path and SHA-256 of `target/gate/report.md`.
- The last lines of the gate output, copied exactly.
- Tests added, tests changed and tests removed. Removed must be zero without approval.
- Every ignored test and its reason.
- Claims touched and their new tags.
- Deviations from the spec.
- What was not verified.
- Open questions.

Never write "all tests pass" without the report. Never paraphrase a failure. Never mark a milestone done while a gate step is skipped or a required test is ignored. A statement that the agent ran the code means the report exists.

### Review checklist

The owner or the reviewer session checks each pull request against this list.

1. Did a protected path change?
2. Did a test disappear or an assertion get weaker?
3. Does each new test cite a claim and an independent oracle?
4. Does any code branch on a literal that appears in a test vector?
5. Does any decoder or reader cut, pad or default to meet a length?
6. Does the completion report match the gate report?
7. Did the hygiene script pass?
8. Is every new deviation listed?

## Asset formats and loading

Two byte order layers exist. Keep them apart.

- **Asset layer.** The DAT container, `.PRO`, `.FRM`, `.MAP`, `.GCD`, `.AAF` and `.INT` are big endian.
- **Executable layer.** The data tables inside `falloutw.exe` are little endian. The stat limits, the critical hit tables and the XP table are in this layer.

A wrong read returns plausible numbers and raises no error. Reading the XP table big endian gives 16,777,216 for level 1. Build two reader types, one in `fo1-bigendian` and one in `fo1-endian-host`. Each takes no default order, and no other crate reads an integer by any other route.

### Byte order exceptions

| Format | Field | Order |
| --- | --- | --- |
| Tables in `falloutw.exe` | Every i32 | Little endian |
| `.ACM` | Whole header | Little endian |
| `.MVE` | Every field after the 18 byte magic | Little endian |
| `.LIP` | Trailing marker table | Little endian (seen in the bytes, not confirmed) |
| Save files | Skill use slots and world map flags | Raw x86 memory bytes (see Saves) |

### DAT archives

The game ships two archives. The counts below come from the GOG copy. Use them as test constants for that install.

| Archive | Size in bytes | Directories | Entries |
| --- | --- | --- | --- |
| MASTER.DAT | 339,379,746 | 65 | 19,784 |
| CRITTER.DAT | 157,085,138 | 1 | 5,459 |

The layout is a two level tree.

- The file opens with a 16 byte header of four u32 values. They are the directory count, a capacity, a size field that is 0 at the root, and a dead pointer. Ignore the capacity and the pointer.
- The directory names follow. Each is one length byte and that many characters.
- One block per directory follows, in the same order. A block has the same 16 byte header with the size field set to 16. Its entries come next.
- An entry is a length byte, the name, and four u32 values: flags, absolute offset, uncompressed size and compressed size.
- Nothing separates the blocks. Paths use backslashes. Uppercase a path before you look it up.

Two rules protect the reader.

- Tree order is not physical order. In MASTER.DAT the first tree entry has offset 211,060,669 and the smallest offset is 556,484. Never infer an offset from an entry's position.
- The original engine reads loose files from a patch folder before it reads the archives. Make that a configured search path and leave it off by default. The test install has a loose DATA folder that the high resolution patch changed, so known plaintext tests must read the archive.

The payload mode is the high nibble of the flags (`flags & 0xF0`). A flags value of 0 means 0x10.

| Mode | Payload | Entries in the data |
| --- | --- | --- |
| 0x10 | One LZSS stream. Its input length is the compressed size. | 0 |
| 0x20 | Stored bytes. The length is the uncompressed size. | 4,787, all in MASTER.DAT |
| 0x40 | Block chain | 20,456 |

Every raw entry has a compressed size of 0. Every chain entry has a nonzero one. Test for that split.

A block chain is a series of blocks. Each block starts with a big endian u16 header. If bit 15 is set, the block is raw and holds `header & 0x7FFF` bytes. Otherwise it is an LZSS stream and the header gives its compressed byte count. Repeat until the output reaches the entry's uncompressed size. Every LZSS block starts with a fresh ring buffer. The data holds 67,663 blocks, of which 61,070 are LZSS and 6,593 are raw. Raw blocks sit inside chain entries, so one chain can mix both kinds.

### LZSS decoder

- Keep a 4096 byte ring. Fill indices 0 to 4077 with 0x20 and the rest with 0. Set the write index to 4078.
- Read a control byte. Process its bits from bit 0 to bit 7. Each bit is one token.
- A set bit is a literal. Copy one input byte to the output and to the ring at the write index, then advance the write index.
- A clear bit is a back reference of two bytes, b1 then b2. The source index is `b1 | ((b2 & 0xF0) << 4)`. The length is `(b2 & 0x0F) + 3`, so 3 to 18. Copy one byte at a time starting at the source index and wrapping at 4096. Write each byte to the ring at the write index and advance it. Each read happens before its write, so a run that overlaps its own output replicates.
- The source index is an absolute ring position, not a distance. Output position p lives at ring index `(p + 4078) mod 4096`.
- Stop when the block's input is used up, even in the middle of a control byte. Do not stop on output length.
- Never truncate. When the tokens would write past the declared output size, return an error. When the input ends before a chain reaches its declared size, return an error. The decoder uses the declared size to detect a mismatch and never to cut the output.
- If a back reference starts with one input byte left, return an error. The original underflows its length counter and loops.

This worked example was checked with a reference decoder written for this guide.

| Step | Value |
| --- | --- |
| Output position | 5000 |
| Write index | (5000 + 4078) mod 4096 = 886 |
| Bytes to repeat | 9 bytes from position 4960, which is 40 back |
| Ring index of the source | (4960 + 4078) mod 4096 = 846 = 0x34E |
| Byte 1 | 0x4E, the low 8 bits of 846 |
| Byte 2 | 0x36, which is the high 4 bits of 846 (3) followed by the length minus 3 (6) |

On disk the token is `4E 36`. An older note printed 882 and `72 36` for this case. It applied the 18 byte skew with the wrong sign, and `72 36` at that position copies from 4 bytes back. Pin two more cases in tests. The stream `00 00 0F` decodes to 18 spaces. The token `75 3F` written when the write index is 886 repeats the previous byte 18 times.

Write index starts of 0, 4078 and 4095 have all been proposed. Use 4078. It equals the ring size minus the 18 byte lookahead of standard LZSS, CE's source reads the same, and 28 known plaintext pairs decode byte for byte with it. Allocate 4096 bytes. The original declares 4116 and never reaches the last 20.

### Archive gate

The gate for the archive layer asserts all of the following.

- All 25,243 entries decode to their declared uncompressed size, consume exactly their compressed size, and for chains the block headers add up to the compressed size.
- MASTER.DAT has 4,787 raw entries and 20,456 chain entries. The data holds 67,663 blocks.
- The known plaintext set is exact. A path and size match against the loose DATA folder gives 31 candidates, and the test lists all 31 by name. Three of them have a patched loose copy: `ART\TILES\GRID000.FRM`, `ART\INTRFACE\MAINMENU.FRM` and `TEXT\ENGLISH\GAME\PROTO.MSG`. The test asserts that these three differ from the archive first at byte 117, 14,552 and 3,385. The other 28 must match byte for byte.
- Three more loose files differ in size from their archive entries, so the size match drops them: `GABRIEL.INT` is 26 bytes larger, `KILLIAN.INT` is 14 bytes smaller and `RAZOR.MSG` is 25 bytes smaller. The test lists them as known exclusions.
- A stream one byte short and a stream one byte long return errors.
- An older Python decoder ran a few hundred bytes short on large maps. The decoder above does not. Treat any short output as a decoder bug.

### Sprites (FRM)

A `.FRM` file has a 62 byte header (0x3E) followed by frame records. The header is big endian.

| Offset | Type | Field |
| --- | --- | --- |
| 0x00 | u32 | Version. 4 in 7,523 files and 3 in one file, `ART\INVEN\OKNIFE.FRM`. |
| 0x04 | u16 | Frames per second |
| 0x06 | u16 | Action frame |
| 0x08 | u16 | Frames per direction |
| 0x0A | i16 x 6 | X offset per direction |
| 0x16 | i16 x 6 | Y offset per direction |
| 0x22 | u32 x 6 | Data offset per direction |
| 0x3A | u32 | Size of the frame region. The file size is 0x3E plus this value. |

Each frame record has a 12 byte header: u16 width, u16 height, u32 size, i16 x, i16 y. Then come `size` bytes of 8 bit palette indices. Size always equals width times height. Fallout 1 art has six directions. The frame region walks exactly in 7,524 of 7,524 `.FRM` files and holds 119,163 frames. Of those, 111,317 are in `ART\CRITTERS`.

Pin `ART\TILES\GRID000.FRM` at 80 by 36 pixels with a frame size of 2,880. It is the check on the isometric projection.

The 2,862 files with extensions `.FR0` to `.FR5` use the same header and the same walk, and every one walks exactly. Nothing about their layout is open. What is open is the mapping from an extension to a direction index. Specify it from CE's art name builder, then test it on one real critter. Measure the frame count of these files and record it in the constants ledger.

A frame id (FID) packs the art type, an id, an animation code, a weapon code and a direction. The proto values seen so far fit a layout with the type in bits 24 to 27, the id in bits 0 to 11, the animation in bits 16 to 23, the weapon in bits 12 to 15 and the direction in bits 28 to 30. Confirm the last three fields against CE's `art_id` behaviour before you rely on them. Art files are found through the list file in each art folder, for example `ART\WALLS\NEW.LST`. Lines end in CRLF and may carry trailing spaces.

### Prototypes (PRO)

A `.PRO` file has no magic and no version. The type is the top byte of the pid. The low 24 bits select a line in the type's `.LST` file, counting from 1. Confirm the first line against `PROTO\ITEMS\ITEMS.LST`.

| Type | Top byte | Files | Size on disk |
| --- | --- | --- | --- |
| Item | 0x00 | 242 | 61 to 129 bytes by subtype |
| Critter | 0x01 | 312 | 412 bytes |
| Scenery | 0x02 | 908 | 45 bytes, or 49 for doors, stairs and elevators |
| Wall | 0x03 | 1,176 | 36 bytes |
| Tile | 0x04 | 1,622 | 28 bytes |
| Misc | 0x05 | 46 | 28 bytes |

Item sizes by subtype are key 61, container 65, misc 69, ammo 81, weapon 122, drug 125 and armor 129. The data holds 6, 77, 77, 16, 43, 14 and 9 of them in that order.

The spec author writes the full field tables in `docs/spec/pro.md` from the data and the binary before any PRO code exists. All 4,306 files parse with zero bytes left over, and the spec must reproduce that. The on disk field order differs from the in memory struct order. For example, the item subtype data sits at +0x24 in memory and comes last on disk. Do not read a prototype by casting bytes to a `#[repr(C)]` struct, because the cast returns garbage and raises no error. Read each field in sequence into a typed value.

Two values in real files are leftover memory, not data. A scenery key code of 0xCCCCCCCC is the compiler's uninitialised fill. Treat it as no key.

### Maps (MAP)

There are 72 maps. A map file is big endian and starts with a 236 byte (0xEC) header. An earlier note said 240. That was an arithmetic slip, because every map parsed with 236 and the byte accounting closed exactly.

| Offset | Field | Notes |
| --- | --- | --- |
| 0x00 | version | 19 in every map. Reject any other value. |
| 0x04 | name | 16 bytes, NUL padded, matches the file name |
| 0x14 | entering tile | Hex tile, 0 to 39,999. 20,100 is the default. |
| 0x18 | entering elevation | 0 to 2 |
| 0x1C | entering rotation | 0 to 5 |
| 0x20 | local variable count | Clamp to 0 or more |
| 0x24 | script index | 1 based. Above 0 means a map script exists. |
| 0x28 | flags | See below |
| 0x2C | darkness | Ambient light level |
| 0x30 | global variable count | Clamp to 0 or more |
| 0x34 | map number | Used for town lookup |
| 0x38 | last visit time | Game ticks |
| 0x3C | reserved | 44 i32 values, ends at 0xEC |

The body follows in this order.

- The global variable values, then the local variable values, as i32 each.
- One 100 by 100 grid of u32 tile values per stored elevation, 40,000 bytes each.
- The scripts section.
- An object count, three per elevation object counts, then the object records.

In `flags`, bits 1, 2 and 3 (0x02, 0x04, 0x08) mean elevation 0, 1 or 2 is empty and not stored. Outdoor maps use 0x0C and store only elevation 0. Bit 0 means the map was saved mid game. It suppresses ammo and combat fixups and the `.GAM` reload. The check for every map is that the offset after the grids equals `0xEC + 4 * (globals + locals) + 40000 * stored elevations`. VAULT13 gives 120,260 and HUBENT gives 40,248.

Each tile value is a u32. The low half is the floor and the high half is the roof.

| Bits | Mask | Meaning |
| --- | --- | --- |
| 0 to 11 | 0x00000FFF | Floor tile id. The value 1 is the empty tile. |
| 12 | 0x00001000 | Floor flag. When set, the floor is not drawn. Binary confirmed. |
| 13 to 15 | 0x0000E000 | Floor flags with no known use |
| 16 to 27 | 0x0FFF0000 | Roof tile id |
| 28 | 0x10000000 | Roof flag read through the same nibble test. CE's roof fill code toggles it. |
| 29 to 31 | 0xE0000000 | Roof flags with no known use |

The instructions at 0x49F518 are `and 0xF000`, `shr 12`, `test 1`, which test bit 12. Confirm the direction of the roof test in the roof drawing function at 0x49EBDC before you draw roofs. Bit 28 is not an object gate.

The scripts and objects sections need the object record layout and the script loader behaviour. The spec author writes both from the binary and CE's behaviour, then validates them against all 72 maps. The shape is 18 i32 values per object plus prototype update data. With the decoder fixed, this also gives the NPC counts that the earlier analysis could not read.

### Palettes, fonts and text

- `.PAL`: 19 files of 33,536 bytes each, no header. The first 768 bytes are 256 RGB triplets, which is all CE reads for the live palette. The depth differs by file. Four files hold 8 bit values with a maximum of 255: `color.pal`, `subtitle.pal`, `death.pal` and `helpscrn.pal`. Fifteen hold 6 bit values with a maximum of 63: `dp.pal` and 14 `seq*.pal` files. `color.pal` begins `FF FF FF 3B 3B 3B` and `seq2ad.pal` begins `00 00 3F 00 00 00 3F 39 21`. Decide the depth from the maximum over all 768 bytes of each file and never from the first triplet. The other 32,768 bytes equal 2 to the power of 15. They are probably a lookup table from 15 bit colour to a palette index. That is a guess.
- `.AAF` fonts: 5 files. The magic is `AAFF`. The header is 2,060 bytes: max height, letter spacing, word spacing and line spacing as u16, then 256 glyph records of u16 width, u16 height and u32 offset. Glyph bitmaps follow. The 5 `.FON` files use a different, shorter layout. Ignore them.
- `.MSG`: plain CRLF text. Lines starting with `#` are comments. Records have three brace fields: `{id}{audio}{text}`. The count over `TEXT\**` is 1,935 files with 132,623 non blank lines. An earlier count over a wider scope gave 1,950 files and 132,744 lines. Reconcile the directory scope before either pair becomes a test constant.
- `.GAM`: 36 files of plain text. Find the first line whose first 16 characters equal the section name (`GAME_GLOBAL_VARS:` or `MAP_GLOBAL_VARS:`). Then read to the end. Skip blank lines and lines starting with `//`. Cut each line at the first `;`. A value after `=` is read as an integer. A line without `=` has the value 0. Every surviving line is one variable in file order. A stray comment line becomes a variable. Reproduce that.
- `.GCD`: 6 files of 428 bytes, which is 107 big endian i32 values and no header. The field map is open.
- `.BIO`: 9 files of CRLF text in three locales.
- `.LST` files list names. `.EDG` files do not exist in the data.

### Audio and movies

The `.ACM` header is 14 bytes and little endian. The magic is `97 28 03 01`. Then come the sample count (u32), the channel count (u16), the sample rate (u16, 22,050 in all 1,746 raw files) and one u16 that holds two fields. The low 4 bits are the levels and the high 12 bits are the rows. The values 263 and 264 in the data give levels 7 and 8 with 16 rows. Public format notes describe this layout (the multimedia.cx wiki and the fodev.net ACM page).

Those notes describe ACM as a block transform codec read LSB first. It is not ADPCM. An earlier note concluded that it is a 4 bit codec with an 8 bit reference. That conclusion rests on a size ratio of about 2, which also fits a transform codec that halves the size of 16 bit samples. Treat it as unsupported.

CE contains no ACM decoder. It downloads `adecode` at build time. Write the decoder from the public format description. Do not copy code from `adecode` or from libacm. Check each licence before you read it. Do not size output from the sample count alone, because the count can disagree with the decoded length in a few files.

Two `.VOC` files are not ACM. `OVER_21Q.VOC` starts with `Creative Voice File`. Give the audio crate a second path for it.

`.MVE` movies (13 files) start with the 18 byte text `Interplay MVE File`. The format is publicly documented and CE has no parser to follow. Leave movies for the end.

## Game rules

All rule arithmetic is 32 bit signed integer math. The damage functions contain no floating point. Signed division truncates toward zero. Rust's `/` and `%` on `i32` do the same. Never use `div_euclid` or `rem_euclid` in rules code.

Hit chance, skills, perks, traits, combat AI and world map travel are not specified yet. They are listed under Open items.

### Data compiled into the executable

Several tables live in `falloutw.exe` and not in the game data files. They belong to the executable layer, so every i32 in them is little endian. Read them from the player's own executable at start up, or extract them once into a local cache under `target/` that stays out of the repository. Look them up by virtual address through the PE section headers. Check the file hash first. The reference build has SHA-256 `444257e5fe91e6c333261aecadacb1f46dbba0319fa0b187de89f36914f32272`. Stop with a clear message on any other build. Do not copy table initialisers out of CE.

| Table | Address | Layout |
| --- | --- | --- |
| Hit location penalty | 0x4FEC90 | 9 i32 values |
| Critical success, critters | 0x4FECB4 | 15 x 9 x 6 records of 28 bytes |
| Critical failure | 0x504B3C | 7 x 5 i32 values |
| Critical success, player | 0x50454C | 9 x 6 records of 28 bytes |
| Stat limits | 0x507EC8 | 38 records of 24 bytes |
| Player stat limits | 0x508258 | 5 records of 24 bytes |
| XP thresholds | 0x5082D0 | 21 i32 values |

Record layouts, all little endian i32 fields:

- **Critical record, 7 fields.** The multiplier, the effect flags, the massive critical stat, its modifier, the massive critical flags, message id 1 and message id 2.
- **Stat limit record, 6 fields.** Field 0 looks like the stat index. Fields 1 and 2 are not understood. Field 3 is the minimum, field 4 the maximum and field 5 the default. The minimum of the first record sits at 0x507ED4.
- **Player stat limits.** Five records with the same layout. They cover unspent skill points, level, experience, reputation and karma.
- **XP thresholds.** Each value is L(L+1)/2 and is not scaled. The caller multiplies by 1,000.

Three adjacency tests catch a wrong address or a wrong record size. Each is a class 1 oracle.

- `0x4FECB4 + 22,680 == 0x50454C`. The critter table is 15 x 9 x 6 x 28 bytes and ends where the player table starts.
- `0x507EC8 + 912 == 0x508258`. The stat limits are 38 x 24 bytes.
- `0x508258 + 120 == 0x5082D0`. The player limits are 5 x 24 bytes.

CE's own address comments for the two critical tables are wrong. Use the addresses above.

### Time

| Constant | Value |
| --- | --- |
| Ticks per game second | 10 |
| Ticks per game day | 864,000 |
| Ticks per game year | 315,360,000 |

### Random numbers

The generator is a multiplicative congruential generator with a 32 slot shuffle table. Its state is `idum`, `iy` and `iv[32]`.

```
fn ran1(&mut self, max: i32) -> i32 {
    let mut v = 16807 * (self.idum % 127773) - 2836 * (self.idum / 127773);
    if v < 0 { v += 0x7FFFFFFF; }
    if v < 0 { v += 0x7FFFFFFF; }
    let slot = (self.iy & 31) as usize;
    let out = self.iv[slot];
    self.iv[slot] = v;
    self.iy = out;
    self.idum = v;
    out % max
}

fn seed(&mut self, seed: i32) {
    let mut num = seed;
    for i in (0..40).rev() {
        num = 16807 * (num % 127773) - 2836 * (num / 127773);
        if num < 0 { num &= 0x7FFFFFFF; }
        if i < 32 { self.iv[i] = num; }
    }
    self.iy = self.iv[0];
    self.idum = num;
}
```

The sketch uses `as` for brevity. Production code follows the casting rule in Engineering philosophy.

`roll_random(min, max)` returns `min + ran1(max - min + 1)`, so both ends are included. If the result falls outside the range, it returns `min`. A call with `min` above `max` therefore returns `min` every time. No caller does this. Leave it alone.

The seed loop must write `iv[0]` to `iv[31]`. CE's loop writes `iv[1]` to `iv[31]` and leaves `iv[0]` stale, so its sequence differs from the original. Do not use CE to check random results. The game seeds from the clock at start and does not save the generator state.

These vectors were reproduced with an independent model written for this guide. With seed 12345, twelve calls to `roll_random(1, 100)` give 14, 26, 22, 8, 70, 76, 2, 13, 28, 15, 20, 46. With seed 12345, five calls to `roll_random(3, 7)` give 6, 3, 4, 5, 7.

### Stats

There are 38 stats. Indices 0 to 34 are saved and have base and bonus values in the critter's prototype. Indices 35 to 37 are read live from the critter.

| Index | Stats | Min | Max | Default |
| --- | --- | --- | --- | --- |
| 0 to 6 | Strength, perception, endurance, charisma, intelligence, agility, luck | 1 | 10 | 5 |
| 7 to 14 | Max hit points, max action points, armor class, unarmed damage, melee damage, carry weight, sequence, healing rate | 0 | 2,147,483,647 | 0 |
| 15 | Critical chance | 0 | 100 | 0 |
| 16 | Better criticals | 0 | 100 | 0 |
| 17 to 23 | Damage threshold: normal, laser, fire, plasma, electrical, EMP, explosion | 0 | 100 | 0 |
| 24 to 30 | Damage resistance, same order | 0 | 90 (EMP, index 29, is 100) | 0 |
| 31, 32 | Radiation resistance, poison resistance | 0 | 100 | 0 |
| 33 | Age | 16 | 35 | 25 |
| 34 | Gender | 0 | 1 | 0 |
| 35 to 37 | Current hit points, poison level, radiation level | 0 | 2,000 | 0 |

The stat value is the prototype base value plus the prototype bonus value. For the player, a trait adjustment joins the base value before the bonus is added. Then three situational terms apply. Blindness takes 5 from perception. In combat, armor class gains the critter's current action points when it is not that critter's turn. Age gains `game ticks / 315,360,000`. Last, clamp to the stat's minimum and maximum. Worn armor writes into the bonus values. Base writes are refused for indices 7 to 32.

Vanilla sets the minimum of better criticals to 0. CE sets it to -60. Keep 0. A test must fail if it is below 0. The table above is the check: read the limits from the executable and compare.

CE gives these derived formulas. They come from CE source only, and the function that applies them was not found in the binary. Treat them as the standard Fallout rules and check them against the executable.

| Stat | Formula |
| --- | --- |
| Max hit points | strength + 2 x endurance + 15, using unclamped base values |
| Max action points | agility / 2 + 5 |
| Armor class | agility |
| Melee damage | the larger of strength - 5 and 1 |
| Carry weight | 25 x strength + 25 |
| Sequence | 2 x perception |
| Healing rate | the larger of endurance / 3 and 1 |
| Critical chance | luck |
| Radiation resistance | 2 x endurance |
| Poison resistance | 5 x endurance |

### Damage

Three separate functions compute damage. The earlier claim that one formula covers all of them is wrong.

| Path | Address | Used for |
| --- | --- | --- |
| Weapon | 0x422118 | Every weapon and unarmed attack |
| Explosion | 0x413044 | Explosions |
| Typed | 0x4133D8 | Script damage actions |

Damage types are 0 normal, 1 laser, 2 fire, 3 plasma, 4 electrical, 5 EMP and 6 explosion. A type's threshold stat is 17 plus the type. Its resistance stat is 24 plus the type.

The explosion and typed paths work like this.

1. Roll `roll_random(min, max)`.
2. Subtract the threshold from the roll to get d.
3. If d is above 0, subtract `resistance * d / 100` from d.
4. If d is below 0, set d to 0.
5. Write knockback as `d / 10` unless the target has flag 0x800 (multihex). The typed path also skips electrical damage. A skipped knockback is not written at all. It is not written as 0.

The explosion path always uses stat 23 for the threshold and stat 30 for the resistance. These vectors follow from the steps above and were recomputed by hand.

| Roll | Threshold | Resistance | Multihex | Damage | Knockback |
| --- | --- | --- | --- | --- | --- |
| 19 | 5 | 0 | no | 14 | 1 |
| 12 | 0 | 50 | no | 6 | 0 |
| 17 | 5 | 50 | no | 6 | 0 |
| 19 | 0 | 90 | no | 2 | 0 |
| 10 | 0 | 0 | yes | 10 | not written |
| 10 | 0 | 100 | no | 0 | 0 |
| 12 | 100 | 0 | no | 0 | 0 |
| 22 | 25 | 20 | no | 0 | 0 |
| 2 | 3 | 25 | no | 0 | 0 |

The weapon path runs once per round fired and adds the rounds together.

1. Start with the weapon damage plus the ranged bonus.
2. Multiply by the critical multiplier. Then divide by 2.
3. Multiply by the difficulty percentage. Then divide by 100.
4. Subtract the threshold.
5. If the result is above 0, subtract `result * resistance / 100`. Add the result to the total only if it is still above 0.

The terms are these.

- The ranged bonus is 2 per rank of Bonus Ranged Damage. It applies only when the player attacks with a ranged weapon.
- The difficulty percentage is 100 when the attacker is on the player's team. Otherwise it is 75 on easy, 100 on normal and 125 on hard.
- The threshold and resistance come from the defender. The bypass flag (0x800) sets both to 0, except for EMP damage. A weapon with the Penetrate perk sets the threshold to 0.
- The player's Finesse trait adds 30 to the defender's resistance against the player's attacks.
- Knockback is the total divided by 10, or by 5 with the Weapon Knockback perk. It is set only for explosion damage, attacks with no weapon and melee attacks. It is never set for multihex targets.

The critical multiplier is applied before the halving. A critical therefore also raises the amount the threshold removes. A normal hit should use a multiplier of 2 so that the halving cancels. The default is not confirmed. Check it in the attack computation at 0x421490. The meaning of the weapon damage query is also open. It is not known whether it returns a rolled value or a range. Resolve both before you build the weapon path.

### Critical hits and failures

A critical hit is a multiplier on weapon damage. It is not a bonus term.

1. The attack rolls to hit. A second roll checks for a critical using the attacker's critical chance (stat 15) minus the hit location penalty. A critical success is possible only from game day 2.
2. Compute `chance = roll_random(1, 100) + stat_level(attacker, 16)`. Pick the tier from the cutoffs 20, 45, 70, 90 and 100. A chance of 100 or less falls in tiers 0 to 4. Anything above 100 is tier 5.
3. Look up the record by the defender's kill type (15 types), hit location (9 locations) and tier. The player has its own table that needs only location and tier.
4. A record holds the multiplier, the effect flags, a massive critical stat and its modifier, the massive critical flags, and two message ids. If the stat is not -1 and the defender fails a check on it, add the massive flags and message.
5. If the flags include random cripple (0x00200000), pick a random limb to cripple.
6. Return the multiplier.

Multipliers in the critter table are 2, 3, 4, 5, 6 and 8. Those in the player table are 2, 3, 4 and 6. CE's code returns 2 at once when the defender's pid is 0x1000008, and its comment calls that the player. The binary does contain that comparison at 0x421A7B. Elsewhere CE sets the player prototype's pid to 0x1000000. The two values disagree. Find out which pid the player object carries before you copy this rule.

Vanilla differs from CE in one critical row. For a robot hit in the groin at tiers 4 and 5, vanilla sets random cripple as the massive critical flag. CE sets 0. Keep the vanilla value. The check reads the row from the executable table.

A critical failure rolls `roll_random(1, 100) - 5 * (luck - 5)`. The cutoffs are 20, 50, 75 and 95. The result and the weapon's critical failure type pick a cell in the 7 by 5 table. The effects are lose turn, drop weapon, lose ammo, dud, destroy weapon, random hit, explode, hit self, hurt self, knocked down, random cripple and on fire.

Burst and continuous fire do not call the critical function. On a critical roll they add 20 to accuracy and raise the number of rounds that hit. They apply no multiplier. This is not yet verified in the binary.

### Experience and levels

The experience needed to leave level L is `L * (L + 1) / 2 * 1000`. The executable table holds the unscaled `L * (L + 1) / 2` and the caller applies the factor of 1,000. The maximum level is 21 and a level of 21 or more returns -1. The experience gain is raised by 5 percent of itself per rank of Swift Learner. Each level up gives `endurance / 2 + 2 + 4 * Lifegiver rank` hit points, using unclamped base endurance. The player stats are unspent skill points, level (minimum 1), experience, reputation (-20 to 20) and karma.

### Grids and coordinates

Rendering uses a 100 by 100 square grid. Gameplay uses a 200 by 200 hex grid. Both have 3 elevations. Keep them as separate types. An object's tile is a hex index. The hex neighbour rules are not specified yet. The spec author writes them from CE's tile direction and distance behaviour and checks them against the pathfinder tests below.

The packed tile in a saved object holds the tile in bits 0 to 25 (mask 0x03FFFFFF), the rotation in bits 26 to 28 (0x1C000000) and the elevation in bits 29 to 31 (0xE0000000). An earlier note said elevation overlaps rotation. Its own masks do not overlap. Use the masks. The spec author writes the 132 byte runtime object layout and its flag bits in `docs/spec/object.md`.

### Pathfinding

The original runs A star from address 0x4159E8. The binary confirms it byte for byte, apart from the 800 step cap that only CE has.

| Property | Value |
| --- | --- |
| Search space | The 200 by 200 hex grid, six neighbours, one elevation |
| Open list | 2,000 nodes of 20 bytes: tile, parent tile, direction, h, g. Pick the lowest g plus h by linear scan. No heap. |
| Closed list | 2,000 nodes. If either list fills, return no path. |
| Visited set | A bitset of 40,000 bits (5,000 bytes). Set a tile's bit when you add it to the open list, not when you expand it. |
| Step cost | 50 for every step in any of the six directions |
| Turn cost | Add 10 when the direction differs from the parent's. Do this only outside combat. The start node has direction 0. |
| Heuristic | Take the screen coordinates of the tile and the goal from `tile_coord`. With dx and dy as absolute differences, h is dx + dy - min(dx, dy) / 2, using a right shift by 1 |
| Blocking | A callback, normally the blocking test at the tile and elevation. The goal tile is not tested. |
| Occupied goal | If the caller demands a free goal and it is occupied, return 0 before searching |
| Doors | A critter other than the player with a biped or robotic body may path through a door. Check the lock rule in `anim_can_use_door`. |
| Output | One byte per step, the direction index from 0 to 5, in walking order. Return the step count, or 0 for failure. |

Marking tiles on insert means the result can be longer than the best path. Reproduce that. Tests: with the heuristic function on pixel coordinates, the distance from (0, 0) to (3, 4) is 6 and to (4, 4) is 6. CE caps the backtrack at 800 steps. The binary shows no such bound, so size the output buffer by the animation slot (3,200 bytes) and test long paths.

A move runs the search once. The path then lives in the animation slot and the game replays one step per tick. It does not replan. Stairs use a separate routine. Projectiles and line of sight use a Bresenham style line walk, not A star. World map travel uses a precomputed walkmask and is a different subsystem.

## Script virtual machine

Game logic runs in compiled scripts. MASTER.DAT holds 952 `.INT` files under `SCRIPTS`. They run on a stack machine with 342 opcode slots. The analysis decoded 950 of them with no unexplained word. Two (`ANIMFR~1.INT` and `HBETH.INT`) failed only because the old Python decoder could not unpack them, so the fixed archive layer should give all 952. The spec author writes the full opcode table in `docs/spec/script.md`. This section states the rules the VM must follow.

### File layout

Every integer is big endian.

| Offset | Content |
| --- | --- |
| 0x00 | A 42 byte entry preamble. It is executed code, not a header. |
| 0x2A | A u32 procedure count, then 24 byte procedure records |
| After the records | A u32 identifier pool size, then the pool |
| After the pool | The static string table, the entry thunk and the code |

The preamble is the bytes `80 02 C0 01 00 00 00 12 80 0D C0 01`, then a u32 address of the entry thunk, then `80 04`. A 24 byte abort epilogue follows at offset 18. An earlier table gave offset 0x18. Eighteen plus 24 gives 42, so 18 is right.

The entry thunk is `80 2C`, then N copies of `C0 01 00 00 00 00`, then `80 03`, then `C0 01` and the address of the first procedure, then `80 04`. N is the script's global count. The first two procedure records share one code address. Record 0 has no name and record 1 is `start`.

A procedure record has six u32 fields: the name offset in the identifier pool, flags, a timer deadline, a condition address, the code address and the declared argument count. The flags are 0x01 timed, 0x02 conditional, 0x04 imported, 0x08 exported and 0x10 critical. All 17,093 procedure names across the 950 readable scripts resolve to ASCII. A name offset counts from the first byte of the pool's size field. In `ABEL.INT` that field sits at 0x2E6, offset 24 reaches `start` and offset 32 lands inside a record. The pool is raw bytes. Do not require an offset to fall on a record boundary.

The static string table has only one real string in the whole corpus (`ABEL.INT`, offset 6, the text `off`, framed as a u32 size of 6, a u16 length of 4 and the bytes). One sample cannot fix the framing. Treat it as unverified.

### Instructions and values

- Every instruction is a u16 word. If bit 15 is clear, the script errors. The handler index is `word & 0x3FF`.
- Only index 1 (push) has an operand, a u32 after the word. Every other instruction takes its operands from the data stack.
- The push word is also the value's type tag. The compiler emits 0xC001 for an integer, 0xA001 for a float, 0x9001 for a static string offset, 0x9801 for a dynamic string offset and 0xE001 for a pointer. The data holds 276,198 integer pushes, 3,192 string pushes and 4 float pushes. It has no pointer or dynamic string pushes. The bare word 0x8001 never appears, but your VM must accept it.
- The table has 342 slots and 340 handlers. Slots 0x807D and 0x807E are empty on purpose and raise `Undefined opcode`. Keep both holes. The core machine is 0x8000 to 0x804B (76 opcodes). The dialogue and window library is 0x804C to 0x80A0 (83). The game builtins are 0x80A1 to 0x8155 (181). Nothing exists above 0x8155.
- `IF` and `WHILE` branch on falsy values. An integer or string offset of 0, a float of 0.0 and a null pointer are falsy. `NOT` pushes 0 or 1.
- Strings use 80 byte buffers. `ADD` joins two strings and cuts the result at 80 bytes. A string compared with a float formats the float with five decimals first. These rules come from CE and are unverified.

### Stacks and calls

Each program has a data stack and a return stack. Each is 2,048 bytes of 6 byte entries (a u16 tag and a u32 payload), which gives 341 entries. A push past the limit or a pop from an empty stack ends that script with an error and leaves the game running. CE uses an unbounded vector. Do not copy that. The 342nd push must fail.

A program has an instruction pointer that starts at 0, a frame pointer and a base pointer. `SET_GLOBAL` sets the base pointer to the stack size. The entry thunk's zero pushes after it become the script's own globals. Global n is `stack[base + n]` and local n is `stack[frame + n]`.

A call pushes its arguments, and then the callee's `PUSH_BASE` pops the argument count, saves the frame pointer on the return stack and sets the new frame pointer to the stack size minus the argument count. The callee ends with `POP_TO_BASE`, `POP_BASE` and `POP_RETURN`.

### SWAP and SWAPA

CE's `SWAP` and `SWAPA` look like no operations to a quick reader. They are not. CE pops the top value, then the second value, then pushes the top value first and the second value last. The two values trade places. Implement a real swap.

The shipped code proves it. `SWAPA` appears 15,472 times, once per procedure epilogue. This epilogue comes from `SCRIPTS\BASIC.INT`. The call has already pushed the return address.

| Instruction | Return stack afterwards, top on the right |
| --- | --- |
| `PUSH_BASE` | return address, saved frame pointer |
| `PUSH_INT 0`, then `D_TO_A` | return address, saved frame pointer, 0 |
| `SWAPA` | return address, 0, saved frame pointer |
| `POP_TO_BASE` | no change |
| `POP_BASE` | return address, 0. The frame pointer is restored. |
| `A_TO_D` | return address. The 0 is now the result on the data stack. |
| `POP_RETURN` | empty. The instruction pointer is the return address. |

If `SWAPA` did nothing, `POP_BASE` would load 0 as the frame pointer and `A_TO_D` would return the saved frame pointer as the result.

### Scheduling

- Each tick every loaded program runs at most 10 instructions. The burst is configurable and at least 1.
- A program inside a critical section ignores the burst and runs until it leaves the section. Every script starts inside one. The preamble enters it and the thunk leaves it.
- A program that has exited is removed after its burst. Then the event queue runs, then the dialogue library updates.
- `WAIT` takes milliseconds. Script time is `1000 * timer / timerTick` with a tick of 1000. No shipped script uses `WAIT`. Dialogue builtins block by installing their own wait check that stays false until the player answers.
- `CALL_AT` arms a procedure with a deadline. `CALL_WHEN` arms a condition address that the engine checks each tick. The builtins `add_timer_event` (425 uses) and `rm_timer_event` (62 uses) use the object event queue.
- A script error stops only that script. A nested call to the scheduler returns at once.

### Builtins, exports and globals

- The 264 builtins are called by opcode number. No name table exists at run time and no `.INTBND` file exists in this game. A builtin pops its arguments and pushes its result. The first argument the script pushes is the last one popped. Example from `ABEL.INT`: `self_obj`, `push 1`, `push 6`, `push 34`, then `critter_add_trait`. It pops 34, 6 and 1, then the object.
- Exported variables and procedures are found by name. The reference uses two 1,013 slot tables. The hash is `v = v + tolower(ch) + v * 8 + (v >> 29)` over the name in unsigned 32 bit wrapping math, the slot is `v % 1013`, the probe step is 7 and names compare without case. Only the result matters if you keep no save data that depends on slot order. A map from lowercase name to value gives the same behaviour. The data uses `EXPORT_VARIABLE` 528 times, `FETCH_EXTERNAL` 1,300 times and `STORE_EXTERNAL` 989 times. `EXPORT_PROCEDURE` never appears.
- No shipped script calls an imported procedure. CE deleted that path. Return an unsupported error for it.
- CE's enum lists 618 global variables. Read the real count and starting values from the `GAME_GLOBAL_VARS:` section of `DATA\VAULT13.GAM`. Do not hard code 618. Each map has its own `MAP_GLOBAL_VARS:` section in its `.GAM` file. `global_var` appears 13,861 times and `set_global_var` 4,443 times. A read out of range logs an error and returns 0. A write returns -1.
- The engine calls scripts by procedure name. Names seen in real identifier pools include `start`, `map_enter_p_proc`, `map_update_p_proc`, `use_p_proc`, `use_skill_on_p_proc`, `talk_p_proc`, `critter_p_proc`, `damage_p_proc`, `timed_event_p_proc` and `look_at_p_proc`. The spec author writes the full ordered list from CE's behaviour and real identifier pools.

### Where CE differs

| Area | Vanilla | Rule here |
| --- | --- | --- |
| `SWAP`, `SWAPA` | Swap | Swap |
| Stack size | 2,048 bytes, errors on overflow | Same. CE is unbounded. |
| `NEGATE` on floats | Integer only, per CE's comment | Accept floats. Record it in the deviation list. Only 4 float pushes exist. |
| `DUMP` | Pops count plus 1 values | Follow vanilla. No shipped script uses it. |
| Divide by zero | Error `Division (DIV) by zero` | Raise it for integer and float zero. |
| `AND`, `OR` | Operate on two finished values | Plain two operand operations. Short circuit would have to come from compiled jumps. |
| Imported procedure calls | Worked | Unsupported error, and nothing ships that needs them |

### Acceptance tests

- All 952 scripts unpack. Each starts with the preamble above. Each entry thunk has the thunk shape. Each procedure name resolves.
- Decode with a decoder that follows jumps. A straight sweep desynchronises in 19 large scripts. The 669,318 word count and the per opcode counts are lower bounds. The set of 197 distinct opcodes is stable. Expected counts: `POP_TO_BASE` 30,943, `POP_RETURN` 30,942, `IF` 22,282, `D_TO_A` 26,034, `PUSH_BASE` 15,490, `SWAPA` 15,472 and `WAIT` 0.
- `SCRIPTS\BASIC.INT` is 176 bytes. Its entry thunk is at 132 and its code is at 150. The code is `PUSH_BASE`, `PUSH_INT 0`, `D_TO_A`, `SWAPA`, `POP_TO_BASE`, `POP_BASE`, `A_TO_D`, `POP_RETURN`, `POP_TO_BASE`, `POP_BASE`, `POP_RETURN`. Run it and check the frame pointer and stack depth afterwards.
- The first instructions at the `start` procedure of `ABEL.INT` (address 1242) are `PUSH_BASE`, `PUSH_INT 1320`, `PUSH_INT 1`, `FETCH_GLOBAL`, `NOT`, `IF`, `self_obj`, `PUSH_INT 1`. The thunk is at 1206 and has 4 globals.
- The table has 342 entries with holes at 0x807D and 0x807E. Pushing a 342nd entry fails. Popping an empty stack fails.
- A canary: run the `BASIC.INT` epilogue with a `SWAPA` that does nothing and assert that the frame pointer is wrong. The suite must reject the no operation reading.

## Saves

The original save is a flat stream of 27 blocks after a fixed header. It has no tags, no lengths, no terminator and no checksum. No real `SAVE.DAT` exists in the project, so no body layout has been checked against a file. Only the header sizes and the handler table were checked against the binary.

### Decision: two formats

The port writes its own format and reads the original header only.

- **Port format.** One self contained file with a new magic. Little endian fixed width integers, written by one writer module. Tagged sections, each with a tag and a length. A format version. A CRC32 (the IEEE polynomial) over every byte before the final 4 byte trailer. The reader checks the CRC before it parses any section and rejects a mismatch. Per map state lives inside the file as sections. The thumbnail uses the original 224 by 133 layout so the slot list code is shared.
- **Original format.** Read the header now to list slots. Read the body only after a real `SAVE.DAT` fixture exists and the map `.SAV` format is specified. Never write it.

This is a choice made by this guide. The original has no integrity check, so a damaged file loads as wrong state instead of an error. The port should not inherit that. The owner can overrule the choice.

### Slot paths and side files

- Slots are numbered 1 to 10 on disk. The directory is `SAVEGAME\SLOTnn\` and the file is `SAVE.DAT`. The root is the `master_patches` value from `FALLOUT.CFG`, which defaults to `DATA`. `DATA\SAVEGAME` ships empty.
- If `master_patches` is missing, the original builds the directories under `\FALLOUT\CD\DATA\SAVEGAME` (bytes confirmed in the binary at 0x505974) but opens files under the default root. Do not copy that split. Use one configurable root and record the deviation.
- The original also copies every `MAPS\*.SAV` file and `MAPS\AUTOMAP.SAV` into the slot directory, and keeps `.BAK` copies during a save. A save that holds only `SAVE.DAT` cannot load in the original. The port format avoids this by embedding map state.

### Header

The header is 30,051 bytes and is fixed. Integers are big endian. Text fields are single byte, padded with NUL. The binary confirms the constants 18, 24, 30, 128 and 29,792. The other offsets come from CE and sum to the same total.

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 18 | Magic `FALLOUT SAVE FILE`. This is the only validation. |
| 18 | 6 | NUL padding. Never checked. |
| 24 | 2 | Version major, u16, value 1 |
| 26 | 2 | Version minor, u16, value 1 |
| 28 | 1 | Release letter `R` |
| 29 | 32 | Player name |
| 61 | 30 | Slot description |
| 91 | 2 | File date, day |
| 93 | 2 | File date, month |
| 95 | 2 | File date, year |
| 97 | 4 | File time, i32. The value is hour plus minute. |
| 101 | 2 | Game month |
| 103 | 2 | Game day |
| 105 | 2 | Game year |
| 107 | 4 | Game time in ticks |
| 111 | 2 | Map elevation |
| 113 | 2 | Map index |
| 115 | 16 | Map name without the `.SAV` extension |
| 131 | 29,792 | Thumbnail, 224 by 133 bytes |
| 29,923 | 128 | Zero padding |
| 30,051 | none | First byte of block 0 |

- Compare 18 bytes of magic, not 24. Accept any file that matches.
- The version triple is stored and never compared. The code path that would report an old version is unreachable. A bad file shows as corrupt. Show one error state in the slot list.
- A time of 14:37 stores 51. Reproduce this when you write the original header, and when you show the stored value. A reader that treats it as a clock is wrong.
- CE names the day, month and year fields in a scrambled order. Name fields by offset. The writer order above is the ground truth.
- The loader skips the thumbnail and the padding without parsing them. Whether the thumbnail holds real pixels in shipped saves is unchecked.

### Body

The loader runs the 27 handlers in order. The save and load tables are not mirror images. Index 0 does nothing on save and prepares the load. Index 26 does nothing on save and finishes the load. Any failure aborts the whole operation, restores the backup and deletes the `.BAK` files.

| # | Contents | Bytes |
| --- | --- | --- |
| 0 | None | 0 |
| 1 | Player object id | 4 |
| 2 | Game global variables | 4 x G + 1 |
| 3 | Map file list: count, NUL ended names, automap size | 4 + sum of (name length + 1) + 4 |
| 4 | Game global variables again. The loader reads and discards them. | 4 x G + 1 |
| 5 | Player object with its inventory, then the tile center | object bytes + 4 |
| 6 | Player critter data | 372 |
| 7 | Kill counts | 64 |
| 8 | Tagged skills | 16 |
| 9 | None | 0 |
| 10 | Perks | 256 |
| 11 | Combat state | 4, or 4 + 28 + 4 x list total in combat |
| 12 | None | 0 |
| 13 | Player stats | 20 |
| 14 | None | 0 |
| 15 | Event queue | 4 + sum per node |
| 16 | Traits | 8 |
| 17 | Automap | 4 |
| 18 | Options | 80 |
| 19 | Character editor | 5 |
| 20 | World map | 1,028 |
| 21 | None | 0 |
| 22 | Movies seen | 56 |
| 23 | Skill use slots | 216 |
| 24 | Party members | 8 + 4 x (count - 1) |
| 25 | Interface | 16 |
| 26 | None | 0 |

G is the game global variable count. It is the same number the script VM uses, read from `VAULT13.GAM`. An earlier note said G comes from a map header. The map header counts belong to map variables, which are a different set. Treat that statement as wrong until a file shows otherwise.

Block 6 is 4 bytes for the sneak flag, then 368 bytes of critter data. The critter data is the flags (4), base stats (35 x 4), bonus stats (35 x 4), skills (18 x 4), body type, experience and kill type (4 each). Block 18 is 13 ints, a float, 4 ints and 2 floats (80 bytes). Block 20 is a 31 by 29 byte grid (899), a 15 by 7 byte grid (105) and six ints (24).

The six zero byte handlers are real. Four share addresses in the binary because the compiler merged identical functions. A reader must step over them without consuming anything.

### Variable length parts

These parts cannot be skipped. A reader must know each length from its own data.

- Blocks 2 and 4 depend on G.
- Block 3 depends on its file count.
- Block 11 depends on whether combat is running and on the list total.
- Block 15 depends on the node count. Each node holds a time, a type and an owner id, then a payload that depends on the type. Match on the type exhaustively and return an error for an unknown value.
- Block 24 depends on the party count. A count of 0 writes 8 bytes.
- Block 5 holds an object record, then data that depends on the object's prototype type, then each inventory entry as a quantity followed by a nested object. Depth equals the container nesting. The original has no depth limit.

The prototype update data lengths and the queue payload lengths are not specified yet. List every prototype type and every event type from the object and queue code, and write one test for each length before you read block 5 or block 15.

### Bug compatibility

These apply when you write or read the original format. Each one gets a test that fails if someone fixes it.

- The file time is hour plus minute.
- Booleans in the interface block take 4 bytes. A one byte write shifts the rest of the stream by 9 bytes.
- The world map grids are raw bytes. Copy them. Do not run them through the integer writer.
- Block 4 repeats block 2. A writer must emit it.
- Block 25 aborts the whole save when the interface window does not exist.
- An earlier note says the skill use block writes an 18 by 3 byte array as raw little endian data. That would read 216 bytes from a 54 byte array, so it looks wrong. The block is 216 bytes either way. Read the function at 0x499520 in the binary before you decide the element type.

### Reader rules

- The reader is total. It never panics, never reads past the end and never allocates more than the remaining bytes can fill.
- Check every count against the bytes left before you allocate. This covers G, the file count, the combat list total, the queue count, the party count and each inventory count.
- Cap inventory nesting at a fixed depth and return an error past it. Use an explicit stack or a bounded recursion.
- Reject a file whose magic fails. Accept anything else at the header, then parse the body strictly.

### Tests

- The header constants add up: 24 + 4 + 1 + 32 + 30 + 6 + 4 + 6 + 4 + 2 + 2 + 16 + 29,792 + 128 = 30,051.
- Fixed block sizes match the table: 372, 64, 16, 256, 20, 8, 80, 5, 1,028, 56, 216 and 16.
- Both handler tables hold 27 entries. Save index 0 and 26 do nothing. Load index 0 and 26 do work. Save indexes 2 and 4 are the same function.
- Slot paths for slots 0 and 9 are `SAVEGAME\SLOT01\SAVE.DAT` and `SAVEGAME\SLOT10\SAVE.DAT`.
- A crafted block 5 with 100,000 nested inventories returns an error and does not overflow the stack.
- Flipping any single bit of a synthetic save either parses or errors. It never panics.
- A port format file with one flipped bit fails the CRC check before any section parses.
- Round trip tests use saves your own writer produced and are labelled `synthetic_*`. A test that needs a real `SAVE.DAT` is marked ignored with a message that names the missing fixture, and the gate counts it as an open item and not as a pass. Never build a fake fixture and call it original.

## Rendering, audio and input

The renderer draws every tile and object into an 8 bit indexed framebuffer on the CPU. It converts the frame to screen colors once, at the end. CE replaced the original DirectDraw path with SDL2 and changed clipping and resolution handling, so CE is the weakest reference in this section. The rules below keep what the binary confirms and drop what CE changed.

### Surface and viewport

- The logical surface is 640 by 480 pixels at 8 bits per pixel. It never follows the window size. Scaling to the window happens only in the final present step.
- The isometric view sits at the top and the interface fills the rest. CE's constants give a view of 640 by 380. A comment in the High Resolution Patch config says 400. Use 380 as a named constant and keep the conflict in the open items. The original's surface setup settles it.
- The numbers 480 and 400 found in the binary are not sizes. They are limits in the scroll check that stops the camera center from moving too far from the player.
- Widescreen is out of scope. Most players run the High Resolution Patch, and nobody has disassembled it. Keep the view size a parameter so a later change does not touch the passes.

### Two grids

| Grid | Size | Holds | Projection |
| --- | --- | --- | --- |
| Square grid | 100 by 100 per elevation | Floor and roof ids | `square_coord`, `square_xy` |
| Hex grid | 200 by 200 | Object tiles and pathfinding | `tile_coord` |

There are 3 elevations. Give each grid its own index type so one cannot be passed for the other. Mixing them is the source of the false 48 by 32 tile size.

### Square projection

```
mcol = width - 1 - (square % width)      // width is 100, the column is mirrored
row  = square / width
screen_x = off_x + 48 * (mcol - center_col) + 32 * (row - center_row)
screen_y = off_y - 12 * (mcol - center_col) + 24 * (row - center_row)
```

One column step is (48, -12). One row step is (32, 24). A tile sprite is the bounding box of the two steps. `GRID000.FRM` measures 80 by 36, which equals (48 + 32) by (12 + 24). The binary confirms the structure of the function at 0x49E8B4: the bounds check against the grid size, the two divisions, the column mirror and the global variables. It does not confirm the four literals, which rest on CE and on the sprite size. The roof version adds -96 to y (CE only).

The inverse takes a screen point and returns a square:

```
a = sx - off_x
b = sy - off_y - 12
mcol = floor_div(3 * a - 4 * b, 192) + center_col
row  = floor_div(4 * b + a, 128) + center_row
square = row * width + (width - 1 - mcol)
```

CE writes the division for negative numbers as `(v + 1) / d - 1`, which equals floor division. Use `div_euclid` with a positive divisor, which is the one place in the engine where the rules code exception applies. A plain truncating division fails in the upper left of the view. The inverse expects a point inside the diamond. The point (anchor x + 40, anchor y + 18) returns the tile's own square. Test every square this way. The anchor corner itself returns the wrong row.

### Hex projection and the view center

The hex grid advances at half the rate of the square grid per column. The long run step is (48, -12) per two hex columns and (16, 12) per hex row.

```
dc = hcol - center_col
screen_x = off_x + 48 * (dc / 2) + 16 * (hrow - center_row)
screen_y = off_y + 12 * (dc / -2) + 12 * (hrow - center_row)
if hcol is odd:
    if hcol <= center_col: screen_x -= 16, screen_y += 12
    else:                  screen_x += 32
```

Division here truncates toward zero. The function above starts after the column and row are known. How they come from the tile index is not specified. Assume the same column mirror as the square grid and confirm it against a known object position on a real map.

The two grids meet when the view center is set. Let `view_w` and `view_h` be the iso view size and let `cx`, `cy` be the center hex.

```
off_x = (view_w - 32) / 2
off_y = (view_h - 16) / 2
if cx is odd: cx -= 1, off_x -= 32
center_col = cx / 2, center_row = cy / 2
square_off_x = off_x - 16
square_off_y = off_y - 2
if cy is odd: square_off_y -= 12, square_off_x -= 16
```

### Draw passes

There is no depth sort anywhere. Correct overlap comes from the order of traversal. The binary confirms this for the floor pass. It survives CE's rewrite unchanged.

1. **Floor.** Walk the clipped rectangle row by row, and within a row column by column. For each square read the tile word. If bit 12 of the word is set, skip the square. Otherwise draw the floor tile named by the low 12 bits at the projected point. The binary confirms the mask and the test at 0x49F518.
2. **Objects below the roof.** Build two offset tables, one for each parity of the center hex, by walking screen space. Step 12 pixels down per row and 32 pixels across per cell. Shift every second row by 16 pixels. Convert each cell to a hex offset from the upper left tile. The walk is serpentine because odd hex rows sit 16 pixels over. For each cell in table order, take the tile's object list. Draw the flat objects at the head of the list first. Stop at the first object that is not flat and queue the rest of that list. A second loop draws every queued tail. Flat decals therefore sit under standing objects. Skip hidden objects. Stop at the first object on a higher elevation than the one drawn.
3. **Roof.** Same row by row walk as the floor. The roof word is the high half of the tile word. Its flag test uses the same nibble form, which puts the flag at bit 28. CE's roof flood fill clears this flag to hide a connected roof and sets it to show the roof again. That implies a set bit means draw. The binary check covers only the floor side. Confirm the direction on a real map.
4. **Overlays.** Outlines, floating text and the mouse hex cursor draw last.

The order of the four calls is not confirmed. The order above follows from what each pass does. Confirm it in CE's tile refresh behaviour before you rely on it.

Put every blit behind a draw backend trait. A recording backend that stores the art id and position of each call lets you test pass order without a window.

### Camera

- The center move is refused when the new center would put the player 480 or more pixels across or 400 or more pixels down from the screen center, if the new center is also farther away than the current one. Test it on real maps.
- Scroll borders are computed once from a 640 by 480 design surface, using the hex at screen point (-320, -240) and the hex at (-320, 620). The margins are 6 columns and 7 rows. The left border is forced odd.

### Bugs to reproduce

Each of these is in vanilla and in CE. A reviewer will want to fix them. Do not.

- The floor and roof passes compare the left edge of the clip rectangle with the grid length and then reset the top edge. The test should have used the top edge.
- The border code tests whether the right border is odd and then changes the left border.

### CE changes to leave out

- Four early exits that constrain the clip rectangle to tile bounds. One of them zeroes the rectangle instead of returning.
- The special case that draws the mouse hex cursor when the rectangle is out of bounds.
- Any use of the window size in scroll or border math.
- The 1.2 MB generated walkmask for the world map. Nothing shows it matches vanilla walkability.

### Palette and lighting

- Art is indexed. Never convert a sprite to RGBA when you load it. Lighting works on indexes through palette tables, and RGBA breaks that.
- A `.PAL` file is 33,536 bytes. The first 768 are the 256 colors. The depth is per file, so detect it from the data as the asset section describes. A 6 bit palette needs scaling for display and an 8 bit palette does not. The other 32,768 bytes are probably a 15 bit RGB to index table.
- CE's color helper assumes 5 bits per channel. CE blends in indexed space with four routines: translucent, dark, dark and translucent, and intensity mask. Their arithmetic is not specified. Specify them from CE's behaviour and test each against known pixels.
- Convert the finished frame to the screen format once per frame. Palette cycling is expected in the render crate and is not specified yet.

### Input

Input is not specified yet. The design rules are these.

- One input layer in `fo1-game` turns platform events into game events. The core crates never see a platform type.
- Picking a hex under the pointer uses the inverse of the hex projection. Only the square inverse is specified. Derive the hex one and test it against the forward function.
- The mouse hex cursor is an object that draws in the overlay pass.

### Audio

`fo1-audio` is a separate crate. The game must run with no sound until it works.

- The `.ACM` container is solved. The header is 14 bytes, little endian. The asset section lists the fields. The analysis counts 2,530 audio files, 2,511 in archives and 19 loose.
- The codec is not solved. CE has no decoder. It calls an outside library, `adecode`, that its build downloads. Public format notes describe a block transform codec, not ADPCM. Use them as the starting reference. The theory of a 4 bit codec with an 8 bit reference sample has no support.
- Define a decoder trait that returns PCM or an `Unsupported` error. Ship the stub first. A future decoder replaces it.
- Check the licence of `adecode` before you depend on it. Do not copy code from it.
- `.LIP` lip sync files are unresolved. CE's reader does not fit the shipped files.
- `.MVE` movies have no crate and no milestone. Add one when the game needs them.

### Tests

- Every square projects to the point whose inverse at (+40, +18) returns the same square.
- `floor_div(-1, 192)` is -1 and `floor_div(-192, 192)` is -1.
- Square 0 and square 9,999 project to the corner values for a known center.
- An odd and an even hex column both match hand computed points.
- The same map and center produce the same framebuffer for any window size.
- A recording backend shows the floor pass visiting squares row by row, and shows flat objects before standing objects on one tile.
- Both parity offset tables have the same length and no repeated entry.
- A golden frame test needs a reference screenshot from the original game, made by the owner. Mark it ignored with a reason that names the missing fixture until one exists. The gate reports it as an open item.

## Build order and milestones

Build in this order. Each milestone can be checked on its own. A gate closes only when `tools/gate.sh` passes in full against the reference install. That includes the anti deviation tests for the milestone, the hold out vectors and the completion report.

&#91;embedded content: milestones M0 to M8 with gates\]

Do M0 and M1 together. M0 takes a few hours and M1 depends on it. These notes change gates that older plans set.

- M0. The block length defect in earlier notes came from an old Python tool. The gate proves that the Rust decoder reads every entry, consumes exactly its input and never truncates its output.
- M1. The plaintext set is 28 named pairs, drawn from 31 candidates with 3 patched copies. The list of names comes from the test author, not from the code under test. The executable tables are read little endian and pass the three adjacency tests.
- M3. The mutation job runs with the install present, so the data tests count.
- M5. The port writes one self contained file with a CRC. It does not write `MAPS\*.SAV` beside `SAVE.DAT`.
- M6. The heuristic runs on screen coordinates and the search has no 800 step cap. Follow the pathfinding section.

## Open items

Each row is a question the analysis leaves open. Settle it before the code that depends on it. When you settle one, add the ruling and its evidence tag to the rulings table in Source material and trust.

| Area | Open question | Settle by |
| --- | --- | --- |
| Rules | Hit chance, derived stat formulas, skills, perks, traits and combat AI are not analysed. | Read the original and CE behaviour for each. Write a spec first. |
| Rules | The meaning of the weapon damage query and the weapon damage range | Binary at the call from the attack computation |
| Rules | The default damage multiplier of 2. The binary confirms the multiply, not the default. | Binary |
| Rules | Burst fire adds 20 to accuracy and applies no multiplier. The kill count type read is unchecked. | Binary and CE behaviour |
| Rules | Player prototype id. CE compares against 0x1000008 and sets 0x1000000. | Check which id the player object carries in a real map |
| Movement | Hex neighbour rules and the multihex optimisation | Binary |
| Movement | The step cost 50 and the turn cost 10 are confirmed by one analysis but not located as immediates by another. | Binary |
| World map | Travel uses a 1.2 MB generated walkmask. Nothing shows it matches vanilla. | Compare against the original's walk code |
| Assets | The mapping from `.FR0` to `.FR5` to direction indexes, and the frame id layout | CE's art behaviour and a real critter |
| Assets | Frame count of the 2,862 `.FR0` to `.FR5` files | Walk them and record the total in the ledger |
| Assets | The 32,768 byte tail of `.PAL` | Read the files |
| Assets | `.MSG` directory scope: 1,935 files and 132,623 lines against 1,950 and 132,744 | Count each scope and choose one |
| Assets | `.ACM` bitstream | Public format notes, then known audio |
| Assets | `.LIP` reading. CE's reader does not fit shipped files. | Real `.LIP` files |
| Assets | `VAULT13.GAM` global count G | Parse the file |
| Assets | Map object and script sections. NPC instance counts depend on them. | Parse after the archive gate passes |
| Scripts | The script loader and attach logic, and the full list of engine called procedure names | CE behaviour and real identifier pools |
| Render | Viewport height 380 or 400 | The original's surface setup and `fallout.cfg` |
| Render | Roof flag direction. A set bit probably means draw. | A real map |
| Render | How the hex column and row come from a tile index | A known object position on a real map |
| Render | The order of the four passes in the tile refresh | CE's refresh behaviour |
| Render | Palette cycling. Tile word bits 12 to 14 and 29 to 30 beyond the flag. The function at 0x49F71C. | Binary |
| Render | What the High Resolution Patch changed (`f1_res.dll`, `ddraw.dll`, `falloutwHR.exe`) | Disassemble the patch files |
| Saves | No real `SAVE.DAT` exists. Every body layout is unmeasured. | Obtain one save from the owner |
| Saves | The map `.SAV` format (map data, scripts, objects) | Specify from the map write code |
| Saves | Object record field count. One note says 19 integers and lists 18 names. | Binary |
| Saves | Where the inventory count sits, the prototype update lengths and the queue payload lengths | Object, prototype and queue code |
| Saves | Skill use block element type | Binary at 0x499520 |
| Audio | The `adecode` licence | Read it before any use |
| Media | `.MVE` playback has no crate and no milestone | Owner decision |
| Install | Other installs differ from the reference install, so the gate constants need new measurements. | Measure each install and add a separate fingerprint |
| Project | Repository licence, trademark wording and whether to commit hash manifests of the player's files | Owner decision after legal advice |

## Rules for the agent

These rules govern how you work. They apply to every crate and every milestone. The sections Publication rules, Engineering philosophy and Verification and anti faking carry the detail.

### Working order

1. Read this guide and the spec for the subsystem.
2. Write or update the spec in `docs/spec/<subsystem>.md` before any code. Give every claim an id and one of the four evidence tags.
3. The test author writes the gate tests from the spec's vectors and commits them alone. They fail first.
4. The implementer writes the code until they pass. The implementer does not edit gate tests or protected paths.
5. Run `tools/gate.sh`. Update the spec tags to match what the tests proved.
6. Write the completion report from the gate output.

A claim with no tag counts as UNVERIFIED. Never write the spec after the code.

### Standing rules

- The binary outranks real game data. Real game data outranks CE. CE outranks the notes. Write down every ruling.
- Never commit game data, CE source or private paths. Never run the game, a patcher or an installer.
- Never turn a failing test green by one of the moves listed in Verification and anti faking.
- Every reader is total. Every reader has a bit flip test.
- Take no step on an unmeasured claim. Isolate it and list it as open.

### Anti deviation tests

CE differs from vanilla in these places. Each row needs a test that fails if CE's behaviour is copied. Review every pull request against this table.

| Area | Vanilla rule | Test |
| --- | --- | --- |
| Executable table byte order | Little endian | The XP value for level 1 is 1, not 16,777,216. |
| Script `SWAP`, `SWAPA` | Real swap | Run the `BASIC.INT` epilogue. The frame pointer is restored and the result is 0. |
| Script stacks | 2,048 bytes each, 341 entries | The 342nd push fails and stops only that script. |
| Imported procedure calls | Unused by shipped scripts | A call returns the unsupported error and never enters the local table. |
| Stat 16 minimum | 0 | The minimum read from the executable is 0, not -60. |
| Explosion threshold and radiation resistance | Maximum 100 | The caps are 100, not 90. |
| Robot groin critical row | Entries 4 and 5 carry the random cripple flag | Check that one row. |
| RNG seeding | Writes `iv[0]` to `iv[31]` | Reproduce the seed vectors in the random numbers section. |
| DAT block chain | The LZSS header is the compressed size. Nothing truncates the output. | Decode every entry. Size and consumption both match. |
| LZSS edge cases | Ring indexes above 4,095 are unreachable. A pair that starts with 1 byte left is an error. | Feed a stream that ends mid pair. Expect an error, not a loop. |
| Pathfinder | No 800 step cap | A path longer than 800 steps is returned whole or fails on the node budget. |
| Player prototype | Lookup never leaves the prototype list | Looking up the player's prototype stays inside the list for any id. |
| Palette depth | Per file | `color.pal` reads as 8 bit and `dp.pal` as 6 bit. |
| Save booleans | 4 bytes each | The interface block is 16 bytes. |
| Save raw arrays | The world map grids bypass the integer writer | Block 20 is 1,028 bytes and the grids are unchanged. |
| Clipping | Partly visible tiles are drawn with a clipped blit | The floor pass issues a draw call for every square in the clamped rectangle. |
| Resolution | Fixed 640 by 480 surface | The framebuffer is identical for any window size. |
| Palette handling | Indexed art until the final present | Sprite pixels stay indexes in memory. |
| Audio | No decoder in the tree and no download at build time | The decoder trait returns `Unsupported` and the build needs no network. |

### Tests and fixtures

- Name each test after the claim it proves. Put the claim id and the evidence tag in a comment.
- Tests that need the player's install read `FO1_DATA` through the `fo1-tests` harness. The gate runs them with the ignored flag. They never create fake game data.
- Tests that need no game data use small synthetic inputs built in the test. CI runs only these.
- Turn the corpus counts into gate constants: 7,524 sprite files with 119,163 frames, 4,306 prototype files with no trailing bytes, 72 maps at version 19, 952 scripts, and 25,243 DAT entries that match their declared sizes.

### When to stop and ask the owner

- A licence or trademark question.
- A choice between the original 640 by 480 target and a wider view.
- Any change that makes a vanilla behaviour differ for the player's benefit.
- Any change that makes a file your code wrote unreadable by an earlier build.
- A claim that you can neither test nor tag.
- A protected file that looks wrong, or a gate test that looks wrong.
- A file that the hygiene script flags and that you believe belongs in the repository.
