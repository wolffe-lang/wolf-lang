#!/usr/bin/env bash
# s275 job "red" (hasu, the block given): the red rows at the commit named
# in red.sha (trunk 7332772f + the prediction + the red rows; the runtime a
# program links is trunk's, the hooks being cfg(test)). In r41-toolchain:1:
#   build    the release wolf and runtime (the runtime a release archive ships)
#   rows     wolf_rt's three s275 rows (each in a child of the test binary),
#            under four of the block's CPUs: expected 3 FAILED
#   witness  fork_lanes' drain.lu, native and release tiers, 30 runs each of
#            3,000 rounds under four of the block's CPUs, 60 s cap a run
# Every step prints END <step> exit=N.
# usage: red.sh <A|B>
set -uo pipefail
L=/home/mfwolffe/lanes/s275
blk=${1:?block}
case $blk in
A) SIX=0-5 FOUR=0-3 ;;
B) SIX=6-11 FOUR=6-9 ;;
*) echo "block must be A or B"; exit 2 ;;
esac
sha=$(cat "$L/red.sha")
echo "BEGIN red $(date -Iseconds) host $(hostname) block $blk six $SIX four $FOUR sha $sha kernel $(uname -r)"
git -C "$L/wolf-lang" fetch -q origin s275 || { echo "REFUSED fetch"; exit 2; }
W=$L/b-$sha
[ -d "$W" ] || git -C "$L/wolf-lang" worktree add -q --detach "$W" "$sha" || { echo "REFUSED worktree"; exit 2; }
test "$(git -C "$W" rev-parse HEAD)" = "$sha" || { echo "REFUSED head"; exit 2; }
git -C "$W" diff --quiet || { echo "REFUSED dirty"; exit 2; }
O=$L/out/red-$sha
mkdir -p "$O"
C="bash $L/scripts/ctr.sh"
step() {
    name=$1
    shift
    echo "BEGIN $name $(date -Iseconds)"
    "$@"
    rc=$?
    echo "END $name exit=$rc $(date -Iseconds)"
}
step build $C net "$W" taskset.real -c $SIX cargo build --locked --release -p wolf_driver -p wolf_rt
step rows-build $C net "$W" taskset.real -c $SIX cargo test --locked -p wolf_rt --lib --no-run
step rows $C nonet "$W" taskset.real -c $FOUR cargo test --locked --offline -p wolf_rt --lib -- s275 --test-threads=1
F=crates/wolf_driver/tests/fixtures/fork/drain.lu
step witness-build-native $C nonet "$W" taskset.real -c $SIX target/release/wolf build --no-cache $F -o "$O/drain-native"
step witness-build-release $C nonet "$W" taskset.real -c $SIX target/release/wolf build --no-cache --release $F -o "$O/drain-release"
step witness-native $C nonet "$O" taskset.real -c $FOUR bash $L/scripts/witness-runs.sh trunk-native "$O/drain-native" 30 3000 60
step witness-release $C nonet "$O" taskset.real -c $FOUR bash $L/scripts/witness-runs.sh trunk-release "$O/drain-release" 30 3000 60
echo "END red $(date -Iseconds)"
