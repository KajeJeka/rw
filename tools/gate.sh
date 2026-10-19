#!/usr/bin/env bash
# The pre-push gate. Ten steps, in order, stopping at the first failure.
#
# A step has one of three outcomes:
#   pass    the check ran and was satisfied
#   fail    the check ran and was not satisfied; the gate stops here
#   skipped the check could not run on this machine or in this state
#
# A skipped required step fails the gate. "Skipped" is not "passed": the guide's
# rule is that nothing reports success it has not measured, and a gate that
# treats an unmeasurable step as a green one cannot be used to decide whether to
# push.
#
# Usage:
#   tools/gate.sh [base-ref]
#
# base-ref defaults to origin/main, the ref the protected-path check diffs
# against. If it does not resolve, steps 1 and 8 say so and skip rather than
# quietly comparing against the wrong thing.
#
# Report: target/gate/report.md, written whatever the outcome, ending with the
# SHA-256 of everything above it.
#
# Exit status: 0 only when all ten steps passed.

set -uo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

BASE_REF="${1:-origin/main}"
REPORT_DIR='target/gate'
REPORT="$REPORT_DIR/report.md"
TOTAL_STEPS=10

# The dev-only band in tools/layers.txt. A crate in it is the proof (the
# reference decoders, the gate test host), not the code being proved, so the
# mutation step does not mutate it. The same number is in
# tools/check_layers.sh, which fails the gate if the two disagree.
HARNESS_BAND=9

STEP_NAME=()
STEP_STATE=()
STEP_DETAIL=()
FAILED_AT=''
SKIPPED_REQUIRED=''
INSTALL_OK=0

PROTECTED_PATHS='
tests/
crates/fo1-tests/
docs/spec/
spec/
tools/
.github/
clippy.toml
deny.toml
expected_counts.toml
mutants-allow.md
Cargo.toml
'

step() {
    STEP_NAME+=("$1")
    STEP_STATE+=("$2")
    STEP_DETAIL+=("$3")
}

skip_step() {
    # skip_step <number> <name> <reason> [required]
    step "$2" skip "$3"
    if [ "${4:-required}" = required ]; then
        SKIPPED_REQUIRED="$SKIPPED_REQUIRED $2"
    fi
    printf '\n=== step %s/10: %s === SKIPPED ===\n  %s\n' "$1" "$2" "$3"
}

have() {
    command -v "$1" >/dev/null 2>&1
}

tool_version() {
    local name="$1"
    shift
    if ! have "$name"; then
        printf 'absent'
        return 0
    fi
    local out
    out=$("$@" 2>&1 | head -1)
    [ -n "$out" ] || out='(no version output)'
    printf '%s' "$out"
}

# Some tools only answer a version question through their cargo subcommand, so
# a wrong flag would otherwise be printed into the report as if it were a
# version. Absent is an honest answer; a usage error is not a version.
cargo_subtool_version() {
    # cargo_subtool_version <subcommand>
    local sub="$1"
    if ! have "cargo-$sub"; then
        printf 'absent'
        return 0
    fi
    local out
    out=$(cargo "$sub" --version 2>&1 | head -1)
    if [ -n "$out" ]; then
        printf '%s' "$out"
    else
        printf 'present, version unknown'
    fi
}

# Run one numbered step. $1 number, $2 name, rest the command.
# Prints the command's own output so a failure is readable without the report.
run_step() {
    local number="$1" name="$2"
    shift 2
    printf '\n=== step %s/10: %s ===\n' "$number" "$name"
    local output status
    output=$("$@" 2>&1)
    status=$?
    printf '%s\n' "$output"
    if [ "$status" -eq 0 ]; then
        step "$name" pass 'ok'
        return 0
    fi
    step "$name" fail "$(printf '%s' "$output" | grep -v '^[[:space:]]*$' | tail -1)"
    FAILED_AT="$name"
    printf '=== step %s FAILED ===\n' "$number"
    return 1
}

