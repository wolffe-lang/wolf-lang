#!/bin/bash
# s265 short job 2 (inside the container): the lazy guard at the head —
# its toolchain, hello's count, the overflow rows on both tiers, the
# equivalence test and the runtime's own overflow test.
set -uo pipefail
L=/home/mfwolffe/lanes/s265
E=$L/evidence/short2; mkdir -p $E
cd $L/wolf-head && git log --oneline -1
WOLF_COMMIT=$(git rev-parse --short=7 HEAD) cargo build --offline --release -p wolf_driver -p wolf_rt --quiet || exit 1
mkdir -p $L/tc/head; cp $L/target/release/wolf $L/target/release/libwolf_rt.a $L/tc/head/; ln -sfn $L/tc/0.2.35/std $L/tc/head/std
H=$L/witness/hello
$L/tc/head/wolf build $L/wolf-lang/corpus/hello.lu -o $H/hello-head --release && $H/hello-head
strace -qq -o $E/strace-head.txt $H/hello-head > /dev/null; echo "release head: $(grep -v -c -E '^(\+\+\+|execve)' $E/strace-head.txt) calls"
cargo build --offline -p wolf_driver -p wolf_rt --quiet || exit 1
$L/target/debug/wolf build corpus/hello.lu -o $E/gate-hello-wolf --release --no-cache
clang -O2 -o $E/gate-hello-c bench/startup/hello.c
for b in wolf c; do strace -qq -o $E/gate-$b.strace $E/gate-hello-$b > /dev/null; echo "gate $b: $(grep -v -c -E '^(\+\+\+|execve)' $E/gate-$b.strace) calls"; done
cargo test --offline -p wolf_rt --test main_stack_bounds --test stack_overflow 2>&1 | grep -E 'main_stack_bounds: (ok|.*exit)|test result|panicked|FAILED|stack_overflow' | head -20
O=$E/overflow; mkdir -p $O; cp corpus/faults/stack_overflow.lu $O/main.lu
for v in 0.2.35 head; do
  for tier in native release; do
    fl=; [ $tier = release ] && fl=--release
    $L/tc/$v/wolf build $O/main.lu -o $O/$v-$tier $fl || { echo "build $v $tier failed"; continue; }
    for d in 200000 300000; do
      out=$(bash -c "ulimit -c 0; ulimit -s 8192; exec $O/$v-$tier $d" 2>&1 >/dev/null); rc=$?
      printf '%s %s depth=%s exit=%s %s\n' $v $tier $d $rc "$(echo "$out" | head -2 | tr '\n' '|')"
    done
  done
done
