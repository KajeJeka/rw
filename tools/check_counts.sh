#!/usr/bin/env bash
# Gate test count floors. The gate's step 7, extracted into a script so that
# tools/gate.sh and the CI workflow run the SAME code instead of two copies.
#
# expected_counts.toml is `key value`, one per line, and EVERY key in it is a
# floor this script enforces. A key the script does not understand fails it: a
# floor nobody reads is a floor nobody keeps, and the guide says the counts may
# only rise.
#
# The keys are four kinds:
#   <gate file stem>   a floor on the number of #[test] functions in
#                      tests/gates/<key>.rs
#   min_synthetic_tests   the sum of the synthetic_* file counts
#   min_install_tests     the sum of the install_* file counts
#   min_harness_tests     the number of #[test] functions in crates/fo1-tests
#
# This script fails when:
#   - a line of expected_counts.toml is not `key <count>`;
#   - a key names neither one of the three totals above nor a gate test file in
#     the HEAD tree;
#   - a gate test file holds no test function, so the file proves nothing;
#   - a measured count is below its floor;
#   - expected_counts.toml declares no min_harness_tests, no min_synthetic_tests
#     or no min_install_tests;
#   - a file under tests/gates/ has no floor: its tests can then vanish
#     without anything noticing.
#
# The counts come from the HEAD tree, not from the working tree: the gate
# judges the commit, and a dirty tree is the owner's business, not the floor's.
# The floors come from expected_counts.toml in the working tree, which is the
# file the gate reads.
#
# The count of a file is the number of `#[test]` attribute lines in it. The
# attribute has to start its own line, so a `#[test]` inside a comment or
# inside a string is not a test. The floors in expected_counts.toml were
# measured that way, and the count is the number of test functions, not the
# number of times the word appears.
#
# Every key is printed with its floor and its measurement, so a reader can see
# what was compared and not only what failed.
#
# Environment: none. This script needs no game data, no private term list and
# no reference folder. It runs in CI unchanged.
#
# Exit status: 0 every floor is met, 1 at least one floor is not.

set -uo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

COUNTS_FILE='expected_counts.toml'
GATE_DIR='tests/gates'
HARNESS_DIR='crates/fo1-tests/src'

if ! git rev-parse --verify --quiet HEAD >/dev/null 2>&1; then
    printf 'FAIL check_counts.sh: no HEAD commit, so there is no tree to measure\n' >&2
    exit 1
fi

if [ ! -f "$COUNTS_FILE" ]; then
    printf 'FAIL %s is missing, so there is no floor to enforce\n' "$COUNTS_FILE" >&2
    exit 1
fi

# The number of #[test] functions in one file of the HEAD tree.
count_tests_in_head() {
    # count_tests_in_head <path in HEAD>
    local path="$1" n
    n=$(git show "HEAD:$path" 2>/dev/null | grep -c '^[[:space:]]*#\[test\]' || true)
    printf '%s' "${n:-0}"
}

# Does the HEAD tree hold this path?
head_has_path() {
    git cat-file -e "HEAD:$1" 2>/dev/null
}

COUNT_FAIL=0
SYNTHETIC_COUNT=0
INSTALL_COUNT=0
HARNESS_COUNT=0
SYNTHETIC_FLOOR=''
INSTALL_FLOOR=''
HARNESS_FLOOR=''
NAMED_FILES=''

printf 'check_counts.sh: bash %s\n' "$BASH_VERSION"
printf 'check_counts.sh: floors %s\n' "$COUNTS_FILE"
printf 'check_counts.sh: commit %s\n' "$(git rev-parse HEAD 2>/dev/null || printf 'unknown')"

# `key value`, one per line. A line that is not that shape is a malformed
# floor, and a malformed floor fails the check.
COUNT_PAIRS=$(sed -e 's/#.*$//' -e '/^[[:space:]]*$/d' "$COUNTS_FILE" |
    awk '{ if (NF != 2 || $2 !~ /^[0-9]+$/) print $1 " MALFORMED"; else print $1 " " $2 }')

