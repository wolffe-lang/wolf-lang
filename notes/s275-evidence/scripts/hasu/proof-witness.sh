#!/usr/bin/env bash
# s275 job "proof-witness" (hasu, the block given). One rig, r41-toolchain:1,
# four of the block's CPUs for every run:
#   the head (head.sha): every signal.rs unit test (the s275 rows among
#     them), then drain.lu 200 runs native and 200 release, 3,000 rounds a
#     run, a 120 s cap: expected 0 red, 0 stalled;
#   trunk (red.sha: trunk's runtime, the hooks being cfg(test)): 200 runs
#     native, the same rounds, a 60 s cap: trunk's rate from the same rig;
#   the plant (plant.sha, the head with the errno read back after the
#     park): 30 runs native: expected red.
# usage: proof-witness.sh <A|B>
set -uo pipefail
L=/home/mfwolffe/lanes/s275
blk=${1:?block}
case $blk in
A) SIX=0-5 FOUR=0-3 ;;
B) SIX=6-11 FOUR=6-9 ;;
*) echo "block must be A or B"; exit 2 ;;
esac
head=$(cat "$L/head.sha")
red=$(cat "$L/red.sha")
plant=$(cat "$L/plant.sha")
echo "BEGIN proof-witness $(date -Iseconds) host $(hostname) block $blk six $SIX four $FOUR head $head trunk(red) $red plant $plant kernel $(uname -r)"
C="bash $L/scripts/ctr.sh"
R="bash $L/scripts/witness-runs.sh"
step() {
    name=$1
    shift
    echo "BEGIN $name $(date -Iseconds)"
    "$@"
    rc=$?
    echo "END $name exit=$rc $(date -Iseconds)"
}
step build-head bash "$L/scripts/build-at.sh" "$head" $SIX
step build-trunk bash "$L/scripts/build-at.sh" "$red" $SIX
step build-plant bash "$L/scripts/build-at.sh" "$plant" $SIX
WH=$L/b-$head
step rows-build $C net "$WH" taskset.real -c $SIX cargo test --locked -p wolf_rt --lib --no-run
step rows $C nonet "$WH" taskset.real -c $FOUR cargo test --locked --offline -p wolf_rt --lib -- signal::tests --test-threads=1
OH=$L/out/bin-$head
OT=$L/out/bin-$red
OP=$L/out/bin-$plant
step head-native $C nonet "$OH" taskset.real -c $FOUR $R head-native "$OH/drain-native" 200 3000 120
step head-release $C nonet "$OH" taskset.real -c $FOUR $R head-release "$OH/drain-release" 200 3000 120
step trunk-native $C nonet "$OT" taskset.real -c $FOUR $R trunk-native "$OT/drain-native" 200 3000 60
step plant-native $C nonet "$OP" taskset.real -c $FOUR $R plant-native "$OP/drain-native" 30 3000 60
echo "END proof-witness $(date -Iseconds)"
