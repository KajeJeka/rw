#!/usr/bin/env bash
# Public repository hygiene check. PROTECTED path.
#
# Guide: Publication rules, "Hygiene checks". The repository is public from its
# first commit and git keeps everything ever committed, so this runs before
# every commit and never after. It fails the build on
#
#   1. a tracked file with a game extension,
#   2. a tracked file larger than 262144 bytes (256 KB),
#   3. a private path pattern,
#   4. a term from $FO1_PRIVATE_TERMS,
#   5. an identifier from $FO1_REF/ce-identifiers.txt,
#   6. an integer literal array of more than 64 elements,
#   7. a tracked file that .gitignore should have excluded.
#
# Failure messages print the path relative to the repository root, the byte
# offset of the match, the match length and a hash. They never print more than
# 16 bytes of file content, and in practice print none at all: a leak is
# reported by hash so the owner can find it without the tool re-publishing it.
#
# Environment
#   FO1_PRIVATE_TERMS  newline delimited private term list. May be unset.
#   FO1_REF            directory holding ce-identifiers.txt. May be unset.
#
# Both may be unset: the corresponding rule is then reported as SKIPPED and the
# rest of the check still runs. A skipped rule never prints "pass".
#
# Exit status: 0 clean, 1 at least one violation.

set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(cd -- "$SCRIPT_DIR/.." && pwd)
cd "$REPO_ROOT"

# --- configuration -----------------------------------------------------------

MAX_FILE_BYTES=262144
MAX_ARRAY_ELEMENTS=64

GAME_EXTENSIONS="dat exe frm fr0 fr1 fr2 fr3 fr4 fr5 pal pro map int acm mve aaf gam msg sav"

# Private path patterns. Built from fragments so that this script's own bytes do
# not contain the literals it bans. A hygiene rule that flags its own rule text
# is a rule people disable.
#
#   a windows drive letter path:   [A-Za-z]:[\\/]
#   a macOS user home:             /(Users)/
#   a linux user home:             /(home)/
#   a windows downloads folder:    Down(loads)
#
# The drive letter pattern requires a boundary before the letter, at the start of
# a line or after a character that cannot continue a URL scheme. Without that,
# the "https://" in deny.toml's registry URL is a match, which is a false
# positive and a good way to teach people to ignore this script.
PAT_DRIVE='(^|[^A-Za-z0-9+._-])[A-Za-z]:[\\/]'
PAT_MAC_HOME='/(Users)/'
PAT_LINUX_HOME='/(home)/'
PAT_DOWNLOADS='Down(loads)'
RID_DRIVE='publication-rules/drive-letter-path'
RID_MAC_HOME='publication-rules/mac-home'
RID_LINUX_HOME='publication-rules/linux-home'
RID_DOWNLOADS='publication-rules/downloads-folder'

