#!/usr/bin/env bash
# s270b: the repaired witness tree: yew's fl with the one-word repair the new E1010 names
# (`push(copy fl_empty_slot())` in fl_gc_alloc), beside the untouched copy.
set -euo pipefail
base=/home/mfwolffe/lanes/s270b
rm -rf "$base/witnesses/yew-copy"
mkdir -p "$base/witnesses/yew-copy"
for d in fl util unicode; do cp -r "$base/yew/src/$d" "$base/witnesses/yew-copy/$d"; done
f="$base/witnesses/yew-copy/fl/gc.lu"
grep -c '(mut vm.gc.slots).push(fl_empty_slot())' "$f"
sed -i 's/(mut vm\.gc\.slots)\.push(fl_empty_slot())/(mut vm.gc.slots).push(copy fl_empty_slot())/' "$f"
diff -u "$base/yew/src/fl/gc.lu" "$f" || true
diff -r -q "$base/yew/src/fl" "$base/witnesses/yew/fl" && echo "untouched copy identical to yew bf7ab4a3 src/fl"
