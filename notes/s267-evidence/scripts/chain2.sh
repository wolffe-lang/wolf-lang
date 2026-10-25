#!/bin/bash
# s267 chain 2, after chain 1: the gauntlet, then the differ against trunk
# (both ctr-g, each leg taking the CPUs 12-15 lock for itself).
set -u
L=/home/mfwolffe/lanes/s267
while [ ! -e $L/evidence/chain1.exit ]; do sleep 30; done
echo "chain2 after chain1 exit=$(cat $L/evidence/chain1.exit) start=$(date -u +%FT%TZ)"
S267_JOB=gauntlet $L/scripts/ctr-g bash $L/scripts/gauntlet.sh; g=$?
S267_JOB=differ $L/scripts/ctr-g bash $L/scripts/differ.sh; d=$?
echo "chain2 gauntlet=$g differ=$d end=$(date -u +%FT%TZ)"
[ $g = 0 ] && [ $d -le 1 ]