while IFS=' ' read -r key floor; do
    [ -n "$key" ] || continue
    if [ "$floor" = MALFORMED ]; then
        printf 'FAIL the line for key %s is not `key <count>`\n' "$key"
        COUNT_FAIL=1
        continue
    fi
    case "$key" in
        min_synthetic_tests)
            SYNTHETIC_FLOOR=$floor
            ;;
        min_install_tests)
            INSTALL_FLOOR=$floor
            ;;
        min_harness_tests)
            HARNESS_FLOOR=$floor
            ;;
        *)
            path="$GATE_DIR/$key.rs"
            if ! head_has_path "$path"; then
                printf 'FAIL %s names the gate test file %s, which is not in the HEAD tree\n' \
                    "$key" "$path"
                printf '     floor %s, measured 0\n' "$floor"
                COUNT_FAIL=1
                continue
            fi
            count=$(count_tests_in_head "$path")
            NAMED_FILES="$NAMED_FILES $path"
            case "$key" in
                synthetic_*) SYNTHETIC_COUNT=$((SYNTHETIC_COUNT + count)) ;;
                install_*) INSTALL_COUNT=$((INSTALL_COUNT + count)) ;;
            esac
            printf 'count  %-22s floor %-5s measured %s\n' "$key" "$floor" "$count"
            if [ "$count" -eq 0 ]; then
                printf 'FAIL %s has no test function, so the file proves nothing (floor %s)\n' \
                    "$key" "$floor"
                COUNT_FAIL=1
            elif [ "$count" -lt "$floor" ]; then
                printf 'FAIL %s: floor %s, measured %s\n' "$key" "$floor" "$count"
                COUNT_FAIL=1
            fi
            ;;
    esac
done <<EOF
$COUNT_PAIRS
EOF

# The harness is the reference decoders, counted apart from the gate surface,
# and it is not a file under tests/gates/.
if [ -n "$HARNESS_FLOOR" ]; then
    for hfile in $(git ls-tree -r --name-only HEAD "$HARNESS_DIR/" | grep '\.rs$'); do
        n=$(count_tests_in_head "$hfile")
        HARNESS_COUNT=$((HARNESS_COUNT + n))
    done
    printf 'count  %-22s floor %-5s measured %s\n' \
        'min_harness_tests' "$HARNESS_FLOOR" "$HARNESS_COUNT"
    if [ "$HARNESS_COUNT" -lt "$HARNESS_FLOOR" ]; then
        printf 'FAIL min_harness_tests: floor %s, measured %s\n' \
            "$HARNESS_FLOOR" "$HARNESS_COUNT"
        COUNT_FAIL=1
    fi
else
    printf 'FAIL %s declares no min_harness_tests\n' "$COUNTS_FILE"
    COUNT_FAIL=1
fi

if [ -n "$SYNTHETIC_FLOOR" ]; then
    printf 'count  %-22s floor %-5s measured %s\n' \
        'min_synthetic_tests' "$SYNTHETIC_FLOOR" "$SYNTHETIC_COUNT"
    if [ "$SYNTHETIC_COUNT" -lt "$SYNTHETIC_FLOOR" ]; then
        printf 'FAIL min_synthetic_tests: floor %s, measured %s\n' \
            "$SYNTHETIC_FLOOR" "$SYNTHETIC_COUNT"
        COUNT_FAIL=1
    fi
else
    printf 'FAIL %s declares no min_synthetic_tests\n' "$COUNTS_FILE"
    COUNT_FAIL=1
fi

if [ -n "$INSTALL_FLOOR" ]; then
    printf 'count  %-22s floor %-5s measured %s\n' \
        'min_install_tests' "$INSTALL_FLOOR" "$INSTALL_COUNT"
    if [ "$INSTALL_COUNT" -lt "$INSTALL_FLOOR" ]; then
        printf 'FAIL min_install_tests: floor %s, measured %s\n' \
            "$INSTALL_FLOOR" "$INSTALL_COUNT"
        COUNT_FAIL=1
    fi
else
    printf 'FAIL %s declares no min_install_tests\n' "$COUNTS_FILE"
    COUNT_FAIL=1
fi

# A gate test file with no floor is a file whose tests can vanish without the
# gate noticing, which is the hole this check exists to close. Every file
# under tests/gates/ must be named by a key.
for gfile in $(git ls-tree --name-only HEAD "$GATE_DIR/" | grep '\.rs$'); do
    case " $NAMED_FILES " in
        *" $gfile "*) ;;
        *)
            printf 'FAIL no floor in %s names the gate test file %s\n' \
                "$COUNTS_FILE" "$gfile"
            COUNT_FAIL=1
            ;;
    esac
done

if [ "$COUNT_FAIL" -ne 0 ]; then
    printf 'check_counts.sh: FAILED, a floor in %s was not met\n' "$COUNTS_FILE"
    exit 1
fi

printf 'check_counts.sh: PASS, every floor in %s is met\n' "$COUNTS_FILE"
exit 0
