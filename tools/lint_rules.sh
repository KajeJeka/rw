#!/usr/bin/env bash
# Determinism and banned-pattern lint over the production sources.
#
# Guide: Engineering philosophy, "How each principle is checked", row
# "Determinism"; and Verification and anti faking, "Banned patterns", *In
# production code*.
#
# The rule this enforces is one sentence long: production code does not read the
# clock, does not start a thread, does not depend on hash map order, does not
# read the player's install, does not branch on test mode, and does not carry an
# allow attribute nobody approved.
#
# Every .rs file under crates/*/src and the root src/ is a production source,
# including crates/fo1-tests: its oracles ship in a crate and must be just as
# deterministic as the code they check. The one rule that does not apply there is
# the install read, because resolving the player's install is the harness's job.
EXEMPT_PREFIX='crates/fo1-tests/'
#
# Exit status: 0 clean, 1 at least one hit.

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

# --- configuration -----------------------------------------------------------

# Owner approved allow attributes, as `file:line` substrings or literal
# attribute texts. THE LIST IS EMPTY. Every entry needs the owner's approval and
# a written reason (guide: Banned patterns, "In production code").
#
# To extend it, add one exact substring per approved attribute, for example:
#
#   APPROVED_ALLOWS='
#   fo1-lzss/src/lib.rs:#![allow(clippy::arithmetic_side_effects)]
# '
#
# A bare `#[allow(` with no reason attribute is also caught by the workspace lint
# table's `allow_attributes_without_reason = "deny"`, so an approved entry still
# needs a `reason = "..."` to compile. This list is the second lock on the same
# door.
APPROVED_ALLOWS=''

# Determinism: no wall clock, no threads, no hash map or hash set order.
#
# BTreeMap and BTreeSet are deliberately NOT here. A sorted container has a
# defined iteration order, which is what determinism asks for; banning it would
# push the author toward a Vec, or worse, toward a HashMap.
DETERMINISM_PATTERNS='SystemTime|std::time::Instant|\bInstant\b|thread::spawn|std::thread::|HashMap|HashSet'

# No read of the player's install or of the private references, in production.
# Matched on the call rather than the bare name: a doc comment that says "the
# harness reads FO1_DATA" is documentation, and a lint that fails it teaches the
# author to write the comment somewhere less useful.
INSTALL_PATTERNS='env::(var|var_os)[[:space:]]*\([[:space:]]*"FO1_'

# No exit from a library, no test-mode branch outside a test module, no raw debug
# assertion switch.
BANNED_PATTERNS='std::process::exit|process::exit|debug_assertions'

VIOLATIONS=0
CHECKED=0
CFG_SCRATCH=$(mktemp)
cleanup() {
    if [ -n "$CFG_SCRATCH" ]; then rm -f "$CFG_SCRATCH" "$CFG_SCRATCH.code"; fi
    return 0
}
trap cleanup EXIT

hit() {
    local rule="$1" path="$2" line_no="$3" line="$4"
    VIOLATIONS=$((VIOLATIONS + 1))
    printf 'FAIL [%s] %s:%s\n' "$rule" "$path" "$line_no" >&2
    printf '     %s\n' "$line" >&2
}

# Files that are production sources, tracked or not.
production_sources() {
    git ls-files -z -- 'crates/*/src/*.rs' 'crates/*/src/**/*.rs' 'src/*.rs' 'src/**/*.rs' 2>/dev/null |
        tr '\0' '\n' || true
    # New sources are untracked until they are committed, so also walk the tree
    # on disk. git ls-files alone would miss a file before its first commit,
    # which is exactly when a banned pattern is cheapest to remove.
    find crates src -type f -name '*.rs' 2>/dev/null || true
}

# Deduplicate while keeping order.
unique_sorted() {
    sort -u
}

check_patterns() {
    local rule_id="$1" patterns="$2" path="$3"
    local n line
    # Comments are stripped before matching: a rule is about code, and a lint
    # that flags the prose explaining the rule teaches authors to hide it.
    : >"$CFG_SCRATCH"
    sed 's|//.*||' -- "$path" >"$CFG_SCRATCH.code"
    grep -nE "$patterns" -- "$CFG_SCRATCH.code" 2>/dev/null | cut -d: -f1 >"$CFG_SCRATCH" || true
    while IFS= read -r n; do
        [ -n "$n" ] || continue
        line=$(sed -n "${n}p" -- "$path")
        # Report the source line, which is this file's own text and is already in
        # the repository. A banned pattern is a token, never private material.
        hit "$rule_id" "$path" "$n" "$line"
    done <"$CFG_SCRATCH"
}

