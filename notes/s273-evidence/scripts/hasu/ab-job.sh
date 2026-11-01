#!/usr/bin/env bash
# s273: a detached A/B job: pid file, log, exit marker. usage: ab-job.sh <run> <bin> <name> <grep>
set -u
base=/home/mfwolffe/lanes/s273
run=${1:?run}
shift
printf '%s\n' "$$" > "$base/records/$run.pid"
printf 'S273_AB_LAUNCH %s pid=%s %s\n' "$(date -u +%FT%TZ)" "$$" "$*"
bash "$base/scripts/container.sh" A bash /lane/scripts/ab-inner.sh "$@"
code=$?
printf '%s\n' "$code" > "$base/records/$run.exit"
printf 'S273_AB_EXIT %s code=%s\n' "$(date -u +%FT%TZ)" "$code"
exit "$code"
