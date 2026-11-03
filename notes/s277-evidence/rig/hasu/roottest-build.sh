#!/usr/bin/env bash
# s277 roottest, inside r41-toolchain (adapted by copy from s272's
# roottest-build.sh): the wolf binary at the checkout's head and the
# privilege fixture on both tiers, into /lane/records/roottest/t.
source /lane/rig/env.sh
cd /lane/wolf-lang || exit 1
echo "head $(git rev-parse HEAD)"
cargo build --locked -p wolf_driver -p wolf_rt || exit 1
T=/lane/records/roottest/t
rm -rf "$T"; mkdir -p "$T/bin" "$T/fx"
cp target/debug/wolf "$T/wolf"
f=privilege
cp crates/wolf_driver/tests/fixtures/descriptor_walk/$f.lu "$T/fx/"
( cd "$T/fx" && target=/lane/wolf-lang/target/debug/wolf && \
  $target build $f.lu -o "$T/bin/$f-native" --no-cache && \
  $target build $f.lu -o "$T/bin/$f-release" --no-cache --release ) || { echo "build of $f failed"; exit 1; }
ls -la "$T/bin"
sha256sum "$T/wolf" "$T"/bin/*
