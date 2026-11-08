#!/usr/bin/env bash
# s286 on H1: a fresh GitHub clone of branch s286 at HEAD (checked against
# its sha), then the green gauntlet (full-job.sh: four takes on one block,
# each its own lock take), detached with a pid file, a log and an exit
# marker. Every git here passes commit.gpgsign=false.
#   launch.sh HEAD BLOCK
set -euo pipefail
base=$LANE
head=$1; block=$2
export TMPDIR=$base/tmp
g() { git -c commit.gpgsign=false "$@"; }
cd "$base"
work=final-${head:0:8}
g clone -q --branch s286 https://github.com/wolffe-lang/wolf-lang.git "$work"
test "$(g -C "$work" rev-parse HEAD)" = "$head"
printf 'LAUNCH %s head=%s block=%s\n' "$(date -u +%FT%TZ)" "$head" "$block"
setsid -f bash "$base/rig/full-job.sh" final "$head" "$work" "$block" > "$base/records/full-final-${head:0:8}-job.log" 2>&1 < /dev/null
echo "launched"
