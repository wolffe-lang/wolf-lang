#!/bin/bash
# s273: the `--phase=mem` census — every .lu under the given roots through
# `wolf conform-run --json --phase=mem`, one TSV row per file:
#   <path relative to its root's parent>\t<exit>\t<sha256 of the record>\t<verdict>
# usage: census.sh <wolf> <out.tsv> <root>...
set -u
W=$1
OUT=$2
shift 2
export WOLF_STD=/Users/mfwolffe/lanes/s273/std-78b9d3c/std
one() {
  local w=$1 f=$2 rel=$3
  local rec rc
  rec=$(cd "$(dirname "$f")" && timeout 120 "$w" conform-run --json --phase=mem "$(basename "$f")" 2>/dev/null)
  rc=$?
  local sha verdict
  sha=$(printf '%s' "$rec" | shasum -a 256 | cut -c1-16)
  verdict=$(printf '%s' "$rec" | python3 -c 'import json,sys
try: print(json.loads(sys.stdin.read())["verdict"])
except Exception: print("-")')
  printf '%s\t%s\t%s\t%s\n' "$rel" "$rc" "$sha" "$verdict"
}
export -f one
: > "$OUT.tmp"
for root in "$@"; do
  base=$(dirname "$root")
  find "$root" -name '*.lu' -type f | sort | while read -r f; do
    printf '%s\0%s\0' "$f" "${f#$base/}"
  done
done | xargs -0 -n2 -P4 bash -c 'one "$0" "$1" "$2"' "$W" >> "$OUT.tmp"
sort "$OUT.tmp" > "$OUT" && rm -f "$OUT.tmp"
echo "census: $(wc -l < "$OUT") files -> $OUT"
