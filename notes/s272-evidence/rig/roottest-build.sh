#!/usr/bin/env bash
# s272 roottest, inside r41-toolchain: the wolf binary at the checkout's head
# and the two fixtures on both tiers, into /lane/records/roottest/t.
source /lane/scripts/env.sh
cd /lane/wolf-lang || exit 1
echo "head $(git rev-parse HEAD)"
cargo build --locked -p wolf_driver --bin wolf || exit 1
T=/lane/records/roottest/t
rm -rf "$T"; mkdir -p "$T/bin/native" "$T/bin/release" "$T/fx"
cp target/debug/wolf "$T/wolf"
for f in chroot_case priority_case; do
  cp crates/wolf_driver/tests/fixtures/os_surface/$f.lu "$T/fx/"
  ( cd "$T/fx" && mkdir -p b-$f && cp $f.lu b-$f/main.lu && cd b-$f && \
    /lane/wolf-lang/target/debug/wolf build main.lu -o "$T/bin/native/$f" --no-cache && \
    /lane/wolf-lang/target/debug/wolf build main.lu -o "$T/bin/release/$f" --no-cache --release ) || { echo "build of $f failed"; exit 1; }
  rm -rf "$T/fx/b-$f"
done
ls -la "$T/bin/native" "$T/bin/release"
sha256sum "$T/wolf" "$T"/bin/*/*
