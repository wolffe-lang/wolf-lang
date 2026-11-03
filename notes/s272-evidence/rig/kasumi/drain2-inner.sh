#!/usr/bin/env bash
# s272: s275's drain witness, the binaries drain-inner.sh built (trunk and the
# head), N runs each in three loads, interleaved so both see the same host:
#   quiet  — nothing beside it;
#   busy   — two busy loops on the same two CPUs;
#   forks  — two loops of the trunk binary itself beside it on the same two CPUs
#            (CI's fork_lanes runs its fifteen fork rows at once).
# Each run as CI's row runs it: `timeout 120`, here under `taskset -c 0-1` (the
# slot shim moves it to the slot's first two). One line a run; a tally at the end.
# usage: drain2-inner.sh [--loads=quiet,busy,forks] <runs> <label>...   (appends to runs2.txt)
set -uo pipefail
source /lane/scripts/gate-env.sh
loads="quiet busy forks"
case "${1:-}" in --loads=*) loads=$(echo "${1#--loads=}" | tr , ' '); shift ;; esac
runs=$1; shift
labels=("$@")
R=/lane/records/drain
B=$R/bin
for l in "${labels[@]}"; do sha256sum "$B/drain-$l" || exit 2; done
out=$R/runs2.txt
touch "$out"
loaders=()
start_load() {
  case $1 in
    quiet) ;;
    busy) for k in 1 2; do taskset -c 0-1 bash -c 'while :; do :; done' & loaders+=($!); done ;;
    forks) for k in 1 2; do taskset -c 0-1 bash -c "while :; do timeout 120 $B/drain-trunk 1000 >/dev/null 2>&1; done" & loaders+=($!); done ;;
  esac
  echo "load $1: ${loaders[*]:-none}"
}
stop_load() {
  for p in "${loaders[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
  wait 2>/dev/null
  loaders=()
}
for load in $loads; do
  echo "== STEP $load $(date -u +%T)"
  start_load "$load"
  for i in $(seq 1 "$runs"); do
    for label in "${labels[@]}"; do
      t0=$(date +%s.%N)
      timeout 120 taskset -c 0-1 "$B/drain-$label" 1000 > "$R/run2-$load-$label.out" 2>&1
      rc=$?
      t1=$(date +%s.%N)
      line=$(printf 'RUN %s load=%s i=%s rc=%s secs=%.1f %s' "$label" "$load" "$i" "$rc" "$(awk -v a="$t0" -v b="$t1" 'BEGIN{print b-a}')" "$(tail -1 "$R/run2-$load-$label.out" | cut -c1-80)")
      echo "$line" >> "$out"
      [ "$rc" = 0 ] || { echo "$line"; cp "$R/run2-$load-$label.out" "$R/fail-$load-$label-$i.out"; }
    done
  done
  stop_load
  echo "END $load exit=0 $(date -u +%T)"
done
echo "== SUMMARY"
awk '{split($3,l,"="); split($5,r,"="); split($6,t,"="); k=$2" "l[2]; n[k]++; s[k]+=t[2]; if (r[2]==0) ok[k]++; else if (r[2]==124) cap[k]++; else oth[k]++} END {for (k in n) printf "TALLY %s: %d runs, %d rc=0, %d rc=124, %d other, mean %.1f s\n", k, n[k], ok[k], cap[k], oth[k], s[k]/n[k]}' "$out" | sort
echo "END drain2 exit=0"
