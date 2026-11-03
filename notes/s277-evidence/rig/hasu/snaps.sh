#!/usr/bin/env bash
# s277: the corpus snapshots at a commit (insta's update), copied out to
# /lane/records/snaps-<sha>/; the checkout put back clean.
set -u
source /lane/rig/env.sh
want=${1:?sha}
cd /lane/wolf-lang || exit 2
test "$(git rev-parse HEAD)" = "$(git rev-parse "$want")" || { echo "head is not $want"; exit 2; }
OUT=/lane/records/snaps-$want
rm -rf "$OUT"; mkdir -p "$OUT"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step snap-lex env INSTA_UPDATE=always cargo test --locked -p wolf_lex --test corpus_snapshots
step snap-parse env INSTA_UPDATE=always cargo test --locked -p wolf_parse --test corpus_decls
step snap-wir env INSTA_UPDATE=always cargo test --locked -p wolf_wir --test lower_corpus
git status --porcelain > "$OUT/status.txt"
git status --porcelain | awk '{print $2}' | grep '\.snap' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
git checkout -q -- . && git clean -fdq -- crates
echo "clean: $(git status --porcelain --untracked-files=no | wc -l)"
echo "ALL rc=$rc_all"
exit $rc_all
