#!/bin/bash
# s289, third change: the rows' red take at a commit (behind the Air's lock,
# at a commit worktree): the four-machine test, the census, the prelude
# table's test and the corpus step, each step's END printed; then the new
# rows' lexer and parser snapshots and the lowering ledger, written.
set -u
L=${LANE:?}
export LUPIN=$L/inputs/lupin060/x/lupin
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return 0; }
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step build cargo build --locked -p wolf_driver -p wolf_rt
step lanes cargo test --locked -p wolf_driver --test prelude_min_lanes -- --test-threads=4
step census cargo test --locked -p wolf_sema --test prelude_census
step prelude-table cargo test --locked -p wolf_sema --lib prelude::
step corpus cargo xtask corpus
INSTA_UPDATE=always step lex-snapshots cargo test --locked -p wolf_lex --test corpus_snapshots
INSTA_UPDATE=always step parse-snapshots cargo test --locked -p wolf_parse --test corpus_decls
INSTA_UPDATE=always step ledger-snapshot cargo test --locked -p wolf_wir --test lower_corpus
step new-snapshots bash -c "git status --porcelain --untracked-files=all -- crates/wolf_lex/tests/snapshots crates/wolf_parse/tests/snapshots crates/wolf_wir/tests/snapshots && git diff --stat"
echo "== done $(date -u +%FT%TZ)"
