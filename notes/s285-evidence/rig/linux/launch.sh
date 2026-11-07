#!/usr/bin/env bash
# s285 on H1: fresh GitHub clones of the head (branch s285) and of the plant
# branch, each checked against its sha; then the green gauntlet (full-job.sh,
# four takes on one block) and the plant job (plant-job.sh, which holds no lock
# until the green's marker says 0), both detached with a pid file, a log and an
# exit marker. Every git here passes commit.gpgsign=false (tool-bin/git).
#   launch.sh HEAD PLANT PLANT_BRANCH BLOCK ROW...
set -euo pipefail
base=$LANE
head=$1; plant=$2; branch=$3; block=$4; shift 4
export PATH=$base/tool-bin:$PATH TMPDIR=$base/tmp
cd "$base"
work=final-${head:0:8}
pwork=plant-${plant:0:8}
git clone -q --branch s285 https://github.com/wolffe-lang/wolf-lang.git "$work"
test "$(git -C "$work" rev-parse HEAD)" = "$head"
git clone -q --branch "$branch" https://github.com/wolffe-lang/wolf-lang.git "$pwork"
test "$(git -C "$pwork" rev-parse HEAD)" = "$plant"
test "$(git -C "$pwork" rev-parse HEAD^)" = "$head"
printf 'LAUNCH %s head=%s plant=%s branch=%s block=%s rows=%s\n' "$(date -u +%FT%TZ)" "$head" "$plant" "$branch" "$block" "$*"
setsid -f bash "$base/rig/full-job.sh" final "$head" "$work" "$block" > "$base/records/full-final-job.log" 2>&1 < /dev/null
setsid -f bash "$base/rig/plant-job.sh" "$block" "$head" "$plant" "$pwork" "$base/records/full-final-${head:0:8}.exit" "$@" > "$base/records/plant-job.log" 2>&1 < /dev/null
echo "launched"
