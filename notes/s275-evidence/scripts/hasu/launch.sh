#!/usr/bin/env bash
# s275: start a job detached, waiting on both of a block's lock names, the
# block name outside, each with ONE long `flock -w 21600` (lane rules,
# 2026-11-01). Writes logs/<job>.pid (the waiter's pid), logs/<job>.log and,
# at the end, logs/<job>.exit.
# usage: launch.sh <A|B> <job>    (runs scripts/<job>.sh <block>)
L=/home/mfwolffe/lanes/s275
blk=${1:?block}
job=${2:?job}
case $blk in A | B) ;; *) echo "block must be A or B" >&2; exit 2 ;; esac
[ -f "$L/scripts/$job.sh" ] || { echo "no $L/scripts/$job.sh" >&2; exit 2; }
setsid -f bash -c '
L=$1; blk=$2; job=$3
echo $$ > "$L/logs/$job.pid"
echo "queued $(date -Iseconds) on hasu-block-$blk.lock then hasu-$blk.lock" > "$L/logs/$job.log"
flock -w 21600 /home/mfwolffe/lanes/hasu-block-$blk.lock flock -w 21600 /home/mfwolffe/lanes/hasu-$blk.lock bash "$L/scripts/$job.sh" "$blk" >> "$L/logs/$job.log" 2>&1
echo $? > "$L/logs/$job.exit"
' launch "$L" "$blk" "$job" </dev/null >/dev/null 2>&1
echo "launched $job on block $blk (its pid is written to logs/$job.pid)"
