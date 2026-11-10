#!/usr/bin/env bash
# s292 (adapted by copy from s290's): fresh GitHub clones of the head (branch
# GREEN_BRANCH, s292 by default) and of the plant branch, each checked against
# its sha; then the green gauntlet (full-job.sh, four takes on one block) and
# the plant job (plant-job.sh, which holds no lock until the green's marker
# says 0), both detached with nohup (no new session: the rig never calls
# setsid), each with a pid file, a log and an exit marker. Every git here
# passes commit.gpgsign=false.
#   launch.sh HEAD PLANT PLANT_BRANCH ROW...   (PLANT_TEST, PLANT_CONTROLS set)
set -euo pipefail
base=${LANE:?}
head=$1; plant=$2; branch=$3; shift 3
export TMPDIR=$base/tmp
g() { git -c commit.gpgsign=false "$@"; }
cd "$base"
work=final-${head:0:8}
pwork=plant-${plant:0:8}
g clone -q --branch "${GREEN_BRANCH:-s292}" https://github.com/wolffe-lang/wolf-lang.git "$work"
test "$(g -C "$work" rev-parse HEAD)" = "$head"
g clone -q --branch "$branch" https://github.com/wolffe-lang/wolf-lang.git "$pwork"
test "$(g -C "$pwork" rev-parse HEAD)" = "$plant"
test "$(g -C "$pwork" rev-parse HEAD^)" = "$head"
printf 'LAUNCH %s head=%s plant=%s branch=%s rows=%s\n' "$(date -u +%FT%TZ)" "$head" "$plant" "$branch" "$*"
nohup bash "$base/rig/full-job.sh" final "$head" "$work" > "$base/records/full-final-${head:0:8}-job.log" 2>&1 < /dev/null &
printf 'full-job pid %s\n' "$!"
nohup bash "$base/rig/plant-job.sh" "$head" "$plant" "$pwork" "$base/records/full-final-${head:0:8}.exit" "$@" > "$base/records/plant-${plant:0:8}-job.log" 2>&1 < /dev/null &
printf 'plant-job pid %s\n' "$!"
echo "launched"