# ---------------------------------------------------------------------------
# header
# ---------------------------------------------------------------------------

mkdir -p "$REPORT_DIR" || {
    printf 'gate.sh: cannot create %s\n' "$REPORT_DIR" >&2
    exit 1
}

COMMIT=$(git rev-parse HEAD 2>/dev/null || printf 'unknown')
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || printf 'unknown')
CARGO_VER=$(tool_version cargo cargo --version)
if have python; then
    PY_VER=$(tool_version python python --version)
else
    PY_VER=$(tool_version python3 python3 --version)
fi
GIT_VER=$(tool_version git git --version)
DENY_VER=$(cargo_subtool_version deny)
MUTANTS_VER=$(cargo_subtool_version mutants)

if git rev-parse --verify --quiet "$BASE_REF" >/dev/null 2>&1; then
    BASE_STATE=resolved
else
    BASE_STATE=UNRESOLVED
fi

printf 'gate.sh: commit %s on %s\n' "$COMMIT" "$BRANCH"
printf 'gate.sh: base ref %s (%s)\n' "$BASE_REF" "$BASE_STATE"

# ---------------------------------------------------------------------------
# 1 protected paths
# ---------------------------------------------------------------------------

if [ "$BASE_STATE" = UNRESOLVED ]; then
    skip_step 1 '1 protected paths' "$BASE_REF does not resolve, so no diff is possible"
else
    printf '\n=== step 1/10: 1 protected paths ===\n'
    CHANGED=$(git diff --name-only "$BASE_REF...HEAD" -- $PROTECTED_PATHS 2>/dev/null)
    if [ -z "$CHANGED" ]; then
        printf 'no protected path changed since %s\n' "$BASE_REF"
        step '1 protected paths' pass "no protected path changed since $BASE_REF"
    else
        printf '%s\n' "$CHANGED"
        printf 'FAIL these paths belong to another role or to the owner\n'
        step '1 protected paths' fail "protected paths changed: $(printf '%s' "$CHANGED" | tr '\n' ' ')"
        FAILED_AT='1 protected paths'
    fi
fi

# ---------------------------------------------------------------------------
# 2 public hygiene
# ---------------------------------------------------------------------------

if [ -z "$FAILED_AT" ]; then
    run_step 2 '2 public hygiene' bash tools/check_public.sh || true
fi

# ---------------------------------------------------------------------------
# 3 formatting, lints, layering
# ---------------------------------------------------------------------------

step_three() {
    printf -- '--- cargo fmt --check\n'
    cargo fmt --all -- --check || return 1
    printf -- '--- cargo clippy\n'
    cargo clippy --workspace --all-targets -- -D warnings || return 1
    printf -- '--- cargo doc\n'
    RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps || return 1
    printf -- '--- tools/check_layers.sh\n'
    bash tools/check_layers.sh || return 1
    printf -- '--- tools/lint_rules.sh\n'
    bash tools/lint_rules.sh || return 1
    printf -- '--- tools/lint_tests.sh\n'
    bash tools/lint_tests.sh || return 1
    printf -- '--- cargo deny\n'
    if ! have cargo-deny; then
        printf 'cargo-deny is not installed. To enable this check:\n'
        printf '  cargo install cargo-deny --locked\n'
        return 2
    fi
    cargo deny check || return 1
    return 0
}

if [ -z "$FAILED_AT" ]; then
    printf '\n=== step 3/10: 3 formatting, lints, layering ===\n'
    output=$(step_three 2>&1)
    status=$?
    printf '%s\n' "$output"
    if [ "$status" -eq 0 ]; then
        step '3 formatting, lints, layering' pass 'ok'
    elif [ "$status" -eq 2 ]; then
        skip_step 3 '3 formatting, lints, layering' 'cargo-deny is not installed'
    else
        step '3 formatting, lints, layering' fail "$(printf '%s' "$output" | grep -v '^[[:space:]]*$' | tail -1)"
        FAILED_AT='3 formatting, lints, layering'
    fi
