#!/bin/bash
# s266: the container's three requirements, read from inside.
set -u
echo "== identity"; id; printf 'pid 1: '; grep '^Uid:' /proc/1/status; tr '\0' ' ' < /proc/1/cmdline; echo
if kill -0 1 2>/dev/null; then echo "FAIL: pid 1 is not foreign"; else echo "kill -0 1 refused: pid 1 foreign"; fi
echo "== limits"; printf 'pids.max='; cat /sys/fs/cgroup/pids.max; printf 'nproc soft=%s hard=%s\n' "$(ulimit -Su)" "$(ulimit -Hu)"
echo "== this process"; grep -E 'Cpus_allowed_list|SigBlk|SigIgn|NoNewPrivs' /proc/self/status
echo "== the shim"
for req in 0 0-3 1,3 4-7 5 0-7 8 0-3:2; do
    out=$(taskset -c "$req" grep Cpus_allowed_list /proc/self/status 2>&1); echo "taskset -c $req -> rc=$? $out"
done
out=$(taskset 1 grep Cpus_allowed_list /proc/self/status 2>&1); echo "taskset 1 (mask) -> $out"
out=$(taskset --cpu-list=2 grep Cpus_allowed_list /proc/self/status 2>&1); echo "taskset --cpu-list=2 -> $out"
echo "== dispositions through the shim"
sh -c 'trap "" PIPE; exec taskset -c 0 grep SigIgn /proc/self/status'
bash -c 'trap "" PIPE XFSZ INT; exec taskset -c 0-3 grep -E "SigIgn|SigBlk" /proc/self/status'
echo "== args byte for byte"
taskset -c 0 printf '[%s]' "a b" "" '$x' "--" "-c"; echo
echo "== tools"; for t in clang ld.lld strace qemu-system-x86_64 cc python3 rustc cargo; do printf '%s: ' $t; command -v $t || echo MISSING; done
rustc --version; cargo --version
