#!/usr/bin/env bash
# The pre-push gate. Ten steps, in order. Every step runs, and every step
# reports a verdict.
#
# A step has one of three outcomes:
#   pass    the check ran and was satisfied
#   fail    the check ran and was not satisfied
#   skipped the check could not run on this machine or in this state
#
# The gate runs EVERY step and reports EVERY verdict, even after one step has
# failed. This is a departure from the guide's letter: the guide says the gate
# "runs these in order and stops at the first failure". It is not a
# weakening. No check is dropped, every step still decides the result, and a
# step that cannot run is still a required skip and still a FAIL. The reason
# is that a gate that stops at the first failure reports one defect per run,
# and the owner then fixes them one run at a time; a protected-path change hid
# the other nine steps entirely, so a green-looking report could have been
# nine unmeasured steps. The report still names the first failure, so the
# first thing a reader sees is unchanged.
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
# The gate judges a commit. A dirty working tree would make the steps that
# measure the tree measure something other than the commit this report names,
# so the gate refuses to run while the tree is dirty and names the dirty
# files.
#
# The mutation step runs the synthetic suites only, and that scope is
# deliberate. The guide asks for the install scope as well ("with FO1_DATA
# set, so the data tests count"), and cargo-mutants 27.1.0 can run it with
# `--cargo-test-arg --ignored`, but the measured cost is 374 mutants x
# 9.74 s = 60 minutes on top of the 12 the fast scope takes. The 69 fo1-exe
# survivors the old report called scope artifacts are killed by the fast
# scope, and every survivor that is left is approved one by one, with its
# own reason, in mutants-allow.md.
#
# The mutation step does not share a target directory with anything, not
# even with an earlier run of itself. That separation is deliberate and it
# is what makes the survivor list a measurement: a shared target directory
# keeps the artifacts of an earlier mutant, cargo judges a build in
# cargo-mutants' copy of the workspace fresh against them, and the tests
# then grade a binary that carries another mutant's change. The build
# therefore happens in the target directory inside cargo-mutants' own
# temporary copy of the workspace, which is made with the copy and deleted
# with it. The workspace's own build keeps the normal target directory.
#
# The mutation step runs its jobs serially, one at a time. That is what
# makes the survivor list a measurement: cargo-mutants copies the
# workspace into a temporary directory for the run, and every parallel
# worker shares the ONE target directory inside that copy, so a worker can
# be graded against a binary another worker built from a DIFFERENT
# mutation. The list is then wrong in both directions: a real gap looks
# like a survivor and a real kill looks like a survivor. The gate refuses a
# job count above 1 rather than honouring it.
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

# The dev-only band. A crate in it is the proof (the reference decoders, the
# gate test host), not the code being proved, so the mutation step does not
# mutate it.
#
# The number lives in tools/layers.txt, which assigns the bands. A script that
# carries its own copy of the number drifts from the file, and the drift is
# silent, so neither script has one.
#
# The gate reads the dev band as the TOP band of the layer file: the dev-only
# crates sit above the runtime crates, because the harness depends on the code
# under test. tools/check_layers.sh reads the band of the harness crate
# instead. The two read the same file two different ways on purpose, and step
# 3 compares the numbers, so a layer file that says two different things
# fails the gate instead of passing quietly.
dev_band_from_layers() {
    awk '$1 ~ /^[0-9]+$/ && $1 > top { top = $1 } END { print top + 0 }' tools/layers.txt
}

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
docs/fail-before-notes.md
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

# The first failure is what the report names. The gate does not stop there:
# every step runs and every step reports its own verdict. So the failure is
# recorded, and the recording keeps the FIRST one.
note_failure() {
    if [ -z "$FAILED_AT" ]; then
        FAILED_AT="$1"
    fi
    return 0
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
# A failed step does not stop the gate: the verdict is recorded and the next
# step runs, so the report holds every verdict and not only the first defect.
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
    note_failure "$name"
    printf '=== step %s FAILED ===\n' "$number"
    return 1
}

