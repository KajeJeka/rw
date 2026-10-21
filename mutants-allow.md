# PROTECTED file. Approved surviving mutants.

`tools/gate.sh` step 10 runs `cargo mutants`. A surviving mutant marks either a
gap in the proof or dead code, and fails the milestone.

This file is the only place a surviving mutant may be recorded. Every entry
needs a reason and the owner's approval. An empty list is the normal state.

## The format of an entry

One entry per line, under "Approved survivors" below, in this shape:

    <crate> :: <file:line> :: <mutant description> :: <reason>

The separator is `" :: "`, with a space on each side. The entry is read as
the first three separators and the reason is everything after the third, so
the reason may hold a separator of its own.

- `<crate>` — the workspace crate the mutant is in, e.g. `fo1-exe`.
- `<file:line>` — the workspace-relative path and the line, as cargo-mutants
  reports them, e.g. `crates/fo1-exe/src/lib.rs:106`. The line is the line of
  the mutation, not of the function. The column may be added; the gate
  compares the path and the line.
- `<mutant description>` — the mutation cargo-mutants reports, exactly, e.g.
  `replace 1 with 0`. The description must be the survivor's description and
  not a part of it: the gate compares it for equality, so an entry that says
  `replace` admits nothing but the mutant whose description is exactly
  `replace`. A description may hold `::` and ` | `, which is why the
  separator carries spaces: cargo-mutants writes a Rust path as
  `BigEndian<'a>::i32`, with no spaces around the colons, so `" :: "` cannot
  be confused with it. The description must not itself hold `" :: "`.
- `<reason>` — why this survivor is allowed. The owner approves it.

An example line, in the shape the gate reads (this line is an example and
lives here, outside the "Approved survivors" section, so it approves
nothing):

    fo1-bigendian :: crates/fo1-bigendian/src/lib.rs:219 :: replace | with ^ in BigEndian<'a>::u16 :: the low byte of the shifted word is zero, so | and ^ agree

A surviving mutant is admitted only when an entry lists it. A line in that
section that is not in that shape fails step 10, the same way a key in
`expected_counts.toml` that the gate does not understand fails step 7: an
approval nobody can read is not an approval. When a survivor is not admitted,
the gate prints the entry line it expects, so the owner can copy it verbatim
and write the reason. "None yet." is the empty list, and it is removed when
the first entry is added.

## Mutation testing runs

The gate mutates every workspace member that is not in the dev-only band, so
the scope is five crates. `tools/layers.txt` is where the bands live.

- `fo1-bigendian` — the big-endian asset byte order reader: the fold that
  puts the most significant byte first.
- `fo1-endian-host` — the little-endian executable byte order reader: the
  same fold for the tables compiled into `falloutw.exe`.
- `fo1-lzss` — decoder and ring buffer. Target: every surviving mutant must be
  explained by the write-index 4078 ruling, which three canary tests pin.
- `fo1-dat` — tree walk, payload-mode table, chain accounting.
- `fo1-exe` — section lookup, record sizes, table addressing.

## How the entries below were proved

Every entry below was proved equivalent TWICE: once by reasoning plus an
exhaustive sweep over the whole input domain, and once by a second reviewer
who re-derived the argument and ran an independent sweep. The sweeps were
built at `-C opt-level=0`, because at `-O3` the compiler proves the identity
and merges the two functions, and the sweep then measures nothing. Every
sweep carried a control - a change that is NOT equivalent on the same domain -
and every control fired.

## Approved survivors

fo1-bigendian :: crates/fo1-bigendian/src/lib.rs:219 :: replace | with ^ in BigEndian<'a>::u16 :: the fold's two operands never share a bit: `word << 8` has a zero low byte and `u16::from(byte)` a zero high byte, so `|` and `^` agree on every one of the 65,536 byte pairs
fo1-bigendian :: crates/fo1-bigendian/src/lib.rs:229 :: replace | with ^ in BigEndian<'a>::u32 :: the same disjointness in 32 bits, over every one of the 4,294,967,296 byte quads
fo1-bigendian :: crates/fo1-bigendian/src/lib.rs:240 :: replace | with ^ in BigEndian<'a>::i16 :: the same fold as the u16 reader over all 65,536 byte pairs, and the two's complement reinterpretation that follows is a bijection on the folded bits, so it cannot separate them
fo1-bigendian :: crates/fo1-bigendian/src/lib.rs:254 :: replace | with ^ in BigEndian<'a>::i32 :: the same fold as the u32 reader over all 4,294,967,296 byte quads, and the reinterpretation is a bijection
fo1-endian-host :: crates/fo1-endian-host/src/lib.rs:226 :: replace | with ^ in LittleEndian<'a>::u16 :: the little-endian fold joins the same two bit-disjoint operands, only in the other byte order, so `|` and `^` agree on every one of the 65,536 byte pairs
fo1-endian-host :: crates/fo1-endian-host/src/lib.rs:236 :: replace | with ^ in LittleEndian<'a>::u32 :: the same over every one of the 4,294,967,296 byte quads
fo1-endian-host :: crates/fo1-endian-host/src/lib.rs:247 :: replace | with ^ in LittleEndian<'a>::i16 :: the same over all 65,536 byte pairs, and the reinterpretation is a bijection
fo1-endian-host :: crates/fo1-endian-host/src/lib.rs:261 :: replace | with ^ in LittleEndian<'a>::i32 :: the same over all 4,294,967,296 byte quads, and the reinterpretation is a bijection
fo1-exe :: crates/fo1-exe/src/sha256.rs:195 :: replace ^ with | in compress :: the `ch` step computes `(e & f) ^ (!e & g)`, and the two operands can never both be 1: the first needs e's bit set and the second needs it clear, so the two are bit-disjoint and `^` and `|` agree for every one of the 2^96 word triples, and the digest of every message is unchanged
fo1-exe :: crates/fo1-exe/src/sha256.rs:202 :: replace ^ with | in compress :: the `maj` step computes the majority function per bit, and under Rust's precedence the change regroups it as `(a & b) | ((a & c) ^ (b & c))` or `((a & b) ^ (a & c)) | (b & c)`, each of which is still the majority per bit, so BOTH changes on this line are equal for every input
fo1-lzss :: crates/fo1-lzss/src/lib.rs:522 :: replace | with ^ in decode_token :: the back reference's source index is `b1 | ((b2 & 0xF0) << 4)`, where `b1` holds bits 0 to 7 and the shifted nibble holds bits 8 to 11, so the operands never share a bit and `|` and `^` agree on every one of the 65,536 byte pairs
