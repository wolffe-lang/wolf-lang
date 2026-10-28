#!/usr/bin/env bash
# s270b: is trunk's trap(bounds) a change between the releases, or the dev build's runtime?
# (a) v0.2.35's own source (cf0c13d) built as a dev build; (b) trunk built with --release.
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/bisect
git checkout --quiet --detach cf0c13d38479f444b69bb8356ad5709d07eff323
git rev-parse HEAD
cargo build --locked -q -p wolf_driver -p wolf_rt 2>&1 | tail -3
ls -la target/debug/wolf target/debug/libwolf_rt.a
cd /lane/base
git rev-parse HEAD 2>/dev/null || echo "base worktree (5adf3d91)"
cargo build --locked -q --release -p wolf_driver -p wolf_rt 2>&1 | tail -3
ls -la target/release/wolf target/release/libwolf_rt.a
cd /lane
python3 -I /lane/scripts/sweep.py build-profile.jsonl dev035=/lane/bisect/target/debug/wolf,rel036=/lane/base/target/release/wolf,dev036=/lane/base/target/debug/wolf 1024 1,2,16,17,24,25,64 --native --release
