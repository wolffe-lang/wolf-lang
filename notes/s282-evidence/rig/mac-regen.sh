#!/bin/bash
# s282 mac regen job (run by build-at.sh in the build worktree, behind the
# lock): the generated artifacts at a commit. `cargo xtask spec-extract`
# (spec/anchors.json, spec/grammar.ebnf) and `cargo xtask diag-catalog`
# (docs/diagnostics.md, docs/warnings.md) rewrite their files, the row
# snapshot sets are remade with insta's update, and every changed file is
# copied out to records/regen-<sha>/; the worktree is put back clean.
# The anchor key sets before and after are diffed both ways.
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
OUT=$L/records/regen-$SHA
rm -rf "$OUT"; mkdir -p "$OUT"
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
keys() { python3 -I -c 'import json,sys; print("\n".join(sorted(json.load(open(sys.argv[1]))["anchors"])))' "$1"; }
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && rustc -V'
step build cargo build --locked -p wolf_driver -p wolf_rt || { echo "== build failed: stopping"; exit 1; }
keys spec/anchors.json > "$OUT/anchors-before.txt"
step spec-extract cargo xtask spec-extract
keys spec/anchors.json > "$OUT/anchors-after.txt"
echo "-- anchors removed:"; comm -23 "$OUT/anchors-before.txt" "$OUT/anchors-after.txt"
echo "-- anchors added:"; comm -13 "$OUT/anchors-before.txt" "$OUT/anchors-after.txt"
step diag-catalog cargo xtask diag-catalog
step snap-lex env INSTA_UPDATE=always cargo test --locked -p wolf_lex --test corpus_snapshots
step snap-parse env INSTA_UPDATE=always cargo test --locked -p wolf_parse --test corpus_decls
step snap-wir env INSTA_UPDATE=always cargo test --locked -p wolf_wir --test lower_corpus
git status --porcelain > "$OUT/status.txt"; cat "$OUT/status.txt"
git status --porcelain | awk '{print $2}' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
git diff --stat
git checkout -q -- . && git clean -fdq -- crates spec docs
echo "clean: $(git status --porcelain --untracked-files=no | wc -l)"
echo "== done $(date -u +%FT%TZ)"
