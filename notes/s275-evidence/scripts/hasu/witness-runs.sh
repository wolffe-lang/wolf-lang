#!/usr/bin/env bash
# s275: the drain witness (fork_lanes' drain.lu), run N times inside the
# container under the CPUs the caller pinned it to. Exit 1 is red (the
# drain thread gone: the thread count fell), 124 a stall (the reap's
# os_signal_wait parked until `timeout` ended it), 0 green.
# usage: witness-runs.sh <label> <binary> <runs> <rounds> <cap-seconds>
label=${1:?label}
bin=${2:?binary}
n=${3:?runs}
rounds=${4:?rounds}
cap=${5:?cap}
echo "witness $label: $bin, $n runs of $rounds rounds, cap ${cap}s, $(date -Iseconds)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
g=0 r=0 s=0 o=0
for i in $(seq 1 "$n"); do
    t0=$(date +%s%N)
    out=$(timeout "$cap" "$bin" "$rounds" 2>&1)
    rc=$?
    ms=$((($(date +%s%N) - t0) / 1000000))
    case $rc in
    0) g=$((g + 1)) ;;
    1) r=$((r + 1)) ;;
    124) s=$((s + 1)) ;;
    *) o=$((o + 1)) ;;
    esac
    echo "run $i exit=$rc ${ms}ms $out"
done
echo "witness $label: runs $n green $g red $r stalled $s other $o"
