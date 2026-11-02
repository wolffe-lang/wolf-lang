#!/usr/bin/env bash
# s276 host wrapper: choose once, then stay in one FIFO wait on both locks.
set -uo pipefail
base=/home/mfwolffe/lanes/s276
run=${1:?run}; blk=${2:?A or B}; shift 2
printf '%s\n' "$$" > "$base/records/$run.pid"
rm -f "$base/records/$run.exit"
case "$blk" in A) cpus=0-5 offset=0 ;; B) cpus=6-11 offset=6 ;; *) exit 2 ;; esac
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
printf 'S276_QUEUE run=%s block=%s pid=%s %s\n' "$run" "$blk" "$$" "$(date -u +%FT%TZ)"
flock -E 75 -w 21600 "/home/mfwolffe/lanes/hasu-block-$blk.lock" \
  flock -E 75 -w 21600 "/home/mfwolffe/lanes/hasu-$blk.lock" \
  podman run --rm --name "s276-$run" --memory 12g --pids-limit 16384 \
    --ulimit nproc=32768:32768 --ulimit core=0 \
    --userns keep-id:uid=1000,gid=1000 --user 0 -e S276_BLOCK="$blk" \
    -v "$base:/lane:rw" -w /lane/base localhost/r41-toolchain:1 \
    bash -e -c "/usr/bin/taskset.real -c $cpus cc -O2 -static -DBLOCK_BASE=$offset /lane/scripts/taskset-shim.c -o /usr/bin/taskset; /usr/bin/taskset.real -c $cpus setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/scripts/inside.sh \"\$@\"; exit \$?" bash "$@"
rc=$?
printf '%s\n' "$rc" > "$base/records/$run.exit"
printf 'S276_EXIT run=%s exit=%s %s\n' "$run" "$rc" "$(date -u +%FT%TZ)"
exit "$rc"