# `cfg(test)` is only allowed as the attribute of a `mod tests`. It is flagged
# anywhere else, including in production code that is compiled under test.
# awk carries a one line lag, so the look-ahead costs nothing and the script
# stays inside the portable subset these shell scripts use.
#
# awk writes to a temporary file rather than down a pipe: a pipeline runs the
# reader in a subshell, where the violation counter would be lost and the script
# would report a hit and then print PASS.
check_cfg_test() {
    local path="$1"
    local scratch="$CFG_SCRATCH"
    awk '
        function flush() {
            if (held == 0) return
            # `#[cfg(test)]` may gate a module and nothing else. Any name is
            # fine: `mod tests` inline, `mod canary;` for a file of oracles.
            if (prev ~ /mod[ \t]+[A-Za-z_][A-Za-z0-9_]*/) return
            if (cur ~ /mod[ \t]+[A-Za-z_][A-Za-z0-9_]*/) return
            printf "%s\t%s\n", prev_no, prev
        }
        {
            # `cur` must be the line just read BEFORE the flush: the flush asks
            # whether the line after the attribute is the test module.
            sub(/\/\/.*/, "")
            cur = $0
            flush()
            prev = $0
            prev_no = FNR
            held = (index($0, "cfg(test)") > 0) ? 1 : 0
        }
        END { flush() }
    ' "$path" >"$scratch" || true
    local line_no line
    while IFS=$'\t' read -r line_no line; do
        [ -n "$line_no" ] || continue
        hit "cfg-test-outside-test-module" "$path" "$line_no" "$line"
    done <"$scratch"
}

check_allow_attributes() {
    local path="$1"
    local n line allowed=0
    if [ -n "$APPROVED_ALLOWS" ]; then
        while IFS= read -r approved; do
            [ -n "$approved" ] || continue
            case "$approved" in
                *"$path"*) allowed=1 ;;
            esac
        done <<EOF
$(printf '%s\n' "$APPROVED_ALLOWS")
EOF
    fi
    # Matched against the comment-stripped copy so this script's own prose about
    # allow attributes does not trip the rule it is enforcing.
    sed 's|//.*||' -- "$path" >"$CFG_SCRATCH.code"
    grep -nE '#!?\[allow\(' -- "$CFG_SCRATCH.code" 2>/dev/null | cut -d: -f1 >"$CFG_SCRATCH" || true
    while IFS= read -r n; do
        [ -n "$n" ] || continue
        line=$(sed -n "${n}p" -- "$path")
        if [ "$allowed" -eq 1 ]; then
            case "$APPROVED_ALLOWS" in
                *"$line"*) continue ;;
            esac
        fi
        hit "unapproved-allow-attribute" "$path" "$n" "$line"
    done <"$CFG_SCRATCH"
}

# --- main --------------------------------------------------------------------

if ! command -v find >/dev/null 2>&1; then
    printf 'lint_rules.sh: find is required\n' >&2
    exit 1
fi

printf 'lint_rules.sh: bash %s\n' "$BASH_VERSION"
printf 'lint_rules.sh: approved allow attributes: %s\n' \
    "$(printf '%s' "$APPROVED_ALLOWS" | tr -cd '\n' | wc -c | tr -d ' ')"

FILES=$(production_sources | unique_sorted)
if [ -z "$FILES" ]; then
    printf 'lint_rules.sh: no production sources found, which cannot be right\n' >&2
    exit 1
fi

while IFS= read -r path; do
    [ -n "$path" ] || continue
    if [ ! -f "$path" ]; then
        continue
    fi
    CHECKED=$((CHECKED + 1))
    check_patterns "determinism" "$DETERMINISM_PATTERNS" "$path"
    case "$path" in
        "$EXEMPT_PREFIX"*)
            # The harness may read the install; nothing else about it is exempt.
            ;;
        *)
            check_patterns "install-read-in-production" "$INSTALL_PATTERNS" "$path"
            ;;
    esac
    check_patterns "banned-pattern" "$BANNED_PATTERNS" "$path"
    check_cfg_test "$path"
    check_allow_attributes "$path"
done <<EOF
$FILES
EOF

printf 'lint_rules.sh: %s production file(s) checked, %s hit(s)\n' "$CHECKED" "$VIOLATIONS"

if [ "$VIOLATIONS" -ne 0 ]; then
    printf 'lint_rules.sh: FAILED\n' >&2
    exit 1
fi

printf 'lint_rules.sh: PASS\n'
exit 0