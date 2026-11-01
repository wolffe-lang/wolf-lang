#!/usr/bin/env bash
# s273: the detached gauntlet job (setsid -f): pid file, log, exit marker.
# usage: setsid -f bash gauntlet-job.sh <A|B> <head> >> records/gauntlet.log 2>&1
set -u
base=/home/mfwolffe/lanes/s273
blk=${1:?block}
expected=${2:?expected final head}
printf '%s\n' "$$" > "$base/records/gauntlet.pid"
printf 'S273_GAUNTLET_LAUNCH %s block=%s expected=%s pid=%s\n' "$(date -u +%FT%TZ)" "$blk" "$expected" "$$"
bash "$base/scripts/container.sh" "$blk" bash /lane/scripts/prep-inner.sh "$expected"
code=$?
printf 'END prep exit=%s %s\n' "$code" "$(date -u +%FT%TZ)"
if [ "$code" -eq 0 ]; then
  bash "$base/scripts/container.sh" "$blk" bash /lane/scripts/gauntlet-inner.sh "$expected"
  code=$?
fi
printf '%s\n' "$code" > "$base/records/gauntlet.exit"
printf 'S273_GAUNTLET_EXIT %s head=%s code=%s\n' "$(date -u +%FT%TZ)" "$expected" "$code"
exit "$code"
