#!/bin/bash
# s287: fresh shallow clones of the org's wolf trees at their trunks (the
# censuses' inputs), plus wolf-std at the compiler's STD-PIN.
set -u
cd "$LANE/org" || exit 1
for r in aspen boreutils burrow clef cub den fenrir fugue jobq lair ligature lobo lope mk muzzle pack pax pelt scent sniff spell tomlq trail tree-sitter-wolf wolf-book wolf-interp wolf-lsp wolf-std wolf-web yew PRIVATE; do
  if [ -d "$r" ]; then echo "have $r"; continue; fi
  git clone -q --depth 1 "git@github.com:wolffe-lang/$r.git" "$r" && echo "$r $(git -C $r rev-parse HEAD)" || echo "FAILED $r"
done
PIN=$(awk -F' = ' '/^wolf-std-commit/ {print $2}' "$LANE/wolf-lang/crates/wolf_driver/STD-PIN")
if [ ! -d "wolf-std-pin" ]; then
  git clone -q git@github.com:wolffe-lang/wolf-std.git wolf-std-pin && git -C wolf-std-pin checkout -q --detach "$PIN" && echo "wolf-std-pin $(git -C wolf-std-pin rev-parse HEAD)"
fi
echo "== clone done $(date -u +%FT%TZ)"
