#!/bin/bash
# s287: build the head's binary (base-build.sh, behind the lock) and run the
# census on it over the head's own corpus and every org checkout. The corpus
# is read from a worktree of the head made for this job alone (the first
# version shared one head-tree between jobs).
# usage: head-census.sh <sha> <phase>
set -u
SHA=$(git -C "$LANE/wolf-lang" rev-parse --short=8 "$1"); PHASE=$2
"$LANE/rig/build-at.sh" "$SHA" "$LANE/rig/base-build.sh" || exit 1
T=$LANE/census-tree-$SHA-$$
git -C "$LANE/wolf-lang" worktree add -q --detach "$T" "$SHA" || exit 1
trap 'git -C "$LANE/wolf-lang" worktree remove --force "$T" 2>/dev/null' EXIT
sleep 61
CORPUS=$T/corpus "$LANE/rig/census-job.sh" "head-$SHA-$PHASE" "$LANE/bin-$SHA/wolf" "$PHASE"
