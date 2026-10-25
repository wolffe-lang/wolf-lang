#!/bin/bash
# s265: price what is left of the main thread's registration at the head,
# by runtime variants linked under the head's own driver (the driver links
# the libwolf_rt.a beside it): V1 no registration at all, V3 the handler
# only, V2 the handler and the altstack (no bounds query: design B's
# start), head the shipped fix. Runs inside the lane container.
set -euo pipefail
L=/home/mfwolffe/lanes/s265
E=$L/evidence/variants; mkdir -p $E
V=$L/wolf-var
[ -d $V ] || git -C $L/wolf-lang worktree add -q --detach $V origin/s265
git -C $V checkout -q -- . && git -C $V checkout -q --detach origin/s265
git -C $V log --oneline -1
for v in V1 V2 V3; do
  git -C $V checkout -q -- crates/wolf_rt
  case $v in
    V1) python3 - $V <<'PY'
import sys; p=sys.argv[1]+'/crates/wolf_rt/src/native.rs'; s=open(p).read()
old='    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]\n    crate::task::enter_main_thread();\n}'; assert old in s
open(p,'w').write(s.replace(old,'    // s265 variant V1: no registration\n}'))
PY
    ;;
    V2|V3) python3 - $V $v <<'PY'
import sys; p=sys.argv[1]+'/crates/wolf_rt/src/task/stack.rs'; s=open(p).read()
old='    let bounds = main_stack_bounds().or_else(pthread_bounds);'; assert old in s
s=s.replace(old,'    let bounds: Option<(usize, usize)> = Some((0, 1)); // s265 variant: no query')
if sys.argv[2]=='V3':
    old='''    if ensure_altstack().is_none() {
        return;
    }
    register_guard(bounds.0, bounds.1);
    install_overflow_handler();
}

/// The calling thread's guard, from the host's pthread queries.'''
    assert old in s
    s=s.replace(old,'''    register_guard(bounds.0, bounds.1);
    install_overflow_handler();
}

/// The calling thread's guard, from the host's pthread queries.''')
open(p,'w').write(s)
PY
    ;;
  esac
  git -C $V diff --stat
  (cd $V && CARGO_TARGET_DIR=$L/target-var cargo build --offline --release -p wolf_rt --quiet 2>&1 | grep -v '^warning' | head -5 || true)
  mkdir -p $L/tc/$v; cp $L/tc/head/wolf $L/tc/$v/; cp $L/target-var/release/libwolf_rt.a $L/tc/$v/
  $L/tc/$v/wolf build $L/wolf-lang/corpus/hello.lu -o $L/witness/hello/hello-$v --release
  $L/witness/hello/hello-$v
done
git -C $V checkout -q -- crates/wolf_rt
cd $L/witness/hello
for b in 0.2.34 0.2.35 head V1 V2 V3 c; do
  strace -qq -o $E/strace-$b.txt ./hello-$b > /dev/null
  perf stat -r 50 -e page-faults,instructions:u -x, -o $E/perf-$b.csv ./hello-$b > /dev/null
  printf '%s\t%s calls\t%s\n' $b "$(grep -v -c -E '^(\+\+\+|execve)' $E/strace-$b.txt)" "$(grep -E 'instructions|page-faults' $E/perf-$b.csv | cut -d, -f1,3 | tr '\n' ' ')"
done
for r in 1 2 3; do
  $L/scripts/coldstart -n 50 -w 5 -o $E/rows-$r.tsv v0.2.34=./hello-0.2.34 v0.2.35=./hello-0.2.35 head=./hello-head V1=./hello-V1 V2=./hello-V2 V3=./hello-V3 c=./hello-c
done
