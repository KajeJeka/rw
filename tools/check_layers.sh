#!/usr/bin/env bash
# Workspace layering check. PROTECTED file companion: tools/layers.txt.
#
# Guide: Architecture. "Dependencies point down. A crate never uses a crate in a
# higher band. tools/check_layers.sh reads cargo metadata and fails the gate on a
# violation."
#
# Rules, all three checked:
#   1. every workspace member appears in tools/layers.txt;
#   2. no crate depends on a crate whose band is greater than or equal to its own;
#   3. no runtime crate (any band except the dev-only test harness band) depends
#      on the test harness.
#
# Band 9 is the dev-only band. Two band 9 crates may depend on each other through
# dev-dependencies, because a dev-dependency is not shipped and never appears in
# the runtime graph. Rule 2 applies to every non-dev edge without exception.
#
# The JSON is parsed with python or python3, whichever is found, and no JSON
# dependency is added to the workspace: this script runs outside cargo, and a
# published tool must not need a crate to read a file (guide: Few dependencies).
#
# Exit status: 0 clean, 1 at least one violation.

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

LAYERS_FILE="tools/layers.txt"
HARNESS_BAND=9
HARNESS_CRATE="fo1-tests"

if [ ! -f "$LAYERS_FILE" ]; then
    printf 'check_layers.sh: %s is missing\n' "$LAYERS_FILE" >&2
    exit 1
fi

# --- tool discovery ----------------------------------------------------------

CARGO=""
for candidate in cargo "$HOME/.cargo/bin/cargo"; do
    if command -v "$candidate" >/dev/null 2>&1; then
        CARGO=$(command -v "$candidate")
        break
    fi
done
if [ -z "$CARGO" ]; then
    printf 'check_layers.sh: cargo not found on PATH.\n' >&2
    printf '  install rustup, or export CARGO=/absolute/path/to/cargo\n' >&2
    exit 1
fi

PYTHON=""
for candidate in python python3; do
    if command -v "$candidate" >/dev/null 2>&1; then
        if "$candidate" -c 'import json,sys' >/dev/null 2>&1; then
            PYTHON=$(command -v "$candidate")
            break
        fi
    fi
done
if [ -z "$PYTHON" ]; then
    printf 'check_layers.sh: neither python nor python3 with a json module was found.\n' >&2
    printf '  check_layers.sh needs one of them to parse cargo metadata. jq would also work\n' >&2
    printf '  but no JSON dependency is added to the workspace for it.\n' >&2
    exit 1
fi

printf 'check_layers.sh: cargo %s (%s)\n' "$("$CARGO" -V 2>&1)" "$CARGO"
printf 'check_layers.sh: json   %s (%s)\n' "$("$PYTHON" -V 2>&1)" "$PYTHON"
printf 'check_layers.sh: layers %s\n' "$LAYERS_FILE"

# --- inputs ------------------------------------------------------------------

METADATA=$(mktemp)
LAYERS=$(mktemp)
REPORT=$(mktemp)
cleanup() {
    rm -f "$METADATA" "$LAYERS" "$REPORT"
    return 0
}
trap cleanup EXIT

# --offline keeps the metadata read from touching the network, and --no-deps
# keeps the output to the workspace itself.
if ! "$CARGO" metadata --no-deps --offline --format-version 1 >"$METADATA" 2>/dev/null; then
    if ! "$CARGO" metadata --no-deps --format-version 1 >"$METADATA"; then
        printf 'check_layers.sh: cargo metadata failed\n' >&2
        exit 1
    fi
fi

# Strip comments and blank lines, keep "band<TAB or spaces>name".
sed -e 's/#.*$//' -e '/^[[:space:]]*$/d' "$LAYERS_FILE" | tr -s ' \t' ' ' | sed -e 's/^ //' -e 's/ $//' >"$LAYERS"

if [ ! -s "$LAYERS" ]; then
    printf 'check_layers.sh: %s has no entries\n' "$LAYERS_FILE" >&2
    exit 1
fi

# --- the check ---------------------------------------------------------------

"$PYTHON" - "$METADATA" "$LAYERS" "$REPORT" <<'PYEOF' || exit 1
import json
import sys

metadata_path, layers_path, report_path = sys.argv[1], sys.argv[2], sys.argv[3]
HARNESS_BAND = 9

meta = json.load(open(metadata_path, encoding="utf-8"))

def norm(path):
    return path.replace("\\", "/").rstrip("/")

WORKSPACE_ROOT = norm(meta["workspace_root"]).lower()

def rel(path):
    """Path relative to the workspace root, for reporting."""
    text = norm(path)
    if text.lower().startswith(WORKSPACE_ROOT + "/"):
        return text[len(WORKSPACE_ROOT) + 1:]
    return text

def directory_of(manifest_path):
    return norm(manifest_path).rsplit("/", 1)[0].lower()

