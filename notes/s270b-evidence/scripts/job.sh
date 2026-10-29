#!/usr/bin/env bash
# s270b: one container step run detached with a pid file, a log and an exit marker.
# usage (launch): setsid -f bash job.sh <name> <inner-script-under-/lane/scripts> [args...]
set -uo pipefail
base=/home/mfwolffe/lanes/s270b
name=$1; shift
echo $$ > "$base/records/$name.pid"
rm -f "$base/records/$name.exit"
printf 's270b JOB %s pid %s start %s head %s\n' "$name" "$$" "$(date -u +%FT%TZ)" "$(git -C "$base/wolf-lang" rev-parse HEAD)"
bash "$base/scripts/container.sh" "$name" "$@"
rc=$?
echo "$rc" > "$base/records/$name.exit"
printf 's270b JOB %s end %s exit %s\n' "$name" "$(date -u +%FT%TZ)" "$rc"
exit "$rc"
