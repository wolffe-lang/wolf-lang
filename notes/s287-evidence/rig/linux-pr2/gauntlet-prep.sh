#!/usr/bin/env bash
# The prep take's first step: a fresh target for this head, the pins read
# twice against the head's own STD-PIN and PAIRING, then CI's setup steps.
set -euo pipefail
source /lane/rig/gauntlet-env.sh
cd /work
if test -e target; then echo "target/ already exists: not a fresh target for this head"; exit 1; fi
command -v clang ld.lld strace cc
rustc --version
clang --version
ld.lld --version
for target in x86_64-unknown-linux-musl x86_64-unknown-none; do
  libdir=$(rustc --print target-libdir --target "$target")
  ls "$libdir"/libcore-*.rlib
done
std_pin=$(sed -n 's/^wolf-std-commit *= *//p' crates/wolf_driver/STD-PIN | tr -d '[:space:]')
lupin_ver=$(sed -n 's/^lupin-version *= *//p' crates/wolf_driver/PAIRING | tr -d '[:space:]')
printf 'STD-PIN %s  PAIRING lupin-version %s\n' "$std_pin" "$lupin_ver"
test "$std_pin" = d4a4d4cab358e1e8c174f0206f9e13ad5cfe6c61
test "$lupin_ver" = 0.1.60
for pass in 1 2; do
  printf 'e7d4c4f1534736c9262f4ccd01a631f03ff70e3b8122eefe6e9ec1e15c02637e  /lane/inputs/lupin060/lupin-0.1.60-x86_64-unknown-linux-gnu.tar.gz\n' | sha256sum -c -
  test "$(git -C "$WOLF_DIST_STD_SRC" rev-parse HEAD)" = "$std_pin"
  git -C "$WOLF_DIST_STD_SRC" rev-parse HEAD
done
v=$("$LUPIN" --version)
printf '%s\n' "$v"
case "$v" in *0.1.60*) ;; *) echo "lupin is not 0.1.60"; exit 1 ;; esac
mkdir -p target
# The setup scripts write their paths here exactly as they write CI's GITHUB_ENV.
export GITHUB_ENV=/work/target/s287-ci-env
: > "$GITHUB_ENV"
# CI's step semantics: what one step writes to GITHUB_ENV is the next step's environment.
ci_step() { "$@"; set -a; . "$GITHUB_ENV"; set +a; }
cargo build --locked -p wolf_driver -p wolf_rt
ci_step python3 -I ci/fetch-entry-bundle
cargo xtask dist-rt --target x86_64-unknown-linux-musl
rt=$PWD/target/dist-rt/x86_64-unknown-linux-musl/release/libwolf_rt.a
sha256sum "$rt"; sha256sum "$rt"
ci_step sh -c 'echo "WOLF_STATIC_ENTRY_MUSL_RT=$1" >> "$GITHUB_ENV"' sh "$rt"
ci_step ci/build-test-clef
# CI's refusal check first: a corrupt cached bundle must be refused by digest.
mkdir -p target/link-bundle-negative
printf broken > target/link-bundle-negative/ld02-musl-link-bundle.tar.gz
if python3 -I ci/fetch-test-link-bundle target/link-bundle-negative > target/link-bundle-negative.log 2>&1; then
  echo "corrupt cached bundle was accepted"; exit 1
fi
grep 'REFUSED ld02-musl-link-bundle.tar.gz: sha256' target/link-bundle-negative.log
ci_step python3 -I ci/fetch-test-link-bundle
# Its own pinned fetcher supplies the verified toolchain.
ci_step ci/build-test-ligature
echo "the setup scripts' environment ($GITHUB_ENV):"
cat "$GITHUB_ENV"
