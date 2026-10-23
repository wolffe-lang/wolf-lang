#!/usr/bin/env bash
# s260 probes: each program in its own directory (wolf build takes the
# directory as one module), built with $WOLF on the host (taskset 4-7).
# Usage: run.sh <wolf> <label> <group...>
set -uo pipefail
wolf="$1"; label="$2"; shift 2
root="$HOME/lanes/s260"
for d in "$@"; do
  g=$(basename "$d")
  for src in "$d"/*.lu; do
    n=$(basename "$src" .lu)
    out="$root/w/$label/$g/$n"
    rm -rf "$out"; mkdir -p "$out"
    find "$d" -maxdepth 1 -type f ! -name '*.lu' -exec cp -a {} "$out"/ \;
    cp "$src" "$out/main.lu"
    for tier in native release; do
      flags=(); [[ $tier == release ]] && flags+=(--release)
      ( cd "$out" && taskset -c 4-7 "$wolf" build main.lu "${flags[@]}" -o "$tier" ) > "$out/$tier.build" 2>&1
      rc=$?
      echo "== $label $g/$n $tier build rc=$rc"
    done
  done
done
