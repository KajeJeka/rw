#!/usr/bin/env bash
# Banned pattern lint over the gate tests, plus one production ban.
#
# Guide: Verification and anti faking, "Banned patterns". That section exists
# because "an agent told to make the tests pass takes the cheapest path", and
# these are the cheapest paths.
#
# In a test file:
#   - assert!(true)
#   - a #[test] body with no assertion and no `?`
#   - a comparison other than == or != in an assertion
#   - #[ignore] with no reason
#   - continue, break, an early return, or `if let Ok` inside a loop
#   - .ok(), unwrap_or, unwrap_or_default, if let Ok, `let Ok(..) =`
#   - catch_unwind, Command::new
#   - a read of FO1_DATA outside crates/fo1-tests
# In a production source:
#   - cfg(debug_assertions)
#
# A test file is a .rs file under tests/ at the root or under any crate's tests/
# directory, plus any production source that declares a #[cfg(test)] module.
# The last clause matters: crates/fo1-tests keeps its oracles in src/, and a lint
# that silently skipped them would be reporting a clean run it never performed.
#
# Exit status: 0 clean, 1 at least one hit.

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

SCRATCH=$(mktemp)
cleanup() {
    rm -f "$SCRATCH" "$SCRATCH.code"
    return 0
}
trap cleanup EXIT

VIOLATIONS=0
CHECKED=0

hit() {
    VIOLATIONS=$((VIOLATIONS + 1))
    printf 'FAIL [%s] %s:%s\n' "$1" "$2" "$3" >&2
    printf '     %s\n' "$4" >&2
}

# Every grep below writes to $SCRATCH and is read with `while ... < "$SCRATCH"`,
# never through a pipe. A pipeline puts the reader in a subshell, where the
# counter above is lost: the script would print the FAIL lines and then, with the
# counter still zero, print PASS. That is the exact failure this file exists to
# prevent, so it must not contain one itself.
#
# Comments are stripped before matching. A rule is about code: this file, and the
# crate it lints, both *discuss* `unwrap_or` and `#[ignore]` in prose, and a lint
# that flags its own documentation teaches everyone to route around it.
scan() {
    # scan <rule> <ere> <path>
    local n line
    : >"$SCRATCH"
    sed 's|//.*||' -- "$3" >"$SCRATCH.code"
    grep -nE "$2" -- "$SCRATCH.code" 2>/dev/null | cut -d: -f1 >"$SCRATCH" || true
    while IFS= read -r n; do
        [ -n "$n" ] || continue
        line=$(sed -n "${n}p" -- "$3")
        hit "$1" "$3" "$n" "$line"
    done <"$SCRATCH"
}

# --- test files --------------------------------------------------------------

# A test file is a .rs file under a tests/ directory, or any source that carries
# a #[test] attribute. The attribute is the test: crates/fo1-tests/src/canary.rs
# is test code even though its parent, not itself, carries the #[cfg(test)], and
# its siblings ref_dat.rs and sha256.rs are oracle code even though they live in
# the same directory. Classifying by content rather than by path is what keeps
# the test rules from being applied to the wrong crate.
#
# The install rule matches the *call*, not the name. `#[ignore = "needs
# FO1_DATA"]` names the variable to explain itself and reads nothing, so a bare
# string match would fail the one test that documents why it is skipped.
test_files() {
    find . -type d -name tests \
        -not -path './target*' -not -path '*/target/*' -print 2>/dev/null |
        while read -r dir; do
            find "$dir" -type f -name '*.rs' -print 2>/dev/null
        done
    grep -rlE '#\[test\]' --include='*.rs' crates src 2>/dev/null || true
}

production_files() {
    find crates src -type f -name '*.rs' 2>/dev/null |
        grep -v '/tests/' |
        sort -u
}

