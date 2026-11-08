#!/bin/bash
# s287: red takes at the rows' commits on M1, one after another (build-at.sh,
# each in its own worktree): every lanes test, the corpus, conformance and
# the anchors gate. Each take's log is records/red-<sha>.log.
set -u
for sha in "$@"; do
  LANE=$LANE FULL=1 "$LANE/rig/build-at.sh" "$sha" "$LANE/rig/rows.sh" > "$LANE/records/red-$sha.log" 2>&1
  echo "red $sha: exit=$? $(date -u +%T)"
done
