#!/usr/bin/env bash
set -euo pipefail
base=/home/mfwolffe/lanes/s270
phase=$1; shift
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
exec 9>/home/mfwolffe/lanes/hasu-block-A.lock
until flock -w 60 9; do
 printf 's270 %s waiting for block A %s\n' "$phase" "$(date -u +%FT%TZ)"
done
printf 's270 %s acquired block A %s\n' "$phase" "$(date -u +%FT%TZ)"
podman run --rm --name "s270-$phase" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --userns keep-id:uid=1000,gid=1000 --user 0 -v "$base:/lane:rw" -w /lane/wolf-lang localhost/r41-toolchain:1 bash -c 'cc -O2 -static /lane/scripts/taskset-shim.c -o /usr/bin/taskset; /usr/bin/taskset.real -c 0-5 setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash -c '\''bash "$@"; exit $? '\'' bash "$@"; exit $?' bash "$@"
