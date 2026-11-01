#!/bin/bash
# s273: build and run at a COMMIT, never the live tree (the-lane-rules-1031.md,
# "Build from a commit", 2026-10-31 20:40). The tree is a worktree pinned to
# the sha (~/lanes/s273/build-<sha>), its target/ cloned (APFS clonefile) from
# the last retired build's target so registry crates stay built; the command
# runs inside it behind the Mac build lock (locked.sh).
# usage: build-at.sh <rev> <cmd> [args...]
#        build-at.sh --retire <rev>   (keep its target/ as the next seed, remove
#                                      the worktree by its exact path)
set -u
L=/Users/mfwolffe/lanes/s273
SEED=$L/target-seed
if [ "$1" = "--retire" ]; then
  SHA=$(git -C $L/wolf-lang rev-parse --short=8 "$2") || exit 1
  B=$L/build-$SHA
  [ -d "$B" ] || { echo "s273: no $B"; exit 1; }
  if [ -d "$B/target" ]; then
    rm -rf "$SEED.old"
    [ -d "$SEED" ] && mv "$SEED" "$SEED.old"
    mv "$B/target" "$SEED" && rm -rf "$SEED.old"
  fi
  git -C $L/wolf-lang worktree remove "$B" && echo "s273: retired $B"
  exit $?
fi
SHA=$(git -C $L/wolf-lang rev-parse --short=8 "$1") || exit 1
shift
B=$L/build-$SHA
if [ ! -d "$B" ]; then
  git -C $L/wolf-lang worktree add --detach "$B" "$SHA" >/dev/null || exit 1
  if [ -d "$SEED" ]; then cp -cR "$SEED" "$B/target"; fi
fi
[ "$(git -C "$B" rev-parse --short=8 HEAD)" = "$SHA" ] || { echo "s273: $B is not at $SHA"; exit 1; }
cd "$B" || exit 1
echo "s273: building at $SHA in $B"
exec $L/locked.sh "$@"
