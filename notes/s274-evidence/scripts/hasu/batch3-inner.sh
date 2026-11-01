#!/usr/bin/env bash
# s274 batch 3: the floor experiment (item 4), then the gauntlet and the proof at the
# final test file (proof-inner.sh, unchanged), then one plant run and one head run
# with their whole output (the plant's red by name). Inside the container, one
# block held. Exits with the gauntlet's code.
# usage: batch3-inner.sh <trunk sha> <head sha> <plant sha> [HEAD_N] [OTHER_N] [PLANT_EVERY]
set -uo pipefail
printf '== floor %s\n' "$(date -u +%FT%TZ)"
bash /lane/scripts/floor-inner.sh
printf 'S274_FLOOR_DONE exit=%s %s\n' "$?" "$(date -u +%FT%TZ)"
bash /lane/scripts/proof-inner.sh "$@"
code=$?
for who in "plant ${3:0:8}" "head ${2:0:8}"; do
  set -- $who
  printf '== raw %s %s\n' "$1" "$(date -u +%FT%TZ)"
  /usr/bin/taskset.real -c "$S274_FOUR" "/lane/bins/$1-$2/region_copyout_lanes" --exact a_loop_keeping_a_scalar_is_flat --nocapture 2>&1 | sed "s/^/S274_RAW $1: /"
  printf 'S274_RAW_END %s exit=%s\n' "$1" "${PIPESTATUS[0]}"
done
exit "$code"
