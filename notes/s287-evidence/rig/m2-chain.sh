#!/bin/bash
# s287: several jobs at one commit on M2, one after another (never two
# m2-run jobs at once). usage: m2-chain.sh <rev> <tag:job.sh>...
# Each job's log is records/pr2-<tag>-<sha>.log.
set -u
SHA=$(git -C "$LANE/wolf-lang" rev-parse --short=8 "$1") || exit 1; shift
for tj in "$@"; do
  LANE=$LANE "$LANE/rig/m2-run.sh" "$SHA" "${tj#*:}" > "$LANE/records/pr2-${tj%%:*}-$SHA.log" 2>&1
  echo "m2 ${tj%%:*} $SHA: m2-run exit=$? $(date -u +%T)"
done
echo "m2-chain done $(date -u +%T)"
