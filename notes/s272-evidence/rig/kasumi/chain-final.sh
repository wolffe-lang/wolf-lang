#!/usr/bin/env bash
# s272: the final chain on kasumi (inside job.sh's hold of a slot): the plant's
# row (or `-` to skip it), prep and the gauntlet, one container, the scratch on
# btrfs.
# usage: chain-final.sh <slot> <final-sha> <plant-sha|->
set -u
bash /home/mfwolffe/lanes/s272/scripts/container.sh "$1" final 0 /lane/scripts/final-inner.sh "$2" "$3"
