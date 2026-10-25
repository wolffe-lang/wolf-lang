#!/bin/sh
# s266: build the spawn-errno probe with $WOLF on each tier and run it in a
# scratch directory holding the reasons; `run.sh WOLF OUTDIR`.
set -u
W=$1
D=$2
here=$(cd "$(dirname "$0")" && pwd)
rm -rf "$D"
mkdir -p "$D/w"
cp "$here/main.lu" "$D/main.lu"
cd "$D" || exit 1
"$W" --version | head -1
uname -sr
for tier in native release; do
    flag=
    [ "$tier" = release ] && flag=--release
    "$W" build main.lu -o "probe-$tier" $flag >"build-$tier.log" 2>&1
    echo "build $tier exit=$?"
done
cd "$D/w" || exit 1
printf 'echo hi\n' >nox
mkdir dd
printf '\0\0binary\n' >bin
chmod +x bin
printf '#!/bin/sh\necho ran >&2\n' >busy
chmod +x busy
exec 5>>busy
for tier in native release; do
    echo "== $tier"
    if [ -x "../probe-$tier" ]; then
        "../probe-$tier"
        echo "probe-$tier exit=$?"
    else
        echo "no probe-$tier"; cat "../build-$tier.log"
    fi
done
echo "== checked"
"$W" conform-run --checked --json ../main.lu >../checked.json 2>../checked.err
echo "conform-run exit=$?"
# A failed os_exec writes the checked machine's buffered prints to fd 1
# first ([os.proc.exec]), so the record is the file's last line; a
# successful one leaves no record at all.
python3 -I -c '
import json, sys
lines = open(sys.argv[1]).read().splitlines()
try:
    r = json.loads(lines[-1]) if lines else None
except ValueError:
    r = None
if r is None:
    print("".join(l + "\n" for l in lines), end="")
    print("(no record: the exec replaced the machine)")
else:
    print("".join(l + "\n" for l in lines[:-1]), end="")
    print(r.get("stdout_inline") or "", end="")
    print("verdict", r["verdict"])
' ../checked.json
exec 5>&-
