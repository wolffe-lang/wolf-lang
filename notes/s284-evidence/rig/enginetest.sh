#!/usr/bin/env bash
# s284 job "enginetest": prove rig/engine.sh from inside an r41-toolchain:1
# container — a fork_lanes-shaped run through the host's podman service: the
# shell is process 1, the CPUs are the block's, the memory is capped, and a
# bind mount of the lane directory reaches the nested container.
# usage: enginetest.sh <A|B>
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
case $blk in A) SIX=0-5 ;; B) SIX=6-11 ;; *) echo "block must be A or B"; exit 2 ;; esac
source "$L/rig/engine-lib.sh"
export PATH=$HOME/lanes/h1-env/bin:$HOME/lanes/h1-env/profile/bin:$PATH
echo "LOCKS_ACQUIRED $(date -u +%FT%TZ) block=$blk six=$SIX"
trap engine_down EXIT
engine_up enginetest "$SIX" || exit 1
mkdir -p "$L/tmp/enginetest" && echo hello >"$L/tmp/enginetest/probe.txt"
podman run --rm --userns=keep-id -u "$(id -u):$(id -g)" --memory 4g -v "$L:$L" "${ENGINE_FLAGS[@]}" \
    localhost/r41-toolchain:1 taskset.real -c "$SIX" bash -c '
    e=$WOLF_FORK_ENGINE
    echo "inside: uid=$(id -u) affinity=$(taskset.real -pc $$ | sed "s/.*: //")"
    "$e" run --rm --network none docker.io/library/ubuntu:24.04 sh -c "echo \$\$"
    echo "probe rc=$?"
    "$e" run --rm --network none -v '"$L"'/tmp/enginetest:/w -w /w docker.io/library/ubuntu:24.04 sh -c "echo pid=\$\$; nproc; grep -E \"Cpus_allowed_list|Threads\" /proc/self/status; cat /w/probe.txt; cat /sys/fs/cgroup/memory.max; echo rc=\$?"
    echo "run rc=$?"
    exit $?'
echo "ENGINETEST_END rc=$? $(date -u +%FT%TZ)"