# band name -> band
bands = {}
for raw in open(layers_path, encoding="utf-8"):
    parts = raw.split()
    if len(parts) != 2:
        print(f"check_layers.sh: malformed layers line: {raw.strip()}", file=sys.stderr)
        sys.exit(2)
    try:
        bands[parts[1]] = int(parts[0])
    except ValueError:
        print(f"check_layers.sh: band is not an integer: {raw.strip()}", file=sys.stderr)
        sys.exit(2)

problems = []

# package id -> (name, manifest path)
packages = {p["id"]: (p["name"], p["manifest_path"]) for p in meta["packages"]}
members = meta["workspace_members"]

# Directory -> (name, manifest, band). cargo metadata --no-deps omits the resolved
# dependency id, so a workspace internal edge is identified by its path, which is
# exact and cannot be confused by a rename or by a crate on crates.io.
by_dir = {}
for p in meta["packages"]:
    by_dir[directory_of(p["manifest_path"])] = (p["name"], p["manifest_path"], p["id"])

member_names = []
for pid in members:
    if pid not in packages:
        print(f"check_layers.sh: workspace member id not in packages: {pid}", file=sys.stderr)
        sys.exit(2)
    name, manifest = packages[pid]
    relative = rel(manifest)
    member_names.append((name, manifest, relative))
    if name not in bands:
        problems.append(
            f"missing-band\t{name}\t{relative}\tis a workspace member but has no line in "
            f"tools/layers.txt, so its band is undeclared"
        )

lines = []
for name, manifest, relative in sorted(member_names):
    band = bands.get(name)
    lines.append(f"member\t{name}\t{relative}\tband={band}")
lines.append("")

# dependency edges, workspace internal only
for pid in members:
    if pid not in packages:
        continue
    src_name, src_manifest = packages[pid]
    src_band = bands.get(src_name)
    if src_band is None:
        continue
    src_rel = rel(src_manifest)
    pkg = next(p for p in meta["packages"] if p["id"] == pid)
    for dep in pkg.get("dependencies", []):
        if dep.get("source") is not None:
            continue  # crates.io or a git source, never a workspace member
        # dep["path"] is already a directory, so it is normalised and lowered
        # but never split: rsplit would drop the crate directory itself.
        dep_dir = norm(dep.get("path") or "").lower()
        target = by_dir.get(dep_dir)
        if target is None:
            problems.append(
                f"unresolved-dependency\t{src_name}\t{src_rel}\tpath dependency on "
                f"{dep.get('name')} at {rel(dep.get('path') or '')} is not a workspace member; a "
                f"workspace crate may only depend on a declared layer"
            )
            continue
        dep_name, dep_manifest, _ = target
        dep_rel = rel(dep_manifest)
        dep_band = bands.get(dep_name)
        if dep_band is None:
            problems.append(
                f"missing-band\t{dep_name}\t{dep_rel}\t{src_name} depends on it but it has no "
                f"line in tools/layers.txt"
            )
            continue
        kind = dep.get("kind") or "normal"
        lines.append(f"edge\t{src_name}\t{dep_name}\tkind={kind}\tband={src_band}->{dep_band}")

        if dep_band == HARNESS_BAND and src_band != HARNESS_BAND:
            problems.append(
                f"runtime-uses-harness\t{src_name}\t{dep_rel}\tband {src_band} crate depends on "
                f"the band {HARNESS_BAND} test harness {dep_name} (kind={kind}); the harness is a "
                f"dev-dependency only"
            )
        if kind != "dev" and dep_band >= src_band:
            problems.append(
                f"band-not-lower\t{src_name}\t{dep_rel}\tband {src_band} crate depends on band "
                f"{dep_band} crate {dep_name} (kind={kind}); a crate may depend only on a strictly "
                f"lower band"
            )

lines.append("")
if problems:
    lines.append(f"VIOLATIONS\t{len(problems)}")
    for p in problems:
        lines.append("PROBLEM\t" + p)
else:
    lines.append("VIOLATIONS\t0")

open(report_path, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
PYEOF

# --- report ------------------------------------------------------------------

VIOLATION_COUNT=$(awk -F'\t' '$1 == "VIOLATIONS" { print $2 }' "$REPORT")

grep -E '^(member|edge)\b' "$REPORT" | sed 's/^/  /'
printf 'check_layers.sh: %s workspace member(s), %s violation(s)\n' \
    "$(grep -c '^member' "$REPORT" || true)" "$VIOLATION_COUNT"

if [ "$VIOLATION_COUNT" -ne 0 ]; then
    awk -F'\t' '$1 == "PROBLEM" {
        printf "FAIL [%s] %s\n", $2, $3
        printf "     %s\n", $4
    }' "$REPORT" >&2
    printf 'check_layers.sh: FAILED\n' >&2
    exit 1
fi

printf 'check_layers.sh: PASS\n'
exit 0