#!/usr/bin/env bash
# One H2 take for the plant record (adapted by copy from s285's
# plant-take.sh, with the locks, CPUs and wineserver rule of this rig's
# gauntlet-take.sh).
#   plant-take.sh run LABEL WORK GREEN PLANT ROW...
set -uo pipefail
base=$LANE
podman=$LANES/H2-env/bin/podman
export TMPDIR=$base/tmp
cpus=0-3,8-11
wine_wait() {
  n=0
  while [ -n "$(ps -C wineserver -o pid= 2>/dev/null)" ]; do
    if [ $((n % 10)) -eq 0 ]; then printf 'WINESERVER_WAIT %s\n' "$(date -u +%FT%TZ)"; fi
    n=$((n + 1)); sleep 30
  done
}
mode=$1; shift
if [ "$mode" = inner ]; then
  label=$1; work=$2; green=$3; plant=$4; shift 4
  if [ -n "$(ps -C wineserver -o pid= 2>/dev/null)" ]; then
    printf 'LOCKS_RELEASED_FOR_WINESERVER %s\n' "$(date -u +%FT%TZ)"
    exit 75
  fi
  printf 'LOCKS_ACQUIRED H2-A,H2-B %s cpus=%s host_kernel=%s\n' "$(date -u +%FT%TZ)" "$cpus" "$(uname -r)"
  ( while :; do printf '%s %s\n' "$(date -u +%FT%TZ)" "$(sensors 2>/dev/null | grep -E '^Package id 0' | tr -s ' ')"; sleep 30; done ) \
    > "$base/records/sensors-$label.log" 2>&1 &
  sensor=$!
  timeout --foreground --kill-after=30 6900 \
  "$podman" run --rm --name "s287-$label" --memory 12g --memory-swap 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
    -e S287_CPUS="$cpus" -e R42_CPU_FOUR=0-3 -e R42_CPU_ONE=0 -e R42_TASKSET_LOG="/lane/records/$label-taskset.log" \
    -v "$base:/lane:rw" -v /mnt/space/lanes:/mnt/space/lanes:rw -v "$base/$work:/work:rw" -w /work localhost/r41-toolchain:1 bash -e -c '
    /usr/bin/taskset.real -c "$S287_CPUS" cc -O2 -static /work/ci/r42/taskset-shim.c -o /usr/bin/taskset
    /usr/bin/taskset.real -c "$S287_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/rig/plant-inner.sh record "$@"
    exit $?
  ' bash "$green" "$plant" "$@"
  rc=$?
  kill "$sensor" 2>/dev/null; wait "$sensor" 2>/dev/null
  printf 'LOCKS_RELEASING rc=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
  exit "$rc"
fi
label=$1; work=$2; green=$3; plant=$4; shift 4
printf 'QUEUED %s plant=%s base=%s\n' "$(date -u +%FT%TZ)" "$plant" "$green"
while :; do
  wine_wait
  flock -w 21600 $LANES/H2-A.lock flock -w 21600 $LANES/H2-B.lock bash "$0" inner "$label" "$work" "$green" "$plant" "$@"
  rc=$?
  [ "$rc" -eq 75 ] || break
done
exit "$rc"
