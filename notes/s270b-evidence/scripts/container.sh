#!/usr/bin/env bash
# s270b: run one inner script in the r41 toolchain image on hasu block A.
# usage: bash container.sh <phase> <inner-script-under-/lane> [args...]
# The block lock is taken per run and released when the container exits.
set -uo pipefail
base=/home/mfwolffe/lanes/s270b
phase=$1; shift
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
exec 9>/home/mfwolffe/lanes/hasu-block-A.lock
until flock -w 60 9; do
  printf 's270b %s waiting for block A %s\n' "$phase" "$(date -u +%FT%TZ)"
done
printf 's270b %s acquired block A %s\n' "$phase" "$(date -u +%FT%TZ)"
podman run --rm --init --name "s270b-$phase" --memory 12g --pids-limit 16384 \
  --ulimit nproc=32768:32768 --ulimit core=0 --userns keep-id:uid=1000,gid=1000 --user 0 \
  -v "$base:/lane:rw" -w /lane/wolf-lang localhost/r41-toolchain:1 \
  bash -c 'cc -O2 -static /lane/scripts/taskset-shim.c -o /usr/bin/taskset; /usr/bin/taskset.real -c 0-5 setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash -c '\''bash "$@"; exit $? '\'' bash "$@"; exit $?' bash "$@"
rc=$?
printf 's270b %s released block A %s rc=%s\n' "$phase" "$(date -u +%FT%TZ)" "$rc"
exit "$rc"
