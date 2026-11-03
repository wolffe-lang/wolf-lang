#!/usr/bin/env bash
# s272: the rows a rebase can move, at one commit, inside r41-toolchain on a
# kasumi slot: wolf_rt's term tests (the pty helper's row alone too) and
# signal tests in loops, the statfs rows and os_surface_lanes whole (the
# scratch on btrfs), editor_calls_lanes, the spec's extract and host table,
# fmt. One `END <step> exit=N` line each; a loop's count is its result.
# usage: verify-inner.sh <sha>
set -uo pipefail
source /lane/scripts/gate-env.sh
cd /lane/wolf-lang || exit 2
git checkout -q --detach "$1" && test "$(git rev-parse HEAD)" = "$1" || { echo "END checkout exit=2"; exit 2; }
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ) $(rustc --version) WOLF_TEST_CPUS=$WOLF_TEST_CPUS"
R=/lane/records/verify
mkdir -p "$R"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
loop() {
  local n=$1 runs=$2 fails=0; shift 2
  for i in $(seq 1 "$runs"); do
    if ! "$@" > "$R/$n-$i.log" 2>&1; then
      fails=$((fails + 1))
      grep -E "panicked|FAILED|left:|right:" "$R/$n-$i.log" | head -6 | sed "s/^/  run $i: /"
    fi
  done
  echo "LOOP $n fails=$fails of $runs"
  [ $fails -eq 0 ]
}
step build cargo build --locked -p wolf_driver -p wolf_rt
step rt-build cargo test --locked -p wolf_rt --lib --no-run
step pair-row loop pair-row 10 cargo test --locked -q -p wolf_rt --lib -- term::tests::the_pty_helper_answers_its_own_pair --exact
step term-tests loop term-tests 10 cargo test --locked -q -p wolf_rt --lib -- term::tests
step signal-tests loop signal-tests 3 cargo test --locked -q -p wolf_rt --lib -- signal::tests
step rt-lib cargo test --locked -q -p wolf_rt --lib
step os-surface cargo test --locked -p wolf_driver --test os_surface_lanes -- --nocapture
step editor-calls cargo test --locked -p wolf_driver --test editor_calls_lanes -- --nocapture --test-threads=2
step host-table cargo test --locked -p wolf_sema --test host_table_spec --test prelude_names --test sandbox_s272
step spec-extract cargo xtask spec-extract --check
step fmt cargo fmt --all --check
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
