#!/usr/bin/env bash
# One H1 take for the plant record: the same locks and container recipe as
# gauntlet-take.sh (block, inner, light; the block's CPUs inside).
#   plant-take.sh run BLOCK LABEL WORK GREEN PLANT ROW...
set -uo pipefail
base=/home/mfwolffe/lanes/s284
podman=/home/mfwolffe/lanes/h1-env/bin/podman
export TMPDIR=$base/tmp
cpus_of() { case $1 in A) echo "0-5 0-3 0";; B) echo "6-11 6-9 6";; *) return 1;; esac; }
mode=$1; shift
if [ "$mode" = outer ]; then
  block=$1
  deadline=$(( $(date +%s) + 6900 ))
  printf 'OUTER_ACQUIRED h1-block-%s %s\n' "$block" "$(date -u +%FT%TZ)"
  exec flock -w 21600 "/home/mfwolffe/lanes/h1-$block.lock" flock -w 21600 "/home/mfwolffe/lanes/h1-$block-light.lock" bash "$0" inner "$deadline" "$@"
elif [ "$mode" = inner ]; then
  deadline=$1; block=$2; label=$3; work=$4; green=$5; plant=$6; shift 6
  read -r cpus four one <<< "$(cpus_of "$block")"
  budget=$(( deadline - $(date +%s) ))
  [ "$budget" -gt 600 ] || exit 124
  printf 'LOCKS_ACQUIRED h1-block-%s,H1-%s,H1-%s-light %s budget=%s cpus=%s host_kernel=%s\n' "$block" "$block" "$block" "$(date -u +%FT%TZ)" "$budget" "$cpus" "$(uname -r)"
  # s284: fork_lanes' engine through the host's podman service, as in gauntlet-take.sh.
  source "$base/rig/engine-lib.sh"
  engine_up "$label" "$cpus" || exit 1
  ctr=
  stop_inner() { [ -n "$ctr" ] && kill -TERM "$ctr" 2>/dev/null; [ -n "$ctr" ] && wait "$ctr" 2>/dev/null; engine_down; exit "$1"; }
  trap 'stop_inner 143' TERM; trap 'stop_inner 130' INT; trap 'stop_inner 129' HUP
  "$podman" run --rm --timeout "$budget" --name "s284-$label" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
    -e S284_CPUS="$cpus" -e R42_CPU_FOUR="$four" -e R42_CPU_ONE="$one" -e R42_TASKSET_LOG="/lane/records/$label-taskset.log" \
    "${ENGINE_FLAGS[@]}" \
    -v "$base:/lane:rw" -v "$base:$base:rw" -v "$base/$work:/work:rw" -w /work localhost/r41-toolchain:1 bash -e -c '
    /usr/bin/taskset.real -c "$S284_CPUS" cc -O2 -static /work/ci/r42/taskset-shim.c -o /usr/bin/taskset
    /usr/bin/taskset.real -c "$S284_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/scripts3/plant-inner.sh record "$@"
    exit $?
  ' bash "$green" "$plant" "$@" &
  ctr=$!
  wait "$ctr"
  rc=$?
  ctr=
  engine_down
  exit "$rc"
fi
block=$1; label=$2; work=$3; green=$4; plant=$5; shift 5
cpus_of "$block" > /dev/null || exit 2
printf 'QUEUED %s plant=%s base=%s block=%s\n' "$(date -u +%FT%TZ)" "$plant" "$green" "$block"
exec flock -w 21600 "/home/mfwolffe/lanes/h1-block-$block.lock" bash "$0" outer "$block" "$label" "$work" "$green" "$plant" "$@"
