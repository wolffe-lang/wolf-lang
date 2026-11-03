#!/usr/bin/env bash
# s277 roottest job (adapted by copy from s272's roottest-job.sh): one block's
# two locks (block name outside), one long wait each, held for the build
# container and the host's case containers; pid, log and exit marker.
# usage: roottest-job.sh <A|B>
set -u
block=$1
case $block in A) cpus=0-5 ;; B) cpus=6-11 ;; *) echo "block A or B"; exit 2 ;; esac
L=/home/mfwolffe/lanes
d=$L/s277/records/roottest
mkdir -p "$d"; rm -f "$d/exit.marker"; echo $$ > "$d/pid"
{
echo "S277 roottest waiting for block $block at $(date -u +%FT%TZ)"
flock -w 21600 $L/hasu-block-$block.lock flock -w 21600 $L/hasu-$block.lock env S277_CPUS=$cpus bash -c '
  export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
  echo "S277 roottest acquired the block at $(date -u +%FT%TZ), cpus $S277_CPUS"
  podman run --rm --name s277-roottest-build --memory 12g --pids-limit 16384 --userns keep-id:uid=1000,gid=1000 --user 1000 -v /home/mfwolffe/lanes/s277:/lane:rw -v /home/mfwolffe/lanes/r41/inputs:/inputs:ro -w /lane/wolf-lang localhost/r41-toolchain:1 \
    /usr/bin/taskset.real -c $S277_CPUS bash /lane/rig/roottest-build.sh
  rc=$?; echo "END roottest-build exit=$rc"; [ $rc -eq 0 ] || exit $rc
  bash /home/mfwolffe/lanes/s277/rig/roottest-run.sh
  rc=$?; echo "END roottest-run exit=$rc"; exit $rc'
echo "exit=$?" > "$d/exit.marker"
} > "$d/log" 2>&1
