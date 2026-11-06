#!/bin/bash
# s282 mac shakedown (run by build-at.sh in the build worktree at the head,
# behind the lock): the whole `cargo xtask ci` on arm64 macOS (every shard,
# in the gauntlet's own order), with lupin 0.1.59 as the paired sibling and
# no global WOLF_STD. Not a record: the record is the Linux gauntlet at the
# final head; this shows the macOS gauntlet's answer before it.
set -u
L=$LANE
export LUPIN=$L/inputs/lupin059/unpacked/lupin-0.1.59-aarch64-apple-darwin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1
unset WOLF_STD
echo "== head $(git rev-parse HEAD) $(date -u +%FT%TZ)"
rustc -V; "$LUPIN" --version
cargo xtask ci 2>&1
rc=$?
echo "== xtask ci exit=$rc $(date -u +%FT%TZ)"
exit $rc
