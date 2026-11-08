#!/bin/bash
# s287: one census measurement (measure.py) over the corpus and every org
# checkout, at one rung, on one binary, behind the Mac lock (locked.sh).
# Writes $LANE/records/census-<tag>.json and a marker
# $LANE/records/census-<tag>.exit holding the exit code; TERM, INT and HUP
# write their own nonzero codes, never 0.
# usage: census-job.sh <tag> <wolf> <phase>
set -u
TAG=$1 W=$2 PHASE=$3
M=$LANE/records/census-$TAG.exit
rm -f "$M"
trap 'echo 143 > "$M"; exit 143' TERM
trap 'echo 130 > "$M"; exit 130' INT
trap 'echo 129 > "$M"; exit 129' HUP
O=$LANE/org
DIRS="${CORPUS:-$LANE/base-tree/corpus}"
for d in "$O"/*; do DIRS="$DIRS $d"; done
echo "census $TAG: $W ($(sH1m -a 256 "$W" | cut -c1-16)) phase=$PHASE $(date -u +%FT%TZ)"
"$W" --version
LANE=$LANE "$LANE/rig/locked.sh" python3 -I "$LANE/rig/measure.py" "$W" "$O/wolf-std-pin/std" "$PHASE" '*' "$LANE/records/census-$TAG.json" 4 $DIRS
rc=$?
echo "$rc" > "$M"
echo "census $TAG: END exit=$rc $(date -u +%FT%TZ)"
exit $rc
