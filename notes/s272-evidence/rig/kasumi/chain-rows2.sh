#!/usr/bin/env bash
# s272: the rows chain at one commit on kasumi (inside job.sh's hold of slot
# 8-11): the statfs rows and os_surface_lanes whole twice, with the scratch on
# a private tmpfs and then on kasumi's btrfs.
# usage: chain-rows2.sh <sha>
set -u
S=/home/mfwolffe/lanes/s272/scripts
bash $S/container.sh rows2-tmpfs 1 /lane/scripts/rows-inner.sh --all=2 head "$1"
a=$?
bash $S/container.sh rows2-btrfs 0 /lane/scripts/rows-inner.sh --all=2 head "$1"
b=$?
echo "S272 chain-rows2 tmpfs=$a btrfs=$b"
[ $a -eq 0 ] && [ $b -eq 0 ]
