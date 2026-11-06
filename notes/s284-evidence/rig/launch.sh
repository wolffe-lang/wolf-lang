#!/usr/bin/env bash
# s284: start a job detached, waiting on its block's three lock names, the
# block name outside, then H1-X.lock, then H1-X-light.lock (every s284
# take uses the block's light CPUs too, and a stall rate is timing), each
# with ONE long `flock -w 21600` (lane rules, 2026-11-01). Writes
# logs/<job>.pid (the waiter's pid), logs/<job>.log and, at the end,
# logs/<job>.exit. A TERM, INT or HUP writes 143, 130 or 129 (never 0).
# usage: launch.sh <A|B> <job> <script> [args...]   (runs rig/<script> <block> args...)
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
job=${2:?job}
script=${3:?script}
shift 3
case $blk in A | B) ;; *) echo "block must be A or B" >&2; exit 2 ;; esac
[ -f "$L/rig/$script" ] || { echo "no $L/rig/$script" >&2; exit 2; }
mkdir -p "$L/logs" "$L/tmp"
if [ -e "$L/logs/$job.log" ]; then echo "logs/$job.log exists: pick a new job name" >&2; exit 2; fi
setsid -f bash -c '
L=$1; blk=$2; job=$3; script=$4; shift 4
export TMPDIR=$L/tmp
echo $$ > "$L/logs/$job.pid"
child=
stop() { echo "$1" > "$L/logs/$job.exit"; echo "JOB_STOPPED signal-exit=$1 $(date -u +%FT%TZ)" >> "$L/logs/$job.log"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap "stop 143" TERM; trap "stop 130" INT; trap "stop 129" HUP
echo "QUEUED $(date -u +%FT%TZ) job=$job on h1-block-$blk.lock, h1-$blk.lock, h1-$blk-light.lock; waiters A=$(ps -eo args | grep -c "[f]lock .*h1-block-A.lock") B=$(ps -eo args | grep -c "[f]lock .*h1-block-B.lock")" > "$L/logs/$job.log"
flock -w 21600 /home/mfwolffe/lanes/h1-block-$blk.lock flock -w 21600 /home/mfwolffe/lanes/h1-$blk.lock flock -w 21600 /home/mfwolffe/lanes/h1-$blk-light.lock bash "$L/rig/$script" "$blk" "$@" >> "$L/logs/$job.log" 2>&1 &
child=$!
wait "$child"
rc=$?
echo "JOB_END exit=$rc $(date -u +%FT%TZ)" >> "$L/logs/$job.log"
echo $rc > "$L/logs/$job.exit"
' launch "$L" "$blk" "$job" "$script" "$@" </dev/null >/dev/null 2>&1
echo "launched $job on block $blk (pid in logs/$job.pid)"
