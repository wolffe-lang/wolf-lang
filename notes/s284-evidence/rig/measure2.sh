#!/usr/bin/env bash
# s284 job "measure2" (R-s284-3): both detector builds, then 50 interleaved
# rounds of plan-measure2.txt (the quiet pinned row), 25 minutes at most.
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
case $blk in A) SIX=0-5 ;; B) SIX=6-11 ;; *) echo "block must be A or B"; exit 2 ;; esac
for sha in 0d6fb1ff93d6ca7f161d1a2a61ea476e14677921 3062e28eb721d281b5105942e8ef14b7690bba49; do
    bash $L/rig/build-at.sh $sha $SIX || exit $?
    rm -rf "$L/b-$sha/target" && git -C $L/wolf-lang worktree remove "$L/b-$sha" && echo "PRUNED b-$sha"
done
exec bash $L/rig/repro.sh "$blk" $L/rig/plan-measure2.txt 50 25 measure2
