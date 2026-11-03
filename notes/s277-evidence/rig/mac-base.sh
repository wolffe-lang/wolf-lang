#!/bin/bash
# s277 mac job: build wolf at the base commit (the pinned build worktree) and
# run each issue's witness on native and checked. Expected: E0301, build exit 2;
# bit 32 of fs_open_mode answers invalid.
set -u
B=/Users/mfwolffe/lanes/s277/build
E=/Users/mfwolffe/lanes/s277/ev/base
want=${1:?base sha}
cd "$B" || exit 2
test "$(git rev-parse HEAD)" = "$want" || { echo "build worktree is not at $want"; exit 2; }
test -z "$(git status --porcelain --untracked-files=no)" || { echo "build worktree dirty"; exit 2; }
echo "== build at $(git rev-parse HEAD) $(date +%H:%M:%S)"
rustc --version
cargo build --locked -j4 -p wolf_driver -p wolf_rt 2>&1 | tail -3
rc=${PIPESTATUS[0]}
echo "END build exit=$rc"
[ "$rc" -eq 0 ] || exit "$rc"
W=$B/target/debug/wolf
cd "$E" || exit 2
for s in */; do
  s=${s%/}
  cd "$E/$s" || exit 2
  p=main.lu
  echo "== $s/$p"
  "$W" build "$p" -o "$E/$s.bin" --no-cache > "$E/$s.build.out" 2>&1
  brc=$?
  echo "   native: wolf build exit $brc — $(grep -m1 'error\[' "$E/$s.build.out")"
  if [ "$brc" -eq 0 ]; then
    "$E/$s.bin" > "$E/$s.run.out" 2>&1
    echo "   native run exit $?: $(tr '\n' ' ' < "$E/$s.run.out")"
  fi
  "$W" conform-run "$p" --checked --json > "$E/$s.checked.json" 2>&1
  echo "   checked: conform-run exit $? — $(grep -o '"verdict":"[^"]*"' "$E/$s.checked.json" | head -1) $(grep -o 'error\[E[0-9]*\][^"]*' "$E/$s.checked.json" | head -1)"
  cd "$E"
done
