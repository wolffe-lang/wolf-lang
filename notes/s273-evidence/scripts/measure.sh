#!/bin/bash
# s273: #972's measurement — the witness's modes at N = 1k, 10k, 100k on
# native, release and checked, and the C bar. Run behind the Mac build
# lock (locked.sh). usage: measure.sh <wolf-binary> <label> [modes...]
set -u
W=$1
LABEL=$2
shift 2
MODES=${*:-"issue setonly index keys intmap list region rset rindex"}
export WOLF_STD=/Users/mfwolffe/lanes/s273/std-78b9d3c/std
D=/Users/mfwolffe/lanes/s273/m
P="python3 /Users/mfwolffe/lanes/s273/peak.py"
cd "$D/w972" || exit 1
"$W" build main.lu -o "w972-native-$LABEL" --no-cache || { echo "END build-native exit=$?"; exit 1; }
"$W" build main.lu -o "w972-release-$LABEL" --no-cache --release || { echo "END build-release exit=$?"; exit 1; }
cc -O2 -o "$D/c972/c972" "$D/c972/main.c" || exit 1
hex() { printf '%s' "$1" | xxd -p | tr -d '\n'; }
for n in 1000 10000 100000; do
  echo "C $n: $($P 3000 120 -- "$D/c972/c972" $n)"
  for mode in $MODES; do
    for tier in native release; do
      echo "$LABEL $tier $n $mode: $($P 3000 120 -- "./w972-$tier-$LABEL" $n $mode)"
    done
    if [ "$n" -le 100000 ]; then
      echo "$LABEL checked $n $mode: $($P 3000 120 -- "$W" conform-run --json --checked "--argv-hex=$(hex w972)" "--argv-hex=$(hex $n)" "--argv-hex=$(hex $mode)" main.lu)"
    fi
  done
done