fi

# ---------------------------------------------------------------------------
# 4 workspace tests
# ---------------------------------------------------------------------------

if [ -z "$FAILED_AT" ]; then
    # Release mode. The release profile keeps the overflow checks on, so a
    # wrapping arithmetic bug fails here instead of passing in a debug build
    # and breaking in a shipped binary. It is also twenty times faster, which
    # is what makes the install suite bearable.
    run_step 4 '4 workspace tests' cargo test --workspace --release || true
fi

# ---------------------------------------------------------------------------
# 5 install fingerprint
# ---------------------------------------------------------------------------

if [ -z "$FAILED_AT" ]; then
    printf '\n=== step 5/10: 5 install fingerprint ===\n'
    if [ -z "${FO1_DATA:-}" ]; then
        printf 'FO1_DATA is not set. The gate will not guess an install path.\n'
        printf 'To enable this step:\n'
        printf "  export FO1_DATA='/path/to/Fallout'\n"
        skip_step 5 '5 install fingerprint' 'FO1_DATA is not set'
    else
        printf 'FO1_DATA=%s\n' "$FO1_DATA"
        # The constants live in the harness, so the harness is what checks them.
        if cargo test -p fo1-tests --release -- --ignored --nocapture; then
            step '5 install fingerprint' pass 'FO1_DATA is set and the fingerprint matched'
            INSTALL_OK=1
        else
            step '5 install fingerprint' fail 'the fingerprint check did not pass'
            FAILED_AT='5 install fingerprint'
        fi
    fi
fi

# ---------------------------------------------------------------------------
# 6 ignored tests
# ---------------------------------------------------------------------------

if [ -z "$FAILED_AT" ]; then
    if [ "$INSTALL_OK" -eq 1 ]; then
        run_step 6 '6 ignored tests' cargo test --workspace --release -- --ignored || true
    else
        skip_step 6 '6 ignored tests' 'step 5 did not pass, so there is no install to test against'
    fi
fi

# ---------------------------------------------------------------------------
# 7 count check
# ---------------------------------------------------------------------------

# expected_counts.toml is `key value`, one per line. The floor this step cares
# about is min_synthetic_tests: the number of tests `cargo test --workspace` has
# to execute without reading the player's install.
count_floor() {
    sed -n 's/^[[:space:]]*min_synthetic_tests[[:space:]]\+\([0-9][0-9]*\).*/\1/p' \
        expected_counts.toml | head -1
}

count_actual() {
    cargo test --workspace -- --list 2>/dev/null |
        grep -c ': test$'
}

if [ -z "$FAILED_AT" ]; then
    printf '\n=== step 7/10: 7 count check ===\n'
    if [ ! -f expected_counts.toml ]; then
        skip_step 7 '7 count check' 'expected_counts.toml is missing'
    else
        FLOOR=$(count_floor)
        if [ -z "$FLOOR" ]; then
            skip_step 7 '7 count check' 'expected_counts.toml declares no min_synthetic_tests'
        else
            ACTUAL=$(count_actual)
            printf 'min_synthetic_tests floor is %s\n' "$FLOOR"
            printf 'cargo test --workspace lists %s test(s)\n' "${ACTUAL:-0}"
            if [ -z "$ACTUAL" ]; then
                step '7 count check' fail 'the test count could not be read, so no floor was proven'
                FAILED_AT='7 count check'
            elif [ "$ACTUAL" -lt "$FLOOR" ]; then
                printf 'FAIL the count is below the floor: %s < %s\n' "$ACTUAL" "$FLOOR"
                step '7 count check' fail "count dropped: $ACTUAL < $FLOOR"
                FAILED_AT='7 count check'
            else
                step '7 count check' pass "$ACTUAL >= $FLOOR"
            fi
        fi
    fi
