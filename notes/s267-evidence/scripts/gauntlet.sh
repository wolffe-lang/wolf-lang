#!/bin/bash
# s267's gauntlet: `cargo xtask ci` at the checkout's head in ctr-g. The
# verdict is the command's own exit status, written to evidence/gauntlet.exit.
set -u
L=/home/mfwolffe/lanes/s267
cd $L/wolf-lang
echo "s267 gauntlet head=$(git rev-parse HEAD) host=$(hostname) start=$(date -u +%FT%TZ)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn|NoNewPrivs' /proc/self/status
printf 'pids.max='; cat /sys/fs/cgroup/pids.max
printf 'nproc soft=%s hard=%s\n' "$(ulimit -Su)" "$(ulimit -Hu)"
p1=$(awk '/^Uid:/{print $2}' /proc/1/status)
echo "pid 1 uid $p1 (this command runs as $(id -u))"
[ "$p1" != "$(id -u)" ] || { echo "FAIL: pid 1 is not foreign"; exit 3; }
ls -la "$WOLF_STATIC_ENTRY_MUSL_RT" "$WOLF_STATIC_ENTRY_SYSROOT"
: > "$S267_TASKSET_LOG"
rm -f $L/evidence/gauntlet.exit
cargo run --quiet -j4 --package xtask -- ci > $L/evidence/gauntlet.log 2>&1
rc=$?
echo "$rc" > $L/evidence/gauntlet.exit
echo "s267 gauntlet exit=$rc end=$(date -u +%FT%TZ)"
echo "taskset requests: $(wc -l < "$S267_TASKSET_LOG"), translated: $(grep -c '"translated":true' "$S267_TASKSET_LOG")"
exit $rc
