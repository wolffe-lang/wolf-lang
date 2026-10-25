#!/bin/bash
# s265 recheck at the new head, the gauntlet's own setup (the worktree's
# real target/, not the lane's symlinked one): the two red tests, the
# fuzz check (its crates fetched first), and bench-gates.
set -uo pipefail
L=/home/mfwolffe/lanes/s265
E=$L/evidence/recheck; mkdir -p $E
H=$L/wolf-head
GENV="LUPIN=$L/tc/lupin-0.1.56/lupin WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 WOLF_INTERRUPT_REQUIRE_QEMU=1 WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1 WOLF_DIST_STD_SRC=$L/wolf-std RUST_TEST_NOCAPTURE=1 CARGO_NET_OFFLINE=true"
if [ -L $H/target ]; then rm $H/target; mv $L/target $H/target; fi
ls -ld $H/target
S265_NET=1 $L/ctr bash -c "cd $H && cargo fetch --manifest-path fuzz/Cargo.toml --quiet && echo fetched fuzz crates"
worst=0
for t in copy_write_lanes cwd_region_lanes; do
  $L/ctr bash -c "
cd $H; git log --oneline -1; unset CARGO_TARGET_DIR; export $GENV
cargo test --offline -p wolf_driver --test $t 2>&1 | grep -E '^test |panicked|left:|right:|answer|test result|error' | head -20
exit \${PIPESTATUS[0]}
" > $E/$t.log 2>&1
  r=$?; echo "$t exit=$r"; cat $E/$t.log; [ $r = 0 ] || worst=$r
done
$L/ctr bash -c "
cd $H; unset CARGO_TARGET_DIR; export $GENV
cargo check --offline --manifest-path fuzz/Cargo.toml > $E/fuzz-check.log 2>&1; echo \"fuzz-check exit=\$?\"; tail -2 $E/fuzz-check.log
cargo xtask bench-gates > $E/bench-gates.log 2>&1; r=\$?; echo \"bench-gates exit=\$r\"; grep -E 'bench-gates: (startup|deterministic)' $E/bench-gates.log; exit \$r
"
r=$?; [ $r = 0 ] || worst=$r
echo "recheck worst=$worst"
exit $worst
