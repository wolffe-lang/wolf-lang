#!/usr/bin/env bash
# s284 job "proof1": repro.sh over plan-proof1.txt, 38 rounds (304 runs of the fix,
# 152 of trunk), no new round past 100 minutes.
exec bash /home/mfwolffe/lanes/s284/rig/repro.sh "${1:?block}" /home/mfwolffe/lanes/s284/rig/plan-proof1.txt 38 100 proof1
