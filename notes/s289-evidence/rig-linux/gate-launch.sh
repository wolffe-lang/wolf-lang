#!/usr/bin/env bash
# s289 on H2 (adapted by copy from s287's second change): run launch.sh only once the first
# change's takes are all done and right: its green record at exit 0 and its
# plant record red by name, both verified by the recorder's own --verify (the
# plant record's every named row must FAIL, and its controls pass), so the
# second change never takes the slot's locks between the first change's takes
# (flock wakes its waiters in no fixed order) or ahead of a requeue of them.
# A plant marker of 0 (a plant that did not go red), a nonzero green marker,
# or a record that does not verify closes the gate without launching
# (GATE_CLOSED). Holds no lock while it waits; prints every 5 min.
#   gate-launch.sh GREEN_MARKER PLANT_MARKER GREEN_RECORD PLANT_RECORD REPO GREEN_HEAD LAUNCH_ARGS...
# REPO is a clone holding the green head and the plant branch (the plant take's own clone).
set -u
green=$1; plant=$2; grec=$3; prec=$4; repo=$5; ghead=$6; shift 6
base=$LANE
n=0
while :; do
  if [ -s "$plant" ]; then break; fi
  if [ -s "$green" ] && [ "$(cat "$green")" != 0 ]; then
    printf 'GATE_CLOSED %s green=%s\n' "$(date -u +%FT%TZ)" "$(cat "$green")"; exit 3
  fi
  if [ $((n % 10)) -eq 0 ]; then printf 'GATE_WAIT %s for %s\n' "$(date -u +%FT%TZ)" "$plant"; fi
  n=$((n + 1)); sleep 30
done
if [ "$(cat "$green" 2>/dev/null)" != 0 ]; then
  printf 'GATE_CLOSED %s green=%s\n' "$(date -u +%FT%TZ)" "$(cat "$green" 2>/dev/null)"; exit 3
fi
if [ "$(cat "$plant")" = 0 ]; then
  printf 'GATE_CLOSED %s plant=0: the plant did not go red\n' "$(date -u +%FT%TZ)"; exit 3
fi
if ! bash "$base/rig/gauntlet-record.sh" --verify "$grec" "$repo" "$ghead" wolf-lang "$prec"; then
  printf 'GATE_CLOSED %s plant=%s: the records do not verify\n' "$(date -u +%FT%TZ)" "$(cat "$plant")"; exit 3
fi
printf 'GATE_OPEN %s green=0 plant=%s, both records verified, the plant red by name\n' "$(date -u +%FT%TZ)" "$(cat "$plant")"
exec bash "$base/rig/launch.sh" "$@"
