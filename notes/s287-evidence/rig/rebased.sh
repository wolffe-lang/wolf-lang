#!/bin/bash
# s287: the checks at a rebased head, in one take: the lane's checks
# (checks.sh), with EXTRA set the adjusted fixture's suites (extra-pr2.sh),
# then every corpus-walking snapshot suite with no update (snaps.sh).
set -u
L=$LANE
bash "$L/rig/checks.sh"
[ -n "${EXTRA:-}" ] && bash "$L/rig/extra-pr2.sh"
bash "$L/rig/snaps.sh"
echo "== rebased done $(date -u +%FT%TZ)"
