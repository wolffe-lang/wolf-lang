#!/bin/bash
# s281 mac job 5 (run by build-at.sh in the build worktree, behind the lock):
# the parse-tree snapshots of corpus/grammar that the gauntlet's test step
# found missing (wolf_parse ambiguity_trees, red at 0fa55804 on hasu), made
# with insta's update, copied out to records/snaps-amb-<sha>/, the worktree
# put back clean.
set -u
L=/Users/mfwolffe/lanes/s281
SHA=$(git rev-parse --short=8 HEAD)
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step red-first env INSTA_UPDATE=no cargo test --locked -p wolf_parse --test ambiguity_trees
OUT=$L/records/snaps-amb-$SHA
rm -rf "$OUT"; mkdir -p "$OUT"
step snap-ambiguity env INSTA_UPDATE=always cargo test --locked -p wolf_parse --test ambiguity_trees
git status --porcelain --untracked-files=all > "$OUT/status.txt"; cat "$OUT/status.txt"
git status --porcelain --untracked-files=all | awk '{print $2}' | grep '\.snap' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
git checkout -q -- . && git clean -fdq -- crates
echo "clean: $(git status --porcelain --untracked-files=all -- crates | wc -l)"
echo "== done $(date -u +%FT%TZ)"
