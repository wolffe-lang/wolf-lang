#!/bin/bash
# s281 mac job 2 (run by build-at.sh in the build worktree, behind the lock):
# the rows at a commit. Build; the nine rows on the checked machine, native,
# release and lupin; the four-machine test rows; the corpus step whole; then
# the corpus snapshots (insta's update) copied out to records/snaps-<sha>/
# and the worktree put back clean.
set -u
L=/Users/mfwolffe/lanes/s281
SHA=$(git rev-parse --short=8 HEAD)
export LUPIN=/Users/mfwolffe/.local/bin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
head_info() { git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V && "$LUPIN" --version; }
step head head_info
step build cargo build --locked -p wolf_driver -p wolf_rt
ROWS="grammar/match_nested_bare_variant grammar/match_nested_bare_variant_fields grammar/match_nested_bare_variant_guard rows/match_row_nested_bare_variant grammar/match_nested_bare_variant_deep grammar/match_nested_bare_variant_or grammar/match_nested_qualified_variant grammar/match_nested_name_binds grammar/match_nested_struct_field_variant"
files=(); for r in $ROWS; do files+=("$PWD/corpus/$r.lu"); done
step rows-everywhere "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "${files[@]}"
step rows-test cargo test --locked -p wolf_driver --test four_machine_regressions -- --test-threads=1
step corpus cargo xtask corpus
if [ "${S281_SNAPS:-1}" = 1 ]; then
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
