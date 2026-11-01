#!/usr/bin/env bash
# s274: a detached batch (started with `setsid -f`): pid file, log, exit marker.
# usage: setsid -f bash job.sh <run> <A|B> <inner script> [args...] >> records/<run>.log 2>&1
set -u
base=/home/mfwolffe/lanes/s274
run=${1:?run name}
blk=${2:?block}
shift 2
printf '%s\n' "$$" > "$base/records/$run.pid"
rm -f "$base/records/$run.exit"
printf 'S274_LAUNCH %s run=%s block=%s pid=%s args=%s\n' "$(date -u +%FT%TZ)" "$run" "$blk" "$$" "$*"
bash "$base/scripts/container.sh" "$blk" "$@"
code=$?
printf '%s\n' "$code" > "$base/records/$run.exit"
printf 'S274_EXIT %s run=%s code=%s\n' "$(date -u +%FT%TZ)" "$run" "$code"
exit "$code"
