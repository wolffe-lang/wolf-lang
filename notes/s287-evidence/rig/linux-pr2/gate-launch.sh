#!/usr/bin/env bash
# s287, the second change, on H2: run launch.sh only once the first
# change's takes are all done (its plant's exit marker, or its green's marker
# with a nonzero exit, when no plant take follows), so the second change never
# takes the slot's locks between the first change's takes (flock wakes its
# waiters in no fixed order). Holds no lock while it waits; prints every 5 min.
#   gate-launch.sh FIRST_GREEN_MARKER FIRST_PLANT_MARKER LAUNCH_ARGS...
set -u
green=$1; plant=$2; shift 2
base=$LANE/pr2
n=0
while :; do
  if [ -s "$plant" ]; then why="plant=$(cat "$plant")"; break; fi
  if [ -s "$green" ] && [ "$(cat "$green")" != 0 ]; then why="green=$(cat "$green")"; break; fi
  if [ $((n % 10)) -eq 0 ]; then printf 'GATE_WAIT %s for %s\n' "$(date -u +%FT%TZ)" "$plant"; fi
  n=$((n + 1)); sleep 30
done
printf 'GATE_OPEN %s %s\n' "$(date -u +%FT%TZ)" "$why"
exec bash "$base/rig/launch.sh" "$@"
