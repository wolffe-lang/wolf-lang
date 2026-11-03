#!/usr/bin/env bash
# s272: s275's drain witness (`drain.lu`) native, at two commits, in CI's shape
# (two CPUs: `taskset -c 0-1`, which the slot's shim moves to the slot's first
# two; the row's cap, `timeout 120`), N runs each, interleaved so both see the
# same host; then N more each with two busy loops on the same two CPUs (the
# load a 2-vCPU runner carries while the suite runs). One line a run: label,
# load, rc, seconds. rc 124 is the cap (a stall), 1 the thread count falling.
# usage: drain-inner.sh <runs> <label> <sha> <label> <sha>
set -uo pipefail
source /lane/scripts/gate-env.sh
cd /lane/wolf-lang || exit 2
runs=$1; shift
R=/lane/records/drain
mkdir -p "$R/bin"
labels=()
while [ $# -ge 2 ]; do
  label=$1 sha=$2; shift 2
  git checkout -q --detach "$sha" && test "$(git rev-parse HEAD)" = "$sha" || { echo "END $label-checkout exit=2"; exit 2; }
  echo "== STEP $label-build $(date -u +%T) head=$(git rev-parse HEAD)"
  cargo build --locked -p wolf_driver -p wolf_rt > "$R/$label-build.log" 2>&1
  b=$?
  ./target/debug/wolf build crates/wolf_driver/tests/fixtures/fork/drain.lu -o "$R/bin/drain-$label" --no-cache >> "$R/$label-build.log" 2>&1
  w=$?
  sha256sum "$R/bin/drain-$label"
  echo "END $label-build exit=$((b | w))"
  [ $((b | w)) -eq 0 ] || exit 2
  labels+=("$label")
done
burners=()
round() {
  local load=$1 i label t0 rc t1
  for i in $(seq 1 "$runs"); do
    for label in "${labels[@]}"; do
      t0=$(date +%s.%N)
      timeout 120 taskset -c 0-1 "$R/bin/drain-$label" 1000 > "$R/run-$load-$label-$i.out" 2>&1
      rc=$?
      t1=$(date +%s.%N)
      printf 'RUN %s load=%s i=%s rc=%s secs=%.1f %s\n' "$label" "$load" "$i" "$rc" "$(awk -v a="$t0" -v b="$t1" 'BEGIN{print b-a}')" "$(tail -1 "$R/run-$load-$label-$i.out" | cut -c1-80)"
    done
  done
}
: > "$R/runs.txt"
echo "== STEP quiet $(date -u +%T)"
round quiet | tee -a "$R/runs.txt" | grep -E '^RUN .* rc=[^0]'
echo "== STEP loaded $(date -u +%T)"
for k in 1 2; do
  taskset -c 0-1 bash -c 'while :; do :; done' &
  burners+=($!)
done
echo "burners: ${burners[*]}"
round loaded | tee -a "$R/runs.txt" | grep -E '^RUN .* rc=[^0]'
for p in "${burners[@]}"; do kill "$p"; done
wait 2>/dev/null
echo "== SUMMARY"
grep -E '^RUN' "$R/runs.txt" | awk '{split($3,l,"="); split($5,r,"="); k=$2" "l[2]; n[k]++; if (r[2]==0) ok[k]++; else if (r[2]==124) cap[k]++; else oth[k]++} END {for (k in n) printf "TALLY %s: %d runs, %d rc=0, %d rc=124, %d other\n", k, n[k], ok[k], cap[k], oth[k]}' | sort
echo "END drain exit=0"
