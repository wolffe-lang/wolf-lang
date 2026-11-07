#!/bin/bash
# s285 mac rows job (run by build-at.sh in the build worktree, behind the
# lock; adapted by copy from s282's mac-rows.sh): the anchors gate; build;
# the new rows on the four machines (probe-all); the four-machine test rows;
# the rendered E0415 snapshots; the corpus and conformance steps whole.
# With S285_SNAPS=1, the snapshot sets made with insta's update are copied
# out to records/snaps-<sha>/ and the worktree put back clean.
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
export LUPIN=$L/inputs/lupin060/unpacked/lupin-0.1.60-aarch64-apple-darwin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
head_info() { git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V && "$LUPIN" --version; }
step head head_info
step anchors cargo xtask spec-extract --check
step build cargo build --locked -p wolf_driver -p wolf_rt || { echo "== build failed: stopping"; exit 1; }
ROWS="typecheck/numlit_pattern_wraps typecheck/numlit_pattern_edges typecheck/numlit_pattern_positions typecheck/numlit_pattern_range typecheck/numlit_pattern_default grammar/match_pattern_signed_literal rows/match_literal_pattern_edges"
files=(); for r in $ROWS; do files+=("$PWD/corpus/$r.lu"); done
step rows-everywhere "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "${files[@]}"
step rows-test cargo test --locked -p wolf_driver --test literal_pattern_lanes -- --test-threads=1
step snapshots-test env INSTA_UPDATE=no cargo test --locked -p wolf_sema --test pattern_diagnostics e0415
step corpus cargo xtask corpus
step conformance cargo xtask conformance
if [ "${S285_PROBES:-0}" = 1 ]; then
  pfiles=(); for d in "$L"/probe/*/; do pfiles+=("$d/main.lu"); done
  step probes "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "${pfiles[@]}"
fi
if [ "${S285_EXTRA:-0}" = 1 ]; then
  step fmt cargo fmt --all --check
  step clippy cargo clippy --locked -p wolf_sema -p wolf_parse -p wolf_wir -p wolf_mem -p wolf_driver --all-targets -- -D warnings
  step sema-tests cargo test --locked -p wolf_sema
  step parse-tests cargo test --locked -p wolf_parse
  step wir-unit cargo test --locked -p wolf_wir --lib pattern_literal_tests
fi
if [ "${S285_SUITES:-0}" = 1 ]; then
  # Every test that walks the corpus with snapshots, against the committed
  # snapshots (no update), and the catalogs' checks.
  for t in wolf_codegen_clif:clif_goldens wolf_codegen_llvm:llvm_goldens wolf_fmt:broken wolf_lex:corpus_snapshots \
           wolf_lsp:completion wolf_lsp:transcript wolf_mem:cfg_dumps wolf_mem:mem_diagnostics wolf_mem:ubcheck \
           wolf_parse:ambiguity_trees wolf_parse:corpus_decls wolf_parse:diagnostics wolf_sema:conc_diagnostics \
           wolf_sema:diagnostics wolf_sema:interface_pretty wolf_sema:wave_diagnostics wolf_sema:pattern_diagnostics \
           wolf_wir:build_peephole wolf_wir:lower_corpus wolf_wir:midend_passes wolf_wir:wir_dumps; do
    step "suite-${t/:/-}" env INSTA_UPDATE=no cargo test --locked -p "${t%%:*}" --test "${t##*:}"
  done
  step diag-catalog-check cargo xtask diag-catalog --check
  step doc-catalog-check cargo xtask doc-catalog --check
fi
if [ "${S285_SNAPS:-0}" = 1 ]; then
  OUT=$L/records/snaps-$SHA
  rm -rf "$OUT"; mkdir -p "$OUT"
  step snap-lex env INSTA_UPDATE=always cargo test --locked -p wolf_lex --test corpus_snapshots
  step snap-parse env INSTA_UPDATE=always cargo test --locked -p wolf_parse --test corpus_decls
  step snap-wir env INSTA_UPDATE=always cargo test --locked -p wolf_wir --test lower_corpus
  step snap-sema env INSTA_UPDATE=always cargo test --locked -p wolf_sema --test pattern_diagnostics e0415
  git status --porcelain > "$OUT/status.txt"; cat "$OUT/status.txt"
  git status --porcelain | awk '{print $2}' | grep '\.snap' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
  git diff --stat
  git checkout -q -- . && git clean -fdq -- crates
  echo "clean: $(git status --porcelain --untracked-files=no | wc -l)"
fi
echo "== done $(date -u +%FT%TZ)"
