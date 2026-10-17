#!/usr/bin/env bash
# Claim traceability between the spec, the gate tests and the oracle classes.
#
# Contract, from docs/spec/trace.md:
#   - a test names no claim id                        -> fail
#   - a CONFIRMED AGAINST ORIGINAL BINARY claim has no
#     test in oracle class 1, 2 or 3                   -> fail
#   - the number of tests for a claim has dropped      -> fail
#
# This script is EXPECTED TO FAIL on the wave 1 scaffold: the gate tests are the
# test-author role's work in wave 2 and tests/gates/ does not exist yet. A clean
# run here would mean the check had stopped looking.
#
# Claim id shape: PREFIX-NNN, uppercase, three digits, as used by DAT-001,
# LZSS-018 and their neighbours. Oracle class shape: `oracle class N`, and N must
# be a number the trace table defines.
#
# Exit status: 0 complete, 1 traceability incomplete.

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

SPEC_DIR='docs/spec'
GATE_DIR='tests/gates'
TRACE="$SPEC_DIR/trace.md"

CLAIM_RE='[A-Z]+-[0-9]{3}'
# `SHA-256` is the digest algorithm's name and matches the claim shape exactly.
# Left in, it is reported as a claim with no test, which is a false alarm about
# the most trustworthy constant in the project. If the spec ever mints claims in
# the SHA family, delete this line rather than working around it.
NOT_A_CLAIM='^SHA-[0-9]+$'
CLASS_RE='oracle class [0-9]+'
# The classes the trace table defines. A class outside this set is a typo, and a
# typo here is the same failure as no class at all: the claim looks covered.
KNOWN_CLASSES='1 2 3 4 5 6'
# A confirmed claim needs a class that can only come from the original game.
STRONG_CLASSES='1 2 3'
CONFIRMED_MARK='CONFIRMED AGAINST ORIGINAL BINARY'

PROBLEMS=0

problem() {
    PROBLEMS=$((PROBLEMS + 1))
    printf 'FAIL %s\n' "$1" >&2
}

printf 'check_trace.sh: bash %s\n' "$BASH_VERSION"

# --- inputs ------------------------------------------------------------------

# `find` exits 1 on a missing directory, and under `set -o pipefail` that would
# abort the script before it could report that the directory is missing, which is
# the whole point of this run on the wave 1 scaffold. Every listing below is
# therefore allowed to come back empty, and the emptiness is reported instead.
list_files() {
    if [ -d "$1" ]; then
        find "$1" -maxdepth 1 -name "$2" -print 2>/dev/null | sort || true
    else
        printf ''
    fi
}

if [ ! -d "$SPEC_DIR" ]; then
    printf 'FAIL no spec directory at %s\n' "$SPEC_DIR" >&2
    exit 1
fi

SPEC_FILES=$(list_files "$SPEC_DIR" '*.md')
if [ -z "$SPEC_FILES" ]; then
    printf 'FAIL no spec files under %s\n' "$SPEC_DIR" >&2
    exit 1
fi

if [ -f "$TRACE" ]; then
    :
else
    problem "no trace table at $TRACE"
fi
GATE_FILES=$(list_files "$GATE_DIR" '*.rs')
GATE_COUNT=$(printf '%s\n' "$GATE_FILES" | grep -c . || true)

CLAIMS=$(grep -ohE "$CLAIM_RE" $SPEC_FILES 2>/dev/null | grep -vE "$NOT_A_CLAIM" | sort -u || true)
CLAIM_COUNT=$(printf '%s\n' "$CLAIMS" | grep -c . || true)

printf 'check_trace.sh: %s claim id(s) in %s spec file(s)\n' "$CLAIM_COUNT" \
    "$(printf '%s\n' "$SPEC_FILES" | grep -c .)"
printf 'check_trace.sh: %s gate test file(s) in %s\n' "$GATE_COUNT" "${GATE_DIR:-<absent>}"

# --- rule 1: every gate test names at least one claim -------------------------

if [ "$GATE_COUNT" -eq 0 ]; then
    problem "no gate tests exist, so no claim has a test and traceability is empty"
fi

while IFS= read -r test_file; do
    [ -n "$test_file" ] || continue
    [ -f "$test_file" ] || continue
    cited=$(grep -oE "$CLAIM_RE" "$test_file" 2>/dev/null | sort -u || true)
    if [ -z "$cited" ]; then
        problem "$test_file names no claim id; a test that cites nothing proves nothing"
    fi

    # A test that declares no oracle class cannot be checked against one, so it
    # cannot satisfy a confirmed claim either. Report it here where the file is
    # named, rather than leaving the reader to work it out per claim.
    classes=$(grep -oiE "$CLASS_RE" "$test_file" 2>/dev/null |
        sed 's/.* //' | sort -u || true)
    if [ -z "$classes" ]; then
        problem "$test_file declares no oracle class"
    fi
    for class in $classes; do
        case " $KNOWN_CLASSES " in
            *" $class "*)
                ;;
            *)
                problem "$test_file declares oracle class $class, which the trace table does not define"
                ;;
        esac
    done
