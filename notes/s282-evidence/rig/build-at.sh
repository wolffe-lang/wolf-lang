#!/bin/bash
# s282: build and run at a COMMIT, never the live tree (the lane rule
# "Build from a commit"). Adapted by copy from s278's build-at.sh: ONE build
# worktree (~/lanes/s282/build) moved, detached, to the sha before the lock is
# waited on, so a waiting runner builds that commit and nothing else; its
# target/ kept warm between builds.
# usage: build-at.sh <rev> <cmd> [args...]
set -u
L=$LANE
B=$L/build
SHA=$(git -C $L/wolf-lang rev-parse --short=8 "$1") || exit 1
shift
git -C "$B" checkout -q --detach "$SHA" || { echo "s282: checkout of $SHA in $B failed"; exit 1; }
[ "$(git -C "$B" rev-parse --short=8 HEAD)" = "$SHA" ] || { echo "s282: $B is not at $SHA"; exit 1; }
[ -z "$(git -C "$B" status --porcelain --untracked-files=no)" ] || { echo "s282: $B is not clean"; exit 1; }
cd "$B" || exit 1
echo "s282: building at $SHA in $B"
exec $L/rig/locked.sh "$@"
