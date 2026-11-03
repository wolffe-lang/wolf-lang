#!/usr/bin/env bash
# s278: one detached job on a hasu block (adapted by copy from s277's job.sh):
# both lock names, the block name outside, one long wait each (flock -w
# 21600); a pid file, a log and an exit marker under ~/lanes/s278/records/<name>/.
# Inside r41-toolchain as the lane uid (keep-id), pinned to the block with
# taskset.real, --memory 12g.
# usage: job.sh <A|B> <name> <inner-script> [args...]
set -u
block=$1 name=$2; shift 2
case $block in A) cpus=0-5 ;; B) cpus=6-11 ;; *) echo "block A or B"; exit 2 ;; esac
L=/home/mfwolffe/lanes
R=$L/s278/records/$name
mkdir -p "$R"
rm -f "$R/exit.marker"
echo $$ > "$R/pid"
export PATH=$L/hasu-env/bin:$L/hasu-env/profile/bin:$PATH
{
  printf 'S278 %s waiting for block %s at %s\n' "$name" "$block" "$(date -u +%FT%TZ)"
  flock -w 21600 $L/hasu-block-$block.lock flock -w 21600 $L/hasu-$block.lock \
    podman run --rm --name "s278-$name" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
      --userns keep-id:uid=1000,gid=1000 --user 1000 -e S278_BLOCK=$block -e S278_CPUS=$cpus \
      -v "$L/s278:/lane:rw" -v "$L/r41/inputs:/inputs:ro" -w /lane localhost/r41-toolchain:1 \
      /usr/bin/taskset.real -c "$cpus" bash "$@"
  rc=$?
  printf 'S278 %s END exit=%s at %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
} > "$R/log" 2>&1
echo "exit=$rc" > "$R/exit.marker"