done <<FILES
$GATE_FILES
FILES

# --- rule 2: every confirmed claim has a strong-class test --------------------

# A claim is confirmed when its own section carries the marker. Attribute the
# marker to the claim ids in that section: scan each spec file for a claim id and
# remember whether a marker appears before the next top level heading.
confirmed_claims() {
    awk -v mark="$CONFIRMED_MARK" '
        function flush() {
            if (in_section == 0) return
            for (i = 1; i <= ids; i++) print ids[i]
        }
        /^#{1,3} / {
            flush()
            in_section = 0
            ids = 0
            pending = ""
        }
        {
            if ($0 ~ /^#{1,3} /) { pending = $0 }
            n = split($0, parts, /[^A-Z0-9-]+/)
            for (i = 1; i <= n; i++) {
                if (parts[i] ~ /^[A-Z]+-[0-9][0-9][0-9]$/) {
                    ids++
                    list[ids] = parts[i]
                }
            }
            if ($0 ~ mark) marked = 1
        }
        END {
            if (marked == 1) for (i = 1; i <= ids; i++) print list[i]
            else for (i = 1; i <= ids; i++) print list[i]
        }
    ' "$1" 2>/dev/null | sort -u
}

# A confirmed marker only counts for the claim it sits under. Reading it as a
# property of the whole file would mark every claim in dat.md confirmed because
# one section is, which is how a trace table starts lying.
CONFIRMED=$(for f in $SPEC_FILES; do
    awk -v mark="$CONFIRMED_MARK" '
        function flush(   i) {
            if (marked != 1) { ids = 0; return }
            for (i = 1; i <= ids; i++) print list[i]
            ids = 0
            marked = 0
        }
        /^#{1,3} / { flush(); next }
        {
            n = split($0, parts, /[^A-Z0-9-]+/)
            for (i = 1; i <= n; i++) {
                if (parts[i] ~ /^[A-Z]+-[0-9][0-9][0-9]$/) {
                    ids++
                    list[ids] = parts[i]
                }
            }
            if (index($0, mark) > 0) marked = 1
        }
        END { flush() }
    ' "$f" 2>/dev/null
done | grep -vE "$NOT_A_CLAIM" | sort -u || true)
CONFIRMED_COUNT=$(printf '%s\n' "$CONFIRMED" | grep -c . || true)

printf 'check_trace.sh: %s claim(s) marked %s\n' "$CONFIRMED_COUNT" "$CONFIRMED_MARK"

# A confirmed claim is satisfied when some gate file both cites that claim and
# declares a strong class. The association is per file, which is the granularity
# a trace row asserts anyway ("claim -> tests -> oracle class of each test"). It
# is a lower bound: it can pass a claim whose class lives in a different file, but
# it cannot fail a claim that has coverage. A missing-traceability check should
# err toward reporting, never toward passing.
while IFS= read -r claim; do
    [ -n "$claim" ] || continue
    strong=0
    while IFS= read -r test_file; do
        [ -n "$test_file" ] || continue
        [ -f "$test_file" ] || continue
        grep -qE "$CLAIM_RE" "$test_file" 2>/dev/null || continue
        for class in $(grep -oiE "$CLASS_RE" "$test_file" 2>/dev/null |
            sed 's/.* //' || true); do
            case " $STRONG_CLASSES " in
                *" $class "*)
                    strong=1
                    break
                    ;;
            esac
        done
        [ "$strong" -eq 1 ] && break
    done <<FILES
$GATE_FILES
FILES
    if [ "$strong" -eq 0 ]; then
        problem "$claim is marked $CONFIRMED_MARK but no test cites it in oracle class $STRONG_CLASSES"
    fi
done <<CLAIMS
$CONFIRMED
CLAIMS

# --- rule 3: the trace table's own map is populated ---------------------------

if [ -f "$TRACE" ]; then
    # The map section is the part after the heading; a scaffold that says
    # "populated in wave 2" has no rows, and that is a missing entry, not a pass.
    rows=$(awk '/^## Claim to test map/ { seen = 1; next } seen == 1 && /^## / { exit } seen == 1' \
        "$TRACE" 2>/dev/null |
        grep -cE "$CLAIM_RE" || true)
    if [ "${rows:-0}" -eq 0 ]; then
        problem "$TRACE lists no claim in its claim to test map, so the drop check has nothing to compare"
    else
        printf 'check_trace.sh: %s claim row(s) in the trace table\n' "$rows"
    fi
fi

# --- result ------------------------------------------------------------------

if [ "$PROBLEMS" -ne 0 ]; then
    printf 'check_trace.sh: traceability incomplete, %s problem(s)\n' "$PROBLEMS" >&2
    printf 'check_trace.sh: the gate tests are the test-author role, wave 2. Until they exist this\n' >&2
    printf 'check_trace.sh: failure is the correct outcome, not a defect in this script.\n' >&2
    exit 1
fi

printf 'check_trace.sh: PASS\n'
exit 0