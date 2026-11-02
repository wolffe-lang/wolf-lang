#!/bin/bash
# s272 mac job 6 (run by build-at.sh in the build worktree, behind the Mac
# lock): wolf_rt's unit tests whole, 3 runs, and the pty helper's row, 30
# more runs, counting failures.
set -u
export PATH=$HOME/.cargo/bin:$PATH
B=/Users/mfwolffe/lanes/s272/build
cd "$B" || exit 1
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ) $(rustc --version)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
loop() {
  local n=$1 runs=$2 fails=0; shift 2
  for i in $(seq 1 "$runs"); do
    if ! cargo test -q -j4 -p wolf_rt --lib "$@" >"$B/../ev/job6-$n-$i.log" 2>&1; then
      fails=$((fails + 1))
      grep -E "panicked|FAILED|left|right" "$B/../ev/job6-$n-$i.log" | head -8 | sed "s/^/  run $i: /"
    fi
    grep -h "^test result" "$B/../ev/job6-$n-$i.log" | sed "s/^/  run $i: /"
  done
  echo "LOOP $n fails=$fails of $runs"
  [ $fails -eq 0 ]
}
step rt-lib loop rt-lib 3
step pair-row loop pair-row 30 term::tests::the_pty_helper_answers_its_own_pair -- --exact
echo "clean: $(git status --porcelain | wc -l) paths"
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
