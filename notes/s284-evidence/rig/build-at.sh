#!/usr/bin/env bash
# s284: wolf and its runtime at <sha>, in the worktree b-<sha> (a copy
# pinned to that commit, never a live tree), on BOTH cargo profiles:
#   dbg  `cargo build` — CI's shape: fork_lanes builds drain.lu with
#        `cargo test`'s target/debug/wolf, which links the debug-profile
#        target/debug/libwolf_rt.a beside it;
#   rel  `cargo build --release` — s275's rig and a release archive.
# Then drain.lu on both wolf tiers with each: bins/<sha>/drain-<dbg|rel>-<native|release>.
# A marker records a finished build. Every step prints END <step> exit=N.
# usage: build-at.sh <sha> <cpus> [ref-to-fetch]
set -uo pipefail
L=/home/mfwolffe/lanes/s284
sha=${1:?sha}
cpus=${2:?cpus}
ref=${3:-}
W=$L/b-$sha
O=$L/bins/$sha
C="bash $L/rig/ctr.sh"
step() {
    name=$1
    shift
    echo "START $name $(date -u +%FT%TZ)"
    "$@" 2>&1 | sed 's/^/  /'
    rc=${PIPESTATUS[0]}
    echo "END $name exit=$rc $(date -u +%FT%TZ)"
    return "$rc"
}
if [ -n "$ref" ]; then
    git -C "$L/wolf-lang" fetch -q origin "$ref" || { echo "REFUSED fetch $ref"; exit 2; }
fi
[ -d "$W" ] || git -C "$L/wolf-lang" worktree add -q --detach "$W" "$sha" || { echo "REFUSED worktree"; exit 2; }
test "$(git -C "$W" rev-parse HEAD)" = "$sha" || { echo "REFUSED head $(git -C "$W" rev-parse HEAD) != $sha"; exit 2; }
git -C "$W" diff --quiet || { echo "REFUSED dirty"; exit 2; }
if [ -f "$O/built" ]; then
    echo "built already at $sha: $(cat "$O/built")"
    exit 0
fi
if [ -e "$W/target" ]; then echo "REFUSED: $W/target exists, not a fresh target for this head"; exit 2; fi
mkdir -p "$O"
echo "BUILD $sha cpus=$cpus kernel=$(uname -r) $(date -u +%FT%TZ)"
step build-dbg $C net "$W" taskset.real -c "$cpus" cargo build --locked -p wolf_driver -p wolf_rt || exit 3
step build-rel $C net "$W" taskset.real -c "$cpus" cargo build --locked --release -p wolf_driver -p wolf_rt || exit 3
F=crates/wolf_driver/tests/fixtures/fork/drain.lu
for prof in dbg rel; do
    case $prof in dbg) wolf=target/debug/wolf ;; rel) wolf=target/release/wolf ;; esac
    step "drain-$prof-native" $C nonet "$W" taskset.real -c "$cpus" "$wolf" build --no-cache $F -o "$O/drain-$prof-native" || exit 3
    step "drain-$prof-release" $C nonet "$W" taskset.real -c "$cpus" "$wolf" build --no-cache --release $F -o "$O/drain-$prof-release" || exit 3
done
step digests $C nonet "$W" sha256sum target/debug/wolf target/debug/libwolf_rt.a target/release/wolf target/release/libwolf_rt.a "$O"/drain-*
step symbols $C nonet "$O" bash -c 'for b in drain-*; do nm "$b" | grep -E "signal3HUB|READER_STATE|signal3sys3READER" | sed "s|^|$b |"; done' | tee "$O/symbols.txt"
date -u +%FT%TZ >"$O/built"
echo "BUILT $sha $(cat "$O/built")"
