#!/bin/bash
# s265 phase 2b (detached, hours): yew's cells, the bisection, every
# timing, the behaviour rows, the gauntlet at the head and the differ
# against 0.2.35. Each step is its own container run under the lane lock.
set -uo pipefail
L=$HOME/lanes/s265
E=$L/evidence/phase2b; mkdir -p $E
exec > $E/phase2b.log 2>&1
echo $$ > $E/phase2b.pid
trap 'rc=$?; echo $rc > $E/phase2b.exit; date -u; echo "phase2b exit=$rc"' EXIT
# The phase's exit is its worst step's: a step that goes red ends the
# phase nonzero even when later steps run (RULES §1: no 0 over a red).
worst=0
step() { echo "=== $1 $(date -u +%T) $($L/scripts/hostload.sh)"; }
# Resumable: S265_FROM=<step key> skips the steps before it (yew, bisect,
# hello, yewtime, rows, gauntlet, differ); each step's own work also skips
# what is already built (a yew cell's binary, a commit's hello).
order="yew bisect hello yewtime rows gauntlet differ"
from=${S265_FROM:-yew}
case " $order " in *" $from "*) ;; *) echo "S265_FROM=$from is not a step"; exit 2;; esac
want() { local k; for k in $order; do [ "$k" = "$from" ] && return 0; [ "$k" = "$1" ] && return 1; done; return 0; }
echo "phase2b from step: $from"
H=$L/witness/hello
Y=$L/evidence/yew

