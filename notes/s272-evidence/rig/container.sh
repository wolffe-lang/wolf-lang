#!/usr/bin/env bash
# s272: one run in r41-toolchain on a block (adapted by copy from r41's
# wolf-container.sh): both lock names, the block name outside, one long wait
# each; the taskset shim (lists in 0-5 moved by the block's offset); the test
# command dropped to the lane uid under a foreign pid 1 (bash stays pid 1).
# r41's inputs are mounted read-only at /inputs (as s273's rig mounts them).
# usage: container.sh <A|B> <name> <inner-script> [args...]
set -uo pipefail
block=$1 name=$2; shift 2
case $block in A) cpus=0-5 off=0 ;; B) cpus=6-11 off=6 ;; *) echo "block A or B"; exit 2 ;; esac
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
L=/home/mfwolffe/lanes
printf 'S272 %s waiting for block %s at %s\n' "$name" "$block" "$(date -u +%FT%TZ)"
flock -w 21600 $L/hasu-block-$block.lock flock -w 21600 $L/hasu-$block.lock \
  podman run --rm --name "s272-$name" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
    --userns keep-id:uid=1000,gid=1000 --user 0 -e S272_BLOCK=$block -e S272_CPUS=$cpus \
    -v "$L/s272:/lane:rw" -v "$L/r41/inputs:/inputs:ro" -w /lane/wolf-lang localhost/r41-toolchain:1 \
    bash -c 'echo "S272 acquired block $S272_BLOCK at $(date -u +%FT%TZ)"; cc -O2 -static -DOFFSET='"$off"' /lane/scripts/taskset-shim.c -o /usr/bin/taskset || exit 90; /usr/bin/taskset.real -c '"$cpus"' setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash "$@"; exit $?' bash "$@"
rc=$?
printf 'S272 %s END exit=%s at %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
exit $rc
