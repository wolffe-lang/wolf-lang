#!/bin/bash
# s289, third change: the checks at a commit (behind the Air's lock, at a
# commit worktree): the build, the rows, the spec's anchor registry and
# conformance, the corpus, the lowering ledger written, fmt and clippy;
# with FULL_CHECK, the touched crates' suites. Each step's END printed.
set -u
L=${LANE:?}
export LUPIN=$L/inputs/lupin060/x/lupin
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return 0; }
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step build cargo build --locked -p wolf_driver -p wolf_rt
step lanes cargo test --locked -p wolf_driver --test prelude_min_lanes -- --test-threads=4
step census cargo test --locked -p wolf_sema --test prelude_census
step prelude-table cargo test --locked -p wolf_sema --lib prelude::
step prelude-list cargo test --locked -p wolf_driver --test prelude_list
step sweep-lanes cargo test --locked -p wolf_driver --test diagnostics_sweep_lanes -- --test-threads=4
step spec-extract cargo xtask spec-extract
step conformance cargo xtask conformance
step corpus cargo xtask corpus
INSTA_UPDATE=always step ledger-snapshot cargo test --locked -p wolf_wir --test lower_corpus
step fmt cargo fmt --all --check
step clippy cargo clippy --locked -p wolf_sema -p wolf_wir -p wolf_mem -p wolf_driver --all-targets -- -D warnings
if [ -n "${FULL_CHECK:-}" ]; then
  step sema-all cargo test --locked -p wolf_sema
  step wir-lib cargo test --locked -p wolf_wir --lib
  step mem-lib cargo test --locked -p wolf_mem --lib
  step diag-catalog cargo xtask diag-catalog --check
fi
step tree bash -c 'git status --porcelain --untracked-files=all && git diff --stat'
echo "== done $(date -u +%FT%TZ)"
