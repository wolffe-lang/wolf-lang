#!/usr/bin/env bash
# s274: the container itself, started by container.sh with both block locks held.
# usage: run-ctr.sh <A|B> <cmd> [args...]
set -euo pipefail
base=/home/mfwolffe/lanes/s274
blk=${1:?block}
shift
case "$blk" in
  A) cpus=0-5 shimbase=0 ;;
  B) cpus=6-11 shimbase=6 ;;
  *) echo "s274: block must be A or B" >&2; exit 2 ;;
esac
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
printf 's274 acquired block %s (both locks) %s\n' "$blk" "$(date -u +%FT%TZ)"
podman run --rm --name "s274-run-$blk" --memory 12g --pids-limit 16384 \
  --ulimit nproc=32768:32768 --ulimit core=0 \
  --userns keep-id:uid=1000,gid=1000 --user 0 \
  -e S274_BLOCK="$blk" -e S274_CPUS="$cpus" \
  -v "$base:/lane:rw" \
  -w /lane/wolf-lang localhost/r41-toolchain:1 \
  bash -e -c "/usr/bin/taskset.real -c $cpus cc -O2 -static -DBLOCK_BASE=$shimbase /lane/scripts/taskset-shim.c -o /usr/bin/taskset; /usr/bin/taskset.real -c $cpus setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/scripts/inner.sh \"\$@\"; exit \$?" bash "$@"
