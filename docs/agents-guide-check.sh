#!/bin/sh
# agents-guide-check.sh: run every `wolf` fence in docs/agents-guide.md under
# the compiler and the reference interpreter, and fail when any fence answers
# differently from what the guide says it answers.
#
#   docs/agents-guide-check.sh [guide.md]
#
# Run it with the release archives the guide names (a wolf archive and its
# paired lupin archive), never with a development build: the guide states
# what a released toolchain does.
#
# Environment:
#   WOLF       the wolf binary (default: `wolf` on PATH)
#   LUPIN      the lupin binary (default: `lupin` on PATH)
#   WOLF_STD   the std directory itself (the archive's `std/`), passed to
#              both machines as --std-root; the same meaning the compiler
#              gives the variable. Unset, wolf uses the std its archive
#              bundles, and lupin, which bundles none, uses LUPIN_STD if
#              that is set and otherwise the `std/` beside the wolf binary.
#   LUPIN_STD  the std directory for lupin alone (read by lupin itself).
#   GUIDE_CHECK_KEEP=1   keep the scratch directory and print its path.
#
# The fence language. A checked fence opens with ```wolf and may carry
# space-separated attributes on the same line:
#
#   expect=V      what `wolf build` and then running the binary answers
#                 (default exit:0)
#   lupin=V       what `lupin run --json` answers (default: the same as expect)
#   args=a,b,c    argv for the compiled program. `lupin run` passes no argv,
#                 so a fence with args states lupin's verdict for the run
#                 without them, and its stdout is not compared under lupin.
#   stdin=TEXT    bytes on stdin, printf %b escapes (\n, \t, \0377); no
#                 spaces, write \040 for one
#   warn=W####    the build is expected to warn with this code (the fence is
#                 then built without --deny-warnings, and the code must appear)
#   nofmt         skip `wolf fmt --check` (for a fence that must not parse)
#
# V is one of exit:N, trap:KIND, fail:CODE (E#### or W####), unsupported.
# A build that answers "cannot compile this yet" is `unsupported`.
#
# A fence tagged ```output that follows a checked fence (blank lines between
# are allowed) is the program's exact stdout on every machine whose verdict
# is an exit. Every other fence (```text, ```sh, a bare ```) is prose and is
# not run. Every checked fence is a whole program in a directory of its own.
#
# The prelude list. The backticked names between the lines
# `<!-- prelude-since:begin -->` and `<!-- prelude-since:end -->` must each be
# a name `wolf prelude --json` lists; a name the prelude no longer has fails
# the check, so the list cannot outlive the toolchain it describes.
#
# Exit status: 0 when every fence and the prelude list answer as written, 1
# otherwise, 2 on a usage or setup error. The report is a markdown table on
# stdout.
set -u
LC_ALL=C
export LC_ALL

GUIDE="${1:-docs/agents-guide.md}"
WOLF="${WOLF:-wolf}"
LUPIN="${LUPIN:-lupin}"
STD="${WOLF_STD:-}"

[ -f "$GUIDE" ] || { echo "agents-guide-check: no such guide: $GUIDE" >&2; exit 2; }
command -v "$WOLF" > /dev/null 2>&1 || [ -x "$WOLF" ] || { echo "agents-guide-check: WOLF=$WOLF is not runnable" >&2; exit 2; }
command -v "$LUPIN" > /dev/null 2>&1 || [ -x "$LUPIN" ] || { echo "agents-guide-check: LUPIN=$LUPIN is not runnable" >&2; exit 2; }
command -v jq > /dev/null 2>&1 || { echo "agents-guide-check: jq is required (lupin's record and the prelude are JSON)" >&2; exit 2; }
if command -v sha256sum > /dev/null 2>&1; then SHA="sha256sum"; else SHA="shasum -a 256"; fi

