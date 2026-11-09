#!/usr/bin/env bash
# s289 on H2, via s287: stage, once and outside the slot's locks, every download the
# prep take's setup steps would make, so no take depends on the network (since
# about 08:57Z GitHub's release downloads answer 504 now and then). Each input
# is fetched with retries and checked against the digest the head pins, read
# twice; the prep then puts each one where its own script looks first, so no
# tracked script changes:
#   - the musl link bundle ci/fetch-entry-bundle pins (SHA, its cache
#     .link-bundle-cache/, which it reads before any download);
#   - clef at ci/build-test-clef's pin, with the two archives clef's
#     tools/fetch-toolchain pins in .wolf-bin/ (it downloads only a missing one);
#   - ligature at ci/build-test-ligature's pin, and the complete wolf 0.2.36
#     toolchain its WOLF input takes (the script's own offline path, which
#     ligature's wolf-toolchain.toml pins by digest).
# When GitHub answers 504 past the retries, an archive comes instead from a
# copy already on this host (FALLBACK_DIR's files, by name: an earlier take's
# own download, or another lane's input reused by copy), and is held to the
# same pinned digest, read twice; staged.txt names the source of each.
# Writes INPUTS/staged.sha256 (every archive, by its path under INPUTS, for the prep to read twice) and
# INPUTS/staged.txt (what was staged, from where).
#   FALLBACK_DIR=... stage-inputs.sh INPUTS
set -euo pipefail
inputs=$1
mkdir -p "$inputs"
cd "$inputs"
bundle_sha=8210ac2223d969ab8f0845cc74950a1c68226b58b10dd083cd2e2113acd3e37a
bundle_url=https://github.com/wolffe-lang/ligature/releases/download/link-bundle-wolf-0.2.36/musl-link-bundle-sha256-$bundle_sha.tar.gz
clef_pin=996a3ccdf160923757c4ce6aa474efdffa2135b3
lig_pin=c8ca1be4b3ccb7f8c425638e2d1145d21a62ee5a
fetch() { # URL OUT SHA: with retries, else FALLBACK_DIR's copy; the digest read twice
  local url=$1 out=$2 sha=$3 n src=$1 fb
  if [ ! -s "$out" ]; then
    for n in 1 2 3; do
      if curl -fsSL --retry 3 --retry-all-errors --retry-delay 20 -o "$out.partial" "$url"; then mv "$out.partial" "$out"; break; fi
      rm -f "$out.partial"
      printf 'FETCH_RETRY %s attempt=%s %s\n' "$url" "$n" "$(date -u +%FT%TZ)"; sleep 30
    done
  fi
  if [ ! -s "$out" ]; then
    fb=${FALLBACK_DIR:-/nonexistent}/$(basename "$out")
    test -s "$fb" || { printf 'FETCH_FAILED %s (no copy at %s)\n' "$url" "$fb"; exit 1; }
    cp "$(readlink -f "$fb")" "$out.partial" && mv "$out.partial" "$out"
    src="copy of $(readlink -f "$fb")"
    printf 'FETCH_FALLBACK %s from %s %s\n' "$url" "$src" "$(date -u +%FT%TZ)"
  fi
  for n in 1 2; do
    printf '%s  %s\n' "$sha" "$out" | sha256sum -c - || { printf 'REFUSED %s\n' "$out"; exit 1; }
  done
  printf '%s  %s\n' "$sha" "$out" >> staged.sha256.new
  printf 'archive %s from %s\n' "$inputs/$out" "$src" >> staged.txt.new
}
clone() { # REPO PIN DIR: with retries, detached at the pin, clean
  local repo=$1 pin=$2 dir=$3 n
  if [ ! -d "$dir/.git" ]; then
    for n in 1 2 3 4 5 6; do
      rm -rf "$dir.partial"
      if git clone -q --no-checkout "https://github.com/$repo.git" "$dir.partial"; then mv "$dir.partial" "$dir"; break; fi
      printf 'CLONE_RETRY %s attempt=%s %s\n' "$repo" "$n" "$(date -u +%FT%TZ)"; sleep 60
    done
    git -C "$dir" -c advice.detachedHead=false checkout -q --detach "$pin"
  fi
  test "$(git -C "$dir" rev-parse HEAD)" = "$pin"
  test -z "$(git -C "$dir" status --porcelain --untracked-files=no)"
  printf '%s %s %s\n' "$repo" "$pin" "$inputs/$dir" >> staged.txt.new
}
: > staged.sha256.new; : > staged.txt.new
mkdir -p link-bundle
fetch "$bundle_url" link-bundle/ld02-musl-link-bundle.tar.gz "$bundle_sha"
clone wolffe-lang/clef "$clef_pin" "test-clef-${clef_pin:0:8}"
mkdir -p "test-clef-${clef_pin:0:8}/.wolf-bin"
python3 -I - "test-clef-${clef_pin:0:8}/wolf-toolchain.toml" > clef-archives.txt <<'PY'
import sys, tomllib
for name, p in tomllib.loads(open(sys.argv[1]).read()).items():
    base = f'{name}-{p["version"]}-x86_64-unknown-linux-gnu'
    print(f'https://github.com/{p["repo"]}/releases/download/v{p["version"]}/{base}.tar.gz {base}.tar.gz {p["sha256"]}')
PY
while read -r url file sha; do
  fetch "$url" "test-clef-${clef_pin:0:8}/.wolf-bin/$file" "$sha"
done < clef-archives.txt
clone wolffe-lang/ligature "$lig_pin" "test-ligature-${lig_pin:0:8}"
read -r wolf_ver wolf_sha <<< "$(python3 -I -c 'import sys, tomllib; p = tomllib.loads(open(sys.argv[1]).read())["wolf"]; print(p["version"], p["x86_64-unknown-linux-gnu"])' "test-ligature-${lig_pin:0:8}/wolf-toolchain.toml")"
test "$wolf_ver" = 0.2.36
mkdir -p "wolf-$wolf_ver"
fetch "https://github.com/wolffe-lang/wolf-lang/releases/download/v$wolf_ver/wolf-$wolf_ver-x86_64-unknown-linux-gnu.tar.gz" "wolf-$wolf_ver/wolf-$wolf_ver-x86_64-unknown-linux-gnu.tar.gz" "$wolf_sha"
if [ ! -x "wolf-$wolf_ver/wolf-$wolf_ver-x86_64-unknown-linux-gnu/wolf" ]; then
  tar -xzf "wolf-$wolf_ver/wolf-$wolf_ver-x86_64-unknown-linux-gnu.tar.gz" -C "wolf-$wolf_ver"
fi
v=$("wolf-$wolf_ver/wolf-$wolf_ver-x86_64-unknown-linux-gnu/wolf" --version | head -1)
test "$v" = 'wolf 0.2.36 (wolfgang, pin 5adf3d9)'
printf 'wolf-lang release v%s %s (%s)\n' "$wolf_ver" "$inputs/wolf-$wolf_ver/wolf-$wolf_ver-x86_64-unknown-linux-gnu/wolf" "$v" >> staged.txt.new
mv staged.sha256.new staged.sha256; mv staged.txt.new staged.txt
printf 'STAGED %s\n' "$(date -u +%FT%TZ)"; cat staged.txt; cat staged.sha256
