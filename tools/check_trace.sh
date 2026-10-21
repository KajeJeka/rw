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
#   - the trace table's map row claims an oracle class the test does not
#     declare                                    -> fail
#   - a whole line evidence tag no claim claims      -> fail
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
# The exemptions that were used, one key per line. An exemption that exempts
# nothing is a stale record, and rule 6 says so.
used_exemptions=''

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
#   - a heading that DECLARES the claim id, with the evidence tag on a line of
#     its own inside that section (`docs/spec/dat.md`, `docs/spec/lzss.md`,
#     `docs/spec/exe-tables.md`).
#
# The heading declares the claim wherever the id appears in it: `### LZSS-020 -`
# and `### The LZSS-020 claim -` are the same declaration, and a rename that
# moves the id off the front of the heading no longer loses the claim.
#
# A claim id that merely appears in a sentence is a cross-reference, not a
# declaration, and must never be read as one. That is why the tag is looked for
# on a line that IS a tag and not on any line that mentions one.
#
# A whole line tag that no section claims is UNATTRIBUTED: it states an evidence
# tag with no claim behind it, so the rule that reads the tag reads nothing and
# the claim it was meant for is invisible. The rule reports it, with the file and
# the line, rather than passing over it.
#
# Output: one "CLAIM<TAB>TAG" line per declared claim, and one
# "STRAY<TAB>file:line<TAB>TAG" line per unattributed tag.
declared_claims() {
    awk -v confirmed="$CONFIRMED_MARK" -v file="$1" '
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
        function cell(s, n,   t, parts) {
            t = trim(s)
            if (substr(t, 1, 1) != "|") return ""
            n = split(t, parts, /\|/)
            if (n < 3) return ""
            return trim(parts[2])
        }
        function last_cell(s, n,   t, parts) {
            t = trim(s)
            if (substr(t, 1, 1) != "|") return ""
            n = split(t, parts, /\|/)
            if (n < 3) return ""
            return trim(parts[n - 1])
        }
        # The claim id a text declares, wherever in that text it appears.
        function claim_in(text) {
            if (match(text, /[A-Z]+-[0-9][0-9][0-9]/)) {
                return substr(text, RSTART, RLENGTH)
            }
            return ""
        }
        { lines[NR] = $0 }
        END {
            fence = 0
            claim = ""
            tagged = 0
            for (i = 1; i <= NR; i++) {
                line = lines[i]
                t = trim(line)
                # A fenced code block is not prose and declares nothing.
                if (t ~ /^```/) { fence = !fence; continue }
                if (fence == 1) continue
                if (line ~ /^[ \t]*\|/) {
                    cid = cell(line)
                    if (cid ~ /^[A-Z]+-[0-9][0-9][0-9]$/) {
                        tag = ""
                        lc = last_cell(line)
                        for (k = 1; k <= 4; k++) {
                            if (index(lc, tags[k]) > 0) { tag = tags[k]; break }
                        }
                        if (tag != "") print "CLAIM\t" cid "\t" tag
                    }
                    continue
                }
                if (line ~ /^#+[ \t]/) {
                    rest = line
                    sub(/^#+[ \t]+/, "", rest)
                    claim = claim_in(rest)
                    tagged = 0
                    continue
                }
                tag = tag_of_line(line)
                if (tag == "") continue
                if (claim != "" && tagged == 0) {
                    print "CLAIM\t" claim "\t" tag
                    tagged = 1
                } else {
                    print "STRAY\t" file ":" i "\t" tag
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

RAW_DECLARED=$(for f in $SPEC_DECL_FILES; do declared_claims "$f"; done || true)
DECLARED=$(printf '%s\n' "$RAW_DECLARED" | grep '^CLAIM' | grep -vE "$NOT_A_CLAIM" | sort -u || true)
DECLARED_COUNT=$(printf '%s\n' "$DECLARED" | grep -c . || true)
CONFIRMED=$(printf '%s\n' "$DECLARED" | awk -F'\t' -v m="$CONFIRMED_MARK" '$3 == m { print $2 }' || true)
CONFIRMED_COUNT=$(printf '%s\n' "$CONFIRMED" | grep -c . || true)
STRAYS=$(printf '%s\n' "$RAW_DECLARED" | grep '^STRAY' || true)
STRAY_COUNT=$(printf '%s\n' "$STRAYS" | grep -c . || true)

printf 'check_trace.sh: %s claim(s) declared in the spec\n' "$DECLARED_COUNT"
printf 'check_trace.sh: %s claim(s) marked %s\n' "$CONFIRMED_COUNT" "$CONFIRMED_MARK"
printf 'check_trace.sh: %s unattributed evidence tag(s) in the spec\n' "$STRAY_COUNT"
# --- the exemptions -------------------------------------------------------------

# An exemption is a named thing the gate would otherwise reject, with a reason,
# and it lives in the trace table, which is a PROTECTED file: a change to it is
# noticed by the gate, so the owner reads every entry.
#
# The entry is split on the FIRST "::" separator and the reason is everything
# after it, because a reason is prose and prose may hold a "::" of its own. An
# entry that is not in that shape, or that has an empty key or an empty reason,
# is malformed, and a malformed entry fails the check the same way a key in
# expected_counts.toml that the gate does not understand fails: an approval
# nobody can read is not an approval.
#
# Two kinds of key:
#
#   - a claim id, exempt from the witness rule: the claim is CONFIRMED and no
#     test in oracle class 1, 2 or 3 can witness it yet;
#   - a spec location, "file:line", exempt from the unattributed tag rule: a
#     whole line evidence tag that no section of that file claims.
#
# "None yet." is the empty list, and it is removed when the first entry is added.
read_exemptions() {
    [ -f "$TRACE" ] || return 0
    awk '
        function trim(s) {
            gsub(/^[[:space:]]+|[[:space:]]+$/, "", s)
            return s
        }
        /^##[[:space:]]+Exemptions[[:space:]]*$/ { insec = 1; next }
        /^#+[[:space:]]/ { insec = 0 }
        insec != 1 { next }
        {
            line = $0
            sub(/^[[:space:]]+/, "", line)
            sub(/[[:space:]]+$/, "", line)
            if (line == "") next
            if (line ~ /^#/) next
            if (line == "None yet.") next
            at = index(line, "::")
            if (at == 0) {
                print "MALFORMED\t" line
                next
            }
            key = trim(substr(line, 1, at - 1))
            reason = trim(substr(line, at + 2))
            if (key == "" || reason == "") {
                print "MALFORMED\t" line
                next
            }
            print "OK\t" key "\t" reason
        }
    ' "$TRACE"
}

EXEMPTION_ROWS=$(read_exemptions || true)
EXEMPTION_CLAIMS=''
EXEMPTION_PLACES=''
while IFS=$'\t' read -r state key reason; do
    [ -n "$key" ] || continue
    if [ "$state" != "OK" ]; then
        problem "the exemption \"$key\" in $TRACE is malformed; an entry is \"<key> :: <reason>\" and the reason may not be empty"
        continue
    fi
    case "$key" in
        *:*)
            EXEMPTION_PLACES="$EXEMPTION_PLACES$key
"
            ;;
        *)
            EXEMPTION_CLAIMS="$EXEMPTION_CLAIMS$key
"
            ;;
    esac
done <<EXEMPTIONS
$EXEMPTION_ROWS
EXEMPTIONS

is_exempt_claim() {
    case "
$EXEMPTION_CLAIMS
" in
        *"
$1
"*) return 0 ;;
        *) return 1 ;;
    esac
}

is_exempt_place() {
    case "
$EXEMPTION_PLACES
" in
        *"
$1
"*) return 0 ;;
        *) return 1 ;;
    esac
}

# --- the trace table's map -----------------------------------------------------

# One line per map entry: CLAIM<TAB>FILE<TAB>TEST<TAB>CLASS<TAB>DAGGER. The class
# cell is read in the same order as the test cell, so a class is always paired
# with the test to its left. The dagger marks a class taken from the module
# header of the file rather than declared by the test itself.
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
                    ncl = split(cl, cbits, /,/)
                    for (k = 1; k <= ncl; k++) {
                        c = trim(cbits[k])
                        if (c == "") continue
                        dagger = 0
                        if (index(c, "†") > 0) {
                            dagger = 1
                            gsub(/†/, "", c)
                            c = trim(c)
                        }
                        if (c == "") continue
                        print cid "\t" file "\t" name "\t" c "\t" dagger
                    }
                }
            }
        }
    ' "$TRACE" 2>/dev/null || true
}

MAP_ENTRIES=$(map_entries)

# --- the test inventory --------------------------------------------------------

# For one gate file: "TEST<TAB>CLAIM" for every claim id the test's own doc
# attribute carries. When the doc comment has a `claim:` line, that line is the
# attribute; when it has none, the claim ids the comment states in prose are. A
# doc comment with a `claim:` line that omits the claim does not carry it.
#
# An attribute may run over several doc lines: the hold-out tests write their
# claim list as a wrapped line and its continuation. The attribute ends at the
# next attribute line or at a blank doc line, and every line of it is read.
test_claims() {
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
                k = i - 1
                while (k >= 1 && lines[k] ~ /^[ \t]*#\[/) k--
                dn = 0
                while (k >= 1 && lines[k] ~ /^[ \t]*\/\/\//) {
                    dn++
                    doc[dn] = lines[k]
                    k--
                }
                for (x = 1; x <= dn; x++) {
                    body = doc[x]
                    sub(/^[ \t]*\/\/\/[ \t]*/, "", body)
                    d[dn - x + 1] = body
                }
                attribute = ""
                has_claim_line = 0
                in_claim = 0
                for (x = 1; x <= dn; x++) {
                    if (d[x] ~ /^[A-Za-z][A-Za-z0-9_]*[ \t]*:/) {
                        in_claim = 0
                        if (d[x] ~ /^claim[ \t]*:/) {
                            has_claim_line = 1
                            in_claim = 1
                            attribute = attribute " " d[x]
                        }
                        continue
                    }
                    if (d[x] == "") { in_claim = 0; continue }
                    if (in_claim == 1) attribute = attribute " " d[x]
                }
                if (has_claim_line == 0) {
                    attribute = ""
                    for (x = 1; x <= dn; x++) attribute = attribute " " d[x]
                }
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

# The oracle classes ONE test declares in its own doc attribute: the classes on
# the `oracle:` line of its doc comment, and on its continuation lines. The
# map's class cell has to agree with this, because the cell is what the witness
# rule reads, and a cell the gate takes on trust is a cell a map can lie with.
#
# The line is the attribute, not the prose: a test that mentions "a class 1, 2
# or 3 witness" in passing declares nothing by it.
test_classes() {
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
                k = i - 1
                while (k >= 1 && lines[k] ~ /^[ \t]*#\[/) k--
                dn = 0
                while (k >= 1 && lines[k] ~ /^[ \t]*\/\/\//) {
                    dn++
                    doc[dn] = lines[k]
                    k--
                }
                for (x = 1; x <= dn; x++) {
                    body = doc[x]
                    sub(/^[ \t]*\/\/\/[ \t]*/, "", body)
                    d[dn - x + 1] = body
                }
                attribute = ""
                in_oracle = 0
                for (x = 1; x <= dn; x++) {
                    if (d[x] ~ /^[A-Za-z][A-Za-z0-9_]*[ \t]*:/) {
                        in_oracle = 0
                        if (d[x] ~ /^oracle[ \t]*:/) {
                            in_oracle = 1
                            attribute = attribute " " d[x]
                        }
                        continue
                    }
                    if (d[x] == "") { in_oracle = 0; continue }
                    if (in_oracle == 1) attribute = attribute " " d[x]
                }
                cls = ""
                rest = attribute
                while (match(rest, /[Cc]lass[ \t]+[0-9]/)) {
                    cls = cls substr(rest, RSTART + RLENGTH - 1, 1) ","
                    rest = substr(rest, RSTART + RLENGTH)
                }
                if (cls == "") cls = "NONE"
                print name "\t" cls
            }
        }
    ' "$1" 2>/dev/null || true
}

# The oracle classes the module header of a gate file declares. A class the map
# marks with a dagger comes from here and not from the test's own doc comment.
file_classes() {
    awk '
        { lines[NR] = $0 }
        END {
            n = 0
            for (i = 1; i <= NR; i++) {
                if (lines[i] !~ /^[ \t]*\/\/!/) break
                n = i
            }
            cls = ""
            for (i = 1; i <= n; i++) {
                rest = lines[i]
                while (match(rest, /[Cc]lass[ \t]+[0-9]/)) {
                    cls = cls substr(rest, RSTART + RLENGTH - 1, 1) ","
                    rest = substr(rest, RSTART + RLENGTH)
                }
            }
            if (cls == "") cls = "NONE"
            print cls
        }
    ' "$1" 2>/dev/null || true
}

# The inventory is read once and kept, because the map names every test and a
# lookup per entry would start a process per lookup.
declare -A TEST_CLAIM_SET=()
declare -A TEST_CLASS_SET=()
declare -A FILE_CLASS_SET=()

TEST_CLAIMS=''
for f in $GATE_FILES; do
    [ -f "$f" ] || continue
    base=${f##*/}
    TEST_CLAIMS="$TEST_CLAIMS
$(test_claims "$f" | sed "s|^|$base\t|")"
    while IFS=$'\t' read -r name cls; do
        [ -n "$name" ] || continue
        TEST_CLASS_SET["$base|$name"]="$cls"
    done <<CLASSES
$(test_classes "$f")
CLASSES
    FILE_CLASS_SET["$base"]=$(file_classes "$f")
done

while IFS=$'\t' read -r base name claim; do
    [ -n "$claim" ] || continue
    TEST_CLAIM_SET["$base|$name|$claim"]=1
done <<CLAIMS_TABLE
$TEST_CLAIMS
CLAIMS_TABLE

test_cites_claim() {
    [ -n "${TEST_CLAIM_SET["$1|$2|$3"]:-}" ]
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
    [ -n "${TEST_CLASS_SET["$1|$2"]:-}" ]
}

# The classes a test declares in its own doc attribute, and the classes the
# module header of its file declares.
own_classes() {
    printf '%s' "${TEST_CLASS_SET["$1|$2"]:-NONE}"
}

head_classes() {
    printf '%s' "${FILE_CLASS_SET["$1"]:-NONE}"
}

class_is_strong() {
    # The classes the test declares, comma separated. A shell pattern, not a
    # process: this runs for every cell of every row.
    case ",$1," in
        *",1,"* | *",2,"* | *",3,"*) return 0 ;;
    esac
    return 1
}

class_is_declared() {
    # $1 = the class list, $2 = the class
    case ",$1," in
        *",$2,"*) return 0 ;;
    esac
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
#   - the class the row records is a class the test itself declares, or a
#     class the module header of the file declares when the cell marks it with
#     a dagger;
#   - and at least one of the row's tests is in oracle class 1, 2 or 3 by the
#     class the test itself declares.
#
# The claim id is substituted into every lookup. The old check asked whether a
# file mentioned SOME claim id, so a claim id that existed nowhere scored as
# covered and the rule could never fire.
#
# The map is walked once and the verdict is kept per claim, so the rule does not
# start a process per claim to find the rows it already has.
declare -A CONFIRMED_SET=()
while IFS= read -r claim; do
    [ -n "$claim" ] || continue
    CONFIRMED_SET["$claim"]=1
done <<CONFIRMED_CLAIMS
$CONFIRMED
CONFIRMED_CLAIMS

declare -A ROW_SET=()
declare -A STRONG_SET=()

while IFS=$'\t' read -r row_claim file name class dagger; do
    [ -n "$file" ] || continue
    [ -n "${CONFIRMED_SET["$row_claim"]:-}" ] || continue
    ROW_SET["$row_claim"]=1
    claim=$row_claim
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
    own=$(own_classes "$file" "$name")
    head=$(head_classes "$file")
    if [ "$dagger" = "1" ]; then
        source="$head"
        where="the module header of $file"
    else
        source="$own"
        where="its own doc attribute"
    fi
    if [ "$source" = "NONE" ]; then
        problem "$claim is marked $CONFIRMED_MARK but its trace table row names $file::$name with oracle class $class, and that test declares no class of its own"
        continue
    fi
    if ! class_is_declared "$source" "$class"; then
        problem "$claim is marked $CONFIRMED_MARK but its trace table row names $file::$name with oracle class $class, which the test does not declare in $where"
        continue
    fi
    if class_is_strong "$own"; then
        STRONG_SET["$claim"]=1
    fi
    if [ "$dagger" = "1" ] && class_is_strong "$head"; then
        STRONG_SET["$claim"]=1
    fi
done <<ENTRIES
$MAP_ENTRIES
ENTRIES

while IFS= read -r claim; do
    [ -n "$claim" ] || continue

    # An exemption is a claim the owner has accepted without a witness yet, and
    # it says why in the trace table. The rule accepts the claim, and the entry
    # has to be used: an exemption for a claim that is not CONFIRMED is stale and
    # fails.
    if is_exempt_claim "$claim"; then
        used_exemptions="$used_exemptions$claim
"
        continue
    fi

    if [ -z "${ROW_SET["$claim"]:-}" ]; then
        problem "$claim is marked $CONFIRMED_MARK but the trace table has no row for it"
        continue
    fi
    if [ -z "${STRONG_SET["$claim"]:-}" ]; then
        problem "$claim is marked $CONFIRMED_MARK but no test it names is in oracle class $STRONG_CLASSES"
    fi
done <<CLAIMS
$CONFIRMED
CLAIMS

# --- rule 2b: the map's class cell agrees with the test ------------------------

# Rule 2 reads the map's cell for the claims that are CONFIRMED. This rule reads
# every row of the map, because a map that misstates a class is wrong whether or
# not the claim it names is confirmed, and a cell the gate trusts is a cell a
# map can lie with.
while IFS=$'\t' read -r row_claim file name class dagger; do
    [ -n "$file" ] || continue
    if [ ! -f "$GATE_DIR/$file" ]; then
        continue
    fi
    if ! test_exists "$file" "$name"; then
        continue
    fi
    own=$(own_classes "$file" "$name")
    head=$(head_classes "$file")
    if [ "$dagger" = "1" ]; then
        source="$head"
        where="the module header of $file"
    else
        source="$own"
        where="its own doc attribute"
    fi
    if [ "$source" = "NONE" ]; then
        problem "the trace table's row for $row_claim names $file::$name with oracle class $class, and that test declares no class in $where"
        continue
    fi
    if ! class_is_declared "$source" "$class"; then
        problem "the trace table's row for $row_claim names $file::$name with oracle class $class, which the test does not declare in $where"
    fi
done <<ENTRIES
$MAP_ENTRIES
ENTRIES

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
# every test in the gate files has to be named by some row, and every claim id a
# test names has to have a row. A test nobody names is a test the trace table
# does not know about, and a claim id with no row is a claim the table does not
# map.

named_tests=$(printf '%s\n' "$MAP_ENTRIES" | awk -F'\t' '$2 != "" && $3 != "" { print $2 "\t" $3 }' | sort -u)
declare -A NAMED_TEST_SET=()
while IFS=$'\t' read -r named_file named_name; do
    [ -n "$named_name" ] || continue
    NAMED_TEST_SET["$named_file|$named_name"]=1
done <<NAMED
$named_tests
NAMED

while IFS= read -r test_file; do
    [ -n "$test_file" ] || continue
    [ -f "$test_file" ] || continue
    base=${test_file##*/}
    while IFS=$'\t' read -r name _; do
        [ -n "$name" ] || continue
        if [ -z "${NAMED_TEST_SET["$base|$name"]:-}" ]; then
            problem "$base::$name is a gate test that the trace table's map names by no row"
        fi
    done <<TESTS
$(test_names "$test_file")
TESTS
done <<FILES
$GATE_FILES
FILES

claimed_ids=$(printf '%s\n' "$TEST_CLAIMS" | awk -F'\t' '$3 != "" { print $3 }' | sort -u)
CLAIMED_COUNT=$(printf '%s\n' "$claimed_ids" | grep -c . || true)
printf 'check_trace.sh: %s claim(s) cited by a gate test\n' "$CLAIMED_COUNT"
declare -A MAPPED_CLAIM_SET=()
while IFS= read -r row_claim; do
    [ -n "$row_claim" ] || continue
    MAPPED_CLAIM_SET["$row_claim"]=1
done <<ROWS
$(printf '%s\n' "$MAP_ENTRIES" | awk -F'\t' '$1 != "" { print $1 }' | sort -u)
ROWS

while IFS= read -r claim; do
    [ -n "$claim" ] || continue
    if [ -z "${MAPPED_CLAIM_SET["$claim"]:-}" ]; then
        problem "$claim is named by a gate test but the trace table has no row for it"
    fi
done <<CLAIMS
$claimed_ids
CLAIMS

# --- rule 5: no evidence tag without a claim -----------------------------------

# A whole line evidence tag that no section of a spec claims states a tag with
# no claim behind it. The rule that reads the tag reads nothing, so the claim it
# was written for is invisible to the gate. The rule reports it, with the file
# and the line, rather than passing over it in silence.
#
# An exemption is the owner's record of a tag that is there on purpose.
while IFS=$'\t' read -r kind place tag; do
    [ "$kind" = "STRAY" ] || continue
    [ -n "$place" ] || continue
    if is_exempt_place "$place"; then
        used_exemptions="$used_exemptions$place
"
        continue
    fi
    problem "$place carries a whole line evidence tag ($tag) that no claim claims; attribute it to a claim or record it as an exemption in $TRACE"
done <<STRAYS
$STRAYS
STRAYS

# --- rule 6: every exemption is used --------------------------------------------

# An exemption that exempts nothing is a record of a thing that is not there.
# It is a stale entry, and a stale entry is a lie the owner reads as a fact.
while IFS= read -r key; do
    [ -n "$key" ] || continue
    if ! printf '%s' "$used_exemptions" | grep -qxF "$key"; then
        problem "the exemption for $key in $TRACE exempts nothing: no confirmed claim and no unattributed tag carries that key"
    fi
done <<KEYS
$(printf '%s' "$EXEMPTION_CLAIMS$EXEMPTION_PLACES" | grep -v '^$' || true)
KEYS

# --- result ------------------------------------------------------------------

if [ "$PROBLEMS" -ne 0 ]; then
    printf 'check_trace.sh: traceability incomplete, %s problem(s)\n' "$PROBLEMS" >&2
    exit 1
fi

printf 'check_trace.sh: PASS\n'
exit 0
