#!/bin/bash
# s289: build and run at a COMMIT, never the live tree (the lane rule "Build
# from a commit"). Adapted from s285's build-at-interp.sh, with one change
# s287's rig made after a shared worktree moved under a queued take: every
# commit gets its OWN worktree, $LANE/build-<repo>-<sha7>, created here before
# the lock is waited on and never moved. A warm target is cloned in (clonefile)
# from WARM when given; the fresh checkout's sources are newer, so cargo
# rebuilds the workspace crates and keeps the registry's.
# usage: [WARM=<dir>/target] at-commit.sh <repo> <rev> <cmd> [args...]
set -u
L=${LANE:-$LANE}
repo=$1; rev=$2; shift 2
SHA=$(git -C "$L/$repo" rev-parse --short=7 "$rev^{commit}") || exit 1
B=$L/build-$repo-$SHA
if [ ! -d "$B" ]; then
  git -C "$L/$repo" worktree add -q --detach "$B" "$SHA" || { echo "s289: worktree $B failed"; exit 1; }
  if [ -n "${WARM:-}" ] && [ -d "$WARM" ]; then
    cp -c -R "$WARM" "$B/target" || { echo "s289: warm copy from $WARM failed"; exit 1; }
    echo "s289: warm target cloned from $WARM"
  fi
fi
[ "$(git -C "$B" rev-parse --short=7 HEAD)" = "$SHA" ] || { echo "s289: $B is not at $SHA"; exit 1; }
[ -z "$(git -C "$B" status --porcelain --untracked-files=no)" ] || { echo "s289: $B is not clean"; exit 1; }
cd "$B" || exit 1
echo "s289: at $repo $SHA in $B"
exec "$L/rig/locked.sh" "$@"