case "$GUIDE" in /*) ;; *) GUIDE="$(pwd)/$GUIDE" ;; esac
case "$WOLF" in */*) case "$WOLF" in /*) ;; *) WOLF="$(pwd)/$WOLF" ;; esac ;; esac
case "$LUPIN" in */*) case "$LUPIN" in /*) ;; *) LUPIN="$(pwd)/$LUPIN" ;; esac ;; esac
STDARG=""
LUPIN_STDARG=""
STDNOTE="bundled"
if [ -n "$STD" ]; then
    [ -d "$STD" ] || { echo "agents-guide-check: WOLF_STD=$STD is not a directory (name the std directory itself)" >&2; exit 2; }
    case "$STD" in /*) ;; *) STD="$(pwd)/$STD" ;; esac
    STDARG="--std-root $STD"
    LUPIN_STDARG="$STDARG"
    STDNOTE="WOLF_STD"
elif [ -n "${LUPIN_STD:-}" ]; then
    STDNOTE="wolf bundled, lupin LUPIN_STD"
else
    # lupin's archive bundles no std: give it the one beside the wolf binary
    WOLF_PATH="$(command -v "$WOLF")"
    if [ -d "$(dirname "$WOLF_PATH")/std" ]; then
        LUPIN_STDARG="--std-root $(dirname "$WOLF_PATH")"
        STDNOTE="wolf bundled, lupin $(dirname "$WOLF_PATH")/std"
    else
        echo "agents-guide-check: lupin has no std: set WOLF_STD or LUPIN_STD to the archive's std/ directory, or run wolf from an archive that keeps std/ beside it" >&2
        exit 2
    fi
fi

TMP="$(mktemp -d "${TMPDIR:-/tmp}/agents-guide-check.XXXXXX")" || exit 2
if [ "${GUIDE_CHECK_KEEP:-0}" = "1" ]; then
    echo "agents-guide-check: scratch kept at $TMP" >&2
else
    trap 'rm -rf "$TMP"' EXIT INT TERM
fi

# Extract. Each checked fence becomes $TMP/sNNN/{main.lu,attrs,line} and,
# when an ```output fence follows, expected.out.
awk -v root="$TMP" '
BEGIN { n = 0; state = "prose"; pending = 0 }
{
    if (state == "prose") {
        if ($0 ~ /^```wolf([ \t]|$)/) {
            n++
            dir = sprintf("%s/s%03d", root, n)
            system("mkdir -p \"" dir "\"")
            attrs = $0
            sub(/^```wolf[ \t]*/, "", attrs)
            print attrs > (dir "/attrs"); close(dir "/attrs")
            print NR > (dir "/line"); close(dir "/line")
            src = dir "/main.lu"
            printf "" > src
            state = "code"
            pending = 0
            next
        }
        if (pending && $0 ~ /^```output[ \t]*$/) {
            out = dir "/expected.out"
            printf "" > out
            state = "output"
            pending = 0
            next
        }
        if (pending && $0 !~ /^[ \t]*$/) { pending = 0 }
        if ($0 ~ /^```/) { state = "other"; next }
        next
    }
    if (state == "code") {
        if ($0 ~ /^```[ \t]*$/) { close(src); state = "prose"; pending = 1; next }
        print >> src
        next
    }
    if (state == "output") {
        if ($0 ~ /^```[ \t]*$/) { close(out); state = "prose"; next }
        print >> out
        next
    }
    if (state == "other") {
        if ($0 ~ /^```[ \t]*$/) { state = "prose" }
        next
    }
}
END { print n > (root "/count") }
' "$GUIDE"

COUNT="$(cat "$TMP/count")"
[ "$COUNT" -gt 0 ] || { echo "agents-guide-check: no \`\`\`wolf fences in $GUIDE" >&2; exit 2; }

WOLFV="$("$WOLF" --version 2>/dev/null | head -1)"
LUPINV="$("$LUPIN" --version 2>/dev/null | head -1)"
echo "# agents-guide-check"
echo
echo "- guide: \`$(basename "$GUIDE")\`"
echo "- wolf: \`$WOLFV\`"
echo "- lupin: \`$LUPINV\`"
echo "- host: \`$(uname -sm)\`"
echo "- std: $STDNOTE"
echo
echo "| # | line | wolf expected | wolf got | lupin expected | lupin got | stdout | fmt | result |"
echo "|---|---|---|---|---|---|---|---|---|"

pass=0
fail=0
i=0
while [ "$i" -lt "$COUNT" ]; do
    i=$((i + 1))
    d="$(printf '%s/s%03d' "$TMP" "$i")"
    line="$(cat "$d/line")"
    expect="exit:0"
    lexpect=""
    args=""
    stdin_text=""
    warn=""
    dofmt=1
    for tok in $(cat "$d/attrs"); do
        case "$tok" in
            expect=*) expect="${tok#expect=}" ;;
            lupin=*) lexpect="${tok#lupin=}" ;;
            args=*) args="${tok#args=}" ;;
            stdin=*) stdin_text="${tok#stdin=}" ;;
            warn=*) warn="${tok#warn=}" ;;
            nofmt) dofmt=0 ;;
            *) echo "agents-guide-check: fence at line $line has an unknown attribute: $tok" >&2; exit 2 ;;
        esac
    done
    [ -n "$lexpect" ] || lexpect="$expect"
    printf '%b' "$stdin_text" > "$d/stdin"
    # argv: commas separate words
    set --
    if [ -n "$args" ]; then
        oldifs="$IFS"; IFS=","
        for a in $args; do set -- "$@" "$a"; done
        IFS="$oldifs"
    fi

    # --- wolf
    deny="--deny-warnings"
    [ -n "$warn" ] && deny=""
    ( cd "$d" && "$WOLF" build main.lu -o prog $STDARG $deny > build.log 2>&1 )
    brc=$?
    wgot=""
    if [ "$brc" -ne 0 ]; then
        if grep -q "cannot compile this yet" "$d/build.log"; then
            wgot="unsupported"
        else
            # an error code beats a warning promoted by --deny-warnings: the
            # compiler may print the warning first (W1101 before E1101)
            code="$(grep -o -E 'error\[E[0-9]{4}\]' "$d/build.log" | head -1 | sed 's/error\[\(.*\)\]/\1/')"
            [ -n "$code" ] || code="$(grep -o -E 'error\[W[0-9]{4}\]' "$d/build.log" | head -1 | sed 's/error\[\(.*\)\]/\1/')"
            wgot="fail:${code:-?}"
        fi
    else
        if [ -n "$warn" ] && ! grep -q "$warn" "$d/build.log"; then
            wgot="no-$warn"
        else
            ( cd "$d" && ./prog "$@" < stdin > wolf.out 2> wolf.err )
            rc=$?
            first="$(head -1 "$d/wolf.err" 2>/dev/null)"
            case "$first" in
                "wolf-trap: "*) wgot="trap:${first#wolf-trap: }" ;;
                *) wgot="exit:$rc" ;;
            esac
        fi
    fi

    # --- lupin
    ( cd "$d" && "$LUPIN" run --json $LUPIN_STDARG main.lu < stdin > lupin.json 2> lupin.err )
    lverdict="$(jq -r '.verdict' "$d/lupin.json" 2>/dev/null)"
    case "$lverdict" in
        exit\(*\)) lgot="exit:$(echo "$lverdict" | sed 's/exit(\(.*\))/\1/')" ;;
        trap\(*\)) lgot="trap:$(echo "$lverdict" | sed 's/trap(\(.*\))/\1/')" ;;
        fail\(*\)) lgot="fail:$(echo "$lverdict" | sed 's/fail(\(.*\))/\1/')" ;;
        unsupported) lgot="unsupported" ;;
        *) lgot="error" ;;
    esac

    # --- stdout
    sres="-"
    if [ -f "$d/expected.out" ]; then
        sres="ok"
        case "$wgot" in
            exit:*) cmp -s "$d/wolf.out" "$d/expected.out" || sres="wolf-differs" ;;
        esac
        [ -n "$args" ] && lcmp=0 || lcmp=1
        case "$lcmp:$lgot" in
            1:exit:*)
                want="$($SHA < "$d/expected.out" | cut -d' ' -f1)"
                have="$(jq -r '.stdout_sha256 // empty' "$d/lupin.json")"
                [ -s "$d/expected.out" ] || want=""
                if [ "$have" != "$want" ]; then
                    [ "$sres" = "ok" ] && sres="lupin-differs" || sres="both-differ"
                fi
                ;;
        esac
    fi

    # --- fmt
    fres="-"
    if [ "$dofmt" -eq 1 ]; then
        if ( cd "$d" && "$WOLF" fmt --check main.lu > fmt.log 2>&1 ); then fres="ok"; else fres="not-canonical"; fi
    fi

    result="PASS"
    [ "$wgot" = "$expect" ] || result="FAIL"
    [ "$lgot" = "$lexpect" ] || result="FAIL"
    case "$sres" in ok|-) ;; *) result="FAIL" ;; esac
    [ "$fres" = "not-canonical" ] && result="FAIL"
    if [ "$result" = "PASS" ]; then pass=$((pass + 1)); else fail=$((fail + 1)); fi
    echo "| $i | $line | $expect | $wgot | $lexpect | $lgot | $sres | $fres | $result |"
done

# --- the prelude list
pres="none"
if grep -q '<!-- prelude-since:begin -->' "$GUIDE"; then
    "$WOLF" prelude --json > "$TMP/prelude.json" 2> "$TMP/prelude.err" || { echo "agents-guide-check: \`wolf prelude --json\` failed" >&2; exit 2; }
    jq -r '.names[].name' "$TMP/prelude.json" | sort -u > "$TMP/prelude.names"
    awk '/<!-- prelude-since:begin -->/ { on = 1; next } /<!-- prelude-since:end -->/ { on = 0 } on' "$GUIDE" |
        grep -o '`[A-Za-z_][A-Za-z0-9_]*`' | tr -d '`' | sort > "$TMP/listed.names"
    listed="$(wc -l < "$TMP/listed.names" | tr -d ' ')"
    gone="$(comm -23 "$TMP/listed.names" "$TMP/prelude.names" | tr '\n' ' ')"
    dups="$(uniq -d "$TMP/listed.names" | tr '\n' ' ')"
    if [ "$listed" -gt 0 ] && [ -z "$gone" ] && [ -z "$dups" ]; then
        pres="ok ($listed names, every one in \`wolf prelude --json\`)"
    else
        pres="FAIL ($listed names; not in the prelude: ${gone:-none}; listed twice: ${dups:-none})"
        fail=$((fail + 1))
    fi
fi

echo
echo "- prelude list: $pres"
echo
echo "**$pass passed, $fail failed, $COUNT fences.**"
[ "$fail" -eq 0 ]
