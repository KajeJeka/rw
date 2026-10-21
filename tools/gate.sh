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

if [ -z "$FAILED_AT" ]; then
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
            FAILED_AT='7 count check'
        else
            step '7 count check' pass 'every floor in expected_counts.toml is met'
        fi
    fi
fi

# ---------------------------------------------------------------------------
# 8 fail before, pass after
# ---------------------------------------------------------------------------

# The guide: "The gate copies the test files from the head commit onto the base
# tree and runs them. A new or changed test must fail on the base, and a
# compile error counts as a failure. It must pass on the head. A test that
# already passes on the base proves nothing about the change. It needs a
# written note that explains why, and the owner reviews the note. A pure
# refactor that changes no test is exempt."
#
# The base tree is a throwaway worktree of BASE_REF with HEAD's version of the
# changed test file copied into it, and HEAD's [[test]] entry for that file
# added to the base Cargo.toml when the base does not have it. One worktree
# serves every changed file: the base is built once and the files are copied
# in one at a time. The base has its own target directory, so a base build
# cannot poison the head build, and the whole worktree is removed afterwards,
# build output and all.
#
# A test that already passes on the base needs a written note in
# docs/fail-before-notes.md. The note is the owner's, not the gate's: the gate
# only checks that one exists, that it is in the shape the file documents, and
# it prints the one it relied on. A note in a shape the gate cannot read fails
# the step, the same way a floor in expected_counts.toml that the gate does not
# understand fails step 7: an approval nobody can read is not an approval.

NOTES_FILE='docs/fail-before-notes.md'
BASE_WT=''

cleanup_base_worktree() {
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
    return 0
}
trap cleanup_base_worktree EXIT

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

