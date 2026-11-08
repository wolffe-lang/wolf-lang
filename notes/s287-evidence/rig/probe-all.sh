#!/bin/bash
# s287: run every probe (or the files named) on the four machines: the
# checked machine, native and release (`wolf conform-run --json`), and
# lupin (the reference interpreter). One line per machine: verdict,
# phase reached, stdout, warning codes, and the first diagnostic line.
# usage: probe-all.sh <wolf-binary> <file.lu>...
set -u
W=$1; shift
LUPIN=${LUPIN:-$LANE/inputs/lupin060/unpacked/lupin-0.1.60-aarch64-apple-darwin/lupin}
summ() {
  python3 -I -c '
import json, sys
raw = sys.stdin.read()
err = open(sys.argv[2]).read().strip().splitlines()
try:
    r = json.loads(raw)
except Exception:
    print("%-8s NO-RECORD %s" % (sys.argv[1], (err[:1] or [raw[:120]])[0])); sys.exit()
w = ",".join(sorted("%s@%s-%s" % (x.get("code","?"), x.get("span",[0,0])[0], x.get("span",[0,0])[1]) for x in r.get("warnings", [])))
first = next((l for l in err if "error" in l or "unsupported" in l or "warning" in l), "")
print("%-8s %-12s %-9s out=%r warns=[%s] %s" % (sys.argv[1], r.get("verdict"), r.get("phase_reached"), r.get("stdout_inline"), w, first[:160]))
' "$1" "$2"
}
E=${TMPDIR:-$LANE/tmp}/probe-err.$$
for f in "$@"; do
  echo "## $(basename "$(dirname "$f")")/$(basename "$f")"
  for m in --checked --native --release; do
    "$W" conform-run --json "$m" "$f" 2>"$E" | summ "${m#--}" "$E"
  done
  "$LUPIN" conform-run --json "$f" 2>"$E" | summ lupin "$E"
done
rm -f "$E"
