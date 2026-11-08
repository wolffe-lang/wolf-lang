#!/bin/bash
# s287: the corpus-walking snapshot suites (adapted by copy from s285's
# mac-rows.sh). With UPDATE=1 each runs with insta's update on and every
# snapshot file it wrote or changed is copied out to $LANE/records/snaps-<sha>/
# with the status list; otherwise each runs against the committed snapshots.
set -u
L=$LANE
SHA=$(cat .s287-sha 2>/dev/null || git rev-parse --short=8 HEAD)
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
MODE=no; [ "${UPDATE:-0}" = 1 ] && MODE=always
step build cargo build --locked -p wolf_driver -p wolf_rt || exit 1
for t in wolf_codegen_clif:clif_goldens wolf_codegen_llvm:llvm_goldens wolf_fmt:broken wolf_lex:corpus_snapshots \
         wolf_lsp:completion wolf_lsp:transcript wolf_mem:cfg_dumps wolf_mem:mem_diagnostics wolf_mem:ubcheck \
         wolf_parse:ambiguity_trees wolf_parse:corpus_decls wolf_parse:diagnostics wolf_sema:conc_diagnostics \
         wolf_sema:diagnostics wolf_sema:interface_pretty wolf_sema:wave_diagnostics wolf_sema:pattern_diagnostics wolf_sema:row_diagnostics \
         wolf_wir:build_peephole wolf_wir:lower_corpus wolf_wir:midend_passes wolf_wir:wir_dumps; do
  step "suite-${t/:/-}" env INSTA_UPDATE=$MODE cargo test --locked -p "${t%%:*}" --test "${t##*:}"
done
if [ "$MODE" = always ] && [ -z "${NOCOPY:-}" ]; then
  OUT=$L/records/snaps-$SHA
  rm -rf "$OUT"; mkdir -p "$OUT"
  git status --porcelain > "$OUT/status.txt"; cat "$OUT/status.txt"
  git status --porcelain | awk '{print $2}' | while read -r f; do
    if [ -d "$f" ]; then find "$f" -type f; else echo "$f"; fi
  done | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
  git checkout -q -- . && git clean -fdq -- crates
fi
echo "== done $(date -u +%FT%TZ)"
