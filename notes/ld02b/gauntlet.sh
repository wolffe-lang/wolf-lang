#!/bin/bash
# ld02b's gauntlet: `cargo xtask ci` at the checkout's head, in the lane's
# container. The verdict is the command's own exit status, written to
# evidence/gauntlet.exit (never a pipe's).
set -u
L=/home/mfwolffe/lanes/ld02b
cd $L/wolf-lang
echo "ld02b gauntlet head=$(git rev-parse HEAD) host=$(hostname) start=$(date -u +%FT%TZ)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn|NoNewPrivs' /proc/self/status
printf 'pids.max='; cat /sys/fs/cgroup/pids.max
printf 'nproc soft=%s hard=%s\n' "$(ulimit -Su)" "$(ulimit -Hu)"
printf 'pid 1: '; grep '^Uid:' /proc/1/status
if kill -0 1 2>/dev/null; then echo "FAIL: pid 1 is not foreign"; exit 3; fi
echo "pid 1 foreign (kill -0 1 refused)"
export S266_TASKSET_LOG=$L/evidence/gauntlet-taskset.jsonl
: > "$S266_TASKSET_LOG"
rm -f $L/evidence/gauntlet.exit
cargo run --quiet -j4 --package xtask -- ci > $L/evidence/gauntlet.log 2>&1
rc=$?
echo "$rc" > $L/evidence/gauntlet.exit
echo "ld02b gauntlet exit=$rc end=$(date -u +%FT%TZ)"
echo "taskset requests: $(wc -l < "$S266_TASKSET_LOG"), translated: $(grep -c '"translated":true' "$S266_TASKSET_LOG")"
exit $rc
