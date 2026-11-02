#!/bin/bash
# s275 (the suite condition): the WHOLE wolf_rt lib test binary, default threads, as CI's cargo test runs it, and wait_fd_wakes_for_a_signal_while_parked's 200 waits'
# latencies, at trunk (bb728d66 + a print, c4c0e0fb) and at the head
# (abd9482f + the same print, 9f3c5882), on nomad. Ten turns on the Mac lock,
# one per pair (the lane rules: a series takes the lock once per run, at least
# 60 s apart), each turn running the two test binaries once, the order
# alternating; uptime recorded at each.
# usage: series.sh [turns]
set -u
L=/Users/mfwolffe/lanes/s275
D=$L/lat/suite
turns=${1:-10}
T=$L/lat/wolf_rt-trunk-c4c0e0fb
H=$L/lat/wolf_rt-head-9f3c5882
[ -x "$T" ] && [ -x "$H" ] || { echo "REFUSED: the binaries are not built"; exit 2; }
for i in $(seq 1 "$turns"); do
    if [ $((i % 2)) = 1 ]; then order="trunk head"; else order="head trunk"; fi
    s=$D/steps-run-$i.txt
    : >"$s"
    printf 'uptime\tuptime\n' >>"$s"
    for side in $order; do
        if [ "$side" = trunk ]; then b=$T; else b=$H; fi
        printf '%s\t%s\n' "$side" "cd crates/wolf_rt && WOLF_S275_LAT=1 $b --nocapture > $D/run-$i-$side.log 2>&1; rc=\$?; grep -c S275_LAT $D/run-$i-$side.log; exit \$rc" >>"$s"
    done
    echo "turn $i ($order) $(date +%T)"
    bash "$L/mac-run.sh" 9f3c5882 "$s" >"$D/mac-run-$i.log" 2>&1
    grep -E '^END |load averages' "$D/mac-run-$i.log"
    [ "$i" -lt "$turns" ] && sleep 65
done
echo "series done $(date +%T)"
