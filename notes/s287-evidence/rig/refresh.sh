#!/bin/bash
# s287: regenerate what the rows and the registry text move, at a commit:
# every corpus-walking snapshot suite with insta's update on (snaps.sh), then
# the diagnostics catalog (cargo xtask diag-catalog). Every file changed is
# copied out to $LANE/records/refresh-<sha>/ with the status list, and the
# tree is put back clean.
set -u
L=$LANE
SHA=$(cat .s287-sha 2>/dev/null || git rev-parse --short=8 HEAD)
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
UPDATE=1 NOCOPY=1 bash "$L/rig/snaps.sh"
# After the snapshots: the catalog's fixture rule reads the snapshot files.
step diag-catalog cargo xtask diag-catalog
OUT=$L/records/refresh-$SHA
rm -rf "$OUT"; mkdir -p "$OUT"
git status --porcelain --untracked-files=all > "$OUT/status.txt"; cat "$OUT/status.txt"
git status --porcelain --untracked-files=all | awk '{print $2}' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
git diff --stat | tail -3
git checkout -q -- . && git clean -fdq -- crates docs
echo "== done $(date -u +%FT%TZ)"
