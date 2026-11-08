#!/usr/bin/env bash
# s287, the second change, on H2 (base $LANE/pr2; adapted by copy from s285's): fresh GitHub clones of the head (branch GREEN_BRANCH, s287b by
# default) and of the plant
# branch, each checked against its sha; then the green gauntlet (full-job.sh,
# four takes in slot A) and the plant job (plant-job.sh, which holds no lock
# until the green's marker says 0), both detached with a pid file, a log and an
# exit marker. Every git here passes commit.gpgsign=false. This runs on the
# host, whose git is not the container's /usr/bin/git that tool-bin/git wraps.
#   launch.sh HEAD PLANT PLANT_BRANCH ROW...
set -euo pipefail
base=$LANE/pr2
head=$1; plant=$2; branch=$3; shift 3
export TMPDIR=$base/tmp
g() { git -c commit.gpgsign=false "$@"; }
cd "$base"
work=final-${head:0:8}
pwork=plant-${plant:0:8}
g clone -q --branch "${GREEN_BRANCH:-s287b}" https://github.com/wolffe-lang/wolf-lang.git "$work"
test "$(g -C "$work" rev-parse HEAD)" = "$head"
g clone -q --branch "$branch" https://github.com/wolffe-lang/wolf-lang.git "$pwork"
test "$(g -C "$pwork" rev-parse HEAD)" = "$plant"
test "$(g -C "$pwork" rev-parse HEAD^)" = "$head"
printf 'LAUNCH %s head=%s plant=%s branch=%s host=H2 rows=%s\n' "$(date -u +%FT%TZ)" "$head" "$plant" "$branch" "$*"
setsid -f bash "$base/rig/full-job.sh" final "$head" "$work" > "$base/records/full-final-job.log" 2>&1 < /dev/null
setsid -f bash "$base/rig/plant-job.sh" "$head" "$plant" "$pwork" "$base/records/full-final-${head:0:8}.exit" "$@" > "$base/records/plant-job.log" 2>&1 < /dev/null
echo "launched"
