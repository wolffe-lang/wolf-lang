#!/usr/bin/env bash
# s272: after kasumi's reboot cut drain2 at busy i=38: the busy and forks loads
# (appended to runs2.txt), then CI's container shape (chain-drain3.sh), in one
# hold of the slot.
# usage: chain-drain4.sh <slot> <runs2> <runs3> <label>...
slot=$1 n2=$2 n3=$3; shift 3
bash /home/mfwolffe/lanes/s272/scripts/container.sh "$slot" drain2b 0 /lane/scripts/drain2-inner.sh --loads=busy,forks "$n2" "$@"
bash /home/mfwolffe/lanes/s272/scripts/chain-drain3.sh "$slot" "$n3" "$@"
