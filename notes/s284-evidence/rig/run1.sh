#!/usr/bin/env bash
# s284: ONE run of drain.lu (1000 rounds, the row's own `timeout 120`), in a
# throwaway docker.io/library/ubuntu:24.04 container of its own, exactly as
# fork_lanes runs the row's pinned lane: no network, NO init (sh is the
# namespace's process 1), the binary's directory at /w, and
#   timeout 120 taskset -c <four> /w/<bin> 1000; echo rc=$?
# under the condition <cond> (its contention lives in the same container
# and ends with it):
#   quiet       nothing else
#   hogN        N busy loops (`sh -c 'while :; do :; done'`) on the same four CPUs
#   forksN      N fork+exec loops (`while :; do /bin/true; done`) on the same four CPUs
#   nice19hogN  N busy loops at nice 0, the program at nice 19
#   idlehogN    N busy loops, the program under SCHED_IDLE (chrt -i 0)
#   cpus<X>     podman --cpus X (the CFS quota, period 100 ms)
#   q<Q>p<P>    podman --cpu-quota Q --cpu-period P (microseconds)
#   churnN      N host-side loops, for the run's whole life, each starting one
#               short throwaway container after another (ubuntu:24.04, no
#               network, its sh pinned to the same four CPUs, 100 fork+execs of
#               /bin/true), as CI's neighbouring fork rows start theirs; the
#               loops are this script's own, stopped by their recorded pids
# The program's threads, hogs and children are all pinned inside the container
# with `taskset` (H1 has no cpuset controller); quotas are the cpu controller's.
# A run still going at 40 s is dumped from the host through /proc (twice,
# 5 s apart, to show whether anything moves): every process of the container,
# every task's state, utime/stime, wchan and syscall, the status masks, and
# the program's maps.
# Prints one line: RUN <tag> <cond> <bin> rc=<rc> ms=<ms> | <output>
# usage: run1.sh <cond> <bin> <four> <dumpdir> <tag>
set -u
cond=${1:?cond}
bin=${2:?binary}
four=${3:?cpus}
dumpdir=${4:?dumpdir}
tag=${5:?tag}
export PATH=$HOME/lanes/h1-env/bin:$HOME/lanes/h1-env/profile/bin:$PATH
dir=$(dirname "$bin")
exe=$(basename "$bin")
name=s284-$tag
flags=()
pre=""
wrap="taskset -c $four"
loops() { # $1 count, $2 body
    local i s=""
    for i in $(seq 1 "$1"); do s="$s taskset -c $four sh -c '$2' & "; done
    printf '%s' "$s"
}
case $cond in
quiet) ;;
hog[0-9]*) pre=$(loops "${cond#hog}" 'while :; do :; done') ;;
forks[0-9]*) pre=$(loops "${cond#forks}" 'while :; do /bin/true; done') ;;
nice19hog[0-9]*) pre=$(loops "${cond#nice19hog}" 'while :; do :; done'); wrap="taskset -c $four nice -n 19" ;;
idlehog[0-9]*) pre=$(loops "${cond#idlehog}" 'while :; do :; done'); wrap="taskset -c $four chrt -i 0" ;;
cpus*) flags=(--cpus "${cond#cpus}") ;;
churn[0-9]*) churn=${cond#churn} ;;
q*p*) q=${cond#q}; q=${q%%p*}; p=${cond##*p}; flags=(--cpu-quota "$q" --cpu-period "$p") ;;
*) echo "RUN $tag $cond $exe rc=bad-condition"; exit 2 ;;
esac
churners=()
start_churn() {
    local i
    for i in $(seq 1 "${churn:-0}"); do
        (
            trap 'exit 0' TERM
            k=0
            while :; do
                k=$((k + 1))
                podman run --rm --name "$name-churn$i-$k" --network none --memory 1g --pids-limit 512 \
                    docker.io/library/ubuntu:24.04 taskset -c "$four" sh -c 'i=0; while [ $i -lt 100 ]; do /bin/true; i=$((i+1)); done' \
                    >/dev/null 2>&1 </dev/null &
                wait $!
            done
        ) &
        churners+=("$!")
    done
}
stop_churn() {
    local c
    for c in "${churners[@]}"; do kill -TERM "$c" 2>/dev/null; done
    for c in "${churners[@]}"; do wait "$c" 2>/dev/null; done
}
script="$pre timeout 120 $wrap /w/$exe 1000; echo rc=\$?; echo CPUSTAT \$(tr '\\n' ' ' < /sys/fs/cgroup/cpu.stat)"
out=$dumpdir/$tag.out
start_churn
t0=$(date +%s%N)
podman run --rm --name "$name" --network none --memory 4g --pids-limit 4096 "${flags[@]}" \
    -v "$dir:/w" -w /w docker.io/library/ubuntu:24.04 sh -c "$script" >"$out" 2>&1 </dev/null &
