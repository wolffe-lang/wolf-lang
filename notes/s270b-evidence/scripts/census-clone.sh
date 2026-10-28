#!/usr/bin/env bash
# s270b: fresh shallow clones of every downstream wolf repo at its trunk head, for the census,
# and wolf-std at the STD-PIN sha (the census's std, and the gauntlet's dist source).
set -uo pipefail
base=/home/mfwolffe/lanes/s270b
mkdir -p "$base/census"
cd "$base/census"
for r in aspen boreutils clef cub den fugue jobq lair ligature lobo lope mk muzzle pack pax pelt scent spell tomlq trail wolf-book wolf-lsp wolf-std yew; do
  if [ ! -d "$r" ]; then
    git clone --quiet --depth 1 "https://github.com/wolffe-lang/$r.git" "$r" || echo "CLONE FAILED $r"
  fi
  printf '%s %s %s\n' "$r" "$(git -C "$r" rev-parse HEAD)" "$(find "$r" -name '*.lu' -not -path '*/.git/*' -not -path '*/target/*' | wc -l)"
done
cd "$base"
if [ ! -d wolf-std ]; then
  git clone --quiet https://github.com/wolffe-lang/wolf-std.git wolf-std
fi
git -C wolf-std checkout --quiet --detach 78b9d3cbc41d51db44d18c40b6694403644d1c83
git -C wolf-std rev-parse HEAD
ls wolf-std/std | head -5
