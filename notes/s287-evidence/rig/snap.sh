#!/bin/bash
# s287: a commit object of the live tree (tracked and untracked files, not
# ignored ones), parented on HEAD, without moving any branch: the "build from
# a commit" rule for work in progress. Prints its sha; build-at.sh builds it.
set -eu
cd "$LANE/${TREE:-wolf-lang}"
IDX=$LANE/tmp/snap-index.$$
cp "$(git rev-parse --git-path index)" "$IDX"
GIT_INDEX_FILE=$IDX git add -A
TREE=$(GIT_INDEX_FILE=$IDX git write-tree)
rm -f "$IDX"
git commit-tree "$TREE" -p HEAD -m "s287 snapshot of the working tree"
