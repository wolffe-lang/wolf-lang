#!/usr/bin/env bash
# s272: the rows chain on kasumi (inside job.sh's hold of slot 8-11): the
# statfs rows at the red commit and the fix with the scratch on a private
# tmpfs (a file system that counts an unsynced write at once), then the fix
# with the scratch on kasumi's btrfs (which counts it at its commit).
# usage: chain-rows.sh <red-sha> <fix-sha>
set -u
S=/home/mfwolffe/lanes/s272/scripts
red=$1 fix=$2
bash $S/container.sh rows-tmpfs 1 /lane/scripts/rows-inner.sh red "$red" fix "$fix"
a=$?
bash $S/container.sh rows-btrfs 0 /lane/scripts/rows-inner.sh fix "$fix"
b=$?
echo "S272 chain-rows tmpfs=$a btrfs=$b"
[ $a -eq 0 ] && [ $b -eq 0 ]
