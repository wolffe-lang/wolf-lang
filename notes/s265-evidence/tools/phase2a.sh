#!/bin/bash
# s265 phase 2a: the working head's toolchain (dist crates, release), its
# hello, the start-up row's count against the C twin, a first timing of
# hello on 0.2.34 / 0.2.35 / head, and the per-piece prices.
set -uo pipefail
L=$HOME/lanes/s265
E=$L/evidence/phase2a; mkdir -p $E
exec > $E/phase2a.log 2>&1
echo $$ > $E/phase2a.pid
trap 'echo $? > $E/phase2a.exit' EXIT
date -u
$L/ctr bash -c "
set -e
cd $L/wolf-head
git log --oneline -1; git status --short
WOLF_COMMIT=\$(git rev-parse --short=7 HEAD) cargo build --offline --release -p wolf_driver -p wolf_rt --quiet
mkdir -p $L/tc/head; cp $L/target/release/wolf $L/target/release/libwolf_rt.a $L/tc/head/
ln -sfn $L/tc/0.2.35/std $L/tc/head/std
H=$L/witness/hello
$L/tc/head/wolf build $L/wolf-lang/corpus/hello.lu -o \$H/hello-head --release
\$H/hello-head
clang -O2 -o \$H/hello-c $L/wolf-head/bench/startup/hello.c
for b in 0.2.34 0.2.35 head c; do
  strace -qq -o $E/strace-\$b.txt \$H/hello-\$b > /dev/null
  echo \"\$b: \$(grep -v -c -E '^(\+\+\+|execve)' $E/strace-\$b.txt) calls\"
done
size \$H/hello-0.2.34 \$H/hello-0.2.35 \$H/hello-head
"
$L/scripts/hostload.sh
$L/ctr bash -c "
cd $L/witness/hello
$L/scripts/coldstart -n 50 -w 5 -o $E/rows-hello.tsv v0.2.34=./hello-0.2.34 v0.2.35=./hello-0.2.35 head=./hello-head c=./hello-c
for b in 0.2.34 0.2.35 head c; do perf stat -r 50 -e task-clock,page-faults,instructions:u -o $E/perf-\$b.txt ./hello-\$b > /dev/null; grep -E 'instructions|page-faults|elapsed' $E/perf-\$b.txt; done
P=$L/scripts/piece
$L/scripts/coldstart -n 50 -w 5 -o $E/rows-piece.tsv none=\$P,none getattr=\$P,getattr maps=\$P,maps stat=\$P,stat rlimit=\$P,rlimit alt=\$P,alt sig=\$P,sig all=\$P,all
for m in none getattr maps all; do perf stat -r 30 -e instructions:u -o $E/perf-piece-\$m.txt \$P \$m; grep instructions $E/perf-piece-\$m.txt; done
"
$L/scripts/hostload.sh
echo PHASE2A DONE
