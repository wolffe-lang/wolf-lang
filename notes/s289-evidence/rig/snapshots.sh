#!/bin/bash
# s289: write the parser snapshots of new corpus rows at a commit (behind the
# lock, by at-commit.sh), and list the files it wrote.
set -u
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l'
INSTA_UPDATE=always step snapshots cargo test --locked -p wolf_parse --test corpus_decls
step new bash -c 'git status --porcelain --untracked-files=all -- crates/wolf_parse/tests/snapshots'
