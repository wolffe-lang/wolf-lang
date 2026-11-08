#!/bin/bash
# s287: every probe on the four machines with one wolf binary (behind the lock).
set -u
W=$1
echo "== wolf: $W $(sH1m -a 256 "$W" | cut -c1-16)"; "$W" --version
LUPIN=$LANE/inputs/lupin060/unpacked/lupin-0.1.60-aarch64-apple-darwin/lupin; "$LUPIN" --version
files=(); for d in "$LANE"/probe/*/; do files+=("${d}row.lu"); done
"$LANE/rig/probe-all.sh" "$W" "${files[@]}"
