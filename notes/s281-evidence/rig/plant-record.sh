#!/bin/bash
# s281: the plant record, in the plant worktree on its branch (one commit, one
# file, one hunk on the green head), behind the Mac lock (one take): the org's
# gauntlet recorder (a copy) with --base and --plant, red by name as the four
# test rows that were red at the base (adapted by copy from s278's).
# usage: plant-record.sh <green-head-sha>
set -u
L=/Users/mfwolffe/lanes/s281
BASE=${1:?the green head}
cd "$L/plantwt" || exit 2
PLANT=$(git rev-parse HEAD)
TMPDIR=$L/tmp exec bash "$L/rig/gauntlet-record.sh" --output "$L/records/plant-mac-$PLANT.log" --base "$BASE" \
  --plant a_bare_variant_nested_in_a_payload_tests_its_variant a_bare_variant_in_either_payload_slot_tests_that_slot \
  a_guarded_arm_tests_its_nested_bare_variant_first a_bare_variant_in_a_row_tags_payload_tests_its_variant \
  -- bash "$L/rig/mac-plant-host.sh" "$PLANT"
