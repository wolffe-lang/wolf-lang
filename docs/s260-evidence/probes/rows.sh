#!/usr/bin/env bash
# Run every built probe of <label> in the container: native, release,
# the checked machine (conform-run --json --checked) and lupin.
# rows.sh <label> <wolf-inside-container> <group...>
set -uo pipefail
label="$1"; wolfbin="$2"; shift 2
root="$HOME/lanes/s260"
E="$root/probes/exec.sh"
lupin=/tc/lupin/lupin-0.1.55-x86_64-unknown-linux-gnu/lupin
cd "$root/w/$label"
for g in "$@"; do
  for d in $g/*/; do
    d=${d%/}; n=$(basename "$d")
    redir=""
    [[ $n == witness && $g == copy ]] && redir=" > /dev/full"
    for tier in native release; do
      echo "--- $label $g/$n $tier"
      "$E" "$PWD/$d" bash -c "./$tier$redir" 2>&1
    done
    echo "--- $label $g/$n checked"
    "$E" "$PWD/$d" bash -c "$wolfbin conform-run --json --checked main.lu$redir" 2>&1 | sed 's/"stdout_sha256":"[0-9a-f]*",//; s/"diagnostics":\[\],"file":".\/main.lu",//'
    echo "--- $label $g/$n lupin"
    "$E" "$PWD/$d" bash -c "$lupin run main.lu$redir" 2>&1
  done
done
