#!/bin/bash
# s289: run a job at a COMMIT on the Air (the second Mac): the commit is
# fetched from GitHub into the Air's clone and checked out in its own
# worktree there (never a live tree), the rig mirrored, and the job run
# behind the Air's own build lock (air-locked.sh). WARM names a target
# directory there to clone (cp -c) in for a warm build.
# usage: [WARM=build-xxxxxxx] air-at.sh <branch> <sha> <cmd...>
# (a word rig/X of the command runs as the Air's mirrored $LANE/rig/X)
set -u
L=$LANE
A=$HOME/lanes/air-env/bin/ssh-air
RSH="ssh -o BatchMode=yes -o UserKnownHostsFile=$HOME/lanes/orch-wollf/known_hosts.air -o StrictHostKeyChecking=yes"
branch=$1; sha=$2; shift 2
cmd=""
for w in "$@"; do case "$w" in rig/*) w="$LANE/$w" ;; esac; cmd="$cmd $(printf %q "$w")"; done
# SECOND_MAC stands for the second Mac's ssh host; the evidence does not carry its name.
rsync -a -e "$RSH" "$L/rig/" SECOND_MAC:lanes/s289/rig/ || exit 1
"$A" bash -s <<EOF2
set -u
export LANE=\$HOME/lanes/s289
. \$LANE/env.sh
cd \$LANE/wolf-lang || exit 1
git fetch -q origin "$branch" || exit 1
S=\$(git rev-parse --short=7 "$sha^{commit}") || exit 1
B=\$LANE/build-\$S
if [ ! -d "\$B" ]; then
  git worktree add -q --detach "\$B" "\$S" || exit 1
  if [ -n "${WARM:-}" ] && [ -d "\$LANE/${WARM:-none}/target" ]; then cp -c -R "\$LANE/${WARM:-none}/target" "\$B/target" && echo "s289: warm target from ${WARM:-}"; fi
fi
cd "\$B" || exit 1
[ -z "\$(git status --porcelain --untracked-files=no)" ] || { echo "s289: \$B is not clean"; exit 1; }
echo "s289: Air at \$(git rev-parse --short=8 HEAD) in \$B"
exec \$LANE/rig/air-locked.sh $cmd
EOF2
