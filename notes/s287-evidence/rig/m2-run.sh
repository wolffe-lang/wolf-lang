#!/bin/bash
# s287: run a job at a COMMIT on M2 (the second Mac, correctness takes
# only): the commit's tree is archived and extracted fresh under
# ~/lanes/s287/src there (tar -m, so every file is newer than the kept
# target's fingerprints), its target/ a link to the kept ~/lanes/s287/target,
# the rig and lupin are mirrored, and the job runs behind M2's own
# build lock (m2-locked.sh).
# usage: m2-run.sh <rev> <job.sh> [args...]   (env passed: ROWS TESTS FULL EXTRA)
set -u
L=$LANE
A=$HOME/lanes/m2-env/bin/ssh-m2
# rsync names the host itself, so it takes plain ssh with the pinned key.
RSH="ssh -o BatchMode=yes -o UserKnownHostsFile=$HOME/lanes/orch/known_hosts.m2 -o StrictHostKeyChecking=yes"
SHA=$(git -C "$L/wolf-lang" rev-parse --short=8 "$1") || exit 1
shift
JOB=$(basename "$1"); shift
echo "s287: M2 take at $SHA: $JOB $*"
"$A" bash -s <<'EOF2' || exit 1
mkdir -p ~/lanes/s287/{rig,inputs,records,tmp}
EOF2
rsync -a -e "$RSH" "$L/rig/" M2:lanes/s287/rig/ || exit 1
rsync -a -e "$RSH" "$L/inputs/lupin060/unpacked" M2:lanes/s287/inputs/lupin060/ || exit 1
git -C "$L/wolf-lang" archive --format=tar "$SHA" | "$A" bash -c "'rm -rf ~/lanes/s287/src.new && mkdir -p ~/lanes/s287/src.new && tar -x -m -C ~/lanes/s287/src.new && rm -rf ~/lanes/s287/src && mv ~/lanes/s287/src.new ~/lanes/s287/src && mkdir -p ~/lanes/s287/target && ln -s ~/lanes/s287/target ~/lanes/s287/src/target && echo $SHA > ~/lanes/s287/src/.s287-sha && cd ~/lanes/s287/src && printf \"target\\n.s287-sha\\n\" > .git-info-exclude && git init -q && mv .git-info-exclude .git/info/exclude && git add -A && git -c user.name=s287 -c user.email=s287@localhost -c commit.gpgsign=false commit -q -m \"s287 tree of $SHA\"'" || exit 1
"$A" bash -s <<EOF3
export LANE=\$HOME/lanes/s287 ROWS="${ROWS:-}" TESTS="${TESTS:-}" FULL="${FULL:-0}" EXTRA="${EXTRA:-}"
. \$LANE/env.sh
cd \$LANE/src && exec \$LANE/rig/m2-locked.sh \$LANE/rig/$JOB $*
EOF3