fi

# ---------------------------------------------------------------------------
# 8 fail before, pass after
# ---------------------------------------------------------------------------

if [ -z "$FAILED_AT" ]; then
    printf '\n=== step 8/10: 8 fail before, pass after ===\n'
    if [ "$BASE_STATE" = UNRESOLVED ]; then
        skip_step 8 '8 fail before, pass after' "$BASE_REF does not resolve, so there is no before"
    elif ! have cargo-mutants; then
        printf 'cargo-mutants is not installed, so no mutant can be introduced.\n'
        printf 'To enable this step:\n'
        printf '  cargo install cargo-mutants --locked\n'
        skip_step 8 '8 fail before, pass after' 'cargo-mutants is not installed'
    elif ! have cargo; then
        skip_step 8 '8 fail before, pass after' 'cargo is not installed'
    else
        # Each mutant must fail before the fix and pass after it. The run itself
        # belongs to the mutation step; what this step checks is that the
        # machinery is present to produce that evidence at all.
        # `--list` is the enumeration check. `--dry-run` was removed in
        # cargo-mutants 22 and is a hard argument error in 27, so the old
        # spelling failed this step on a healthy toolchain.
        if cargo mutants --workspace --list >/dev/null 2>&1; then
            step '8 fail before, pass after' pass 'cargo-mutants can enumerate the workspace'
        else
            printf 'FAIL cargo-mutants could not enumerate the workspace\n'
            step '8 fail before, pass after' fail 'cargo-mutants could not enumerate the workspace'
            FAILED_AT='8 fail before, pass after'
        fi
    fi
fi

# ---------------------------------------------------------------------------
# 9 traceability
# ---------------------------------------------------------------------------

if [ -z "$FAILED_AT" ]; then
    run_step 9 '9 traceability' bash tools/check_trace.sh || true
fi

# ---------------------------------------------------------------------------
# 10 mutation testing
# ---------------------------------------------------------------------------