# The number of tests that ran in a cargo test run: the ones that passed plus
# the ones that failed. The ignored ones did not run, so they are not in it.
tests_ran() {
    local out="$1" passed failed
    passed=$(printf '%s\n' "$out" | sed -n 's/^test result: .*\([0-9][0-9]*\) passed.*/\1/p' | tail -1)
    failed=$(printf '%s\n' "$out" | sed -n 's/^test result: .*\([0-9][0-9]*\) failed.*/\1/p' | tail -1)
    printf '%s' "$((${passed:-0} + ${failed:-0}))"
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
        /^##[[:space:]]+Entries/ { insec = 1; next }
        /^##[[:space:]]/ { insec = 0 }
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

# The names of the test functions in the HEAD tree's copy of a file: a
# "#[test]" attribute and the function it marks.
head_test_names() {
    # head_test_names <path>
    git show "HEAD:$1" 2>/dev/null | awk '
        /#\[test\]/ { want = 1; next }
        want && /fn[ \t]+[A-Za-z_][A-Za-z0-9_]*[ \t]*\(/ {
            name = $0
            sub(/^.*fn[ \t]+/, "", name)
            sub(/[ \t]*\(.*$/, "", name)
            print name
            want = 0
        }
    '
}

# Every note must name the test it covers. A note whose test name does not
# exist in the file it names is rejected: a note that names the wrong test
# is not an approval of anything, and the file-level admission decision
# stays where it is.
validate_notes() {
    local kind nfile ntest nreason target bad=0
    while IFS=$'\t' read -r kind nfile ntest nreason; do
        [ "$kind" = OK ] || continue
        if ! head_has_path "$nfile"; then
            printf 'FAIL the note names %s, which is not in the HEAD tree\n' "$nfile"
            bad=1
            continue
        fi
        target=$(head_target_name "$nfile")
        if [ -n "$target" ] && [ "$target" = "$ntest" ]; then
            continue
        fi
        if head_test_names "$nfile" | grep -qxF "$ntest"; then
            continue
        fi
        printf 'FAIL the note names the test %s, which is not in %s\n' "$ntest" "$nfile"
        bad=1
    done <<EOF
$NOTES
EOF
    return "$bad"
}

# The note that covers this test file, if the owner wrote one.
note_for() {
    # note_for <test file>
    local want="$1" kind nfile ntest nreason
    while IFS=$'\t' read -r kind nfile ntest nreason; do
        [ "$kind" = OK ] || continue
        if [ "$nfile" = "$want" ]; then
            printf '%s :: %s :: %s' "$nfile" "$ntest" "$nreason"
            return 0
        fi
    done <<EOF
$NOTES
EOF
    return 1
}

if [ -z "$FAILED_AT" ]; then
    printf '\n=== step 8/10: 8 fail before, pass after ===\n'
    if [ "$BASE_STATE" = UNRESOLVED ]; then
        skip_step 8 '8 fail before, pass after' "$BASE_REF does not resolve, so there is no before"
    elif ! have cargo; then
        skip_step 8 '8 fail before, pass after' 'cargo is not installed'
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
            # The notes are read once, and a malformed entry fails the step
            # before any run is made: an approval the gate cannot read is not
            # an approval, the same way a key in expected_counts.toml that the
            # gate does not understand fails step 7.
            NOTES=$(read_notes)
            NOTES_BAD=$(printf '%s\n' "$NOTES" | grep -c '^MALFORMED' || true)
            if [ "${NOTES_BAD:-0}" -ne 0 ]; then
                printf '%s\n' "$NOTES" | grep '^MALFORMED' |
                    sed 's/^MALFORMED/FAIL malformed note entry in /'
                printf '     the format is <test file> :: <test name> :: <reason>\n'
                printf '     the entries live in the "## Entries" section of %s\n' "$NOTES_FILE"
            fi
            # A note must name the test it covers, so a note that names a test
            # the file does not hold is rejected before any run is made.
            NOTES_UNNAMED=0
            if ! validate_notes; then
                NOTES_UNNAMED=1
                printf '     the test name must be the [[test]] target name or a test\n'
                printf '     function in the file the note names\n'
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
                FAILED_AT='8 fail before, pass after'
            else
                STEP8_FAIL=0
                STEP8_SKIP=''
                if [ "${NOTES_BAD:-0}" -ne 0 ] || [ "${NOTES_UNNAMED:-0}" -ne 0 ]; then
                    STEP8_FAIL=1
                fi
                for rel in $CHANGED_TESTS; do
                    name=$(head_target_name "$rel")
                    if [ -z "$name" ]; then
                        printf 'fail-before  %-34s no [[test]] entry in HEAD Cargo.toml\n' "$rel"
                        printf 'FAIL %s has no [[test]] entry in HEAD Cargo.toml, so it is not a test target\n' \
                            "$rel"
                        STEP8_FAIL=1
                        continue
                    fi

                    # The head run: the test must pass here. It also says how
                    # many tests the target actually runs, which decides
                    # whether the runs need --ignored.
                    head_out=$(cargo test --release --test "$name" 2>&1)
                    head_status=$?
                    ran=$(tests_ran "$head_out")
                    extra=''
                    if [ "$ran" -eq 0 ] && ! is_compile_error "$head_out"; then
                        # Every test in the target is #[ignore]d, so the plain
                        # run measured nothing. The install tests need the
                        # player's install, and without it there is nothing to
                        # run them against.
                        if [ -z "${FO1_DATA:-}" ]; then
                            printf 'fail-before  %-34s base not-run   head passed  (no FO1_DATA)\n' "$rel"
                            printf 'FAIL %s runs no test without FO1_DATA, so the fail-before check is not measurable\n' \
                                "$rel"
                            STEP8_SKIP="$rel runs no test without FO1_DATA"
                            continue
                        fi
                        extra='-- --ignored'
                        head_out=$(cargo test --release --test "$name" $extra 2>&1)
                        head_status=$?
                    fi

                    # Copy HEAD's version of the test file onto the base tree
                    # and give the base the [[test]] entry for it when the
                    # base does not have one.
                    mkdir -p "$BASE_WT/$(dirname "$rel")"
                    if ! git show "HEAD:$rel" >"$BASE_WT/$rel" 2>/dev/null; then
                        printf 'fail-before  %-34s base not-run   head %s\n' \
                            "$rel" "$([ "$head_status" -eq 0 ] && printf passed || printf failed)"
                        printf 'FAIL %s could not be read from HEAD\n' "$rel"
                        STEP8_FAIL=1
                        continue
                    fi
                    if ! grep -q "path = \"$rel\"" "$BASE_WT/Cargo.toml" 2>/dev/null; then
                        block=$(head_test_block "$rel")
                        if [ -n "$block" ]; then
                            printf '\n%s\n' "$block" >>"$BASE_WT/Cargo.toml"
                        fi
                    fi

                    base_out=$(cd "$BASE_WT" &&
                        CARGO_TARGET_DIR="$BASE_TARGET" cargo test --release --test "$name" $extra 2>&1)
                    base_status=$?

                    if [ "$base_status" -ne 0 ]; then
                        # A compile error counts as a failure, and so is a
                        # failing test: either way the test does not pass on
                        # the base, which is what the change had to earn. A
                        # failed test also ends with "error: test failed", so
                        # the compile error is looked for by its own shape.
                        if is_compile_error "$base_out"; then
                            base_verdict='compile error'
                        else
                            base_verdict='failed'
                        fi
                    else
                        base_verdict='passed'
                    fi
                    if [ "$head_status" -eq 0 ]; then
                        head_verdict='passed'
                    else
                        head_verdict='failed'
                    fi
                    printf 'fail-before  %-34s base %-14s head %s\n' \
                        "$rel" "$base_verdict" "$head_verdict"

                    if [ "$base_status" -ne 0 ]; then
                        printf '     base run, last lines:\n'
                        printf '%s\n' "$base_out" | tail -12
                    fi
                    if [ "$head_status" -ne 0 ]; then
                        printf '     head run, last lines:\n'
                        printf '%s\n' "$head_out" | tail -12
                        printf 'FAIL %s does not pass on the head\n' "$rel"
                        STEP8_FAIL=1
                    elif [ "$base_status" -eq 0 ]; then
                        # The test passes on the base, so it proves nothing
                        # about this change. It needs a written note.
                        note=$(note_for "$rel")
                        if [ -z "$note" ]; then
                            printf 'FAIL %s passes on the base, so it proves nothing about this change\n' "$rel"
                            printf '     expected note line: %s :: %s :: <reason>\n' "$rel" "$name"
                            printf '     add it to %s\n' "$NOTES_FILE"
                            STEP8_FAIL=1
                        else
                            printf '     relying on the note: %s\n' "$note"
                        fi
                    fi
                done
                cleanup_base_worktree

                if [ "$STEP8_FAIL" -ne 0 ]; then
                    step '8 fail before, pass after' fail 'a changed test did not fail on the base, or did not pass on the head'
                    FAILED_AT='8 fail before, pass after'
                elif [ -n "$STEP8_SKIP" ]; then
                    skip_step 8 '8 fail before, pass after' "$STEP8_SKIP"
                else
                    step '8 fail before, pass after' pass 'every changed test fails on the base and passes on the head'
                fi
            fi
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
            FAILED_AT='10 mutation testing'
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
            FAILED_AT='10 mutation testing'
        elif [ "${#MUTANT_CRATES[@]}" -eq 0 ]; then
            printf 'FAIL tools/layers.txt names no crate to mutate\n'
            step '10 mutation testing' fail 'tools/layers.txt names no crate to mutate'
            FAILED_AT='10 mutation testing'
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
                FAILED_AT='10 mutation testing'
            elif [ "$MUTANTS_TIMEOUT" -gt 0 ]; then
                printf 'FAIL %s mutant(s) timed out, and a timeout proves nothing\n' \
                    "$MUTANTS_TIMEOUT"
                step '10 mutation testing' fail "$MUTANTS_TIMEOUT mutant(s) timed out"
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
                SURVIVORS=$(survivors_from_log)
                SURVIVOR_COUNT=$(printf '%s\n' "$SURVIVORS" | grep -c . || true)
                if [ -n "$MUTANTS_MISSED" ] && [ "${SURVIVOR_COUNT:-0}" != "$MUTANTS_MISSED" ]; then
                    printf 'FAIL the run reports %s missed mutant(s) but %s could be read, so the survivor list is incomplete\n' \
                        "$MUTANTS_MISSED" "${SURVIVOR_COUNT:-0}"
                    step '10 mutation testing' fail 'the survivor list could not be read completely'
                    FAILED_AT='10 mutation testing'
                elif [ "$MUTANTS_STATUS" -ne 0 ] && [ "${SURVIVOR_COUNT:-0}" -eq 0 ]; then
                    printf 'FAIL the run failed and no surviving mutant explains it\n'
                    step '10 mutation testing' fail 'the run failed and no surviving mutant explains it'
                    FAILED_AT='10 mutation testing'
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
                        FAILED_AT='10 mutation testing'
                    else
                        step '10 mutation testing' pass \
                            "$MUTANTS_CAUGHT of ${MUTANTS_TESTED:-?} mutants caught, ${ADMITTED:-0} survivor(s) approved in $ALLOW_FILE"
                    fi
                fi
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