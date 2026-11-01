#!/usr/bin/env bash
# s275: one gauntlet-shaped run in the rootless container on block A or B,
# the shape of s273's container.sh (hasu-env README: taskset inside the
# container, --memory 12g, --pids-limit 16384 and nproc 32768, the
# disposition-neutral taskset shim mapping a test's 0-3 onto the block, a
# foreign pid 1: the container's bash as root, the command dropped to uid
# 1000 with every capability gone, bash kept as pid 1 by `; exit $?`). It
# takes no lock: the job that calls it holds both of the block's names
# (scripts/launch.sh), for prep and gauntlet together.
# usage: ctr-g.sh <A|B> <cmd> [args...]   (cmd runs inside, as uid 1000)
set -euo pipefail
base=/home/mfwolffe/lanes/s275
blk=${1:?block A or B}
shift
case "$blk" in
A) cpus=0-5 shimbase=0 ;;
B) cpus=6-11 shimbase=6 ;;
*) echo "s275: block must be A or B" >&2; exit 2 ;;
esac
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
printf 's275 run on block %s %s\n' "$blk" "$(date -u +%FT%TZ)"
exec podman run --rm --name "s275-run-$blk" --memory 12g --pids-limit 16384 \
    --ulimit nproc=32768:32768 --ulimit core=0 \
    --userns keep-id:uid=1000,gid=1000 --user 0 \
    -e S275_BLOCK="$blk" -e S275_CPUS="$cpus" \
    -v "$base:/lane:rw" -v "$base/inputs:/inputs:ro" \
    -w /lane/wolf-lang localhost/r41-toolchain:1 \
    bash -e -c "/usr/bin/taskset.real -c $cpus cc -O2 -static -DBLOCK_BASE=$shimbase /lane/scripts/taskset-shim.c -o /usr/bin/taskset; /usr/bin/taskset.real -c $cpus setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/scripts/inner.sh \"\$@\"; exit \$?" bash "$@"
