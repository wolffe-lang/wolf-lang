#!/bin/bash
# s289, third change: at a commit worktree behind the Air's lock, build the
# compiler (wolf-build.sh) and run the min probes with that binary.
set -u
bash "$LANE/rig/wolf-build.sh" || exit 1
sha=$(git rev-parse --short=8 HEAD)
bash "$LANE/rig/min-probes.sh" "$LANE/bin-$sha/wolf" "min-${LABEL:-base}-$sha"
