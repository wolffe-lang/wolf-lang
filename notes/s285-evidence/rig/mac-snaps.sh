#!/bin/bash
# s285 mac snapshot sweep (run by build-at.sh in the build worktree, behind
# the lock): every test that walks the corpus with insta snapshots, run with
# INSTA_UPDATE=always, so a snapshot the change moved is a changed file to
# read rather than a failure to rerun; then `cargo xtask diag-catalog`. Every
# changed file is copied out to records/sweep-<sha>/ and the worktree put
# back clean.
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
OUT=$L/records/sweep-$SHA
rm -rf "$OUT"; mkdir -p "$OUT"
export LUPIN=$L/inputs/lupin059/unpacked/lupin-0.1.59-aarch64-apple-darwin/lupin
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l'
step build cargo build --locked -p wolf_driver -p wolf_rt || exit 1
for t in wolf_codegen_clif:clif_goldens wolf_codegen_llvm:llvm_goldens wolf_fmt:broken wolf_lex:corpus_snapshots \
         wolf_lsp:completion wolf_lsp:transcript wolf_mem:cfg_dumps wolf_mem:mem_diagnostics wolf_mem:ubcheck \
         wolf_parse:ambiguity_trees wolf_parse:corpus_decls wolf_parse:diagnostics wolf_sema:conc_diagnostics \
         wolf_sema:diagnostics wolf_sema:interface_pretty wolf_sema:wave_diagnostics wolf_sema:pattern_diagnostics \
         wolf_wir:build_peephole wolf_wir:lower_corpus wolf_wir:midend_passes wolf_wir:wir_dumps; do
  step "snap-${t/:/-}" env INSTA_UPDATE=always cargo test --locked -p "${t%%:*}" --test "${t##*:}"
done
step diag-catalog cargo xtask diag-catalog
step diag-catalog-check cargo xtask diag-catalog --check
step doc-catalog-check cargo xtask doc-catalog --check
git status --porcelain > "$OUT/status.txt"; cat "$OUT/status.txt"
git status --porcelain | awk '{print $2}' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp -R "$f" "$OUT/$f"; done
git diff --stat
git checkout -q -- . && git clean -fdq -- crates docs && echo "clean: $(git status --porcelain | wc -l)"
echo "== done $(date -u +%FT%TZ)"
