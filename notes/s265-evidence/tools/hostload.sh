#!/bin/bash
# Print the host's load average and the busy share of CPUs 4-7 and their
# SMT siblings 12-15 over a one-second window (/proc/stat deltas).
read -r l1 l5 l15 _ < /proc/loadavg
snap() { awk '/^cpu(4|5|6|7|12|13|14|15) /{b=$2+$3+$4+$7+$8; t=b+$5+$6; print $1, b, t}' /proc/stat; }
a=$(snap); sleep 1; b=$(snap)
busy=$(paste <(echo "$a") <(echo "$b") | awk '{printf "%s=%.0f%% ", $1, 100*($5-$2)/(($6-$3)>0?($6-$3):1)}')
echo "load=$l1/$l5/$l15 busy: $busy"
