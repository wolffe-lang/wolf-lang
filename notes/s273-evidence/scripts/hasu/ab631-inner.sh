#!/usr/bin/env bash
# s273: does the #631 witness compile to the same executables at trunk and at the head?
set -uo pipefail
src=/lane/wolf-lang/target/tmp/region_copyout_lanes/copy_int/main.lu
d=/lane/scratch/ab631
rm -rf "$d"; mkdir -p "$d"; cp "$src" "$d/main.lu"; cd "$d"
for t in trunk head; do
  if [ $t = trunk ]; then w=/lane/wolf-lang-trunk/target/debug/wolf; else w=/lane/wolf-lang/target/debug/wolf; fi
  "$w" build main.lu -o "native-$t" --no-cache || exit 3
  "$w" build main.lu -o "release-$t" --no-cache --release || exit 3
done
sha256sum main.lu native-trunk native-head release-trunk release-head
cmp native-trunk native-head && echo "S273_AB631 native identical"
cmp release-trunk release-head && echo "S273_AB631 release identical"
