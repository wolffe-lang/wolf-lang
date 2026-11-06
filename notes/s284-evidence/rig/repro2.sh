#!/usr/bin/env bash
# s284 job "repro2": the measurement build at 8e8ce00faa1a0b7acf96e9458aa5a62ef47ebbc9, then 30 interleaved rounds of
# plan-repro2.txt (the quiet pinned row), 14 minutes at most for the runs.
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
case $blk in A) SIX=0-5 ;; B) SIX=6-11 ;; *) echo "block must be A or B"; exit 2 ;; esac
bash $L/rig/build-at.sh 8e8ce00faa1a0b7acf96e9458aa5a62ef47ebbc9 $SIX || exit $?
exec bash $L/rig/repro.sh "$blk" $L/rig/plan-repro2.txt 30 14 repro2