pr=$!
dumped=0
dump() { # one look at every process of the container, from the host
    local pid1 p t f
    pid1=$(podman inspect --format '{{.State.Pid}}' "$name" 2>/dev/null) || return
    [ -n "$pid1" ] && [ "$pid1" != 0 ] || return
    echo "LOOK $1 $(date -u +%FT%T.%3NZ) container=$name pid1=$pid1 cond=$cond bin=$exe"
    local todo=("$pid1") seen=()
    while [ "${#todo[@]}" -gt 0 ]; do
        p=${todo[0]}
        todo=("${todo[@]:1}")
        [ -d "/proc/$p" ] || continue
        seen+=("$p")
        for t in /proc/$p/task/*; do
            for c in $(cat "$t/children" 2>/dev/null); do todo+=("$c"); done
        done
    done
    for p in "${seen[@]}"; do
        f=/proc/$p
        echo "PROC $p $(grep -E '^(Name|State|NSpid|PPid|Threads|SigPnd|ShdPnd|SigBlk|SigIgn|SigCgt|voluntary_ctxt_switches|nonvoluntary_ctxt_switches)' "$f/status" 2>/dev/null | tr '\t\n' '  ')"
        for t in "$f"/task/*; do
            # stat: 3 state, 14 utime, 15 stime, 39 processor (after the comm's parens)
            st=$(sed 's/^.*) //' "$t/stat" 2>/dev/null | awk '{print "state=" $1 " utime=" $12 " stime=" $13 " cpu=" $37}')
            echo "  TASK ${t##*/} comm=$(cat "$t/comm" 2>/dev/null) $st wchan=$(cat "$t/wchan" 2>/dev/null) syscall=$(cat "$t/syscall" 2>/dev/null)"
        done
        case $(cat "$f/comm" 2>/dev/null) in
        drain*) echo "  MAPS $(grep -m3 "$exe" "$f/maps" 2>/dev/null | tr '\n' ';')" ;;
        esac
    done
}
while kill -0 "$pr" 2>/dev/null; do
    now=$((($(date +%s%N) - t0) / 1000000))
    if [ "$dumped" = 0 ] && [ "$now" -gt 40000 ]; then
        { dump 1; sleep 5; dump 2; } >"$dumpdir/$tag.dump" 2>&1
        dumped=1
    fi
    sleep 0.2
done
wait "$pr"
prc=$?
stop_churn
ms=$((($(date +%s%N) - t0) / 1000000))
rc=$(sed -n 's/^rc=//p' "$out" | tail -1)
[ -n "$rc" ] || rc="none(podman=$prc)"
thr=$(sed -n 's/.*nr_periods \([0-9]*\) nr_throttled \([0-9]*\) throttled_usec \([0-9]*\).*/periods=\1 throttled=\2 throttled_us=\3/p' "$out" | tail -1)
line=$(grep -v '^rc=\|^CPUSTAT' "$out" | tr '\n' ' ' | cut -c1-200)
echo "RUN $tag $cond $exe rc=$rc ms=$ms dumped=$dumped ${thr:-} | $line"
