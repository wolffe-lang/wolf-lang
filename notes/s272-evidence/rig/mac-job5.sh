#!/bin/bash
# s272 mac job 5 (run by build-at.sh in the build worktree, behind the Mac
# lock): the pty helper's row, 10 runs, and wolf_rt's term tests whole, 20
# runs, counting failures; then fmt and clippy on wolf_rt.
set -u
export PATH=$HOME/.cargo/bin:$PATH
B=/Users/mfwolffe/lanes/s272/build
cd "$B" || exit 1
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ) $(rustc --version)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step build cargo test -j4 -p wolf_rt --lib --no-run
loop() {
  local n=$1 runs=$2 fails=0; shift 2
  for i in $(seq 1 "$runs"); do
    if ! cargo test -q -j4 -p wolf_rt --lib "$@" >"$B/../ev/job5-$n-$i.log" 2>&1; then
      fails=$((fails + 1))
      grep -E "panicked|FAILED|crossed|left|right" "$B/../ev/job5-$n-$i.log" | head -8 | sed "s/^/  run $i: /"
    fi
  done
  echo "LOOP $n fails=$fails of $runs"
  [ $fails -eq 0 ]
}
step pair-row loop pair-row 10 term::tests::the_pty_helper_answers_its_own_pair -- --exact
step term-tests loop term-tests 20 term::tests
step fmt cargo fmt --all --check
step clippy-rt cargo clippy -j4 -p wolf_rt --all-targets -- -D warnings
echo "clean: $(git status --porcelain | wc -l) paths"
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
