#!/bin/bash
# s287: build the base (or any commit) in the build worktree and copy the
# binaries out by value to $LANE/bin-<sha>/ (run by build-at.sh, behind the lock).
set -u
SHA=$(git rev-parse --short=8 HEAD)
cargo build --locked -p wolf_driver -p wolf_rt || exit 1
OUT=$LANE/bin-$SHA
rm -rf "$OUT"; mkdir -p "$OUT"
cp target/debug/wolf target/debug/libwolf_rt.a "$OUT"/
sH1m -a 256 "$OUT"/wolf "$OUT"/libwolf_rt.a
"$OUT"/wolf --version
