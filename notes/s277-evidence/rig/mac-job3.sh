#!/bin/bash
# s277 mac job 3 (run by build-at.sh in the build worktree, behind the lock):
# the corpus snapshots at the formatter's form, copied out; then the corpus
# and spec steps of `cargo xtask ci` as a pre-check of the gauntlet. Every
# step runs and prints `END <step> exit=N`; the worktree is put back clean.
set -u
B=/Users/mfwolffe/lanes/s277/build
OUT=/Users/mfwolffe/lanes/s277/ev/mac3
rm -rf "$OUT"; mkdir -p "$OUT/snaps"
cd "$B" || exit 1
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
export LUPIN=/Users/mfwolffe/.local/bin/lupin
step snap-lex env INSTA_UPDATE=always cargo test --locked -j4 -p wolf_lex --test corpus_snapshots
step snap-parse env INSTA_UPDATE=always cargo test --locked -j4 -p wolf_parse --test corpus_decls
step snap-wir env INSTA_UPDATE=always cargo test --locked -j4 -p wolf_wir --test lower_corpus
git status --porcelain > "$OUT/status.txt"
git status --porcelain | awk '{print $2}' | grep '\.snap' | while read -r f; do mkdir -p "$OUT/snaps/$(dirname "$f")"; cp "$f" "$OUT/snaps/$f"; done
git checkout -q -- . && git clean -fdq -- crates
for s in fmt-fixpoint spec-extract conformance diag-catalog doc-catalog fmt-lu audit-surface corpus; do
  case $s in spec-extract|diag-catalog|doc-catalog) a=--check ;; *) a= ;; esac
  step "xtask-$s" cargo xtask $s $a
done
git status --porcelain --untracked-files=no > "$OUT/status-after.txt"
git checkout -q -- . && git clean -fdq -- crates
echo "clean: $(git status --porcelain --untracked-files=no | wc -l) paths"
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
