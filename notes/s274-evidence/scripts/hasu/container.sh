#!/usr/bin/env bash
# s274: one batch in the rootless container on block A or B. Both lock names are
# held for the whole batch, the block name outside, each taken by ONE long
# `flock -w 21600` (the-lane-rules-1031.md, added 2026-11-01: never a short-wait
# retry loop). Shape copied from s273's container.sh (hasu-env README: taskset
# inside the container, --memory 12g, the taskset shim, a foreign pid 1 with
# bash kept as pid 1 by `; exit $?`).
# usage: container.sh <A|B> <cmd> [args...]   (cmd runs inside, as uid 1000)
set -euo pipefail
base=/home/mfwolffe/lanes/s274
blk=${1:?block A or B}
shift
case "$blk" in
  A|B) ;;
  *) echo "s274: block must be A or B" >&2; exit 2 ;;
esac
printf 's274 queued for block %s (hasu-block-%s.lock, then hasu-%s.lock) %s\n' "$blk" "$blk" "$blk" "$(date -u +%FT%TZ)"
exec flock -E 75 -w 21600 "/home/mfwolffe/lanes/hasu-block-$blk.lock" \
  flock -E 75 -w 21600 "/home/mfwolffe/lanes/hasu-$blk.lock" \
  bash "$base/scripts/run-ctr.sh" "$blk" "$@"