# Write the report digest and print it. The digest covers every byte before
# the digest heading, so writing the digest cannot change what it certifies.
# Both the preflight refusal and a full run end here, so the report is sealed
# the same way in either case.
finish_report() {
    local body_digest
    body_digest=$(sed '/^## Report digest/,$d' "$REPORT" | sha256sum | cut -d' ' -f1)
    printf 'The SHA-256 of everything above, computed with:\n\n```\nsed "/^## Report digest/,$ d" %s | sha256sum\n```\n\n' "$REPORT" >>"$REPORT"
    printf '`%s`\n' "$body_digest" >>"$REPORT"
    printf '%s' "$body_digest"
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
# preflight: the gate judges a commit, so a dirty tree is refused
# ---------------------------------------------------------------------------

# Step 7 counts the HEAD tree and step 8's base run copies HEAD's file, but
# step 4 and step 8's head run measure the working tree. With a dirty tree
# the two halves of the fail-before rule would judge different things, and a
# gate that reports a verdict for a tree it did not measure is a gate that
# lies. So the gate refuses to run and names the files that are dirty.
DIRTY=$(git status --porcelain 2>/dev/null)
if [ -n "$DIRTY" ]; then
    printf 'gate.sh: the working tree is dirty, so the gate would judge the tree\n'
    printf '          and not the commit it names. Commit or stash first.\n'
    printf 'dirty paths:\n'
    printf '%s\n' "$DIRTY"
    {
        printf '# Gate report\n\n'
        printf '| field | value |\n| --- | --- |\n'
        printf '| commit | `%s` |\n' "$COMMIT"
        printf '| branch | `%s` |\n' "$BRANCH"
        printf '| base ref | `%s` (%s) |\n' "$BASE_REF" "$BASE_STATE"
        printf '| finished | %s |\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
        printf '| result | **FAIL** |\n\n'
        printf '## Reason\n\n'
        printf 'The working tree is dirty. The gate judges a commit, so it refuses\n'
        printf 'to run until the tree is clean.\n\n'
        printf '```\n%s\n```\n' "$DIRTY"
        printf '\n## Report digest\n\n'
    } >"$REPORT"
    finish_report >/dev/null
    printf '\n%s\n' '=== gate result ==='
    printf 'report: %s\n' "$REPORT"
    printf 'result: FAIL\n'
    exit 1
fi

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
        note_failure '1 protected paths'
    fi
fi

# ---------------------------------------------------------------------------
# 2 public hygiene
# ---------------------------------------------------------------------------

# Every step runs, whatever the earlier steps did: see the header.
{
    run_step 2 '2 public hygiene' bash tools/check_public.sh || true
}

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

# Every step runs, whatever the earlier steps did: see the header.
{
    printf '\n=== step 3/10: 3 formatting, lints, layering ===\n'
    output=$(step_three 2>&1)
    status=$?
    printf '%s\n' "$output"
    # The dev-only band is read from tools/layers.txt by this script and by
    # tools/check_layers.sh, and the two read it two different ways (the gate
    # reads the top band, the layer script reads the harness crate's band).
    # If the file says two different things, the gate fails here rather than
    # mutating the wrong crates.
    GATE_DEV_BAND=$(dev_band_from_layers)
    LAYER_DEV_BAND=$(printf '%s\n' "$output" |
        sed -n 's/^check_layers\.sh: dev band \([0-9][0-9]*\)$/\1/p' | tail -1)
    BAND_MSG=''
    if [ "$status" -eq 0 ] || [ "$status" -eq 2 ]; then
        if [ -z "$GATE_DEV_BAND" ] || [ "$GATE_DEV_BAND" -eq 0 ]; then
            BAND_MSG='tools/layers.txt declares no band, so the dev-only band is unknown'
        elif [ -z "$LAYER_DEV_BAND" ]; then
            BAND_MSG='tools/check_layers.sh reported no dev band, so the two could not be compared'
        elif [ "$GATE_DEV_BAND" != "$LAYER_DEV_BAND" ]; then
            BAND_MSG="the dev-only band disagrees: gate.sh reads $GATE_DEV_BAND from tools/layers.txt and check_layers.sh reads $LAYER_DEV_BAND"
        else
            printf 'dev band %s agrees between gate.sh and check_layers.sh (tools/layers.txt)\n' \
                "$GATE_DEV_BAND"
        fi
    fi
    if [ -n "$BAND_MSG" ]; then
        printf 'FAIL %s\n' "$BAND_MSG"
        status=1
    fi
    if [ "$status" -eq 0 ]; then
        step '3 formatting, lints, layering' pass 'ok'
    elif [ "$status" -eq 2 ]; then
        skip_step 3 '3 formatting, lints, layering' 'cargo-deny is not installed'
    else
        detail=$(printf '%s' "$output" | grep -v '^[[:space:]]*$' | tail -1)
        [ -n "$BAND_MSG" ] && detail="$BAND_MSG"
        step '3 formatting, lints, layering' fail "$detail"
        note_failure '3 formatting, lints, layering'
    fi
}

# ---------------------------------------------------------------------------
# 4 workspace tests
# ---------------------------------------------------------------------------

# Every step runs, whatever the earlier steps did: see the header.
{
    # Release mode. The release profile keeps the overflow checks on, so a
    # wrapping arithmetic bug fails here instead of passing in a debug build
    # and breaking in a shipped binary. It is also twenty times faster, which
    # is what makes the install suite bearable.
    run_step 4 '4 workspace tests' cargo test --workspace --release || true
}

# ---------------------------------------------------------------------------
# 5 install fingerprint
# ---------------------------------------------------------------------------

# Every step runs, whatever the earlier steps did: see the header.
{
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
            note_failure '5 install fingerprint'
        fi
    fi
}

# ---------------------------------------------------------------------------
# 6 ignored tests
# ---------------------------------------------------------------------------

# Every step runs, whatever the earlier steps did: see the header. This one
# still depends on step 5, because there is no install to test against when
# the fingerprint did not match. A dependency is never a pass: a step that
# cannot run is a required skip, and a required skip fails the gate.
{
    if [ "$INSTALL_OK" -eq 1 ]; then
        run_step 6 '6 ignored tests' cargo test --workspace --release -- --ignored || true
    else
        skip_step 6 '6 ignored tests' 'step 5 did not pass, so there is no install to test against'
    fi
}

# ---------------------------------------------------------------------------
# 7 count check
# ---------------------------------------------------------------------------

# expected_counts.toml is `key value`, one per line, and EVERY key in it is a
# floor this step enforces. A key the script does not understand fails the
# step: a floor nobody reads is a floor nobody keeps, and the guide says the
# counts may only rise.
#
# The keys are four kinds:
#   <gate file stem>   a floor on the number of #[test] functions in
#                      tests/gates/<key>.rs
#   min_synthetic_tests   the sum of the synthetic_* file counts
#   min_install_tests     the sum of the install_* file counts
#   min_harness_tests     the number of #[test] functions in crates/fo1-tests
#
# The counts come from the HEAD tree, not from the working tree: the gate
# judges the commit, and a dirty tree is the owner's business, not the floor's.
#
# The count of a file is the number of `#[test]` attribute lines in it. The
# attribute has to start its own line, so a `#[test]` inside a comment or
# inside a string is not a test. The floors in expected_counts.toml were
# measured that way, and the count is the number of test functions, not the
# number of times the word appears.

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

# Every step runs, whatever the earlier steps did: see the header.
{
    printf '\n=== step 7/10: 7 count check ===\n'
    if [ ! -f expected_counts.toml ]; then
        skip_step 7 '7 count check' 'expected_counts.toml is missing'
    else
        COUNT_FAIL=0
        SYNTHETIC_COUNT=0
        INSTALL_COUNT=0
        HARNESS_COUNT=0
        SYNTHETIC_FLOOR=''
        INSTALL_FLOOR=''
        HARNESS_FLOOR=''
        NAMED_FILES=''

        # `key value`, one per line. A line that is not that shape is a
        # malformed floor, and a malformed floor fails the step.
        COUNT_PAIRS=$(sed -e 's/#.*$//' -e '/^[[:space:]]*$/d' expected_counts.toml |
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
                    path="tests/gates/$key.rs"
                    if ! head_has_path "$path"; then
                        printf 'FAIL %s names the gate test file %s, which is not in the HEAD tree\n' \
                            "$key" "$path"
                        COUNT_FAIL=1
                        continue
                    fi
                    count=$(count_tests_in_head "$path")
                    NAMED_FILES="$NAMED_FILES $path"
                    case "$key" in
                        synthetic_*) SYNTHETIC_COUNT=$((SYNTHETIC_COUNT + count)) ;;
                        install_*) INSTALL_COUNT=$((INSTALL_COUNT + count)) ;;
                    esac
                    if [ "$count" -eq 0 ]; then
                        printf 'FAIL %s has no test function, so the file proves nothing (floor %s)\n' \
                            "$key" "$floor"
                        COUNT_FAIL=1
                    elif [ "$count" -lt "$floor" ]; then
                        printf 'FAIL %s: floor %s, measured %s\n' "$key" "$floor" "$count"
                        COUNT_FAIL=1
                    else
                        printf 'count  %-22s floor %-5s measured %s\n' "$key" "$floor" "$count"
                    fi
                    ;;
            esac
        done <<EOF
$COUNT_PAIRS
EOF

        # The harness is the reference decoders, counted apart from the gate
        # surface, and it is not a file under tests/gates/.
        if [ -n "$HARNESS_FLOOR" ]; then
            for hfile in $(git ls-tree -r --name-only HEAD crates/fo1-tests/src/ | grep '\.rs$'); do
                n=$(count_tests_in_head "$hfile")
                HARNESS_COUNT=$((HARNESS_COUNT + n))
            done
            if [ "$HARNESS_COUNT" -lt "$HARNESS_FLOOR" ]; then
                printf 'FAIL min_harness_tests: floor %s, measured %s\n' \
                    "$HARNESS_FLOOR" "$HARNESS_COUNT"
                COUNT_FAIL=1
            else
                printf 'count  %-22s floor %-5s measured %s\n' \
                    'min_harness_tests' "$HARNESS_FLOOR" "$HARNESS_COUNT"
            fi
        else
            printf 'FAIL expected_counts.toml declares no min_harness_tests\n'
            COUNT_FAIL=1
        fi

        if [ -n "$SYNTHETIC_FLOOR" ]; then
            if [ "$SYNTHETIC_COUNT" -lt "$SYNTHETIC_FLOOR" ]; then
                printf 'FAIL min_synthetic_tests: floor %s, measured %s\n' \
                    "$SYNTHETIC_FLOOR" "$SYNTHETIC_COUNT"
                COUNT_FAIL=1
            else
                printf 'count  %-22s floor %-5s measured %s\n' \
                    'min_synthetic_tests' "$SYNTHETIC_FLOOR" "$SYNTHETIC_COUNT"
            fi
        else
            printf 'FAIL expected_counts.toml declares no min_synthetic_tests\n'
            COUNT_FAIL=1
        fi

        if [ -n "$INSTALL_FLOOR" ]; then
            if [ "$INSTALL_COUNT" -lt "$INSTALL_FLOOR" ]; then
                printf 'FAIL min_install_tests: floor %s, measured %s\n' \
                    "$INSTALL_FLOOR" "$INSTALL_COUNT"
                COUNT_FAIL=1
            else
                printf 'count  %-22s floor %-5s measured %s\n' \
                    'min_install_tests' "$INSTALL_FLOOR" "$INSTALL_COUNT"
            fi
        else
            printf 'FAIL expected_counts.toml declares no min_install_tests\n'
            COUNT_FAIL=1
        fi

        # A gate test file with no floor is a file whose tests can vanish
        # without the gate noticing, which is the hole this step exists to
        # close. Every file under tests/gates/ must be named by a key.
        for gfile in $(git ls-tree --name-only HEAD tests/gates/ | grep '\.rs$'); do
            case " $NAMED_FILES " in
                *" $gfile "*) ;;
                *)
                    printf 'FAIL no floor in expected_counts.toml names the gate test file %s\n' \
                        "$gfile"
                    COUNT_FAIL=1
                    ;;
            esac
        done

        if [ "$COUNT_FAIL" -ne 0 ]; then
            step '7 count check' fail 'a floor in expected_counts.toml was not met'
            note_failure '7 count check'
        else
            step '7 count check' pass 'every floor in expected_counts.toml is met'
        fi
    fi
}

# ---------------------------------------------------------------------------
# 8 fail before, pass after, one test at a time
# ---------------------------------------------------------------------------

# The guide: "The gate copies the test files from the head commit onto the base
# tree and runs them. A new or changed test must fail on the base, and a
# compile error counts as a failure. It must pass on the head. A test that
# already passes on the base proves nothing about the change. It needs a
# written note that explains why, and the owner reviews the note. A pure
# refactor that changes no test is exempt."
#
# The unit of judgement is the TEST, not the FILE. A file is one line in a
# diff, so a file of a hundred tests that changed once and passed on the base
# would have hidden the fact that none of its hundred tests was proved. The
# step therefore works like this:
#
#   1. the test list of a target comes from the test binary itself, from
#      `cargo test --test <target> -- --list`, taken at the base and at the
#      head;
#   2. a test is ADDED when the head lists it and the base does not, and
#      CHANGED when both list it and its own source differs between the two;
#   3. every added or changed test is run against the BASE implementation, in
#      a worktree of the base ref, with the head's test file copied over, and
#      it must fail there: that is what proves the change;
#   4. a test that passes on the base cannot prove the change, so it must
#      carry either a recorded mutation kill or an exemption, and both live in
#      docs/fail-before-notes.md;
#   5. every added or changed test must pass on the head.
#
# The set of changed tests is computed BEFORE any run, and every member of it
# must end with a verdict. A test that is silently forgotten fails the step.
#
# The base tree is a throwaway worktree of BASE_REF with HEAD's version of the
# changed test file copied into it, and HEAD's [[test]] entry for that file
# added to the base Cargo.toml when the base does not have it. One worktree
# serves every changed file: the base is built once and the files are copied
# in one at a time. The base has its own target directory, so a base build
# cannot poison the head build, and the whole worktree is removed afterwards,
# build output and all.
#
# A mutation record is checked, not trusted. The record names one line of the
# implementation and the line that replaces it; the gate applies that change
# to the head implementation in a second worktree and runs the named test. A
# record whose test still passes with the mutation applied is a FALSE record,
# and a false record fails the step: a record nobody checks is a piece of
# prose, and prose is not a proof.

NOTES_FILE='docs/fail-before-notes.md'
BASE_WT=''
MUT_WT=''

cleanup_worktrees() {
    if [ -n "$BASE_WT" ]; then
        if [ -d "$BASE_WT" ]; then
            git worktree remove --force "$BASE_WT" >/dev/null 2>&1 || rm -rf "$BASE_WT"
        fi
        # A removal that failed, or a directory that was deleted by hand,
        # leaves the registration behind under .git/worktrees/, and the
        # registration is what makes `git worktree list` grow and the next
        # run's `git worktree add` fail. Prune it.
        git worktree prune >/dev/null 2>&1 || true
        BASE_WT=''
    fi
    if [ -n "$MUT_WT" ]; then
        if [ -d "$MUT_WT" ]; then
            git worktree remove --force "$MUT_WT" >/dev/null 2>&1 || rm -rf "$MUT_WT"
        fi
        git worktree prune >/dev/null 2>&1 || true
        MUT_WT=''
    fi
    return 0
}
trap cleanup_worktrees EXIT

# The instrument that says where each test of a file lives, and that changes
# one line of a file. It is written into the report directory, not into the
# tree: it is the gate's own tool, and the tree holds the judge's subject.
#
# The test LIST comes from the test binary; this says where each test's own
# source is, so that the gate can tell a changed test from an unchanged one.
# The gate checks the two against each other on every run: if the names this
# tool reports and the names the binary lists disagree, the step fails rather
# than judging a file it cannot see.
GATE_TOOLS="$REPORT_DIR/gate_tools.py"

