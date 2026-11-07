#!/bin/bash
# s285 mac regen job (run by build-at.sh in the build worktree, behind the
# lock): `cargo xtask spec-extract` (and, with S285_CATALOG=1, `cargo xtask
# diag-catalog`), the regenerated files copied out to records/regen-<sha>/,
# anchors.json's key sets diffed both ways against the commit's, and the
# worktree put back clean.
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
OUT=$L/records/regen-$SHA
rm -rf "$OUT"; mkdir -p "$OUT"
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l'
cp spec/anchors.json "$OUT/anchors.before.json"
step spec-extract cargo xtask spec-extract
if [ "${S285_CATALOG:-0}" = 1 ]; then step diag-catalog cargo xtask diag-catalog; fi
step check cargo xtask spec-extract --check
keys() { python3 -I -c 'import json,sys; print("\n".join(sorted(json.load(open(sys.argv[1]))["anchors"])))' "$1"; }
step keys bash -c "diff <($(declare -f keys); keys $OUT/anchors.before.json) <($(declare -f keys); keys spec/anchors.json); echo removed=\$(comm -23 <($(declare -f keys); keys $OUT/anchors.before.json) <($(declare -f keys); keys spec/anchors.json) | wc -l) added=\$(comm -13 <($(declare -f keys); keys $OUT/anchors.before.json) <($(declare -f keys); keys spec/anchors.json) | wc -l)"
git status --porcelain > "$OUT/status.txt"; cat "$OUT/status.txt"
git status --porcelain | awk '{print $2}' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
git checkout -q -- . && echo "clean: $(git status --porcelain --untracked-files=no | wc -l)"
echo "== done $(date -u +%FT%TZ)"
