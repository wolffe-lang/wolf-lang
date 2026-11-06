#!/bin/bash
# s282 mac rows job (run by build-at.sh in the build worktree, behind the
# lock; adapted by copy from s281's mac-job2.sh): build; the new rows on the
# four machines (probe-all); the four-machine test rows; the corpus step
# whole; then, with S282_SNAPS=1, the corpus snapshot sets made with insta's
# update, copied out to records/snaps-<sha>/ and the worktree put back clean.
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
export LUPIN=$L/inputs/lupin059/unpacked/lupin-0.1.59-aarch64-apple-darwin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
head_info() { git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V && "$LUPIN" --version; }
step head head_info
step build cargo build --locked -p wolf_driver -p wolf_rt || { echo "== build failed: stopping"; exit 1; }
ROWS="lints/binder_variant_let lints/binder_variant_for lints/binder_variant_else lints/binder_variant_imported/main lints/binder_variant_controls conc/select_arm_bare_variant conc/select_arm_lowercase_binds"
files=(); for r in $ROWS; do files+=("$PWD/corpus/$r.lu"); done
step rows-everywhere "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "${files[@]}"
step rows-test cargo test --locked -p wolf_driver --test four_machine_regressions -- --test-threads=1
step corpus cargo xtask corpus
if [ "${S282_PROBES:-0}" = 1 ]; then
  pfiles=(); for d in "$L"/probe/*/; do pfiles+=("$d/main.lu"); done
  step probes "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "${pfiles[@]}"
  render() {
    for f in "${pfiles[@]}"; do
      echo "## $(basename "$(dirname "$f")")"
      "$PWD/target/debug/wolf" build "$f" -o "$TMPDIR/probe-bin" 2>&1 | head -80
      echo "   (build exit ${PIPESTATUS[0]})"
      "$PWD/target/debug/wolf" fix "$f" 2>&1 | head -20
      echo "   (fix exit ${PIPESTATUS[0]})"
    done
  }
  step render render
fi
if [ "${S282_EXTRA:-0}" = 1 ]; then
  step fmt cargo fmt --all --check
  step clippy cargo clippy --locked -p wolf_sema -p wolf_driver --all-targets -- -D warnings
  step sema-tests cargo test --locked -p wolf_sema
fi
if [ "${S282_SNAPS:-0}" = 1 ]; then
  OUT=$L/records/snaps-$SHA
  rm -rf "$OUT"; mkdir -p "$OUT"
  step snap-lex env INSTA_UPDATE=always cargo test --locked -p wolf_lex --test corpus_snapshots
  step snap-parse env INSTA_UPDATE=always cargo test --locked -p wolf_parse --test corpus_decls
  step snap-wir env INSTA_UPDATE=always cargo test --locked -p wolf_wir --test lower_corpus
  git status --porcelain > "$OUT/status.txt"; cat "$OUT/status.txt"
  git status --porcelain | awk '{print $2}' | grep '\.snap' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
  git diff --stat
  git checkout -q -- . && git clean -fdq -- crates
  echo "clean: $(git status --porcelain --untracked-files=no | wc -l)"
fi
echo "== done $(date -u +%FT%TZ)"