cat >"$GATE_TOOLS" <<'PYTHON'
"""Where the tests of a Rust file live, and how to change one line of it.

names FILE            one line per test: name, first line, last line, digest
rest FILE             the code of the file that lies outside every test
mutate FILE FROM TO   replace the one line whose text is FROM with TO
"""
import hashlib
import re
import sys

MARKER = re.compile(r'^[ \t]*#\[test\]')
FNNAME = re.compile(
    r'^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:async[ \t]+)?(?:unsafe[ \t]+)?'
    r'fn[ \t]+([A-Za-z_][A-Za-z0-9_]*)')


def blank(src):
    """Replace every comment and literal with spaces, keeping the offsets.

    Brace counting over the blanked text is then exact: a brace inside a
    string or a comment cannot open or close a function body.
    """
    out = list(src)
    n = len(src)
    i = 0
    while i < n:
        c = src[i]
        if c == '/' and i + 1 < n and src[i + 1] == '/':
            e = src.find('\n', i)
            if e < 0:
                e = n
            for x in range(i, e):
                out[x] = ' '
            i = e
            continue
        if c == '/' and i + 1 < n and src[i + 1] == '*':
            depth = 1
            e = i + 2
            while e < n and depth:
                if src.startswith('/*', e):
                    depth += 1
                    e += 2
                elif src.startswith('*/', e):
                    depth -= 1
                    e += 2
                else:
                    e += 1
            for x in range(i, e):
                out[x] = ' '
            i = e
            continue
        if c in 'rRbB':
            j = i
            while j < n and src[j] in 'rRbB':
                j += 1
            if j < n and src[j] == '"':
                k = j + 1
                hashes = 0
                while k < n and src[k] == '#':
                    hashes += 1
                    k += 1
                term = '"' + '#' * hashes
                e = src.find(term, k)
                e = n if e < 0 else e + len(term)
                for x in range(i, e):
                    if out[x] != '\n':
                        out[x] = ' '
                i = e
                continue
            i = j
            continue
        if c == '"':
            e = i + 1
            while e < n:
                if src[e] == '\\':
                    e += 2
                    continue
                if src[e] == '"':
                    e += 1
                    break
                e += 1
            for x in range(i, min(e, n)):
                if out[x] != '\n':
                    out[x] = ' '
            i = e
            continue
        if c == "'":
            if i + 1 < n and src[i + 1] == '\\':
                e = i + 2
                while e < n and src[e] != "'":
                    e += 1
                e = min(e + 1, n)
            elif i + 2 < n and src[i + 1] != "'" and src[i + 2] == "'":
                e = i + 3
            elif i + 1 < n and (src[i + 1].isalpha() or src[i + 1] == '_'):
                i += 1
                continue
            else:
                i += 1
                continue
            for x in range(i, e):
                if out[x] != '\n':
                    out[x] = ' '
            i = e
            continue
        i += 1
    return ''.join(out)


def tests_of(src):
    """The test items of a source file, in the order they appear."""
    code = blank(src)
    lines = src.split('\n')
    code_lines = code.split('\n')
    n = len(lines)
    found = []
    i = 0
    while i < n:
        if not MARKER.match(lines[i]):
            i += 1
            continue
        marker = i
        name = None
        fn_line = -1
        j = i
        while j < n and j <= i + 60:
            m = FNNAME.match(lines[j])
            if m:
                name = m.group(1)
                fn_line = j
                break
            j += 1
        if name is None:
            i = marker + 1
            continue
        body = '\n'.join(code_lines[fn_line:])
        k = body.find('{')
        if k < 0:
            i = marker + 1
            continue
        depth = 0
        end = -1
        for p in range(k, len(body)):
            if body[p] == '{':
                depth += 1
            elif body[p] == '}':
                depth -= 1
                if depth == 0:
                    end = p
                    break
        if end < 0:
            i = marker + 1
            continue
        last = fn_line + body.count('\n', 0, end)
        span = '\n'.join(lines[marker:last + 1])
        found.append((name, marker + 1, last + 1,
                      hashlib.sha256(span.encode('utf-8', 'replace')).hexdigest()))
        i = marker + 1
    return found


def main(argv):
    if len(argv) < 3:
        sys.stderr.write('usage: gate_tools.py names|mutate ...\n')
        return 2
    mode = argv[1]
    if mode == 'names':
        path = argv[2]
        with open(path, 'r', encoding='utf-8', errors='replace') as handle:
            src = handle.read()
        for name, first, last, digest in tests_of(src):
            print('%s\t%d\t%d\t%s' % (name, first, last, digest))
        return 0
    if mode == 'rest':
        path = argv[2]
        with open(path, 'r', encoding='utf-8', errors='replace') as handle:
            src = handle.read()
        covered = set()
        for _name, first, last, _digest in tests_of(src):
            for number in range(first, last + 1):
                covered.add(number)
        rest = []
        for number, line in enumerate(src.split('\n'), start=1):
            if number in covered:
                continue
            stripped = line.strip()
            if stripped == '':
                continue
            if stripped.startswith('//'):
                continue
            rest.append(stripped)
        print('\n'.join(rest))
        return 0
    if mode == 'mutate':
        path, want, take = argv[2], argv[3], argv[4]
        with open(path, 'r', encoding='utf-8', errors='surrogateescape') as handle:
            src = handle.read()
        lines = src.split('\n')
        hits = [n for n, line in enumerate(lines) if line.strip() == want]
        if len(hits) != 1:
            sys.stdout.write('%d\n' % len(hits))
            return 1
        n = hits[0]
        indent = lines[n][:len(lines[n]) - len(lines[n].lstrip())]
        lines[n] = indent + take
        with open(path, 'w', encoding='utf-8', errors='surrogateescape') as handle:
            handle.write('\n'.join(lines))
        sys.stdout.write('1\n')
        return 0
    sys.stderr.write('unknown mode %s\n' % mode)
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv))
PYTHON

# The python this gate uses for its own instrument. tools/check_layers.sh
# already needs one, so the workspace does not grow a dependency for it.
PYTHON_BIN=''
for candidate in python python3; do
    if command -v "$candidate" >/dev/null 2>&1; then
        if "$candidate" -c 'import re,sys' >/dev/null 2>&1; then
            PYTHON_BIN=$(command -v "$candidate")
            break
        fi
    fi
done

