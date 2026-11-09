#!/bin/bash
# s289: run every probe under ~/lanes/s289/probe on the three tiers and lupin.
# usage: probes.sh <wolf-binary> <label>   (behind the Mac lock, via locked.sh)
# Writes ~/lanes/s289/runs/<label>/ (a fresh copy of the probes) and prints
# one block per case. Never the live probe tree: the copy takes the caches.
set -u
WOLF=$1
LABEL=$2
L=$LANE
LUPIN=${LUPIN:-$L/inputs/lupin060/x/lupin}
R=$L/runs/$LABEL
rm -rf "$R"
mkdir -p "$R"
cp -R "$L/probe/." "$R/"
echo "== s289 probes, label $LABEL, $(date -u +%FT%TZ)"
sH3m -a 256 "$WOLF" "$LUPIN"
"$WOLF" --version | head -1
"$LUPIN" --version
show() { sed 's/^/    /'; }
for c in $(cd "$R" && ls -d e* p* | sort -V); do
  [ -f "$R/$c/main.lu" ] || continue
  cd "$R/$c" || exit 1
  echo
  echo "######## $c"
  echo "--- checked: conform-run --json --checked"
  "$WOLF" conform-run main.lu --json --checked 2>&1 | show
  echo "--- native: wolf build"
  "$WOLF" build main.lu -o n.out > n.stdout 2> n.stderr; rc=$?
  show < n.stderr
  echo "    build exit $rc"
  if [ $rc -eq 0 ]; then ./n.out > /dev/null 2>&1; echo "    run exit $?"; fi
  echo "--- release: wolf build --release"
  "$WOLF" build --release main.lu -o r.out > r.stdout 2> r.stderr; rc=$?
  show < r.stderr
  echo "    build exit $rc"
  if [ $rc -eq 0 ]; then ./r.out > /dev/null 2>&1; echo "    run exit $?"; fi
  echo "--- lupin run"
  "$LUPIN" run main.lu > l.stdout 2> l.stderr; rc=$?
  show < l.stderr
  echo "    lupin exit $rc"
  echo "--- lupin conform-run --json"
  "$LUPIN" conform-run main.lu --json 2>&1 | show
done
# the wolf test shape
cd "$R/e12_wolf_test" || exit 1
echo
echo "######## e12_wolf_test (wolf test dialog_test.lu)"
"$WOLF" test dialog_test.lu > t.stdout 2> t.stderr; rc=$?
show < t.stdout
show < t.stderr
echo "    wolf test exit $rc"
echo "--- lupin run dialog_test.lu"
"$LUPIN" run dialog_test.lu 2>&1 | show
# the manifest writer (wolf add) and its reader: each case copies the
# template and names the dependency's directory with the characters under
# test (made here, never committed: a Windows checkout cannot hold them)
for c in 'k1_backslash:dep\q' 'k2_brace:dep{1}' 'k3_quote:dep"q' 'k4_hex:dep\x'; do
  dep=${c#*:}; c=${c%%:*}
  mkdir -p "$R/$c" && cp -R "$R/k_template/app" "$R/$c/app" && cp -R "$R/k_template/dep" "$R/$c/$dep"
  cd "$R/$c" || exit 1
  echo
  echo "######## $c (wolf add util --path '../$dep')"
  "$WOLF" add util --path "../$dep" --dir app > a.stdout 2> a.stderr; rc=$?
  show < a.stderr
  echo "    wolf add exit $rc"
  echo "--- app/wolf.pkg"
  show < app/wolf.pkg 2>/dev/null || echo "    (no manifest)"
  echo "--- wolf build app/main.lu (reads the manifest back)"
  "$WOLF" build app/main.lu -o app/m > b.stdout 2> b.stderr; rc=$?
  show < b.stderr
  echo "    build exit $rc"
  if [ $rc -eq 0 ]; then ./app/m 2>&1 | show; fi
done
echo
echo "== done $(date -u +%FT%TZ)"
