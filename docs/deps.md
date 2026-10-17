# Third-party dependencies

**This workspace has zero third-party dependencies.** Every crate depends only on
the Rust standard library and on its siblings in this workspace.

| Crate | Third-party dependency | Version | Licence | Why |
| --- | --- | --- | --- | --- |
| all | none | - | - | the workspace builds from the standard library alone |

## Workspace members

These are path dependencies inside this repository, not third-party packages.
They carry no version and no licence of their own beyond this repository's.

| Crate | Depends on | Purpose |
| --- | --- | --- |
| `fo1-bigendian` | - | byte order primitives |
| `fo1-endian-host` | - | host byte order selection |
| `fo1-lzss` | - | the LZSS decompressor |
| `fo1-dat` | `fo1-bigendian`, `fo1-lzss` | the DAT tree reader |
| `fo1-exe` | `fo1-endian-host` | the PE reader |
| `fo1-tests` | - | the gate harness and its reference decoders |
| root crate | dev only: every member above | the binary |

The layering that keeps those edges one-directional is checked by
`tools/check_layers.sh`, with the bands in `tools/layers.txt`.

## What "zero" buys

Three things, and all three are worth the constraint:

- The build is reproducible from the Rust toolchain alone. No registry, no
  vendored tree, no lock file to go stale, no supply chain to audit.
- Every byte the project produces is traceable to code in this repository or to
  the standard library. There is no third path where a bug can hide.
- The licence surface stays empty, so there is nothing to attribute and no
  obligation that survives the project.

The standard library is not a workaround. `std` is the one dependency the project
is allowed to have, and it is part of the toolchain the project already requires.

## Adding one

Adding a third-party dependency is an owner decision, not an implementation
detail. Before one lands:

1. The owner approves it in writing, saying what it is for.
2. Its licence is checked and recorded in the table above.
3. `tools/gate.sh` step 3 runs `cargo deny`, which must accept it.

Until all three are done the dependency does not belong in the tree. A crate
added "just to parse one header" is the usual first step towards a build that
cannot be reproduced offline, and this project pays for that with every user who
has to trust a binary they cannot rebuild.

## Verified

`cargo metadata` reports no registry packages for any member. `tools/gate.sh`
re-checks this at every run through `cargo deny`.