# A #[test] body with nothing in it that can fail.
check_test_bodies() {
    awk '
        { sub(/\/\/.*/, "") }
        function step(  o, c) {
            if ($0 ~ /assert|panic!|should_panic|unreachable!|expect\(|unwrap\(|\?/) asserts++
            o = gsub(/\{/, "{")
            c = gsub(/\}/, "}")
            depth += o - c
            if (depth <= 0) {
                if (asserts == 0) printf "%d\t%s\n", fn_no, fn_text
                inside = 0
            }
        }
        /#\[test\]/ { pending = 1; next }
        pending == 1 {
            # Skip attributes, comments and blank lines between the marker and
            # the signature; the body starts at the first `fn`.
            if ($0 ~ /^[[:space:]]*($|\/\/|\/\*|\*|#\[)/) next
            pending = 0
            if ($0 !~ /fn[ \t]/) next
            fn_no = FNR
            fn_text = $0
            depth = 0
            asserts = 0
            inside = 1
            step()
            next
        }
        inside == 1 { step() }
    ' "$1" >"$SCRATCH" || {
        printf 'lint_tests.sh: awk failed on %s\n' "$1" >&2
        return 1
    }
    local line
    while IFS=$'\t' read -r line_no line_text; do
        [ -n "$line_no" ] || continue
        hit "test-without-assertion" "$1" "$line_no" "$line_text"
    done <"$SCRATCH"
}

# continue, break, an early return, or `if let Ok` inside a loop body.
check_loop_skips() {
    # Rust writes `for pat in expr {` with no parentheses and `while cond {`
    # with none either, so the loop keyword is followed by a space or a brace.
    # Matching on `for(` alone is why this rule was silent on the first run.
    awk '
        BEGIN { depth = 0; inside = 0 }
        {
            sub(/\/\/.*/, "")
            was = inside
            if ($0 ~ /(^|[^A-Za-z_])(for|while)([ \t]|\()/ || $0 ~ /(^|[^A-Za-z_])loop[ \t]*\{/) {
                inside = 1
                depth = 0
            }
            if (inside == 1) {
                if ($0 ~ /(^|[^A-Za-z_])(continue|break)([^A-Za-z_]|$)/) {
                    printf "%s\t%s\n", FNR, $0
                } else if ($0 ~ /(^|[^A-Za-z_])return([^A-Za-z_]|$)/) {
                    printf "%s\t%s\n", FNR, $0
                } else if ($0 ~ /if[ \t]+let[ \t]+Ok/) {
                    printf "%s\t%s\n", FNR, $0
                }
                o = gsub(/\{/, "{")
                c = gsub(/\}/, "}")
                depth += o - c
                if (depth < 0) inside = 0
            }
            if (was == 1 && inside == 1 && $0 ~ /^[[:space:]]*\}/ && depth <= 0) inside = 0
        }
    ' "$1" >"$SCRATCH" || {
        printf 'lint_tests.sh: awk failed on %s\n' "$1" >&2
        return 1
    }
    local line_no line_text
    while IFS=$'\t' read -r line_no line_text; do
        [ -n "$line_no" ] || continue
        hit "skip-inside-loop" "$1" "$line_no" "$line_text"
    done <"$SCRATCH"
}

# A comparison operator other than == or != inside an assertion. Arrows and
# shifts are removed first so a closure body or a diagnostic message is not
# mistaken for a comparison.
check_ordering_comparison() {
    awk '
        { sub(/\/\/.*/, "") }
        /assert/ {
            t = $0
            gsub(/->/, "", t)
            gsub(/=>/, "", t)
            gsub(/<<|>>/, "", t)
            if (t ~ /</ || t ~ />/) printf "%d\t%s\n", FNR, $0
        }
    ' "$1" >"$SCRATCH" || {
        printf 'lint_tests.sh: awk failed on %s\n' "$1" >&2
        return 1
    }
    local line_no line_text
    while IFS=$'\t' read -r line_no line_text; do
        [ -n "$line_no" ] || continue
        hit "count-assertion-not-equality" "$1" "$line_no" "$line_text"
    done <"$SCRATCH"
}

# --- main --------------------------------------------------------------------

printf 'lint_tests.sh: bash %s\n' "$BASH_VERSION"

TEST_FILES=$(test_files | sed 's|^\./||' | sort -u)
TEST_COUNT=$(printf '%s\n' "$TEST_FILES" | grep -c . || true)
printf 'lint_tests.sh: %s test file(s)\n' "$TEST_COUNT"

if [ "$TEST_COUNT" -eq 0 ]; then
    printf 'note: no test files exist yet, so the test rules prove nothing.\n'
    printf 'note: gate tests belong in tests/gates/ and are the test-author role (guide: Separate the roles).\n'
fi

while IFS= read -r path; do
    [ -n "$path" ] || continue
    [ -f "$path" ] || continue
    CHECKED=$((CHECKED + 1))
    scan "assert-true" 'assert!\(true\)' "$path"
    scan "bare-ignore" '#\[ignore\][[:space:]]*$' "$path"
    scan "swallowed-error" '\.ok\(\)|unwrap_or_default|unwrap_or|if[[:space:]]+let[[:space:]]+Ok|let[[:space:]]+Ok\(' "$path"
    scan "process-escape" 'catch_unwind|Command::new' "$path"
    if [ "${path#crates/fo1-tests/}" != "$path" ]; then
        # The harness crate is the one place allowed to read the install: that is
        # the whole reason it exists (guide: Verification and anti faking).
        :
    else
        scan "install-read-in-test" 'env::(var|var_os)[[:space:]]*\([[:space:]]*"FO1_' "$path"
    fi
    check_test_bodies "$path"
    check_loop_skips "$path"
    check_ordering_comparison "$path"
done <<FILES
$TEST_FILES
FILES

while IFS= read -r path; do
    [ -n "$path" ] || continue
    [ -f "$path" ] || continue
    case "$path" in
        crates/fo1-tests/*) continue ;;
        */tests/*) continue ;;
    esac
    CHECKED=$((CHECKED + 1))
    scan "debug-assertions-in-production" 'debug_assertions' "$path"
done <<FILES
$(production_files)
FILES

printf 'lint_tests.sh: %s file(s) checked, %s hit(s)\n' "$CHECKED" "$VIOLATIONS"

if [ "$VIOLATIONS" -ne 0 ]; then
    printf 'lint_tests.sh: FAILED\n' >&2
    exit 1
fi

printf 'lint_tests.sh: PASS\n'
exit 0