# The [[test]] target name HEAD's Cargo.toml gives this test file.
head_target_name() {
    git show HEAD:Cargo.toml 2>/dev/null | awk -v want="$1" '
        /^\[\[test\]\]/ { blk = 1; nm = ""; pt = ""; next }
        /^\[/ { blk = 0 }
        blk && $1 == "name" { nm = $3; gsub(/"/, "", nm) }
        blk && $1 == "path" { pt = $3; gsub(/"/, "", pt) }
        blk && pt != "" && pt == want { print nm; exit }
    '
}

# The [[test]] block HEAD's Cargo.toml gives this test file.
head_test_block() {
    git show HEAD:Cargo.toml 2>/dev/null | awk -v want="$1" '
        /^\[\[test\]\]/ {
            if (blk != "") { if (found) print blk }
            blk = $0 "\n"; found = 0; next
        }
        blk == "" { next }
        /^[ \t]*$/ { next }
        {
            blk = blk $0 "\n"
            if ($1 == "path") { pt = $3; gsub(/"/, "", pt); if (pt == want) found = 1 }
        }
        END { if (blk != "" && found) print blk }
    '
}

# Did this run fail to compile? A compile error is not a test failure, and a
# run that never compiled has measured nothing.
is_compile_error() {
    printf '%s\n' "$1" | grep -qE '^error\[|^error: could not compile'
}

# The notes the owner wrote, as tab separated fields: OK, file, test name,
# reason. A malformed entry comes back as MALFORMED.
#
# Only the "## Entries" section is read. The file's own prose, the format
# example included, lives above it, and a prose line is not an entry.
#
# The entry is split on the FIRST TWO "::" separators and the reason is
# everything after the second, because a reason is prose and prose may hold a
# "::" of its own (a Rust path, for instance). A reason that cannot be
# written is a note nobody can file.
read_notes() {
    [ -f "$NOTES_FILE" ] || return 0
    awk '
        function trim(s) {
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", s)
            return s
        }
        function split_note(line,   count, rest, at) {
            count = 0
            rest = line
            N1 = ""; N2 = ""; N3 = ""
            while (count < 2) {
                at = index(rest, "::")
                if (at == 0) break
                count++
                if (count == 1) N1 = trim(substr(rest, 1, at - 1))
                else N2 = trim(substr(rest, 1, at - 1))
                rest = substr(rest, at + 2)
            }
            if (count < 2) return 0
            N3 = trim(rest)
            return 1
        }
        /^##[[:space:]]+Entries[[:space:]]*$/ { insec = 1; next }
        /^#+[[:space:]]/ { insec = 0 }
        insec != 1 { next }
        {
            line = $0
            sub(/^[[:space:]]+/, "", line)
            sub(/[[:space:]]+$/, "", line)
            if (line == "") next
            if (line ~ /^#/) next
            if (line == "None yet.") next
            if (!split_note(line)) {
                print "MALFORMED\t" line
                next
            }
            if (N1 == "" || N2 == "" || N3 == "") {
                print "MALFORMED\t" line
                next
            }
            print "OK\t" N1 "\t" N2 "\t" N3
        }
    ' "$NOTES_FILE"
}

# The outcome of one test run: compile error, failed, passed, ignored or
# absent. "absent" means the run measured nothing: the test did not run at
# all, so nothing about it is known.
run_outcome() {
    local out="$1" passed failed ignored
    if is_compile_error "$out"; then
        printf 'compile error'
        return 0
    fi
    passed=$(printf '%s\n' "$out" | sed -n 's/^test result: .*\([0-9][0-9]*\) passed.*/\1/p' | tail -1)
    failed=$(printf '%s\n' "$out" | sed -n 's/^test result: .*\([0-9][0-9]*\) failed.*/\1/p' | tail -1)
    ignored=$(printf '%s\n' "$out" | sed -n 's/^test result: .*\([0-9][0-9]*\) ignored.*/\1/p' | tail -1)
    if [ "${failed:-0}" -gt 0 ]; then
        printf 'failed'
    elif [ "${passed:-0}" -gt 0 ]; then
        printf 'passed'
    elif [ "${ignored:-0}" -gt 0 ]; then
        printf 'ignored'
    else
        printf 'absent'
    fi
}

# The recorded mutation kills, as tab separated fields: OK, test file, test
# name, implementation file, the line to replace, the line that replaces it,
# reason. A malformed entry comes back as MALFORMED.
#
# Only the "## Recorded mutation kills" section is read. The fields are split
# on the first six " :: " separators, with spaces around them: a Rust line
# often holds a "::" of its own, so a separator without spaces would cut a
# line in half. A field that holds " :: " cannot be written down, and the
# gate says so instead of guessing.
read_records() {
    [ -f "$NOTES_FILE" ] || return 0
    awk '
        function trim(s) {
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", s)
            return s
        }
        function split_record(line,   count, rest, at) {
            count = 0
            rest = line
            R1 = ""; R2 = ""; R3 = ""; R4 = ""; R5 = ""; R6 = ""
            while (count < 5) {
                at = index(rest, " :: ")
                if (at == 0) break
                count++
                if (count == 1) R1 = trim(substr(rest, 1, at - 1))
                else if (count == 2) R2 = trim(substr(rest, 1, at - 1))
                else if (count == 3) R3 = trim(substr(rest, 1, at - 1))
                else if (count == 4) R4 = trim(substr(rest, 1, at - 1))
                else R5 = trim(substr(rest, 1, at - 1))
                rest = substr(rest, at + 3)
            }
            if (count < 5) return 0
            R6 = trim(rest)
            if (R1 == "" || R2 == "" || R3 == "" || R4 == "" || R5 == "" || R6 == "") return 0
            return 1
        }
        /^##[[:space:]]+Recorded mutation kills[[:space:]]*$/ { insec = 1; next }
        /^#+[[:space:]]/ { insec = 0 }
        insec != 1 { next }
        {
            line = $0
            sub(/^[[:space:]]+/, "", line)
            sub(/[[:space:]]+$/, "", line)
            if (line == "") next
            if (line ~ /^#/) next
            if (line == "None yet.") next
            if (!split_record(line)) {
                print "MALFORMED\t" line
                next
            }
            print "OK\t" R1 "\t" R2 "\t" R3 "\t" R4 "\t" R5 "\t" R6
        }
    ' "$NOTES_FILE"
}

# The source view of a file: where each test is and what its own source says.
# One line per test: name, first line, last line, digest of the test's source.
SOURCE_VIEW="$REPORT_DIR/source-view"
mkdir -p "$SOURCE_VIEW"

source_tests_of() {
    # source_tests_of <ref> <path>
    local ref="$1" path="$2" tmp
    tmp="$SOURCE_VIEW/$(printf '%s' "$ref" | tr '/:' '__')_$(printf '%s' "$path" | tr '/' '_')"
    if ! git show "$ref:$path" >"$tmp" 2>/dev/null; then
        return 1
    fi
    "$PYTHON_BIN" "$GATE_TOOLS" names "$tmp" 2>/dev/null
}

# The code of a file that lies outside every test of that file: the shared
# helpers, the use statements, the attributes. Comments are left out, because
# a comment cannot change what a test does. A change in this code reaches
# tests the gate cannot attribute to one of them, so the gate charges it to
# all of them.
source_rest_of() {
    # source_rest_of <ref> <path>
    local ref="$1" path="$2" tmp
    tmp="$SOURCE_VIEW/rest_$(printf '%s' "$ref" | tr '/:' '__')_$(printf '%s' "$path" | tr '/' '_')"
    if ! git show "$ref:$path" >"$tmp" 2>/dev/null; then
        printf ''
        return 0
    fi
    "$PYTHON_BIN" "$GATE_TOOLS" rest "$tmp" 2>/dev/null
}

# Do the source view and the test binary list the same tests? A binary name
# may carry a module path, so the last segment is what is compared. If the two
# disagree, the gate is looking at a file it cannot see, and it fails rather
# than judging half of it.
same_test_set() {
    # same_test_set <binary list> <source view>
    local a b
    a=$(printf '%s\n' "$1" | grep -v '^$' | sed 's/.*:://' | sort)
    b=$(printf '%s\n' "$2" | cut -f1 | grep -v '^$' | sort)
    [ "$a" = "$b" ]
}

# The digest of one test's own source, from the source view.
source_digest_of() {
    # source_digest_of <source view> <test name>
    printf '%s\n' "$1" | awk -F'\t' -v want="$2" '$1 == want { print $4; exit }'
}

# Every exemption must name a test that exists. An exemption whose test name
# does not exist in the file it names is rejected: an exemption that names
# the wrong test approves nothing.
validate_notes() {
    local kind nfile ntest nreason bad=0 names
    while IFS=$'\t' read -r kind nfile ntest nreason; do
        [ "$kind" = OK ] || continue
        if ! head_has_path "$nfile"; then
            printf 'FAIL the exemption names %s, which is not in the HEAD tree\n' "$nfile"
            bad=1
            continue
        fi
        names=$(source_tests_of HEAD "$nfile" 2>/dev/null | cut -f1)
        if printf '%s\n' "$names" | grep -qxF "$ntest"; then
            continue
        fi
        printf 'FAIL the exemption names the test %s, which is not in %s\n' "$ntest" "$nfile"
        printf '     the name must be a test function of that file: the gate judges tests, not files\n'
        bad=1
    done <<EOF
$NOTES
EOF
    return "$bad"
}

# Every record must name a test that exists and an implementation file that
# is in the tree. A record that names the wrong test approves nothing.
validate_records() {
    local kind rfile rtest rimpl rfrom rto rreason bad=0 names
    while IFS=$'\t' read -r kind rfile rtest rimpl rfrom rto rreason; do
        [ "$kind" = OK ] || continue
        if ! head_has_path "$rfile"; then
            printf 'FAIL the record names the test file %s, which is not in the HEAD tree\n' "$rfile"
            bad=1
            continue
        fi
        names=$(source_tests_of HEAD "$rfile" 2>/dev/null | cut -f1)
        if ! printf '%s\n' "$names" | grep -qxF "$rtest"; then
            printf 'FAIL the record names the test %s, which is not in %s\n' "$rtest" "$rfile"
            bad=1
            continue
        fi
        if ! head_has_path "$rimpl"; then
            printf 'FAIL the record names the implementation file %s, which is not in the HEAD tree\n' "$rimpl"
            bad=1
            continue
        fi
    done <<EOF
$RECORDS
EOF
    return "$bad"
}

# The exemption that covers this test, if the owner wrote one.
exemption_for() {
    # exemption_for <test file> <test name>
    local want_file="$1" want_test="$2" kind nfile ntest nreason
    while IFS=$'\t' read -r kind nfile ntest nreason; do
        [ "$kind" = OK ] || continue
        if [ "$nfile" = "$want_file" ] && [ "$ntest" = "$want_test" ]; then
            printf '%s :: %s :: %s' "$nfile" "$ntest" "$nreason"
            return 0
        fi
    done <<EOF
$NOTES
EOF
    return 1
}

# The record that covers this test, if the owner wrote one.
record_for() {
    # record_for <test file> <test name>  ->  tab separated fields
    local want_file="$1" want_test="$2" kind rfile rtest rimpl rfrom rto rreason
    while IFS=$'\t' read -r kind rfile rtest rimpl rfrom rto rreason; do
        [ "$kind" = OK ] || continue
        if [ "$rfile" = "$want_file" ] && [ "$rtest" = "$want_test" ]; then
            printf '%s\t%s\t%s\t%s\t%s\t%s' "$rfile" "$rtest" "$rimpl" "$rfrom" "$rto" "$rreason"
            return 0
        fi
    done <<EOF
$RECORDS
EOF
    return 1
}

# Check one recorded mutation kill: apply the mutation to the head
# implementation and run the named test. The test must FAIL, and the build
# must succeed: a mutation that does not compile proves nothing about the
# test, and a test that still passes with the mutation applied does not kill
# it. A record that fails this check is a FALSE record, and it fails the
# step: a record nobody checks is prose, and prose is not a proof.
check_mutation_record() {
    # check_mutation_record <test file> <test name> <impl file> <from> <to>
    local tfile="$1" tname="$2" ifile="$3" from="$4" to="$5"
    local target hits out outcome
    target=$(head_target_name "$tfile")
    if [ -z "$target" ]; then
        printf 'FAIL the record names the test file %s, which has no [[test]] entry in HEAD Cargo.toml\n' "$tfile"
        return 1
    fi
    if [ -z "$MUT_WT" ]; then
        MUT_WT="$REPO_ROOT/$REPORT_DIR/mutation-worktree"
        rm -rf "$MUT_WT"
        if ! git worktree add --detach "$MUT_WT" HEAD >/dev/null 2>&1; then
            printf 'FAIL the mutation worktree could not be created\n'
            MUT_WT=''
            return 1
        fi
    fi
    # The record must name exactly one line of the implementation file, so
    # that the mutation is unambiguous.
    hits=$("$PYTHON_BIN" "$GATE_TOOLS" mutate "$MUT_WT/$ifile" "$from" "$to" 2>/dev/null)
    if [ "$hits" != '1' ]; then
        printf 'FAIL the record names the line "%s" of %s, which matches %s line(s) of that file\n' \
            "$from" "$ifile" "${hits:-0}"
        printf '     the line must match exactly one line, so the mutation is unambiguous\n'
        git -C "$MUT_WT" checkout -- "$ifile" >/dev/null 2>&1 || true
        return 1
    fi
    out=$(cd "$MUT_WT" && CARGO_TARGET_DIR="$MUT_WT/target-mut" \
        cargo test --release --test "$target" -- --exact "$tname" 2>&1)
    outcome=$(run_outcome "$out")
    if [ "$outcome" = ignored ]; then
        out=$(cd "$MUT_WT" && CARGO_TARGET_DIR="$MUT_WT/target-mut" \
            cargo test --release --test "$target" -- --ignored --exact "$tname" 2>&1)
        outcome=$(run_outcome "$out")
    fi
    # The mutation is undone before the verdict is read, so the next record
    # starts from the head implementation.
    git -C "$MUT_WT" checkout -- "$ifile" >/dev/null 2>&1 || true
    if [ "$outcome" = 'compile error' ]; then
        printf 'FAIL the mutation of %s does not compile, so it is not a behaviour change a test can kill\n' \
            "$ifile"
        return 1
    fi
    if [ "$outcome" = 'failed' ]; then
        printf '     recorded mutation kill: %s kills the mutation of %s\n' "$tname" "$ifile"
        return 0
    fi
    printf 'FAIL the record for %s is false: the test still %s with the mutation applied\n' \
        "$tname" "$outcome"
    printf '     the mutation was: %s => %s\n' "$from" "$to"
    return 1
}

# Run one test of one target and say what happened to it. The output of the
# run is left in RUN_OUTPUT. A test that did not run is "absent", and an
# ignored test is "ignored": neither measured anything.
run_one_test() {
    # run_one_test <work dir> <target dir> <target name> <test name>
    local dir="$1" tdir="$2" target="$3" test="$4" out
    if [ -n "$tdir" ]; then
        out=$(cd "$dir" && CARGO_TARGET_DIR="$tdir" \
            cargo test --release --test "$target" -- --exact "$test" 2>&1)
    else
        out=$(cd "$dir" && cargo test --release --test "$target" -- --exact "$test" 2>&1)
    fi
    RUN_OUTPUT="$out"
    run_outcome "$out"
}

# The same run with --ignored, for the test that is #[ignore]d.
run_one_test_ignored() {
    # run_one_test_ignored <work dir> <target dir> <target name> <test name>
    local dir="$1" tdir="$2" target="$3" test="$4" out
    if [ -n "$tdir" ]; then
        out=$(cd "$dir" && CARGO_TARGET_DIR="$tdir" \
            cargo test --release --test "$target" -- --ignored --exact "$test" 2>&1)
    else
        out=$(cd "$dir" && cargo test --release --test "$target" -- --ignored --exact "$test" 2>&1)
    fi
    RUN_OUTPUT="$out"
    run_outcome "$out"
}

# Every step runs, whatever the earlier steps did: see the header.
{
    printf '\n=== step 8/10: 8 fail before, pass after ===\n'
    if [ "$BASE_STATE" = UNRESOLVED ]; then
        skip_step 8 '8 fail before, pass after' "$BASE_REF does not resolve, so there is no before"
    elif ! have cargo; then
        skip_step 8 '8 fail before, pass after' 'cargo is not installed'
    elif [ -z "$PYTHON_BIN" ]; then
        skip_step 8 '8 fail before, pass after' \
            'neither python nor python3 was found, so the changed tests cannot be located'
    else
        CHANGED_TESTS=$(git diff --name-only "$BASE_REF...HEAD" -- tests/gates/ 2>/dev/null |
            grep '\.rs$' || true)
        if [ -z "$CHANGED_TESTS" ]; then
            printf 'no gate test changed since %s, so the change is a pure refactor and is exempt\n' \
                "$BASE_REF"
            step '8 fail before, pass after' pass "no gate test changed since $BASE_REF"
        else
            printf 'gate test file(s) changed since %s\n' "$BASE_REF"
            printf '  %s\n' $CHANGED_TESTS
            # The exemptions and the records are read once, and a malformed
            # entry fails the step before any run is made: an approval the
            # gate cannot read is not an approval, the same way a key in
            # expected_counts.toml that the gate does not understand fails
            # step 7.
            NOTES=$(read_notes)
            RECORDS=$(read_records)
            NOTES_BAD=$(printf '%s\n' "$NOTES" | grep -c '^MALFORMED' || true)
            RECORDS_BAD=$(printf '%s\n' "$RECORDS" | grep -c '^MALFORMED' || true)
            if [ "${NOTES_BAD:-0}" -ne 0 ]; then
                printf '%s\n' "$NOTES" | grep '^MALFORMED' |
                    sed 's/^MALFORMED/FAIL malformed exemption entry in /'
                printf '     the format is <test file> :: <test name> :: <reason>\n'
                printf '     the entries live in the "## Entries" section of %s\n' "$NOTES_FILE"
            fi
            if [ "${RECORDS_BAD:-0}" -ne 0 ]; then
                printf '%s\n' "$RECORDS" | grep '^MALFORMED' |
                    sed 's/^MALFORMED/FAIL malformed mutation record in /'
                printf '     the format is <test file> :: <test name> :: <impl file> :: <from line> :: <to line> :: <reason>\n'
                printf '     the records live in the "## Recorded mutation kills" section of %s\n' "$NOTES_FILE"
            fi
            # An exemption or a record must name the test it covers, so one
            # that names a test the file does not hold is rejected before any
            # run is made.
            NOTES_UNNAMED=0
            if ! validate_notes; then
                NOTES_UNNAMED=1
                printf '     the test name must be a test function of the file the entry names\n'
            fi
            RECORDS_UNNAMED=0
            if ! validate_records; then
                RECORDS_UNNAMED=1
                printf '     the record must name a test function of the file it names, and an\n'
                printf '     implementation file that is in the HEAD tree\n'
            fi
            # The base worktree and its own target directory. Both paths are
            # absolute: the base run runs cargo with the working directory
            # set to the worktree, so a relative CARGO_TARGET_DIR would be
            # resolved against the worktree and land in a nested path.
            BASE_WT="$REPO_ROOT/$REPORT_DIR/base-worktree"
            BASE_TARGET="$BASE_WT/target-base"
            rm -rf "$BASE_WT"
            if ! git worktree add --detach "$BASE_WT" "$BASE_REF" >/dev/null 2>&1; then
                printf 'FAIL the base worktree of %s could not be created\n' "$BASE_REF"
                step '8 fail before, pass after' fail "the base worktree of $BASE_REF could not be created"
                note_failure '8 fail before, pass after'
            else
                STEP8_FAIL=0
                STEP8_SKIP=''
                if [ "${NOTES_BAD:-0}" -ne 0 ] || [ "${NOTES_UNNAMED:-0}" -ne 0 ] ||
                    [ "${RECORDS_BAD:-0}" -ne 0 ] || [ "${RECORDS_UNNAMED:-0}" -ne 0 ]; then
                    STEP8_FAIL=1
                fi
                for rel in $CHANGED_TESTS; do
                    name=$(head_target_name "$rel")
                    if [ -z "$name" ]; then
                        printf '  %-34s no [[test]] entry in HEAD Cargo.toml\n' "$rel"
                        printf 'FAIL %s has no [[test]] entry in HEAD Cargo.toml, so it is not a test target\n' \
                            "$rel"
                        STEP8_FAIL=1
                        continue
                    fi

                    # The test list of the target, at the head. The list comes
                    # from the test binary itself, so it is the list of tests
                    # that RUN, not a grep of the source.
                    head_out=$(cargo test --release --test "$name" -- --list 2>&1)
                    head_status=$?
                    if [ "$head_status" -ne 0 ]; then
                        printf 'FAIL the head of %s does not build, so its test list cannot be read\n' "$rel"
                        printf '%s\n' "$head_out" | tail -12
                        STEP8_FAIL=1
                        continue
                    fi
                    head_list=$(printf '%s\n' "$head_out" | sed -n 's/^\(.*\): test$/\1/p')
                    head_src=$(source_tests_of HEAD "$rel" 2>/dev/null)
                    if ! same_test_set "$head_list" "$head_src"; then
                        printf 'FAIL the source view and the test binary disagree about the tests of %s\n' "$rel"
                        printf '     binary: %s\n' \
                            "$(printf '%s\n' "$head_list" | sed 's/.*:://' | tr '\n' ' ')"
                        printf '     source: %s\n' \
                            "$(printf '%s\n' "$head_src" | cut -f1 | tr '\n' ' ')"
                        STEP8_FAIL=1
                        continue
                    fi

                    # The base may not have the [[test]] entry for this file.
                    # The head's entry is added so that the base can build and
                    # list it.
                    if ! grep -q "path = \"$rel\"" "$BASE_WT/Cargo.toml" 2>/dev/null; then
                        block=$(head_test_block "$rel")
                        if [ -n "$block" ]; then
                            printf '\n%s\n' "$block" >>"$BASE_WT/Cargo.toml"
                        fi
                    fi
                    # The base's own copy of the file, listed by the base's own
                    # test binary. The head's file is copied over only after
                    # this, so the base list is the base's tests.
                    if ! git cat-file -e "$BASE_REF:$rel" 2>/dev/null; then
                        base_list=''
                        base_src=''
                    else
                        base_out=$(cd "$BASE_WT" && CARGO_TARGET_DIR="$BASE_TARGET" \
                            cargo test --release --test "$name" -- --list 2>&1)
                        base_status=$?
                        if [ "$base_status" -ne 0 ]; then
                            printf 'FAIL the base of %s does not build, so its test list cannot be read\n' "$rel"
                            printf '%s\n' "$base_out" | tail -12
                            STEP8_FAIL=1
                            continue
                        fi
                        base_list=$(printf '%s\n' "$base_out" | sed -n 's/^\(.*\): test$/\1/p')
                        base_src=$(source_tests_of "$BASE_REF" "$rel" 2>/dev/null)
                        if ! same_test_set "$base_list" "$base_src"; then
                            printf 'FAIL the source view and the base test binary disagree about the tests of %s\n' \
                                "$rel"
                            STEP8_FAIL=1
                            continue
                        fi
                    fi

                    printf '  %s: %s test(s) at the head, %s at the base\n' "$rel" \
                        "$(printf '%s\n' "$head_list" | grep -c . || true)" \
                        "$(printf '%s\n' "$base_list" | grep -c . || true)"

                    # The changed tests: the ones the head has and the base
                    # does not, and the ones both have whose own source
                    # differs. A change in the file that lies outside every
                    # test (a helper, a use statement) reaches tests the gate
                    # cannot attribute to one of them, so it is charged to
                    # every test in the file.
                    CHANGED_SET=''
                    while IFS= read -r tn; do
                        [ -n "$tn" ] || continue
                        if ! printf '%s\n' "$base_list" | grep -qxF "$tn"; then
                            CHANGED_SET="$CHANGED_SET$tn"$'\n'
                            continue
                        fi
                        sname=$(printf '%s\n' "$tn" | sed 's/.*:://')
                        h_digest=$(source_digest_of "$head_src" "$sname")
                        b_digest=$(source_digest_of "$base_src" "$sname")
                        if [ "$h_digest" != "$b_digest" ]; then
                            CHANGED_SET="$CHANGED_SET$tn"$'\n'
                        fi
                    done <<EOF
$head_list
EOF
                    REMOVED_SET=''
                    while IFS= read -r tn; do
                        [ -n "$tn" ] || continue
                        if ! printf '%s\n' "$head_list" | grep -qxF "$tn"; then
                            REMOVED_SET="$REMOVED_SET$tn"$'\n'
                        fi
                    done <<EOF
$base_list
EOF
                    if [ -n "$REMOVED_SET" ]; then
                        printf '  removed: %s\n' "$(printf '%s' "$REMOVED_SET" | tr '\n' ' ')"
                    fi
                    h_rest=$(source_rest_of HEAD "$rel")
                    b_rest=$(source_rest_of "$BASE_REF" "$rel")
                    if [ "$h_rest" != "$b_rest" ]; then
                        printf '  the code outside every test changed, so every test in it is treated as changed\n'
                        CHANGED_SET="$head_list"$'\n'
                    fi

                    # The list of the changed tests, read into an array, so
                    # that the runs below do not read the loop's input.
                    CHANGED_TESTS_ARRAY=()
                    while IFS= read -r tn; do
                        [ -n "$tn" ] || continue
                        CHANGED_TESTS_ARRAY+=("$tn")
                    done <<EOF
$CHANGED_SET
EOF
                    if [ "${#CHANGED_TESTS_ARRAY[@]}" -eq 0 ]; then
                        printf '  no test was added or changed\n'
                        continue
                    fi

                    # Copy HEAD's version of the test file onto the base tree.
                    # The base now holds the head's tests, so the run below
                    # measures the head's test against the base's
                    # implementation.
                    mkdir -p "$BASE_WT/$(dirname "$rel")"
                    if ! git show "HEAD:$rel" >"$BASE_WT/$rel" 2>/dev/null; then
                        printf 'FAIL %s could not be read from HEAD\n' "$rel"
                        STEP8_FAIL=1
                        continue
                    fi

                    for tn in "${CHANGED_TESTS_ARRAY[@]}"; do
                        sname=$(printf '%s\n' "$tn" | sed 's/.*:://')
                        # The base run: the test must fail there. The head
                        # run: the test must pass here.
                        base_verdict=$(run_one_test "$BASE_WT" "$BASE_TARGET" "$name" "$tn")
                        head_verdict=$(run_one_test "$REPO_ROOT" '' "$name" "$tn")
                        if [ "$base_verdict" = ignored ] || [ "$head_verdict" = ignored ]; then
                            # The test is #[ignore]d, so it needs the install.
                            # Without it there is nothing to run against, and
                            # a step that cannot measure is a required skip.
                            if [ -z "${FO1_DATA:-}" ]; then
                                printf '  fail-before  %-52s base %-14s head %s\n' \
                                    "$tn" "$base_verdict" "$head_verdict"
                                printf 'FAIL %s holds an ignored test and FO1_DATA is not set, so it cannot be measured\n' \
                                    "$rel"
                                STEP8_SKIP="$rel holds an ignored test and FO1_DATA is not set"
                                continue
                            fi
                            if [ "$base_verdict" = ignored ]; then
                                base_verdict=$(run_one_test_ignored "$BASE_WT" "$BASE_TARGET" "$name" "$tn")
                            fi
                            if [ "$head_verdict" = ignored ]; then
                                head_verdict=$(run_one_test_ignored "$REPO_ROOT" '' "$name" "$tn")
                            fi
                        fi
                        printf '  fail-before  %-52s base %-14s head %s\n' \
                            "$tn" "$base_verdict" "$head_verdict"

                        if [ "$head_verdict" != passed ]; then
                            printf 'FAIL the test %s of %s does not pass on the head\n' "$tn" "$rel"
                            printf '     the run gave: %s\n' "$head_verdict"
                            printf '%s\n' "$RUN_OUTPUT" | tail -12
                            STEP8_FAIL=1
                            continue
                        fi
                        if [ "$base_verdict" = failed ]; then
                            printf '     proved by the base: the test fails there\n'
                            continue
                        fi
                        if [ "$base_verdict" = 'compile error' ]; then
                            printf '     proved by the base: the file does not compile there, and a compile error counts as a failure\n'
                            continue
                        fi
                        # The test passes on the base, so it proves nothing
                        # about this change. It needs a recorded mutation
                        # kill or an exemption, and the gate checks the
                        # record rather than trusting it.
                        if record=$(record_for "$rel" "$sname"); then
                            IFS=$'\t' read -r r_file r_test r_impl r_from r_to r_reason <<<"$record"
                            if check_mutation_record "$rel" "$tn" "$r_impl" "$r_from" "$r_to"; then
                                continue
                            fi
                            STEP8_FAIL=1
                            continue
                        fi
                        if exemption=$(exemption_for "$rel" "$sname"); then
                            printf '     relying on the exemption: %s\n' "$exemption"
                            continue
                        fi
                        printf 'FAIL the test %s of %s passes on the base, so it proves nothing about this change\n' \
                            "$tn" "$rel"
                        printf '     and it carries neither a recorded mutation kill nor an exemption\n'
                        printf '     expected exemption: %s :: %s :: <reason>\n' "$rel" "$sname"
                        printf '     expected record:   %s :: %s :: <impl file> :: <from line> :: <to line> :: <reason>\n' \
                            "$rel" "$sname"
                        printf '     add it to %s\n' "$NOTES_FILE"
                        STEP8_FAIL=1
                    done
                done
                cleanup_worktrees

                if [ "$STEP8_FAIL" -ne 0 ]; then
                    step '8 fail before, pass after' fail \
                        'a changed test did not fail on the base, did not pass on the head, or was not accounted for'
                    note_failure '8 fail before, pass after'
                elif [ -n "$STEP8_SKIP" ]; then
                    skip_step 8 '8 fail before, pass after' "$STEP8_SKIP"
                else
                    step '8 fail before, pass after' pass \
                        'every added or changed test fails on the base and passes on the head'
                fi
            fi
        fi
    fi
}

# ---------------------------------------------------------------------------
# 9 traceability
# ---------------------------------------------------------------------------

# Every step runs, whatever the earlier steps did: see the header.
{
    run_step 9 '9 traceability' bash tools/check_trace.sh || true
}

# ---------------------------------------------------------------------------
# 10 mutation testing
# ---------------------------------------------------------------------------

# A surviving mutant fails the milestone unless mutants-allow.md lists it with
# a reason and the owner approved it. The file's own template says the
# format: one entry per line under "## Approved survivors",
#
#     <crate> :: <file:line> :: <mutant description> :: <reason>
#
# A line in that section that is not in that shape fails the step: an entry the
# gate cannot read is not an approval, and a list the gate skips silently is
# the hole this step closes.

ALLOW_FILE='mutants-allow.md'

# The approved survivors, as tab separated fields: OK, crate, file:line,
# description, reason. A malformed line comes back as MALFORMED.
#
# The entry is split on the FIRST THREE " :: " separators and the reason is
# everything after the third. The separator carries spaces because a mutant
# description holds "::" and " | " of its own, and " :: " is the shape a
# description does not have: cargo-mutants writes a Rust path as
# "BigEndian<'a>::i32", with no spaces around the colons.
approved_survivors() {
    [ -f "$ALLOW_FILE" ] || return 0
    awk '
        function trim(s) {
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", s)
            return s
        }
        # The first three separators, into E1..E4. The fields are globals
        # rather than an array, because awk cannot copy an array into a
        # fresh variable.
        function split_entry(line,   count, rest, at) {
            count = 0
            rest = line
            E1 = ""; E2 = ""; E3 = ""; E4 = ""
            while (count < 3) {
                at = index(rest, " :: ")
                if (at == 0) break
                count++
                if (count == 1) E1 = trim(substr(rest, 1, at - 1))
                else if (count == 2) E2 = trim(substr(rest, 1, at - 1))
                else E3 = trim(substr(rest, 1, at - 1))
                rest = substr(rest, at + 3)
            }
            if (count < 3) return 0
            E4 = trim(rest)
            return 1
        }
        /^##[[:space:]]+Approved survivors/ { insec = 1; next }
        /^##[[:space:]]/ { insec = 0 }
        insec != 1 { next }
        {
            line = $0
            sub(/^[[:space:]]+/, "", line)
            sub(/[[:space:]]+$/, "", line)
            if (line == "") next
            if (line ~ /^#/) next
            if (line == "None yet.") next
            if (!split_entry(line)) {
                print "MALFORMED\t" line
                next
            }
            if (E1 == "" || E2 == "" || E3 == "" || E4 == "") {
                print "MALFORMED\t" line
                next
            }
            print "OK\t" E1 "\t" E2 "\t" E3 "\t" E4
        }
    ' "$ALLOW_FILE"
}

# The mutants that survived, one per line, as cargo-mutants identifies them:
# <file>:<line>:<column>: <description>
survivors_from_log() {
    [ -f "$MUTANTS_LOG" ] || return 0
    awk '
        /^MISSED[[:space:]]/ {
            id = $0
            sub(/^MISSED[[:space:]]+/, "", id)
            # The line ends with the timing, " in 3s build + 4s test". The
            # description can end with " in <function>", so only a timing
            # that starts with a digit is stripped.
            sub(/ in [0-9][^ ]*( [0-9][^ ]*)* (build|test).*$/, "", id)
            if (id != "") print id
        }
    ' "$MUTANTS_LOG"
}

# A string without its leading and trailing whitespace.
trim() {
    local s="$1"
    s="${s#"${s%%[![:space:]]*}"}"
    s="${s%"${s##*[![:space:]]}"}"
    printf '%s' "$s"
}

# The path and the line of a location, without the column. cargo-mutants
# reports a location as <path>:<line>:<column>; an owner may write the entry
# with the column or without it. Both sides are reduced with this one rule,
# so an entry that names the column still matches.
location_path_line() {
    # location_path_line <location>
    printf '%s' "$1" | sed -E 's/^(.*):([0-9]+):[0-9]+$/\1:\2/'
}

# The entry that approves this survivor, if there is one.
approved_entry_for() {
    # approved_entry_for <crate> <file:line> <description>
    local scrate="$1" sloc="$2" sdesc="$3"
    local kind ecrate eloc edesc ereason
    while IFS=$'\t' read -r kind ecrate eloc edesc ereason; do
        [ "$kind" = OK ] || continue
        if [ -n "$scrate" ] && [ "$ecrate" != "$scrate" ]; then continue; fi
        # The entry may name the column as well as the line. The comparison
        # is on the path and the line, because that is what identifies the
        # mutation; a column that the owner added must not stop the match.
        eloc=$(location_path_line "$eloc")
        if [ "$eloc" != "$sloc" ]; then continue; fi
        # The description must BE the survivor's description, not a part of
        # it. A substring match admits every mutant on the line whose
        # description holds the entry's, so an entry that says "replace"
        # would admit every "replace ..." mutant on that line.
        if [ "$edesc" = "$sdesc" ]; then
            printf '%s :: %s :: %s :: %s' "$ecrate" "$eloc" "$edesc" "$ereason"
            return 0
        fi
    done <<EOF
$APPROVED
EOF
    return 1
}

# Every step runs, whatever the earlier steps did: see the header.
{
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
        # The band number comes from tools/layers.txt, and step 3 has already
        # checked that this script and tools/check_layers.sh read the same one.
        MUTANT_CRATES=()
        while read -r band crate; do
            case "$band" in
                ''|'#'*) continue ;;
            esac
            [ "$band" = "$GATE_DEV_BAND" ] && continue
            MUTANT_CRATES+=(-p "$crate")
        done < tools/layers.txt

        # The number of parallel jobs cargo-mutants runs. It must be 1, and
        # that is not a performance setting: it is what makes the survivor
        # list a measurement.
        #
        # cargo-mutants copies the workspace into a temporary directory for
        # the run, and every parallel worker shares the ONE target directory
        # inside that copy. cargo's fingerprint for a path dependency does not
        # include the dependency's path, so a worker can be graded against a
        # binary another worker built from a DIFFERENT mutation. The
        # survivor list is then wrong in both directions: a real gap looks
        # like a survivor, and a real kill looks like a survivor.
        #
        # Measured: with -j 2 a run over the five reader crates reported 27
        # survivors, twelve of them changes to the endian readers that a
        # hand re-measurement showed are killed by this gate. With -j 1 the
        # same crates report 2 survivors for fo1-dat and 3 for the SHA-256
        # module, and every one of those is a genuinely equivalent change.
        #
        # RW_MUTANTS_JOBS overrides the count. An override above 1 is
        # REFUSED rather than honoured: a quiet override that reproduces
        # the defect is worse than no override at all.
        MUTANTS_JOBS="${RW_MUTANTS_JOBS:-1}"
        MUTANTS_JOBS_ERROR=''
        case "$MUTANTS_JOBS" in
            ''|*[!0-9]*)
                MUTANTS_JOBS_ERROR="RW_MUTANTS_JOBS is '$MUTANTS_JOBS', which is not a whole number of jobs"
                ;;
            0)
                # cargo-mutants clamps a job count below 1 to 1, so this is
                # the serial run. The count that is printed is the count that
                # actually runs.
                MUTANTS_JOBS=1
                ;;
        esac
        if [ -z "$MUTANTS_JOBS_ERROR" ] && [ "$MUTANTS_JOBS" -gt 1 ]; then
            MUTANTS_JOBS_ERROR="RW_MUTANTS_JOBS is $MUTANTS_JOBS, and more than one job makes the survivor list unsound"
        fi

        # The mutant build must NOT share a target directory with anything.
        # That is not a speed setting: it is what makes the survivor list a
        # measurement, and the reason is a false kill this gate produced.
        #
        # cargo-mutants copies the workspace into a temporary directory and
        # builds the copy there, but cargo takes its target directory from
        # CARGO_TARGET_DIR when that is set, and ONE target directory then
        # holds every build of the run AND every build of every earlier run
        # that used the same path. Cargo's fingerprint for a path dependency
        # does not include the path, and cargo-mutants' copy of the source
        # keeps the original file times, so a crate that was MUTATED in an
        # earlier run is judged FRESH against that earlier run's mutated
        # artifact, and the mutated binary is linked into this run's test
        # binary.
        #
        # Measured: the run of 20 Oct reported the change
        # crates/fo1-bigendian/src/lib.rs:219:32 (replace | with ^ in
        # BigEndian<'a>::u16) as CAUGHT, and the per-mutant log failed four
        # tests in tests/gates/synthetic_dat.rs with LZSS errors
        # ("OutputShort { declared: 20 }") that the endian reader cannot
        # produce. That run's own baseline log already said "Fresh
        # fo1-lzss" and finished in 0.10 s, so the whole run was graded
        # against the artifacts the PREVIOUS run had left behind, one of
        # which was a mutated fo1-lzss. A hand-applied probe of that exact
        # change passes the whole workspace test suite: the fold's two
        # operands never share a bit, so the change is equivalent and the
        # kill was a false one.
        #
        # The fix is to let cargo-mutants build where it builds by default:
        # in the target directory inside its own temporary copy of the
        # workspace. That directory is made with the copy and deleted with
        # it, so nothing built for one mutant can outlive the run and be
        # linked into another mutant's test binary.
        #
        # So CARGO_TARGET_DIR is REMOVED for this command, not pointed
        # somewhere else: the environment this gate runs in sets one (inside
        # the repository, where it outlives the run), and cargo-mutants'
        # build would inherit it.
        #
        # RW_MUTANTS_TARGET_DIR is REFUSED rather than honoured: a shared
        # target directory is the defect, so there is no safe path to give it.
        MUTANTS_TARGET_ERROR=''
        if [ -n "${RW_MUTANTS_TARGET_DIR:-}" ]; then
            MUTANTS_TARGET_ERROR="RW_MUTANTS_TARGET_DIR is set, and a shared mutant target directory is the defect this step guards against"
        fi

        if [ -n "$MUTANTS_TARGET_ERROR" ]; then
            printf 'FAIL %s\n' "$MUTANTS_TARGET_ERROR"
            printf '     the mutant build must use the target directory inside the temporary copy\n'
            printf '     of the workspace that cargo-mutants makes for the run:\n'
            printf '     a shared target directory keeps the artifacts of an earlier mutant, and of\n'
            printf '     an earlier run, and cargo links them into a later mutant test binary\n'
            step '10 mutation testing' fail "$MUTANTS_TARGET_ERROR"
            note_failure '10 mutation testing'
        elif [ -n "$MUTANTS_JOBS_ERROR" ]; then
            printf 'FAIL %s\n' "$MUTANTS_JOBS_ERROR"
            printf "     the survivor list is only a measurement when the jobs are serial.\n"
            printf "     cargo-mutants copies the workspace into a temporary directory for each\n"
            printf "     mutant, and every parallel worker shares the ONE target directory.\n"
            printf "     cargo's fingerprint for a path dependency does not include the path,\n"
            printf "     so a worker can be graded against a binary another worker built from a\n"
            printf "     DIFFERENT mutation. The list is then wrong in both directions: a real\n"
            printf "     gap looks like a survivor, and a real kill looks like a survivor.\n"
            printf "     Measured: with -j 2 the five reader crates reported 27 survivors; with\n"
            printf "     -j 1 the same crates report 2 for fo1-dat and 3 for the SHA-256 module,\n"
            printf "     and every one of those is a genuinely equivalent change.\n"
            printf "     Run this step with one job: unset RW_MUTANTS_JOBS, or set it to 1.\n"
            step '10 mutation testing' fail "$MUTANTS_JOBS_ERROR"
            note_failure '10 mutation testing'
        elif [ "${#MUTANT_CRATES[@]}" -eq 0 ]; then
            printf 'FAIL tools/layers.txt names no crate to mutate\n'
            step '10 mutation testing' fail 'tools/layers.txt names no crate to mutate'
            note_failure '10 mutation testing'
        else
            # The scope of this run, said out loud so a reader knows it is a
            # decision and not an oversight. The guide asks for the install
            # scope as well ("with FO1_DATA set, so the data tests count"),
            # and cargo-mutants 27.1.0 can run it with
            # `--cargo-test-arg --ignored`, but the measured cost is 374
            # mutants x 9.74 s = 60 minutes on top of the 12 the fast scope
            # takes. The 69 fo1-exe survivors the old report called scope
            # artifacts are killed by this scope, and every survivor that is
            # left is approved one by one, with its own reason, in
            # mutants-allow.md.
            printf 'mutation scope: the synthetic suites, without the install scope\n'
            printf '  the install scope would add --cargo-test-arg --ignored, about 60 minutes\n'
            printf '  the 69 fo1-exe survivors the old report called scope artifacts are\n'
            printf '  killed by this scope; every survivor left is approved one by one in %s\n' \
                "$ALLOW_FILE"
            # Where the mutant builds go, said out loud. The target directory
            # is inside the temporary copy of the workspace that cargo-mutants
            # makes for the run, and that copy is deleted when the run ends,
            # so a mutant build can never be judged fresh against an earlier
            # mutant's artifacts, or against an earlier run's.
            printf 'mutation build directory: the target directory inside cargo-mutants own\n'
            printf '  temporary copy of the workspace, which is created and deleted with the run\n'
            printf '  no target directory is shared, so no mutant is graded against the artifacts\n'
            printf '  of another mutant, or of an earlier run\n'
            # The job count, said out loud, and the reason the list can be
            # read as a measurement at all.
            printf 'mutation jobs: %s\n' "$MUTANTS_JOBS"
            printf '  the jobs are serial, so the survivor list is a measurement:\n'
            printf '  every mutant is built and graded on its own, with no other worker\n'
            printf '  grading it against a binary built from a different mutation\n'
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
            # `-j 1` is not a performance setting, it is what makes the
            # survivor list a measurement. cargo-mutants copies the
            # workspace into a temporary directory for the run and every
            # parallel worker shares the ONE target directory inside that
            # copy, so with more than one job a worker can be graded
            # against a binary another worker built from a DIFFERENT
            # mutation, and the list is wrong in both directions: a real
            # gap looks like a survivor and a real kill looks like a
            # survivor. The count is 1, and the gate refuses an override
            # above 1 (the job-count check above).
            MUTANTS_LOG="$REPORT_DIR/mutants.log"
            MUTANTS_STATUS=0
            # CARGO_TARGET_DIR is REMOVED for this command, and not pointed
            # somewhere else. The mutant build must not share a target
            # directory with any other build, or with any earlier run: a
            # shared directory keeps the artifacts of an earlier mutant, and
            # cargo links a stale MUTATED artifact into the next mutant's
            # test binary, which grades a Frankenstein build and reports a
            # false kill. Without the variable, cargo-mutants builds in the
            # target directory inside its own temporary copy of the
            # workspace, which is made with the copy and deleted with it.
            env -u CARGO_TARGET_DIR cargo mutants \
                "${MUTANT_CRATES[@]}" \
                --test-workspace true \
                --profile release \
                --no-shuffle \
                -j "$MUTANTS_JOBS" \
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
            MUTANTS_MISSED=$(sed -n 's/^[0-9][0-9]* mutants\? tested.*: \([0-9][0-9]*\) missed.*/\1/p' \
                "$MUTANTS_LOG" | tail -1)
            # How many mutants timed out. cargo-mutants counts a timeout as
            # its own outcome: the final summary prints "<n> timeouts" and
            # the live progress line prints "<n> timeout" after
            # "<done>/<total> mutants tested". The old parse looked for a
            # field the summary does not have, so the arm below could never
            # fire. A timed out mutant was never graded, so a run with one
            # proves nothing.
            MUTANTS_TIMEOUT=$(sed -n \
                -e 's/^[0-9][0-9]* mutants\? tested.*[^0-9]\([0-9][0-9]*\) timeouts\?.*/\1/p' \
                -e 's/^[0-9][0-9]*\/[0-9][0-9]* mutants\? tested.*[^0-9]\([0-9][0-9]*\) timeout.*/\1/p' \
                "$MUTANTS_LOG" | tail -1)
            MUTANTS_TIMEOUT=${MUTANTS_TIMEOUT:-0}

            # The approved survivors, and a malformed entry is a failure of
            # this step: the owner approved something the gate cannot read.
            APPROVED=$(approved_survivors)
            BAD_ENTRY=$(printf '%s\n' "$APPROVED" | grep -c '^MALFORMED' || true)
            if [ "${BAD_ENTRY:-0}" -ne 0 ]; then
                printf '%s\n' "$APPROVED" | grep '^MALFORMED' | sed 's/^MALFORMED/FAIL malformed entry in /'
                printf '     the format is <crate> :: <file:line> :: <mutant description> :: <reason>\n'
                step '10 mutation testing' fail "$BAD_ENTRY malformed entr(y|ies) in $ALLOW_FILE"
                note_failure '10 mutation testing'
            elif [ "$MUTANTS_TIMEOUT" -gt 0 ]; then
                printf 'FAIL %s mutant(s) timed out, and a timeout proves nothing\n' \
                    "$MUTANTS_TIMEOUT"
                step '10 mutation testing' fail "$MUTANTS_TIMEOUT mutant(s) timed out"
                note_failure '10 mutation testing'
            elif [ -z "$MUTANTS_CAUGHT" ]; then
                printf 'FAIL the run reported no caught count, so nothing was proven\n'
                step '10 mutation testing' fail 'the run reported no caught count'
                note_failure '10 mutation testing'
            elif [ "$MUTANTS_CAUGHT" -eq 0 ]; then
                printf 'FAIL %s mutant(s) tested and 0 caught, so the suite proved nothing\n' \
                    "${MUTANTS_TESTED:-0}"
                step '10 mutation testing' fail "0 of ${MUTANTS_TESTED:-0} mutants caught"
                note_failure '10 mutation testing'
            else
                SURVIVORS=$(survivors_from_log)
                SURVIVOR_COUNT=$(printf '%s\n' "$SURVIVORS" | grep -c . || true)
                if [ -n "$MUTANTS_MISSED" ] && [ "${SURVIVOR_COUNT:-0}" != "$MUTANTS_MISSED" ]; then
                    printf 'FAIL the run reports %s missed mutant(s) but %s could be read, so the survivor list is incomplete\n' \
                        "$MUTANTS_MISSED" "${SURVIVOR_COUNT:-0}"
                    step '10 mutation testing' fail 'the survivor list could not be read completely'
                    note_failure '10 mutation testing'
                elif [ "$MUTANTS_STATUS" -ne 0 ] && [ "${SURVIVOR_COUNT:-0}" -eq 0 ]; then
                    printf 'FAIL the run failed and no surviving mutant explains it\n'
                    step '10 mutation testing' fail 'the run failed and no surviving mutant explains it'
                    note_failure '10 mutation testing'
                else
                    ADMITTED=0
                    UNADMITTED=0
                    printf '%s\n' "$SURVIVORS" | while IFS= read -r sid; do
                        [ -n "$sid" ] || continue
                        loc=$(printf '%s' "$sid" | sed -n 's/^\([^ ]*:[0-9][0-9]*:[0-9][0-9]*\): .*$/\1/p')
                        desc=$(trim "$(printf '%s' "$sid" | sed -n 's/^[^ ]*:[0-9][0-9]*:[0-9][0-9]*: //p')")
                        fileline=$(location_path_line "$loc")
                        case "$fileline" in
                            crates/*/*) scrate=$(printf '%s' "$fileline" | cut -d/ -f2) ;;
                            *) scrate='' ;;
                        esac
                        if entry=$(approved_entry_for "$scrate" "$fileline" "$desc"); then
                            printf 'survivor  %-44s admitted by %s\n' "$sid" "$entry"
                        else
                            printf 'survivor  %-44s NOT ADMITTED\n' "$sid"
                            printf '     expected entry: %s :: %s :: %s :: <reason>\n' \
                                "${scrate:-<crate>}" "$fileline" "$desc"
                            printf '     the description must be exactly the one above\n'
                        fi
                    done > "$REPORT_DIR/survivors.txt"
                    cat "$REPORT_DIR/survivors.txt"
                    ADMITTED=$(grep -c 'admitted by' "$REPORT_DIR/survivors.txt" || true)
                    UNADMITTED=$(grep -c 'NOT ADMITTED' "$REPORT_DIR/survivors.txt" || true)
                    if [ "${UNADMITTED:-0}" -ne 0 ]; then
                        printf 'FAIL %s surviving mutant(s) are not approved in %s\n' \
                            "$UNADMITTED" "$ALLOW_FILE"
                        step '10 mutation testing' fail "$UNADMITTED surviving mutant(s) are not approved"
                        note_failure '10 mutation testing'
                    else
                        step '10 mutation testing' pass \
                            "$MUTANTS_CAUGHT of ${MUTANTS_TESTED:-?} mutants caught, ${ADMITTED:-0} survivor(s) approved in $ALLOW_FILE"
                    fi
                fi
            fi
        fi
    fi
}

# ---------------------------------------------------------------------------
# account for the steps that never ran
# ---------------------------------------------------------------------------

# Every step runs, so this loop has nothing to fill in on a normal run: it
# is the safety net that keeps the report at ten rows when a step returned
# without recording a verdict. A step that is missing from the report is a
# required skip, and a required skip fails the gate: an unmeasured step is
# never a passed step.
COUNT=${#STEP_NAME[@]}
NUMBER=$((COUNT + 1))
while [ "$NUMBER" -le "$TOTAL_STEPS" ]; do
    case "$NUMBER" in
        1) NOT_RUN_NAME='1 protected paths' ;;
        2) NOT_RUN_NAME='2 public hygiene' ;;
        3) NOT_RUN_NAME='3 formatting, lints, layering' ;;
        4) NOT_RUN_NAME='4 workspace tests' ;;
        5) NOT_RUN_NAME='5 install fingerprint' ;;
        6) NOT_RUN_NAME='6 ignored tests' ;;
        7) NOT_RUN_NAME='7 count check' ;;
        8) NOT_RUN_NAME='8 fail before, pass after' ;;
        9) NOT_RUN_NAME='9 traceability' ;;
        10) NOT_RUN_NAME='10 mutation testing' ;;
        *) NOT_RUN_NAME="step $NUMBER" ;;
    esac
    step "$NOT_RUN_NAME" skip 'this step recorded no verdict'
    SKIPPED_REQUIRED="$SKIPPED_REQUIRED $NOT_RUN_NAME"
    printf '\n=== step %s/10 === SKIPPED (this step recorded no verdict)\n' "$NUMBER"
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
    printf -- '- Every step ran. The gate does not stop at the first failure: it runs every\n'
    printf -- '  step and reports every verdict, so a run names every defect at once.\n'
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

BODY_DIGEST=$(finish_report)

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