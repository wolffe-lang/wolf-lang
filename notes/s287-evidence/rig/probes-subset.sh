#!/bin/bash
# s287: the named probes (dirs under $LANE/probe) on the four machines.
set -u
W=$1; shift
echo "== wolf: $W $(sH1m -a 256 "$W" | cut -c1-16)"
files=(); for d in "$@"; do files+=("$LANE/probe/$d/row.lu"); done
"$LANE/rig/probe-all.sh" "$W" "${files[@]}"
