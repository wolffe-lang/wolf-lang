#!/usr/bin/env bash
# s275: the release wolf and runtime at <sha>, in the worktree b-<sha> (a
# copy pinned to that commit, never a live tree), then drain.lu built on
# both tiers into out/bin-<sha>/. Once per commit: a marker records a
# finished build, and a lock file serializes two jobs that want the same one.
# usage: build-at.sh <sha> <cpus>
L=/home/mfwolffe/lanes/s275
sha=${1:?sha}
cpus=${2:?cpus}
W=$L/b-$sha
O=$L/out/bin-$sha
exec 9>"$L/b-$sha.lock"
flock 9
git -C "$L/wolf-lang" fetch -q origin s275 s275-plant-errno || { echo "REFUSED fetch"; exit 2; }
[ -d "$W" ] || git -C "$L/wolf-lang" worktree add -q --detach "$W" "$sha" || { echo "REFUSED worktree"; exit 2; }
test "$(git -C "$W" rev-parse HEAD)" = "$sha" || { echo "REFUSED head $(git -C "$W" rev-parse HEAD) != $sha"; exit 2; }
git -C "$W" diff --quiet || { echo "REFUSED dirty"; exit 2; }
if [ -f "$O/built" ]; then
    echo "built already at $sha: $(cat "$O/built")"
    exit 0
fi
mkdir -p "$O"
C="bash $L/scripts/ctr.sh"
$C net "$W" taskset.real -c "$cpus" cargo build --locked --release -p wolf_driver -p wolf_rt || exit 3
F=crates/wolf_driver/tests/fixtures/fork/drain.lu
$C nonet "$W" taskset.real -c "$cpus" target/release/wolf build --no-cache $F -o "$O/drain-native" || exit 3
$C nonet "$W" taskset.real -c "$cpus" target/release/wolf build --no-cache --release $F -o "$O/drain-release" || exit 3
sha256sum "$W/target/release/wolf" "$W/target/release/libwolf_rt.a" "$O/drain-native" "$O/drain-release"
date -Iseconds >"$O/built"
