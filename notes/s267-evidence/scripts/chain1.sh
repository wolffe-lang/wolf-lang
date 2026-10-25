#!/bin/bash
# s267 chain 1 at the head: lint (ctr-g), the rows (ctr-u), the bundle
# candidate (ctr-u), the rows against the candidate's runtime (ctr-u). Each
# leg takes the CPUs 12-15 lock for itself.
set -u
L=/home/mfwolffe/lanes/s267
S267_JOB=lint $L/scripts/ctr-g bash $L/scripts/lint.sh; a=$?; echo "lint exit=$a"
S267_JOB=rows $L/scripts/ctr-u bash $L/scripts/rows.sh after2; b=$?; echo "rows exit=$b"
S267_JOB=bundle $L/scripts/ctr-u bash $L/scripts/bundle.sh; c=$?; echo "bundle exit=$c"
d=9
if [ $c = 0 ]; then S267_JOB=rows-cand $L/scripts/ctr-u bash $L/scripts/rows.sh candidate $L/bundle/stage/libwolf_rt.a; d=$?; fi
echo "rows-candidate exit=$d"
echo "chain1 lint=$a rows=$b bundle=$c candidate-rows=$d"
[ $a = 0 ] && [ $b = 0 ] && [ $c = 0 ] && [ $d = 0 ]
