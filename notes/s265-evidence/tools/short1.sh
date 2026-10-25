#!/bin/bash
# s265 short job 1: cargo fetch for every commit the lane builds (the only
# networked run), the equivalence test at the working head, and the
# start-up row's count with the debug toolchain bench-gates uses.
set -uo pipefail
L=$HOME/lanes/s265
E=$L/evidence/short1; mkdir -p $E
exec > $E/short1.log 2>&1
echo $$ > $E/short1.pid
trap 'echo $? > $E/short1.exit' EXIT
date -u
S265_NET=1 $L/ctr bash -c "
set -e
cd $L/wolf-lang
for c in \$(awk '{print \$1}' $L/scripts/bisect-commits.txt) 7b7b1753; do git checkout -q --detach \$c; cargo fetch --quiet; done
echo fetched bisection commits
cd $L/wolf-head && cargo fetch --quiet && echo fetched head
" || exit 1
date -u
$L/ctr bash -c "
set -e
cd $L/wolf-head
git log --oneline -1
cargo test --offline -p wolf_rt --test main_stack_bounds 2>&1 | tail -15
cargo build --offline -p wolf_driver -p wolf_rt --quiet
D=$L/witness/gate; mkdir -p \$D
target/debug/wolf build corpus/hello.lu -o \$D/hello-wolf --release --no-cache
clang -O2 -o \$D/hello-c bench/startup/hello.c
for b in wolf c; do strace -qq -o \$D/\$b.strace \$D/hello-\$b > /dev/null; echo \"\$b: \$(grep -v -c -E '^(\+\+\+|execve)' \$D/\$b.strace) calls\"; done
" 2>&1
date -u
echo SHORT1 DONE
