#!/usr/bin/env bash
# s272: the rows a rebase can move, at one commit, on kasumi (inside job.sh's
# hold of a slot), the scratch on btrfs.
# usage: chain-verify.sh <slot> <sha>
bash /home/mfwolffe/lanes/s272/scripts/container.sh "$1" verify 0 /lane/scripts/verify-inner.sh "$2"
