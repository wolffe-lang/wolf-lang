#!/bin/bash
# s287: regenerate the diagnostics catalog at a commit and print its patch
# (between PATCH-BEGIN and PATCH-END), then put the tree back clean.
set -u
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
step diag-catalog cargo xtask diag-catalog
git status --porcelain --untracked-files=all
echo PATCH-BEGIN
git add -A docs && git diff --cached -- docs
echo PATCH-END
git reset -q && git checkout -q -- . && git clean -fdq -- docs
echo "== done $(date -u +%FT%TZ)"
