#!/bin/bash
# s285 mac measure job (run by build-at.sh in the build worktree at the head,
# behind the lock; adapted by copy from s282's mac-measure.sh): wolf at the
# head copied out; every .lu file of the corpus, of the org's wolf
# repositories and of wolf-std (at the compiler's STD-PIN and at its trunk)
# at the typecheck rung on the base binary (a4d2a9c9) and on the head's
# (measure-org.py, four at a time, one run per directory module); the two
# compared run by run; then the gates shard as a shakedown.
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && rustc -V'
step build-head cargo build --locked -p wolf_driver -p wolf_rt || exit 1
step fmt cargo fmt --all --check
step clippy cargo clippy --locked -p wolf_sema -p wolf_parse -p wolf_wir -p wolf_mem -p wolf_driver --all-targets -- -D warnings
step parse-tests cargo test --locked -p wolf_parse
step wir-unit cargo test --locked -p wolf_wir --lib pattern_literal_tests
mkdir -p "$L/records/wolf-$SHA"; cp target/debug/wolf "$L/records/wolf-$SHA/wolf"
O=$L/org
DIRS="$PWD/corpus"; for d in "$O"/*; do DIRS="$DIRS $d"; done
step measure-base python3 -I "$L/rig/measure-org.py" "$L/base-bin/wolf" "$O/wolf-std-5b3a5994/std" "$L/records/measure-base-a4d2a9c9.json" 4 $DIRS
step measure-head python3 -I "$L/rig/measure-org.py" "$L/records/wolf-$SHA/wolf" "$O/wolf-std-5b3a5994/std" "$L/records/measure-head-$SHA.json" 4 $DIRS
step compare python3 -I "$L/rig/compare.py" "$L/records/measure-base-a4d2a9c9.json" "$L/records/measure-head-$SHA.json"
# The gates shard as a shakedown (not a record: the record is the Linux
# gauntlet), with lupin 0.1.59 as the paired sibling and no global WOLF_STD.
step ci-gates env -u WOLF_STD LUPIN="$L/inputs/lupin059/unpacked/lupin-0.1.59-aarch64-apple-darwin/lupin" WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1 cargo xtask ci --shard gates
echo "== done $(date -u +%FT%TZ)"
