#!/bin/bash
# s282 mac probe job (run by build-at.sh in the build worktree, behind the
# lock): build wolf at the worktree's commit, then every probe on the four
# machines (checked, native, release, lupin 0.1.59), and the human rendering
# of each probe's diagnostics (`wolf build`, then `wolf fix`'s dry run), so a warning's text, its
# secondary span and its fixes are read, not just its code.
set -u
L=$LANE
export LUPIN=$L/inputs/lupin059/unpacked/lupin-0.1.59-aarch64-apple-darwin/lupin
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
head_info() { git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V && "$LUPIN" --version; }
step head head_info
step build cargo build --locked -p wolf_driver -p wolf_rt
W=$PWD/target/debug/wolf
files=(); for d in "$L"/probe/*/; do files+=("$d/main.lu"); done
step probes "$L/rig/probe-all.sh" "$W" "${files[@]}"
render() {
  for f in "${files[@]}"; do
    echo "## $(basename "$(dirname "$f")")"
    "$W" build "$f" -o "$TMPDIR/probe-bin" 2>&1 | head -60
    echo "   (build exit ${PIPESTATUS[0]})"
    "$W" fix "$f" 2>&1 | head -20
    echo "   (fix exit ${PIPESTATUS[0]})"
  done
}
step render render
echo "== done $(date -u +%FT%TZ)"
