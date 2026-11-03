#!/bin/bash
# s278: the plant record, in the plant worktree on its branch (one commit, one
# file, one hunk on the green head), behind the Mac lock (one take): the org's
# gauntlet recorder (a copy) with --base and --plant, the chunk `plant`
# (plant-head, plant-build, plant-corpus), red by name as
# `FAIL corpus/os/signal_action_rows.lu`.
# usage: plant-record.sh <green-head-sha>
set -u
L=/Users/mfwolffe/lanes/s278
BASE=${1:?the green head}
cd "$L/plantwt" || exit 2
PLANT=$(git rev-parse HEAD)
TMPDIR=$L/tmp exec bash "$L/rig/gauntlet-record.sh" --output "$L/records/plant-mac-$PLANT.log" --base "$BASE" --plant corpus/os/signal_action_rows.lu -- bash "$L/rig/mac-gauntlet-host.sh" "$PLANT" plant
