#!/usr/bin/env bash
# Claim traceability between the spec, the gate tests and the oracle classes.
#
# Contract, from docs/spec/trace.md:
#   - a test names no claim id                        -> fail
#   - a test declares no oracle class, or a class the trace table does not define -> fail
#   - a CONFIRMED AGAINST ORIGINAL BINARY claim has no
#     test in oracle class 1, 2 or 3                   -> fail
#   - the trace table's map row for that claim names a test that does not
#     exist, or that does not cite the claim          -> fail
#   - the map is empty                                 -> fail
#
# Exit status: 0 complete, 1 traceability incomplete.

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

SPEC_DIR='docs/spec'
GATE_DIR='tests/gates'
TRACE="$SPEC_DIR/trace.md"
CARGO_TOML='Cargo.toml'

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

# --- the gate targets the root manifest declares --------------------------------

# A file under tests/gates/ is only a gate target if the root Cargo.toml says so.
# Cargo does not auto-discover a subdirectory, so every gate file needs its own
# [[test]] entry, and a file without one is never run: a map row that names it
# names a test the gate does not execute.
GATE_TARGETS=''
if [ -f "$CARGO_TOML" ]; then
    GATE_TARGETS=$(awk '
        /^\[\[test\]\][ \t]*$/ { intest = 1; next }
        /^\[/ { intest = 0 }
        intest && /^path[ \t]*=[ \t]*"tests\/gates\// {
            p = $0
            sub(/.*"tests\/gates\//, "", p)
            sub(/".*/, "", p)
            print p
        }
    ' "$CARGO_TOML" 2>/dev/null | sort -u || true)
fi

is_gate_target() {
    case "
$GATE_TARGETS
" in
        *"
$1
"*) return 0 ;;
        *) return 1 ;;
    esac
}

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

# --- the CONFIRMED extractor ---------------------------------------------------

# A claim is declared by the spec in one of two shapes, and both must be read:
#
#   - a table row whose FIRST CELL is the claim id, with the evidence tag in the
#     row's LAST CELL (`docs/spec/endian.md`);
#   - a heading that begins with the claim id, with the evidence tag on a line of
#     its own inside that section (`docs/spec/dat.md`, `docs/spec/lzss.md`,
#     `docs/spec/exe-tables.md`).
#
# A claim id that merely appears in a sentence is a cross-reference, not a
# declaration, and must never be read as one. That is why the tag is looked for on
# a line that IS a tag and not on any line that mentions one, and why the heading
# branch only fires when the heading itself starts with the claim id.
#
# The old extractor skipped heading lines outright, so every claim the spec
# declares in a heading was invisible to the rule, and it attributed a section's
# marker to every id mentioned in the section's prose. It also matched the marker
# as a bare substring, so a sentence that says a claim "cannot carry CONFIRMED
# AGAINST ORIGINAL BINARY" marked that claim confirmed.
#
# Output: one "CLAIM<TAB>TAG" line per declared claim.
declared_claims() {
    awk -v confirmed="$CONFIRMED_MARK" '
        BEGIN {
            tags[1] = confirmed
            tags[2] = "CONFIRMED IN COMMUNITY EDITION ONLY"
            tags[3] = "VERSION DIFFERENCE IDENTIFIED"
            tags[4] = "UNVERIFIED"
        }
        function trim(s) {
            gsub(/^[ \t]+/, "", s)
            gsub(/[ \t]+$/, "", s)
            return s
        }
        # The line must BE a tag: the tag text alone, optionally in backticks or
        # bold. A sentence that mentions the tag is not a declaration.
        function tag_of_line(s,   t, i) {
            t = trim(s)
            gsub(/^`+/, "", t)
            gsub(/`+$/, "", t)
            gsub(/^\*+/, "", t)
            gsub(/\*+$/, "", t)
            t = trim(t)
            for (i = 1; i <= 4; i++) if (t == tags[i]) return tags[i]
            return ""
        }
        function cell(s, n,   t, parts, i) {
            t = trim(s)
            if (substr(t, 1, 1) != "|") return ""
            n = split(t, parts, /\|/)
            if (n < 3) return ""
            return trim(parts[2])
        }
        function last_cell(s, n,   t, parts, i) {
            t = trim(s)
            if (substr(t, 1, 1) != "|") return ""
            n = split(t, parts, /\|/)
            if (n < 3) return ""
            return trim(parts[n - 1])
        }
        { lines[NR] = $0 }
        END {
            for (i = 1; i <= NR; i++) {
                line = lines[i]
                if (line ~ /^[ \t]*\|/) {
                    cid = cell(line)
                    if (cid ~ /^[A-Z]+-[0-9][0-9][0-9]$/) {
                        tag = ""
                        lc = last_cell(line)
                        for (t = 1; t <= 4; t++) {
                            if (index(lc, tags[t]) > 0) { tag = tags[t]; break }
                        }
                        if (tag != "") print cid "\t" tag
                    }
                    continue
                }
                if (line ~ /^#+[ \t]/) {
                    rest = line
                    sub(/^#+[ \t]+/, "", rest)
                    cid = ""
                    if (match(rest, /^[A-Z]+-[0-9][0-9][0-9]/)) {
                        cid = substr(rest, RSTART, RLENGTH)
                    }
                    if (cid == "") continue
                    tag = ""
                    for (j = i + 1; j <= NR; j++) {
                        if (lines[j] ~ /^#/) break
                        tag = tag_of_line(lines[j])
                        if (tag != "") break
                    }
                    if (tag != "") print cid "\t" tag
                    continue
                }
            }
        }
    ' "$1" 2>/dev/null
}

# The trace table is the map, not a spec: it declares no claim of its own.
SPEC_DECL_FILES=''
for f in $SPEC_FILES; do
    [ "$f" = "$TRACE" ] && continue
    SPEC_DECL_FILES="$SPEC_DECL_FILES $f"
done

DECLARED=$(for f in $SPEC_DECL_FILES; do declared_claims "$f"; done | grep -vE "$NOT_A_CLAIM" | sort -u || true)
DECLARED_COUNT=$(printf '%s\n' "$DECLARED" | grep -c . || true)
CONFIRMED=$(printf '%s\n' "$DECLARED" | awk -F'\t' -v m="$CONFIRMED_MARK" '$2 == m { print $1 }' || true)
CONFIRMED_COUNT=$(printf '%s\n' "$CONFIRMED" | grep -c . || true)

printf 'check_trace.sh: %s claim(s) declared in the spec\n' "$DECLARED_COUNT"
printf 'check_trace.sh: %s claim(s) marked %s\n' "$CONFIRMED_COUNT" "$CONFIRMED_MARK"

# --- the trace table's map -----------------------------------------------------

# One line per map entry: CLAIM<TAB>FILE<TAB>TEST<TAB>CLASSES. The class cell is
# read in the same order as the test cell, so a class is always paired with the
# test to its left.
map_entries() {
    awk '
        function trim(s) {
            gsub(/^[ \t]+/, "", s)
            gsub(/[ \t]+$/, "", s)
            return s
        }
        /^## / {
            if (inmap) exit
            if (index($0, "## Claim to test map") == 1) inmap = 1
            next
        }
        inmap && /^[ \t]*\|/ {
            n = split($0, parts, /\|/)
            if (n < 4) next
            cid = trim(parts[2])
            sub(/[ \t].*$/, "", cid)
            if (cid !~ /^[A-Z]+-[0-9][0-9][0-9]$/) next
            tests = parts[3]
            classes = parts[4]
            nt = split(tests, tparts, /;[ \t]*/)
            nc = split(classes, cparts, /;[ \t]*/)
            for (i = 1; i <= nt; i++) {
                if (match(tparts[i], /`[^`]+`::[A-Za-z0-9_]+/)) {
                    e = substr(tparts[i], RSTART, RLENGTH)
                    file = e
                    sub(/^`/, "", file)
                    sub(/`.*$/, "", file)
                    name = e
                    sub(/^.*::/, "", name)
                    cl = ""
                    if (i <= nc) cl = trim(cparts[i])
                    print cid "\t" file "\t" name "\t" cl
                }
            }
        }
    ' "$TRACE" 2>/dev/null || true
}

MAP_ENTRIES=$(map_entries)
MAP_ROW_COUNT=$(printf '%s\n' "$MAP_ENTRIES" | grep -c . || true)

# --- the test inventory --------------------------------------------------------

# For one gate file: "TEST<TAB>CLAIM" for every claim id the test's own doc
# attribute carries. When the doc comment has a `claim:` line, that line is the
# attribute; when it has none, the claim ids the comment states in prose are. A
# doc comment with a `claim:` line that omits the claim does not carry it.
test_claims() {
    awk '
        function claim_ids(text,   rest, id) {
            rest = text
            while (match(rest, /[A-Z]+-[0-9][0-9][0-9]/)) {
                id = substr(rest, RSTART, RLENGTH)
                print id
                rest = substr(rest, RSTART + RLENGTH)
            }
        }
        { lines[NR] = $0 }
        END {
            for (i = 1; i <= NR; i++) {
                if (lines[i] !~ /^[ \t]*#\[test\]/) continue
                j = i + 1
                while (j <= NR && lines[j] !~ /^[ \t]*fn[ \t]+[A-Za-z0-9_]+/) j++
                if (j > NR) continue
                name = lines[j]
                sub(/^[ \t]*fn[ \t]+/, "", name)
                sub(/[ \t(].*$/, "", name)
                k = i - 1
                while (k >= 1 && lines[k] ~ /^[ \t]*#\[/) k--
                doc = ""
                while (k >= 1 && lines[k] ~ /^[ \t]*\/\/\//) {
                    doc = lines[k] "\n" doc
                    k--
                }
                attribute = ""
                has_claim_line = 0
                n = split(doc, dl, "\n")
                for (x = 1; x <= n; x++) {
                    if (dl[x] ~ /^[ \t]*\/\/\/[ \t]*claim:/) {
                        has_claim_line = 1
                        attribute = attribute " " dl[x]
                    }
                }
                if (has_claim_line == 0) attribute = doc
                n2 = split(attribute, al, "\n")
                for (x = 1; x <= n2; x++) {
                    rest = al[x]
                    while (match(rest, /[A-Z]+-[0-9][0-9][0-9]/)) {
                        id = substr(rest, RSTART, RLENGTH)
                        print name "\t" id
                        rest = substr(rest, RSTART + RLENGTH)
                    }
                }
            }
        }
    ' "$1" 2>/dev/null || true
}

TEST_CLAIMS=''
for f in $GATE_FILES; do
    [ -f "$f" ] || continue
    base=${f##*/}
    TEST_CLAIMS="$TEST_CLAIMS
$(test_claims "$f" | sed "s|^|$base\t|")"
done

test_cites_claim() {
    printf '%s\n' "$TEST_CLAIMS" | awk -F'\t' -v f="$1" -v t="$2" -v c="$3" '
        $1 == f && $2 == t && $3 == c { found = 1 }
        END { exit(found ? 0 : 1) }
    '
}

# Every test function in one gate file, one name per line.
test_names() {
    awk '
        { lines[NR] = $0 }
        END {
            for (i = 1; i <= NR; i++) {
                if (lines[i] !~ /^[ \t]*#\[test\]/) continue
                j = i + 1
                while (j <= NR && lines[j] !~ /^[ \t]*fn[ \t]+[A-Za-z0-9_]+/) j++
                if (j > NR) continue
                name = lines[j]
                sub(/^[ \t]*fn[ \t]+/, "", name)
                sub(/[ \t(].*$/, "", name)
                print name
            }
        }
    ' "$1" 2>/dev/null || true
}

test_exists() {
    printf '%s\n' "$TEST_CLAIMS" | awk -F'\t' -v f="$1" -v t="$2" '
        $1 == f && $2 == t { found = 1 }
        END { exit(found ? 0 : 1) }
    '
}

class_is_strong() {
    # The class cell may list several classes for one test ("4, 5") and may mark
    # a class taken from the file header with a dagger ("2†").
    printf '%s\n' "$1" | grep -qE "(^|[^0-9])(1|2|3)([^0-9]|$)" && return 0
    return 1
}

# --- rule 2: every confirmed claim has a strong-class witness ------------------

# The guide (Engineering philosophy, "Constants ledger and traceability") says:
# `docs/spec/trace.md` maps each claim id to its tests and the oracle class of
# each test, and the gate fails when a CONFIRMED claim has no test in oracle
# class 1, 2 or 3. So the map is the place the class is recorded, and the check
# reads the map and then proves that the map is telling the truth:
#
#   - the file the row names is under tests/gates/;
#   - that file is a declared gate target in the root Cargo.toml;
#   - the test the row names exists in that file;
#   - the test's own doc attribute carries that claim id;
#   - and at least one of the row's tests is in oracle class 1, 2 or 3.
#
# The claim id is substituted into every lookup. The old check asked whether a
# file mentioned SOME claim id, so a claim id that existed nowhere scored as
# covered and the rule could never fire.
while IFS= read -r claim; do
    [ -n "$claim" ] || continue
    entries=$(printf '%s\n' "$MAP_ENTRIES" | awk -F'\t' -v c="$claim" '$1 == c' || true)
    if [ -z "$entries" ]; then
        problem "$claim is marked $CONFIRMED_MARK but the trace table has no row for it"
        continue
    fi
    strong=0
    while IFS=$'\t' read -r row_claim file name classes; do
        [ -n "$file" ] || continue
        if [ ! -f "$GATE_DIR/$file" ]; then
            problem "$claim is marked $CONFIRMED_MARK but its trace table row names $file, which is not a file under $GATE_DIR"
            continue
        fi
        if ! is_gate_target "$file"; then
            problem "$claim is marked $CONFIRMED_MARK but its trace table row names $file, which the root $CARGO_TOML does not declare as a [[test]] gate target"
            continue
        fi
        if ! test_exists "$file" "$name"; then
            problem "$claim is marked $CONFIRMED_MARK but its trace table row names $file::$name, and no such test exists in that file"
            continue
        fi
        if ! test_cites_claim "$file" "$name" "$claim"; then
            problem "$claim is marked $CONFIRMED_MARK but its trace table row names $file::$name, whose own doc attribute does not carry $claim"
            continue
        fi
        if class_is_strong "$classes"; then
            strong=1
        fi
    done <<ENTRIES
$entries
ENTRIES
    if [ "$strong" -eq 0 ]; then
        problem "$claim is marked $CONFIRMED_MARK but no test it names is in oracle class $STRONG_CLASSES"
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

# --- rule 4: the map and the gate files name each other ------------------------

# The map is the claim to test map, so it has to be complete in both directions:
# every test in the seven gate files has to be named by some row, and every claim
# id a test names has to have a row. A test nobody names is a test the trace
# table does not know about, and a claim id with no row is a claim the table
# does not map.

named_tests=$(printf '%s\n' "$MAP_ENTRIES" | awk -F'\t' '$2 != "" && $3 != "" { print $2 "\t" $3 }' | sort -u)

while IFS= read -r test_file; do
    [ -n "$test_file" ] || continue
    [ -f "$test_file" ] || continue
    base=${test_file##*/}
    while IFS=$'\t' read -r name _; do
        [ -n "$name" ] || continue
        if ! printf '%s\n' "$named_tests" | grep -qF "$(printf '%s\t%s' "$base" "$name")"; then
            problem "$base::$name is a gate test that the trace table's map names by no row"
        fi
    done <<TESTS
$(test_names "$test_file")
TESTS
done <<FILES
$GATE_FILES
FILES

claimed_ids=$(printf '%s\n' "$TEST_CLAIMS" | awk -F'\t' '$3 != "" { print $3 }' | sort -u)
while IFS= read -r claim; do
    [ -n "$claim" ] || continue
    if ! printf '%s\n' "$MAP_ENTRIES" | awk -F'\t' -v c="$claim" '$1 == c { found = 1 } END { exit(found ? 0 : 1) }'; then
        problem "$claim is named by a gate test but the trace table has no row for it"
    fi
done <<CLAIMS
$claimed_ids
CLAIMS

# --- result ------------------------------------------------------------------

if [ "$PROBLEMS" -ne 0 ]; then
    printf 'check_trace.sh: traceability incomplete, %s problem(s)\n' "$PROBLEMS" >&2
    exit 1
fi

printf 'check_trace.sh: PASS\n'
exit 0
