#!/bin/bash
# s289: the rows' red take at a commit (behind the lock, by at-commit.sh):
# the four-machine test and the corpus step, each step's END printed.
set -u
L=${LANE:-$LANE}
export LUPIN=$L/inputs/lupin060/x/lupin
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return 0; }
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step build cargo build --locked -p wolf_driver -p wolf_rt
step lanes cargo test --locked -p wolf_driver --test "${LANES_TEST:-diagnostics_sweep_lanes}" -- --test-threads=4
step corpus cargo xtask corpus
INSTA_UPDATE=always step lex-snapshots cargo test --locked -p wolf_lex --test corpus_snapshots
INSTA_UPDATE=always step ledger-snapshot cargo test --locked -p wolf_wir --test lower_corpus
step new-snapshots bash -c "git status --porcelain --untracked-files=all -- crates/wolf_lex/tests/snapshots crates/wolf_parse/tests/snapshots crates/wolf_wir/tests/snapshots && git diff --stat"
echo "== done $(date -u +%FT%TZ)"
