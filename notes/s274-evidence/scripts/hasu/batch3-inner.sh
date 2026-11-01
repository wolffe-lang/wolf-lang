#!/usr/bin/env bash
# s274 batch 3: the floor experiment (item 4), then the gauntlet and the proof at the
# final test file (proof-inner.sh, unchanged). Inside the container, one block held.
# usage: batch3-inner.sh <trunk sha> <head sha> <plant sha> [HEAD_N] [OTHER_N] [PLANT_EVERY]
set -uo pipefail
printf '== floor %s\n' "$(date -u +%FT%TZ)"
bash /lane/scripts/floor-inner.sh
printf 'S274_FLOOR_DONE exit=%s %s\n' "$?" "$(date -u +%FT%TZ)"
exec bash /lane/scripts/proof-inner.sh "$@"
