#!/usr/bin/env bash
# s275: the gauntlet's inputs, each read twice (copied from s273's, itself s271's).
set -euo pipefail
source /lane/scripts/gate-env.sh
cd /lane/wolf-lang
expected=${1:?expected head}
test "$(git rev-parse HEAD)" = "$expected"
printf 'S275_PREP_START %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
command -v clang ld.lld strace cc
clang --version
ld.lld --version
rustup target list --installed
for target in x86_64-unknown-linux-musl x86_64-unknown-none; do
  libdir=$(rustc --print target-libdir --target "$target")
  ls "$libdir"/libcore-*.rlib
done
for input in \
  '34f52792bb59bc7565e0edc1235cd47d9a6ad13d5e6528993627f2e699b62193  /inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz' \
  '4cc4045bbbb605c07835bc4f32685953c784eeef4cd3139a48b457dfc90dad1a  /inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz'; do
  for pass in 1 2; do printf '%s\n' "$input" | sha256sum -c -; done
done
"$LUPIN" --version
if ! test -d /lane/wolf-std/.git; then
  git clone https://github.com/wolffe-lang/wolf-std.git /lane/wolf-std
fi
git -C /lane/wolf-std checkout --detach 78b9d3cbc41d51db44d18c40b6694403644d1c83
for pass in 1 2; do
  test "$(git -C /lane/wolf-std rev-parse HEAD)" = 78b9d3cbc41d51db44d18c40b6694403644d1c83
  git -C /lane/wolf-std rev-parse HEAD
done
cargo build --locked -p wolf_driver -p wolf_rt
ci/fetch-entry-bundle
cargo xtask dist-rt --target x86_64-unknown-linux-musl
first=$(sha256sum "$WOLF_STATIC_ENTRY_MUSL_RT")
second=$(sha256sum "$WOLF_STATIC_ENTRY_MUSL_RT")
printf '%s\n%s\n' "$first" "$second"
test "$first" = "$second"
ci/fetch-test-link-bundle
ci/build-test-clef
WOLF=/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu.tar.gz/unpacked/wolf-0.2.35-x86_64-unknown-linux-gnu/wolf ci/build-test-ligature
printf 'S275_PREP_GREEN %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