# Files that are allowed to quote the rules and are not private material:
#   - the implementation guide, which is the specification and names every
#     extension and every private path pattern in the rules above;
#   - the plans under docs/superpowers, which quote the guide.
# They are exempt from the *text* rules (3, 4, 5, 6). The extension, size and
# gitignore rules (1, 2, 7) still apply to them, because those are about what the
# repository *ships*, not about what it says.
is_exempt_from_text_rules() {
    case "$1" in
        "RUSTY WASTELAND Engine Implementation Guide, Revision 2.md") return 0 ;;
        docs/superpowers/*) return 0 ;;
        *) return 1 ;;
    esac
}

# --- reporting ---------------------------------------------------------------

VIOLATIONS=0
CHECKED=0

report_violation() {
    # rule, path, detail
    VIOLATIONS=$((VIOLATIONS + 1))
    printf 'FAIL [%s] %s\n' "$1" "$2" >&2
    printf '     %s\n' "$3" >&2
}

report_match() {
    # rule, path, byte offset, matched text, file sha256, rule id
    local rule="$1" path="$2" offset="$3" match="$4" file_sha="$5" rid="$6"
    local length match_sha
    length=${#match}
    match_sha=$(printf '%s' "$match" | sha256sum | cut -d' ' -f1)
    VIOLATIONS=$((VIOLATIONS + 1))
    printf 'FAIL [%s] %s\n' "$rule" "$path" >&2
    printf '     %s id=%s byte-offset=%s match-length=%s match-sha256=%s\n' \
        "$rid" "$offset" "$length" "$match_sha" >&2
    printf '     file-sha256=%s file-bytes=%s\n' "$file_sha" "$(stat -c%s "$path")" >&2
}

note() {
    printf 'note: %s\n' "$1"
}

skip() {
    printf 'SKIP [%s] %s\n' "$1" "$2"
}

# --- rule 3, private path patterns ------------------------------------------

scan_private_paths() {
    local path="$1" file_sha="$2"
    scan_one_pattern "$path" "$file_sha" "$RID_DRIVE" "$PAT_DRIVE"
    scan_one_pattern "$path" "$file_sha" "$RID_MAC_HOME" "$PAT_MAC_HOME"
    scan_one_pattern "$path" "$file_sha" "$RID_LINUX_HOME" "$PAT_LINUX_HOME"
    scan_one_pattern "$path" "$file_sha" "$RID_DOWNLOADS" "$PAT_DOWNLOADS"
}

# One pattern, one rule id. For the drive letter rule the leading boundary byte
# is part of the regex match, so it is stripped here and the offset advanced by
# one, which keeps the reported offset pointing at the drive letter itself.
scan_one_pattern() {
    local path="$1" file_sha="$2" rid="$3" pattern="$4"
    local line offset match rest
    while IFS= read -r line; do
        offset=${line%%:*}
        match=${line#*:}
        if [ "$rid" = "$RID_DRIVE" ]; then
            rest=${match#?}
            case "$rest" in
                [A-Za-z]:*)
                    match=$rest
                    offset=$((offset + 1))
                    ;;
            esac
        fi
        report_match "private-path" "$path" "$offset" "$match" "$file_sha" "$rid"
    done < <(grep -aboE "$pattern" -- "$path" || true)
}

# --- rule 6, integer literal arrays -----------------------------------------

# Counts elements inside each top level [ ... ] group and reports any group with
# more than MAX_ARRAY_ELEMENTS of them. Depth is tracked across the whole file so
# a multi line table counts as one array.
#
# A `0x` prefixed hex literal counts as ONE element, not as its digit runs.
# Without that rule the SHA-256 round constant table in the harness reports 219
# elements instead of 64, because `0x428a_2f98` is four separate runs of
# characters that happen to be digits or underscores. The rule exists to catch a
# pasted table dump, and a hand written 64 entry constant table is not one.
#
# One "path line= elements=" report per offending group, on stdout, so the caller
# can count them and set the exit status. An awk print on its own would be a
# report the script then goes on to call a pass.
scan_integer_arrays() {
    awk -v limit="$MAX_ARRAY_ELEMENTS" -v path="$1" '
        function ishex(c) { return (c >= "0" && c <= "9") || (c >= "a" && c <= "f") || (c >= "A" && c <= "F") }
        function isdigit(c) { return c >= "0" && c <= "9" }
        FNR == 1 { depth = 0; count = 0; start = 0; in_num = 0 }
        {
            line = $0
            n = length(line)
            i = 1
            while (i <= n) {
                c = substr(line, i, 1)
                if (c == "[") {
                    depth++
                    if (depth == 1) { count = 0; start = FNR; in_num = 0 }
                    i++
                } else if (c == "]") {
                    if (depth == 1 && count > limit) {
                        printf "%s line=%s elements=%s limit=%s\n", path, start, count, limit
                    }
                    if (depth > 0) depth--
                    in_num = 0
                    i++
                } else if (depth >= 1) {
                    nxt = (i < n) ? substr(line, i + 1, 1) : ""
                    if (isdigit(c) && c == "0" && (nxt == "x" || nxt == "X")) {
                        # One hex literal: consume 0x and every hex digit and
                        # underscore after it.
                        count++
                        i += 2
                        while (i <= n) {
                            d = substr(line, i, 1)
                            if (ishex(d) || d == "_") { i++ } else { break }
                        }
                        in_num = 0
                    } else if (isdigit(c)) {
                        if (!in_num) { count++; in_num = 1 }
                        i++
                    } else if (c == "_") {
                        i++
                    } else {
                        in_num = 0
                        i++
                    }
                } else {
                    i++
                }
            }
        }
    ' "$1"
}

# --- rule 4, private term list -----------------------------------------------

scan_terms() {
    local path="$1" file_sha="$2" terms="$3" label="$4"
    local line offset match
    [ -s "$terms" ] || return 0
    while IFS= read -r line; do
        offset=${line%%:*}
        match=${line#*:}
        report_match "private-term" "$path" "$offset" "$match" "$file_sha" "$label"
    done < <(grep -aboF -f "$terms" -- "$path" || true)
}

# --- main --------------------------------------------------------------------

if ! git rev-parse --git-dir >/dev/null 2>&1; then
    printf 'check_public.sh: not a git repository\n' >&2
    exit 1
fi

# Blank lines in a term file would make grep match everything, so strip them.
TERMS_FILE=""
CE_FILE=""
CE_TERMS=""

cleanup() {
    # `rm -f ""` fails, and a failing command in an EXIT trap can change the
    # script's exit status, so every removal is guarded and the trap ends clean.
    if [ -n "$TERMS_FILE" ]; then rm -f "$TERMS_FILE"; fi
    if [ -n "$CE_TERMS" ]; then rm -f "$CE_TERMS"; fi
    return 0
}
trap cleanup EXIT

if [ -n "${FO1_PRIVATE_TERMS:-}" ]; then
    if [ ! -f "$FO1_PRIVATE_TERMS" ]; then
        printf 'check_public.sh: FO1_PRIVATE_TERMS=%s does not exist\n' "$FO1_PRIVATE_TERMS" >&2
        exit 1
    fi
    TERMS_FILE=$(mktemp)
    grep -v '^[[:space:]]*$' "$FO1_PRIVATE_TERMS" >"$TERMS_FILE" || true
fi

if [ -n "${FO1_REF:-}" ]; then
    CE_FILE="$FO1_REF/ce-identifiers.txt"
    if [ ! -f "$CE_FILE" ]; then
        printf 'check_public.sh: FO1_REF=%s has no ce-identifiers.txt\n' "$FO1_REF" >&2
        exit 1
    fi
    CE_TERMS=$(mktemp)
    grep -v '^[[:space:]]*$' "$CE_FILE" >"$CE_TERMS" || true
fi

[ -n "$TERMS_FILE" ] || skip private-term "FO1_PRIVATE_TERMS unset, the private term rule did not run"
[ -n "$CE_FILE" ] || skip ce-identifier "FO1_REF unset, the Community Edition identifier rule did not run"

if [ -z "$(git ls-files)" ]; then
    printf 'check_public.sh: no tracked files\n' >&2
    exit 1
fi

printf 'check_public.sh: root %s\n' "$REPO_ROOT"
printf 'check_public.sh: sha256sum %s\n' "$(sha256sum --version 2>&1 | head -1)"
printf 'check_public.sh: bash %s\n' "$BASH_VERSION"

while IFS= read -r -d '' path; do
    CHECKED=$((CHECKED + 1))

    # rule 1, game extension
    base=${path##*/}
    lower=$(printf '%s' "$base" | tr '[:upper:]' '[:lower:]')
    for ext in $GAME_EXTENSIONS; do
        case "$lower" in
            *."$ext")
                report_violation "game-extension" "$path" \
                    "a tracked file carries the game extension .$ext. Synthetic fixtures live in code or as .hex text files (guide: Publication rules)."
                ;;
        esac
    done

    # rule 2, file size
    bytes=$(stat -c%s "$path")
    if [ "$bytes" -gt "$MAX_FILE_BYTES" ]; then
        report_violation "file-too-large" "$path" \
            "$bytes bytes exceeds the $MAX_FILE_BYTES byte ceiling"
    fi

    # rule 7, gitignore should have excluded it
    # --no-index is required: without it git skips every tracked path, which is
    # exactly the case this rule is about.
    if git check-ignore -q --no-index -- "$path"; then
        report_violation "gitignored-but-tracked" "$path" \
            "a .gitignore rule matches this tracked file, so it should never have been added"
    fi

    if is_exempt_from_text_rules "$path"; then
        continue
    fi

    file_sha=$(sha256sum "$path" | cut -d' ' -f1)

    # rule 3, private path patterns
    scan_private_paths "$path" "$file_sha"

    # rule 6, integer literal arrays
    array_report=$(scan_integer_arrays "$path")
    if [ -n "$array_report" ]; then
        while IFS= read -r line; do
            [ -n "$line" ] || continue
            VIOLATIONS=$((VIOLATIONS + 1))
            printf 'FAIL [integer-array] %s\n' "$path" >&2
            printf '     publication-rules/integer-array %s\n' "$line" >&2
        done <<EOF
$array_report
EOF
    fi

    # rule 4, private term list
    if [ -n "$TERMS_FILE" ]; then
        scan_terms "$path" "$file_sha" "$TERMS_FILE" "publication-rules/FO1_PRIVATE_TERMS"
    fi

    # rule 5, Community Edition identifiers
    if [ -n "$CE_FILE" ]; then
        scan_terms "$path" "$file_sha" "$CE_TERMS" "publication-rules/ce-identifiers.txt"
    fi
done < <(git ls-files -z)

printf 'check_public.sh: %s tracked file(s) checked, %s violation(s)\n' "$CHECKED" "$VIOLATIONS"

if [ "$VIOLATIONS" -ne 0 ]; then
    printf 'check_public.sh: FAILED\n' >&2
    exit 1
fi

printf 'check_public.sh: PASS\n'
exit 0