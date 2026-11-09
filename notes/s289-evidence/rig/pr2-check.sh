#!/bin/bash
# s289: the second change's checks at a commit (behind a lock, at a commit
# worktree): the build, then each of its rows' test targets.
set -u
L=${LANE:-$LANE}
export LUPIN=$L/inputs/lupin060/x/lupin
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return 0; }
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step build cargo build --locked -p wolf_driver -p wolf_rt
step manifest-round-trip cargo test --locked -p wolf_pkg --test manifest_round_trip
step pkg-add cargo test --locked -p wolf_driver --test pkg add_writes_a_manifest_that_reads_back -- --nocapture
step lld-note cargo test --locked -p wolf_driver --test lld_note -- --nocapture
step census cargo test --locked -p wolf_sema --test prelude_census
step sweep-lanes cargo test --locked -p wolf_driver --test diagnostics_sweep_lanes -- --test-threads=4
echo "== done $(date -u +%FT%TZ)"
