#!/bin/bash
# s265 gauntlet 3 (detached): `cargo xtask ci` at the final head in the
# gauntlet's own set-up — the worktree's real target/ (take 2's symlink
# moved the corpus rows' target/*.tmp out of lupin's served tree) and the
# fuzz crates fetched. The exit is the gauntlet's.
set -uo pipefail
L=/home/mfwolffe/lanes/s265
E=$L/evidence/gauntlet3; mkdir -p $E
exec > $E/gauntlet3.log 2>&1
echo $$ > $E/gauntlet3.pid
trap 'rc=$?; echo $rc > $E/gauntlet3.exit; date -u; echo "gauntlet3 exit=$rc"' EXIT
H=$L/wolf-head
[ -L $H/target ] && { echo "target is a symlink: refusing"; exit 2; }
GENV="LUPIN=$L/tc/lupin-0.1.56/lupin WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 WOLF_INTERRUPT_REQUIRE_QEMU=1 WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1 WOLF_DIST_STD_SRC=$L/wolf-std RUST_TEST_NOCAPTURE=1 CARGO_NET_OFFLINE=true"
echo "=== gauntlet $(date -u +%T) $($L/scripts/hostload.sh)"
$L/ctr bash -c "
cd $H; git log --oneline -1; git status --short | head -3
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status; cat /sys/fs/cgroup/pids.max; ulimit -Su; ulimit -Hu
unset CARGO_TARGET_DIR; export $GENV
cargo run --quiet -j4 --package xtask -- ci > $E/gauntlet.log 2>&1
rc=\$?; echo gauntlet exit=\$rc
tail -30 $E/gauntlet.log
grep -E 'bench-gates: startup|xtask ci: (step|all)' $E/gauntlet.log
grep -c SKIP $E/gauntlet.log
exit \$rc
"
g=$?
echo "=== done $(date -u +%T) $($L/scripts/hostload.sh)"
du -sh $H/target; df -h --output=avail $L | tail -1
exit $g
