#!/bin/bash
# s281 mac job 1 (run by build-at.sh in the build worktree, behind the lock):
# the base measured. Build wolf at the base; every probe on the checked
# machine, native, release and lupin; the issue's reproducer on the
# published 0.2.36 too.
set -u
L=/Users/mfwolffe/lanes/s281
export LUPIN=/Users/mfwolffe/.local/bin/lupin
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
head_info() { git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l && uname -srm && rustc -V && cargo -V && "$LUPIN" --version; }
step head head_info
step build cargo build --locked -p wolf_driver -p wolf_rt
step probes-base "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "$L"/probe/*/main.lu
step probes-0.2.36 "$L/rig/probe-all.sh" /Users/mfwolffe/.local/bin/wolf "$L/probe/p01_k_drink/main.lu" "$L/probe/p02_item_two/main.lu"
echo "== done $(date -u +%FT%TZ)"
