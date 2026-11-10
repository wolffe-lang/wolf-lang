#!/usr/bin/env bash
# s292: one waiter at a time on the block: the helper take first; only when both
# of its records end green, launch.sh (the gauntlet, then the plant).
#   chain.sh TRUNK_WORK TRUNK_SHA HEAD_WORK HEAD_SHA PLANT PLANT_BRANCH ROW...
set -uo pipefail
base=${LANE:?}
tw=$1; ts=$2; hw=$3; hs=$4; plant=$5; branch=$6; shift 6
bash "$base/rig/helper-take.sh" run "helper-${hs:0:8}" "$tw" "$ts" "$hw" "$hs"
rc=$?
for s in "$ts" "$hs"; do tail -n 1 "$base/records/helper-$s.record" 2>/dev/null; done
if [ "$rc" -ne 0 ]; then printf 'CHAIN_STOPPED helper exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"; exit "$rc"; fi
printf 'CHAIN_LAUNCH %s\n' "$(date -u +%FT%TZ)"
exec bash "$base/rig/launch.sh" "$hs" "$plant" "$branch" "$@"
