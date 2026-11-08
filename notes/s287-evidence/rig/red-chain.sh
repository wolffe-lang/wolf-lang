#!/bin/bash
# s287: the red takes at the rows' commits (the base's code for each item),
# one after another on M2: every lanes test, the corpus, conformance and
# the anchors gate. Each take's log is records/red-<sha>.log.
set -u
for sha in "$@"; do
  LANE=$LANE FULL=1 "$LANE/rig/m2-run.sh" "$sha" rows.sh > "$LANE/records/red-$sha.log" 2>&1
  echo "red $sha: m2-run exit=$? $(date -u +%T)"
done
