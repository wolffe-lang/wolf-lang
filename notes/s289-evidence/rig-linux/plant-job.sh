#!/usr/bin/env bash
# Waits (holding no lock) for the green gauntlet's exit marker; only on 0 does
# it queue the plant take, in the same slot. Prints at least every five minutes.
#   plant-job.sh GREEN PLANT WORK MARKER ROW...
set -uo pipefail
base=$LANE
green=$1; plant=$2; work=$3; marker=$4; shift 4
label=plant-${plant:0:8}
out=$base/records/$label.exit
export TMPDIR=$base/tmp
printf '%s\n' "$$" > "$base/records/$label.pid"
child=
on_signal() { printf '%s\n' "$1" > "$out"; printf 'PLANT_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
printf 'PLANT_JOB_START %s green=%s plant=%s host=H2 rows=%s\n' "$(date -u +%FT%TZ)" "$green" "$plant" "$*"
n=0
while ! test -s "$marker"; do
  if [ $((n % 8)) -eq 0 ]; then printf 'PLANT_WAIT %s for %s\n' "$(date -u +%FT%TZ)" "$marker"; fi
  n=$((n + 1))
  sleep 30 & child=$!; wait "$child"; child=
done
green_rc=$(cat "$marker")
if [ "$green_rc" != 0 ]; then
  printf 'PLANT_NOT_RUN green gauntlet exit=%s %s\n' "$green_rc" "$(date -u +%FT%TZ)"
  printf '125\n' > "$out"
  exit 125
fi
bash "$base/rig/plant-take.sh" run "$label" "$work" "$green" "$plant" "$@" &
child=$!
wait "$child"
rc=$?
printf 'PLANT_JOB_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$out"
exit "$rc"
