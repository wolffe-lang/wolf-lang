#!/bin/bash
# s287: the suites the second change's adjusted witnesses touch, at a commit:
# wolf_mem's rendered diagnostics (the induction-written fixture) and the
# element-places lanes test that runs the adjusted corpus row on four machines.
set -u
export LUPIN=$LANE/inputs/lupin060/unpacked/lupin-0.1.60-aarch64-apple-darwin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
echo "tree of $(git rev-parse HEAD)"
step mem-diagnostics env INSTA_UPDATE=no cargo test --locked -p wolf_mem --test mem_diagnostics
step element-places cargo test --locked -p wolf_driver --test element_places_lanes -- --test-threads=1
step clippy-mem cargo clippy --locked -p wolf_mem --all-targets -- -D warnings
echo "== done $(date -u +%FT%TZ)"
