#!/usr/bin/env bash
# s284 job "repro3": repro.sh over plan-repro3.txt, 12 rounds, 15 min at most.
exec bash /home/mfwolffe/lanes/s284/rig/repro.sh "${1:?block}" /home/mfwolffe/lanes/s284/rig/plan-repro3.txt 12 15 repro3
