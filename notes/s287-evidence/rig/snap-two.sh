#!/bin/bash
# s287: the lexer and parser corpus snapshot suites with insta's update on, at
# a commit; prints the patch of every snapshot they changed (between the
# PATCH-BEGIN and PATCH-END lines) and puts the tree back clean.
set -u
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
for t in wolf_lex:corpus_snapshots wolf_parse:corpus_decls; do
  step "suite-${t/:/-}" env INSTA_UPDATE=always cargo test --locked -p "${t%%:*}" --test "${t##*:}"
done
git status --porcelain --untracked-files=all
echo PATCH-BEGIN
git add -A crates && git diff --cached -- crates
echo PATCH-END
git reset -q && git checkout -q -- . && git clean -fdq -- crates
echo "== done $(date -u +%FT%TZ)"
