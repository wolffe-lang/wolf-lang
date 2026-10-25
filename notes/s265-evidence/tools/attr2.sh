#!/bin/bash
# s265 attribution, second take: copy_write_lanes at trunk 6480a27b in the
# gauntlet's own configuration (the worktree's default target/, no
# CARGO_TARGET_DIR; the first take set one and its trunk run failed a
# different row, `error: not_found`, so it attributes nothing), then the
# same test at the head, same container, same lupin 0.1.56. Queues on the
# lane lock behind the running gauntlet.
set -uo pipefail
L=/home/mfwolffe/lanes/s265
E=$L/evidence/attr2; mkdir -p $E
exec > $E/attr2.log 2>&1
echo $$ > $E/attr2.pid
trap 'rc=$?; echo $rc > $E/attr2.exit; date -u; echo "attr2 exit=$rc"' EXIT
GENV="LUPIN=$L/tc/lupin-0.1.56/lupin WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 WOLF_INTERRUPT_REQUIRE_QEMU=1 WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1 WOLF_DIST_STD_SRC=$L/wolf-std RUST_TEST_NOCAPTURE=1 CARGO_NET_OFFLINE=true"
T=$L/wolf-trunk
git -C $T checkout -q --detach 6480a27b; rm -rf $T/target
worst=0
for w in $T $L/wolf-head; do
  echo "=== copy_write_lanes at $(git -C $w log --oneline -1) $(date -u +%T) $($L/scripts/hostload.sh)"
  $L/ctr bash -c "
cd $w; unset CARGO_TARGET_DIR; export $GENV
\$LUPIN --version | head -1
cargo test --offline -p wolf_driver --test copy_write_lanes 2>&1 | grep -E '^test |panicked|left:|right:|lupin|test result|error' | head -30
exit \${PIPESTATUS[0]}
"
  r=$?; echo "exit=$r"; [ $r = 0 ] || worst=$r
done
du -sh $T/target; rm -rf $T/target; df -h --output=avail $L | tail -1
exit $worst
