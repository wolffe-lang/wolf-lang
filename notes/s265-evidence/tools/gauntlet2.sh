#!/bin/bash
# s265 gauntlet 2 (detached, on the host; every step in the lane container):
#  1. a_failed_copy_write_puts_the_source_back_on_every_lane at trunk
#     6480a27b, same container, same lupin 0.1.56: the attribution of the
#     one red the first gauntlet met;
#  2. the gauntlet at the rebased head; if a step goes red, the test step
#     re-run with --no-fail-fast and every later step run alone, so the
#     whole is seen; the exit is the worst step's, never 0 over a red.
set -uo pipefail
L=/home/mfwolffe/lanes/s265
E=$L/evidence/gauntlet2; mkdir -p $E
exec > $E/gauntlet2.log 2>&1
echo $$ > $E/gauntlet2.pid
worst=0
trap 'rc=$?; echo $rc > $E/gauntlet2.exit; date -u; echo "gauntlet2 exit=$rc"' EXIT
GENV="LUPIN=$L/tc/lupin-0.1.56/lupin WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 WOLF_INTERRUPT_REQUIRE_QEMU=1 WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1 WOLF_DIST_STD_SRC=$L/wolf-std RUST_TEST_NOCAPTURE=1 CARGO_NET_OFFLINE=true"
echo "=== 1. copy_write at trunk $(date -u +%T) $($L/scripts/hostload.sh)"
T=$L/wolf-trunk
[ -d $T ] || git -C $L/wolf-lang worktree add -q --detach $T 6480a27b
git -C $T checkout -q --detach 6480a27b; git -C $T log --oneline -1
$L/ctr bash -c "
cd $T; export $GENV CARGO_TARGET_DIR=$L/target-trunk
\$LUPIN --version | head -1
cargo test --offline -p wolf_driver --test copy_write_lanes 2>&1 | grep -E '^test |panicked|lupin|test result|error' | head -20
echo \${PIPESTATUS[0]} > $E/copy-write-trunk.exit
"
echo "copy_write at trunk 6480a27b exit=$(cat $E/copy-write-trunk.exit)"
rm -rf $L/target-trunk
echo "=== 2. gauntlet at the head $(date -u +%T) $($L/scripts/hostload.sh)"
H=$L/wolf-head
[ -e $H/target ] || ln -s $L/target $H/target
$L/ctr bash -c "
cd $H; git log --oneline -1; git status --short | head -3
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status; cat /sys/fs/cgroup/pids.max; ulimit -Su; ulimit -Hu
unset CARGO_TARGET_DIR; export $GENV
cargo run --quiet -j4 --package xtask -- ci > $E/gauntlet.log 2>&1
rc=\$?; echo \$rc > $E/gauntlet.exit; echo gauntlet exit=\$rc
tail -25 $E/gauntlet.log
failed=\$(sed -n 's/^xtask ci: step \`\(.*\)\` failed$/\1/p' $E/gauntlet.log)
[ -z \"\$failed\" ] && exit \$rc
echo \"the first red step: \$failed; the rest, each alone:\"
steps='fmt fmt-fixpoint clippy rt-none test fuzz-check deps-check corpus abi-check debug-check midend-rate bench-gates spec-extract conformance print-gate diag-catalog doc-catalog fmt-lu fmt-fuzz audit-surface dist-smoke differ-self lane-coverage'
after=0; worst=\$rc
for s in \$steps; do
  [ \$s = \$failed ] && after=1
  [ \$after = 1 ] || continue
  case \$s in
    test) cmd='cargo test --workspace --no-fail-fast' ;;
    fuzz-check) cmd='cargo check --manifest-path fuzz/Cargo.toml' ;;
    spec-extract|diag-catalog|doc-catalog) cmd=\"cargo xtask \$s --check\" ;;
    fmt-fuzz) cmd='cargo xtask fmt-fuzz --ci' ;;
    dist-smoke) cmd='cargo xtask dist' ;;
    differ-self) cmd='cargo xtask differ --self' ;;
    *) cmd=\"cargo xtask \$s\" ;;
  esac
  \$cmd > $E/step-\$s.log 2>&1; r=\$?
  echo \"step \$s: exit \$r\"
  [ \$s = test ] && grep -E '^test .* FAILED|^    [a-z_0-9:]+$|failed, to rerun' $E/step-test.log | head -30
  [ \$s = bench-gates ] && grep -E 'bench-gates: startup' $E/step-bench-gates.log
  [ \$r = 0 ] || worst=\$r
done
exit \$worst
"
g=$?
echo "gauntlet steps worst exit=$g"
[ "$g" = 0 ] || worst=$g
du -sh $L/target; df -h --output=avail $L | tail -1
exit $worst