if [ -z "$FAILED_AT" ]; then
    printf '\n=== step 10/10: 10 mutation testing ===\n'
    if [ "${RW_MUTANTS:-0}" != '1' ]; then
        printf 'RW_MUTANTS is not 1. Mutation testing costs minutes, so it is opt in.\n'
        printf 'To enable this step:\n'
        printf '  export RW_MUTANTS=1\n'
        skip_step 10 '10 mutation testing' 'RW_MUTANTS is not 1'
    elif ! have cargo-mutants; then
        skip_step 10 '10 mutation testing' 'cargo-mutants is not installed'
    else
        # The crates under test are the readers: every workspace member that
        # is not in the dev-only band. The guide says to run mutation on the
        # readers, and the layer file is where the project declares which
        # crates those are. The dev-only band holds the reference decoders
        # and the gate test host, which are the proof, not the thing being
        # proved, and mutating them measures the proof rather than the code.
        MUTANT_CRATES=()
        while read -r band crate; do
            case "$band" in
                ''|'#'*) continue ;;
            esac
            [ "$band" = "$HARNESS_BAND" ] && continue
            MUTANT_CRATES+=(-p "$crate")
        done < tools/layers.txt

        if [ "${#MUTANT_CRATES[@]}" -eq 0 ]; then
            printf 'FAIL tools/layers.txt names no crate to mutate\n'
            step '10 mutation testing' fail 'tools/layers.txt names no crate to mutate'
            FAILED_AT='10 mutation testing'
        else
            # The gate tests live in the `rw-gate` package and the crates
            # under test carry their own unit tests. BOTH have to run:
            # cargo-mutants runs the tests of the mutated crate by default, and
            # `--test-package` REPLACES that with the named package's tests,
            # which drops the mutated crate's own unit tests. On fo1-exe that
            # is the difference between 148 and 68 surviving mutants.
            #
            # `--test-workspace true` runs every test in the workspace, so the
            # gate tests and the crates' own unit tests both grade each
            # mutant.
            #
            # `--profile release` builds the mutant and runs the tests the way
            # the gate runs everything else, and the release profile keeps the
            # overflow checks on.
            MUTANTS_LOG="$REPORT_DIR/mutants.log"
            MUTANTS_STATUS=0
            cargo mutants \
                "${MUTANT_CRATES[@]}" \
                --test-workspace true \
                --profile release \
                --no-shuffle \
                -j "${RW_MUTANTS_JOBS:-4}" \
                --timeout 1200 \
                > "$MUTANTS_LOG" 2>&1 || MUTANTS_STATUS=$?
            cat "$MUTANTS_LOG"

            # A run that caught nothing has proved nothing. cargo-mutants exits
            # zero when it finds no surviving mutant, and it also exits zero
            # when it tested no mutant at all, so the exit status alone cannot
            # tell "the suite is strong" from "the suite never ran". The count
            # of caught mutants is what tells them apart.
            MUTANTS_CAUGHT=$(sed -n 's/^[0-9][0-9]* mutants\? tested.*[^0-9]\([0-9][0-9]*\) caught.*/\1/p' \
                "$MUTANTS_LOG" | tail -1)
            MUTANTS_TESTED=$(sed -n 's/^\([0-9][0-9]*\) mutants\? tested.*/\1/p' "$MUTANTS_LOG" | tail -1)

            if [ "$MUTANTS_STATUS" -ne 0 ]; then
                step '10 mutation testing' fail 'a mutant survived'
                FAILED_AT='10 mutation testing'
            elif [ -z "$MUTANTS_CAUGHT" ]; then
                printf 'FAIL the run reported no caught count, so nothing was proven\n'
                step '10 mutation testing' fail 'the run reported no caught count'
                FAILED_AT='10 mutation testing'
            elif [ "$MUTANTS_CAUGHT" -eq 0 ]; then
                printf 'FAIL %s mutant(s) tested and 0 caught, so the suite proved nothing\n' \
                    "${MUTANTS_TESTED:-0}"
                step '10 mutation testing' fail "0 of ${MUTANTS_TESTED:-0} mutants caught"
                FAILED_AT='10 mutation testing'
            else
                step '10 mutation testing' pass \
                    "$MUTANTS_CAUGHT of ${MUTANTS_TESTED:-?} mutants caught, none survived"
            fi
        fi
    fi
fi

# ---------------------------------------------------------------------------
# account for the steps that never ran
# ---------------------------------------------------------------------------

COUNT=${#STEP_NAME[@]}
NUMBER=$((COUNT + 1))
while [ "$NUMBER" -le "$TOTAL_STEPS" ]; do
    # These did not run because an earlier step failed. The gate has already
    # failed, so they are recorded as not-run rather than as required skips:
    # "the environment could not do it" and "an earlier step stopped us" are
    # different findings and the report must not conflate them.
    case "$NUMBER" in
        4) NOT_RUN_NAME='4 workspace tests' ;;
        5) NOT_RUN_NAME='5 install fingerprint' ;;
        6) NOT_RUN_NAME='6 ignored tests' ;;
        7) NOT_RUN_NAME='7 count check' ;;
        8) NOT_RUN_NAME='8 fail before, pass after' ;;
        9) NOT_RUN_NAME='9 traceability' ;;
        10) NOT_RUN_NAME='10 mutation testing' ;;
        *) NOT_RUN_NAME="step $NUMBER" ;;
    esac
    step "$NOT_RUN_NAME" skip 'an earlier step failed, so this never ran'
    SKIPPED_REQUIRED="$SKIPPED_REQUIRED $NOT_RUN_NAME"
    printf '\n=== step %s/10 === SKIPPED (an earlier step failed)\n' "$NUMBER"
    NUMBER=$((NUMBER + 1))
done

