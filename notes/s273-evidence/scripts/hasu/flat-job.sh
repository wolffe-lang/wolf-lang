#!/usr/bin/env bash
# s273: the detached #631 comparison job: pid file, log, exit marker.
set -u
base=/home/mfwolffe/lanes/s273
printf '%s\n' "$$" > "$base/records/flat.pid"
printf 'S273_FLAT_LAUNCH %s pid=%s\n' "$(date -u +%FT%TZ)" "$$"
bash "$base/scripts/container.sh" A bash /lane/scripts/flat-inner.sh
code=$?
printf '%s\n' "$code" > "$base/records/flat.exit"
printf 'S273_FLAT_EXIT %s code=%s\n' "$(date -u +%FT%TZ)" "$code"
exit "$code"
