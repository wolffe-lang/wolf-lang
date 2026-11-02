#!/usr/bin/env bash
# s272: the final chain on kasumi (inside job.sh's hold of slot 8-11): the
# plant's row, prep and the gauntlet, one container, the scratch on btrfs.
# usage: chain-final.sh <final-sha> <plant-sha>
set -u
bash /home/mfwolffe/lanes/s272/scripts/container.sh final 0 /lane/scripts/final-inner.sh "$1" "$2"
