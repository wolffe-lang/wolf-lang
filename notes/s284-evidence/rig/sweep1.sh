#!/usr/bin/env bash
# s284 job "sweep1": repro.sh over plan-sweep1.txt, 10 rounds, 100 min at most.
exec bash /home/mfwolffe/lanes/s284/rig/repro.sh "${1:?block}" /home/mfwolffe/lanes/s284/rig/plan-sweep1.txt 10 100 sweep1