# ---------------------------------------------------------------------------
# result
# ---------------------------------------------------------------------------

RESULT='PASS'
if [ -n "$FAILED_AT" ]; then
    RESULT='FAIL'
elif [ -n "$SKIPPED_REQUIRED" ]; then
    RESULT='FAIL'
fi

{
    printf '# Gate report\n\n'
    printf '| field | value |\n| --- | --- |\n'
    printf '| commit | `%s` |\n' "$COMMIT"
    printf '| branch | `%s` |\n' "$BRANCH"
    printf '| base ref | `%s` (%s) |\n' "$BASE_REF" "$BASE_STATE"
    printf '| finished | %s |\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
    printf '| result | **%s** |\n\n' "$RESULT"

    printf '## Tool versions\n\n'
    printf '| tool | version |\n| --- | --- |\n'
    printf '| bash | %s |\n' "$BASH_VERSION"
    printf '| cargo | %s |\n' "$CARGO_VER"
    printf '| python | %s |\n' "$PY_VER"
    printf '| git | %s |\n' "$GIT_VER"
    printf '| sha256sum | %s |\n' "$(tool_version sha256sum sha256sum --version)"
    printf '| cargo-deny | %s |\n' "$DENY_VER"
    printf '| cargo-mutants | %s |\n' "$MUTANTS_VER"
    printf '| FO1_DATA | %s |\n' "$([ -n "${FO1_DATA:-}" ] && printf 'set' || printf 'unset')"
    printf '| RW_MUTANTS | %s |\n' "${RW_MUTANTS:-unset}"
    printf '\n## Steps\n\n'
    printf '| step | name | outcome | detail |\n| --- | --- | --- | --- |\n'
    index=0
    while [ "$index" -lt "${#STEP_NAME[@]}" ]; do
        printf '| %s | %s | %s | %s |\n' "$((index + 1))" \
            "${STEP_NAME[$index]}" "${STEP_STATE[$index]}" "${STEP_DETAIL[$index]}"
        index=$((index + 1))
    done

    printf '\n## Notes\n\n'
    printf -- '- A skipped step is not a passed step. A run with any skip is a FAIL.\n'
    [ -n "$SKIPPED_REQUIRED" ] &&
        printf -- '- Skipped:%s\n' "$SKIPPED_REQUIRED"
    [ -n "$FAILED_AT" ] &&
        printf -- '- First failure: %s\n' "$FAILED_AT"
    printf -- '- %s of %s steps recorded a verdict.\n' "${#STEP_NAME[@]}" "$TOTAL_STEPS"
    printf -- '- %s step(s) skipped, %s step(s) failed.\n' \
        "$(printf '%s\n' "${STEP_STATE[@]}" | grep -c '^skip$' || true)" \
        "$(printf '%s\n' "${STEP_STATE[@]}" | grep -c '^fail$' || true)"

    printf '\n## Report digest\n\n'
} >"$REPORT"

# The digest covers every byte before the digest heading, so writing the digest
# cannot change what it certifies.
BODY_DIGEST=$(sed '/^## Report digest/,$d' "$REPORT" | sha256sum | cut -d' ' -f1)
printf 'The SHA-256 of everything above, computed with:\n\n```\nsed "/^## Report digest/,$ d" %s | sha256sum\n```\n\n' "$REPORT" >>"$REPORT"
printf '`%s`\n' "$BODY_DIGEST" >>"$REPORT"

printf '\n%s\n' '=== gate result ==='
printf 'report: %s\n' "$REPORT"
printf 'digest: %s\n' "$BODY_DIGEST"
index=0
while [ "$index" -lt "${#STEP_NAME[@]}" ]; do
    printf '  %-38s %s\n' "${STEP_NAME[$index]}" "${STEP_STATE[$index]}"
    index=$((index + 1))
done
printf 'result: %s\n' "$RESULT"

[ "$RESULT" = 'PASS' ] || exit 1
exit 0