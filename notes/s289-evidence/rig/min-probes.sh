#!/bin/bash
# s289, third change: run every probe under $LANE/probe-min on the three tiers
# and lupin (adapted by copy from probes.sh, with the lane's own paths, so it
# runs on either Mac). Writes $LANE/runs/<label>/ (a fresh copy of the probes).
# usage: min-probes.sh <wolf-binary> <label>   (behind a build lock)
set -u
WOLF=$1
LABEL=$2
L=${LANE:?}
LUPIN=${LUPIN:-$L/inputs/lupin060/x/lupin}
R=$L/runs/$LABEL
rm -rf "$R"
mkdir -p "$R"
cp -R "$L/probe-min/." "$R/"
echo "== s289 min probes, label $LABEL, $(date -u +%FT%TZ)"
shasum -a 256 "$WOLF" "$LUPIN"
"$WOLF" --version | head -1
"$LUPIN" --version
show() { sed 's/^/    /'; }
for c in $(cd "$R" && ls -d m* z* | sort); do
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
  if [ $rc -eq 0 ]; then ./n.out > n.run 2>&1; rc=$?; show < n.run; echo "    run exit $rc"; fi
  echo "--- release: wolf build --release"
  "$WOLF" build --release main.lu -o r.out > r.stdout 2> r.stderr; rc=$?
  show < r.stderr
  echo "    build exit $rc"
  if [ $rc -eq 0 ]; then ./r.out > r.run 2>&1; rc=$?; show < r.run; echo "    run exit $rc"; fi
  echo "--- lupin run"
  "$LUPIN" run main.lu > l.stdout 2> l.stderr; rc=$?
  show < l.stdout
  show < l.stderr
  echo "    lupin exit $rc"
done
echo
echo "== wolf prelude: min and zip"
"$WOLF" prelude 2>&1 | grep -E "^\s*(min|zip)\b|\bmin\b|\bzip\b" | show
echo "== done $(date -u +%FT%TZ)"
