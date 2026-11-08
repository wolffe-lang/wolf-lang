#!/usr/bin/env bash
# s287, the second change, on H2: run launch.sh only once the first
# change's takes are all done and green: its plant's exit marker reading 0, so
# the second change never takes the slot's locks between the first change's
# takes (flock wakes its waiters in no fixed order) or ahead of a requeue of
# them. A nonzero marker from either of the first change's takes closes the
# gate without launching (GATE_CLOSED). Holds no lock while it waits; prints
# every 5 min.
#   gate-launch.sh FIRST_GREEN_MARKER FIRST_PLANT_MARKER LAUNCH_ARGS...
set -u
green=$1; plant=$2; shift 2
base=$LANE/pr2
n=0
while :; do
  if [ -s "$plant" ]; then
    if [ "$(cat "$plant")" = 0 ]; then break; fi
    printf 'GATE_CLOSED %s plant=%s\n' "$(date -u +%FT%TZ)" "$(cat "$plant")"; exit 3
  fi
  if [ -s "$green" ] && [ "$(cat "$green")" != 0 ]; then
    printf 'GATE_CLOSED %s green=%s\n' "$(date -u +%FT%TZ)" "$(cat "$green")"; exit 3
  fi
  if [ $((n % 10)) -eq 0 ]; then printf 'GATE_WAIT %s for %s\n' "$(date -u +%FT%TZ)" "$plant"; fi
  n=$((n + 1)); sleep 30
done
printf 'GATE_OPEN %s plant=0\n' "$(date -u +%FT%TZ)"
exec bash "$base/rig/launch.sh" "$@"