if want yew; then
step "yew builds"
git -C $L/yew worktree add -q --detach $L/yew-fix 280012fe 2>/dev/null || true
git -C $L/yew-fix apply $L/evidence/compile-compat.patch 2>/dev/null || true
git -C $L/yew-fix diff --stat
$L/ctr bash -c "
set -e
for c in a:0.2.34 b:0.2.35 fix:head; do
  cell=\${c%%:*}; v=\${c#*:}
  cd $L/yew-\$cell; mkdir -p target/release
  [ -x target/release/yew ] && { echo \"have \$cell\"; continue; }
  echo \"BUILD \$cell \$v \$(date -u +%T)\"
  WOLF_STD=$L/tc/0.2.35/std; [ \$v = 0.2.34 ] && WOLF_STD=$L/tc/0.2.34/std
  WOLF_STD=\$WOLF_STD /usr/bin/time -v $L/tc/\$v/wolf build --release --deny-warnings src/yew.lu -o target/release/yew 2> $Y/build-\$cell.time
  grep -E 'Elapsed|Maximum resident|Exit status' $Y/build-\$cell.time
done
sha256sum $L/yew-*/target/release/yew; size $L/yew-*/target/release/yew
" || exit 1

fi
if want bisect; then
step "bisection builds"
git -C $L/wolf-lang fetch -q origin s265
$L/ctr bash $L/scripts/bisect.sh $L/scripts/bisect-commits.txt || exit 1
printf '7b7b1753 trunk\n' > $E/trunk.txt
$L/ctr bash $L/scripts/bisect.sh $E/trunk.txt || exit 1
du -sh $L/target; df -h --output=avail $L | tail -1
# Prune: the bisection's release artifacts and the variants' runtime build
# (every toolchain the timings need is staged under tc/, every witness kept).
rm -rf $L/target/release $L/target-var
du -sh $L/target; df -h --output=avail $L | tail -1

fi
if want hello; then
step "hello timings"
$L/ctr bash -c "
cd $L/bisect
args='v0.2.34=$H/hello-0.2.34 v0.2.35=$H/hello-0.2.35'
for c in \$(awk '{print \$1}' $L/scripts/bisect-commits.txt) 7b7b1753; do args=\"\$args \$c=$L/bisect/hello-\$c\"; done
args=\"\$args head=$H/hello-head V1=$H/hello-V1 c=$H/hello-c\"
for r in 1 2 3; do $L/scripts/coldstart -n 50 -w 5 -o $E/rows-bisect-\$r.tsv \$args > $E/summary-bisect-\$r.tsv; cat $E/summary-bisect-\$r.tsv; done
for kv in \$args; do n=\${kv%%=*}; b=\${kv#*=}
  strace -qq -o $E/strace-\$n.txt \$b > /dev/null
  perf stat -r 50 -e task-clock,page-faults,instructions:u -x, -o $E/perf-\$n.csv \$b > /dev/null
  printf '%s\t%s calls\t%s\n' \$n \"\$(grep -v -c -E '^(\+\+\+|execve)' $E/strace-\$n.txt)\" \"\$(grep -E 'instructions|page-faults' $E/perf-\$n.csv | cut -d, -f1,3 | tr '\n' ' ')\"
  size \$b | tail -1
done
"

fi
if want yewtime; then
step "yew timings"
$L/ctr bash -c "
S=$L/sagitta
export HOME=$Y/run/home XDG_CONFIG_HOME=$Y/run/home/config YEW_RUNTIME_DIR=\$S/runtime YEW_PERF_ADVISORY=1
cd \$S
cc -std=c11 -O2 -Wall -Wextra -Isrc -Itests -o $L/scripts/yewpaint $L/scripts/yewpaint.c tests/support/live_pty.c || exit 1
mkdir -p $Y/run/home/config $Y/run/state $Y/run/fixtures; : > $Y/run/fixtures/empty.c
[ -s $Y/run/fixtures/fixture.c ] || yes x | head -n 10000 > $Y/run/fixtures/fixture.c
for r in 1 2 3; do
  for m in cold clean; do
    fx=$Y/run/fixtures/fixture.c; [ \$m = clean ] && fx=$Y/run/fixtures/empty.c
    $L/scripts/yewpaint \$m 50 \$fx $Y/run/state $Y/rows-\$m-\$r.tsv a=$L/yew-a/target/release/yew b=$L/yew-b/target/release/yew fix=$L/yew-fix/target/release/yew || echo \"yewpaint \$m \$r FAILED\"
    python3 -I $L/scripts/summarize.py $Y/rows-\$m-\$r.tsv
  done
done
"

fi
if want rows; then
step "behaviour rows at the head"
$L/ctr bash -c "
cd $L/wolf-head && git log --oneline -1
cargo test --offline -p wolf_rt --test main_stack_bounds --test stack_overflow 2>&1 | grep -E 'main_stack_bounds:|stack_overflow|test result|panicked|FAILED|ok$' | head -40
cargo test --offline -p wolf_driver --test two_machines 2>&1 | grep -E 'test result|panicked|FAILED|running' | head -20
"

fi
if want gauntlet; then
step "gauntlet at the head"
# One target directory for the lane: the gauntlet's cwd-relative
# target/ (xtask reads target/debug/wolf) is the lane's own.
[ -e $L/wolf-head/target ] || ln -s $L/target $L/wolf-head/target
$L/ctr bash -c "
cd $L/wolf-head
git log --oneline -1; git status --short | head
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status; cat /sys/fs/cgroup/pids.max; ulimit -Su; ulimit -Hu
unset CARGO_TARGET_DIR
export LUPIN=$L/tc/lupin-0.1.56/lupin WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 \
  WOLF_INTERRUPT_REQUIRE_QEMU=1 WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1 \
  WOLF_DIST_STD_SRC=$L/wolf-std RUST_TEST_NOCAPTURE=1 CARGO_NET_OFFLINE=true
cargo run --quiet -j4 --package xtask -- ci > $E/gauntlet.log 2>&1
rc=\$?; echo \$rc > $E/gauntlet.exit; echo gauntlet exit=\$rc
tail -40 $E/gauntlet.log
grep -E 'bench-gates: startup' $E/gauntlet.log
"
g=$(cat $E/gauntlet.exit 2>/dev/null || echo 2)
echo "gauntlet step exit=$g"
[ "$g" = 0 ] || worst=$g

fi
if want differ; then
step "differ against 0.2.35"
$L/ctr bash -c "
cd $L/wolf-head
unset CARGO_TARGET_DIR
export WOLF_STD=$L/wolf-std/std
for tier in checked native release; do
  unset WOLF_CHECKED WOLF_NATIVE WOLF_RELEASE
  case \$tier in checked) export WOLF_CHECKED=1;; native) export WOLF_NATIVE=1;; release) export WOLF_RELEASE=1;; esac
  target/debug/xtask differ $L/wolf-head/target/debug/wolf $L/tc/0.2.35/wolf > $E/differ-\$tier.log 2>&1
  echo \$? > $E/differ-\$tier.exit
  echo \"differ \$tier exit=\$(cat $E/differ-\$tier.exit) (1: divergences, listed and named in the note; 2: a fault)\"; tail -12 $E/differ-\$tier.log
done
"
du -sh $L/target; df -h --output=avail $L | tail -1
fi
for t in checked native release; do
  d=$(cat $E/differ-$t.exit 2>/dev/null || echo 0); [ "$d" -le 1 ] || worst=$d
done
step "done"
exit $worst
