#!/bin/bash
# s272: build and run at a COMMIT, never the live tree (the-lane-rules-1031.md,
# "Build from a commit"). Adapted by copy from s273's build-at.sh. The tree is
# ONE build worktree (~/lanes/s272/build) moved, detached, to the sha before the
# lock is waited on (so a waiting runner builds that commit and nothing else),
# its target/ kept warm between builds (nomad's disk is at 98%: one target only).
# usage: build-at.sh <rev> <cmd> [args...]
set -u
L=/Users/mfwolffe/lanes/s272
B=$L/build
SHA=$(git -C $L/wolf-lang rev-parse --short=8 "$1") || exit 1
shift
if [ ! -d "$B" ]; then
  git -C $L/wolf-lang worktree add --detach "$B" "$SHA" >/dev/null || exit 1
fi
git -C "$B" checkout -q --detach "$SHA" || { echo "s272: checkout of $SHA in $B failed"; exit 1; }
[ "$(git -C "$B" rev-parse --short=8 HEAD)" = "$SHA" ] || { echo "s272: $B is not at $SHA"; exit 1; }
[ -z "$(git -C "$B" status --porcelain --untracked-files=no)" ] || { echo "s272: $B is not clean"; exit 1; }
# Take turns: at least 60 s since this lane last released the lock.
if [ -f $L/last-release ]; then
  while [ $(( $(date +%s) - $(cat $L/last-release) )) -lt 60 ]; do
    echo "s272: $(date +%H:%M:%S) released under 60 s ago; waiting my turn"
    sleep 10
  done
fi
cd "$B" || exit 1
echo "s272: building at $SHA in $B"
exec $L/locked.sh "$@"
