#!/usr/bin/env bash
# s272: s275's drain witness in CI's own shape (inside job.sh's hold of a slot):
# each run in a throwaway container as fork_lanes runs it — `podman run --rm
# --network none -v <bins>:/w -w /w <ubuntu 24.04> sh -c 'timeout 120 ...'`,
# no init, sh as pid 1, root in the container — on two CPUs of the slot and
# then on all four (CI's runner: nproc 4), quiet and with two loops of the
# trunk binary beside it on the same CPUs. r41-toolchain:1 is the ubuntu:24.04
# stand-in (the same release; GNU timeout). One line a run; a tally at the end.
# usage: chain-drain3.sh <slot> <runs> <label>...
set -uo pipefail
slot=$1 runs=$2; shift 2
labels=("$@")
lo=${slot%-*}
L=/home/mfwolffe/lanes/s272
B=$L/records/drain/bin
out=$L/records/drain3/runs.txt
mkdir -p "$L/records/drain3"
: > "$out"
loaders=()
for cpus in "$lo-$((lo + 1))" "$slot"; do
  for load in quiet forks; do
    echo "== STEP cpus=$cpus load=$load $(date -u +%T)"
    if [ "$load" = forks ]; then
      for k in 1 2; do
        taskset -c "$cpus" bash -c "while :; do timeout 120 $B/drain-trunk 1000 >/dev/null 2>&1; done" &
        loaders+=($!)
      done
    fi
    for i in $(seq 1 "$runs"); do
      for label in "${labels[@]}"; do
        t0=$(date +%s.%N)
        res=$(podman run --rm --network none --cpuset-cpus "$cpus" -v "$B:/w:ro" -w /w localhost/r41-toolchain:1 \
          sh -c "timeout 120 ./drain-$label 1000; echo rc=\$?" 2>&1 | tail -2 | tr '\n' ' ')
        t1=$(date +%s.%N)
        rc=$(printf '%s' "$res" | grep -oE 'rc=[0-9]+' | tail -1)
        line=$(printf 'RUN %s cpus=%s load=%s i=%s %s secs=%.1f %s' "$label" "$cpus" "$load" "$i" "${rc:-rc=none}" "$(awk -v a="$t0" -v b="$t1" 'BEGIN{print b-a}')" "$(printf '%s' "$res" | cut -c1-80)")
        echo "$line" >> "$out"
        [ "$rc" = rc=0 ] || echo "$line"
      done
    done
    for p in "${loaders[@]:-}"; do [ -n "$p" ] && kill "$p" 2>/dev/null; done
    wait 2>/dev/null
    loaders=()
    echo "END cpus=$cpus load=$load exit=0 $(date -u +%T)"
  done
done
echo "== SUMMARY"
# A run's line: RUN <label> cpus=.. load=.. i=.. rc=.. secs=..: the exit is the
# sixth field (s272's first tally read the fifth, i=, and called every run "other").
awk '{split($6,r,"="); k=$2" "$3" "$4; n[k]++; if (r[2]==0) ok[k]++; else if (r[2]==124) cap[k]++; else oth[k]++} END {for (k in n) printf "TALLY %s: %d runs, %d rc=0, %d rc=124, %d other\n", k, n[k], ok[k], cap[k], oth[k]}' "$out" | sort
echo "END drain3 exit=0"
