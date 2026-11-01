#!/usr/bin/env bash
# s273: one run in the rootless container on block A or B (hasu-env README;
# the-lane-rules-1031.md: both lock names held, the block name outside, per
# run, a printing wait). Shape copied from s271's container.sh.
# usage: container.sh <A|B> <cmd> [args...]   (cmd runs inside, as uid 1000)
set -euo pipefail
base=/home/mfwolffe/lanes/s273
blk=${1:?block A or B}
shift
case "$blk" in
  A) cpus=0-5 shimbase=0 ;;
  B) cpus=6-11 shimbase=6 ;;
  *) echo "s273: block must be A or B" >&2; exit 2 ;;
esac
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
exec 9> /home/mfwolffe/lanes/hasu-block-$blk.lock
printf 's273 waiting for block %s (hasu-block-%s.lock) %s\n' "$blk" "$blk" "$(date -u +%FT%TZ)"
until flock -w 240 9; do
  printf 's273 waiting for block %s (hasu-block-%s.lock) %s\n' "$blk" "$blk" "$(date -u +%FT%TZ)"
done
exec 8> /home/mfwolffe/lanes/hasu-$blk.lock
until flock -w 240 8; do
  printf 's273 waiting for block %s (hasu-%s.lock) %s\n' "$blk" "$blk" "$(date -u +%FT%TZ)"
done
printf 's273 acquired block %s (both locks) %s\n' "$blk" "$(date -u +%FT%TZ)"
podman run --rm --name "s273-run-$blk" --memory 12g --pids-limit 16384 \
  --ulimit nproc=32768:32768 --ulimit core=0 \
  --userns keep-id:uid=1000,gid=1000 --user 0 \
  -e S273_BLOCK="$blk" -e S273_CPUS="$cpus" \
  -v "$base:/lane:rw" -v /home/mfwolffe/lanes/r41/inputs:/inputs:ro \
  -w /lane/wolf-lang localhost/r41-toolchain:1 \
  bash -e -c "/usr/bin/taskset.real -c $cpus cc -O2 -static -DBLOCK_BASE=$shimbase /lane/scripts/taskset-shim.c -o /usr/bin/taskset; /usr/bin/taskset.real -c $cpus setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/scripts/inner.sh \"\$@\"; exit \$?" bash "$@"
