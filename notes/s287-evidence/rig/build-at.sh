#!/bin/bash
# s287: build and run at a COMMIT, never the live tree (the lane rule
# "Build from a commit"). Each job gets its OWN worktree, build-<sha>-<pid>,
# made before the lock is waited on and removed by its exact path after, so
# a waiting runner builds that commit and nothing else. (The first version,
# adapted from s285's, moved one shared worktree; a second queued job moved
# it under the first, which then built the wrong commit: the take queued as
# faf84cae built aa3c9baa, its own head line says so.)
# usage: build-at.sh <rev> <cmd> [args...]
set -u
L=$LANE
SHA=$(git -C $L/wolf-lang rev-parse --short=8 "$1") || exit 1
shift
# A relative command path is the caller's: make it absolute before the cd.
if [ -e "$1" ]; then set -- "$(cd "$(dirname "$1")" && pwd)/$(basename "$1")" "${@:2}"; fi
B=$L/build-$SHA-$$
git -C $L/wolf-lang worktree add -q --detach "$B" "$SHA" || { echo "s287: worktree for $SHA failed"; exit 1; }
cleanup() { cd "$L"; git -C "$L/wolf-lang" worktree remove --force "$B" 2>/dev/null || true; }
trap cleanup EXIT
trap 'exit 143' TERM; trap 'exit 130' INT; trap 'exit 129' HUP
[ "$(git -C "$B" rev-parse --short=8 HEAD)" = "$SHA" ] || { echo "s287: $B is not at $SHA"; exit 1; }
cd "$B" || exit 1
echo "s287: building at $SHA in $B"
$L/rig/locked.sh "$@"
