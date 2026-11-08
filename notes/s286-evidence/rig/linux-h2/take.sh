#!/usr/bin/env bash
# s286 on H2: one locked take of one mode at exact heads, in the
# toolchain container, with a sensor log beside it.
#   take.sh LABEL SCOPE CPUS MODE HEAD...
#   SCOPE: slot (H2-A then -B: run inside CPUS, slot A's 0-3,8-11)
#          full (A, B and light, the whole package; CPUS 0-15)
# Never holds a lock idle: waits for no wineserver BEFORE the locks, and
# again inside them (a wineserver seen inside releases at once and the take
# queues again). Writes records/LABEL.pid, logs/LABEL.log (by the caller),
# logs/sensors-LABEL.log and records/LABEL.exit; TERM, INT and HUP write
# their own non-zero marker.
set -uo pipefail
base=$LANE
label=$1; scope=$2; cpus=$3; mode=$4; shift 4
heads=("$@")
marker=$base/records/$label.exit
case "$scope" in
  slot) locks=($LANES/H2-A.lock $LANES/H2-B.lock) ;;
  full) locks=($LANES/H2-A.lock $LANES/H2-B.lock $LANES/H2-light.lock) ;;
  *) echo "unknown scope $scope"; exit 2 ;;
esac
if [ "${S286_PHASE:-}" = held ]; then
  if [ -n "$(ps -C wineserver -o pid= 2>/dev/null)" ]; then
    printf 'LOCKS_RELEASED_FOR_WINESERVER %s\n' "$(date -u +%FT%TZ)"
    exit 75
  fi
  printf 'LOCKS_ACQUIRED scope=%s locks=%s %s\n' "$scope" "${locks[*]}" "$(date -u +%FT%TZ)"
  ctr=; sensor=
  stop_inner() {
    [ -n "$ctr" ] && kill -TERM "$ctr" 2>/dev/null && wait "$ctr" 2>/dev/null
    [ -n "$sensor" ] && kill "$sensor" 2>/dev/null
    exit "$1"
  }
  trap 'stop_inner 143' TERM; trap 'stop_inner 130' INT; trap 'stop_inner 129' HUP
  ( while :; do printf '%s %s\n' "$(date -u +%FT%TZ)" "$(sensors 2>/dev/null | grep -E '^Package id 0' | tr -s ' ')"; sleep 30; done ) \
    > "$base/logs/sensors-$label.log" 2>&1 &
  sensor=$!
  mounts=(-v "$base:/lane:rw" -v $LANES:$LANES:rw)
  for h in "${heads[@]}"; do
    mounts+=(-v "$base/work-${h:0:8}:/w/${h:0:8}:rw")
  done
  timeout --foreground --kill-after=30 6900 \
  $LANES/H2-env/bin/podman run --rm --name "s286-$label" --memory 12g --memory-swap 12g \
    --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --stop-timeout 10 \
    --userns keep-id:uid=1000,gid=1000 --user 0 "${mounts[@]}" -w "/w/${heads[0]:0:8}" \
    -e S286_LABEL="$label" -e S286_CPUS="$cpus" -e S286_MODE="$mode" -e S286_HEADS="${heads[*]}" \
    -e S286_CPU_FOUR=0-3 -e S286_CPU_ONE=0 -e S286_TASKSET_LOG="/lane/records/$label-taskset.log" \
    localhost/r41-toolchain:1 bash /lane/rig/root.sh &
  ctr=$!
  wait "$ctr"; rc=$?; ctr=
  kill "$sensor" 2>/dev/null; wait "$sensor" 2>/dev/null
  printf 'LOCKS_RELEASING rc=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
  exit "$rc"
fi
[ -e "$marker" ] && { echo "refuse: $marker exists"; exit 2; }
for h in "${heads[@]}"; do
  [ "$(git -C "$base/work-${h:0:8}" rev-parse HEAD)" = "$h" ] || { echo "refuse: work-${h:0:8} is not at $h"; exit 2; }
done
printf '%s\n' "$$" > "$base/records/$label.pid"
child=
on_signal() {
  printf '%s\n' "$1" > "$marker"
  printf 'TAKE_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"
  [ -n "$child" ] && kill -TERM "$child" 2>/dev/null
  exit "$1"
}
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
printf 'QUEUED label=%s scope=%s cpus=%s mode=%s heads=%s %s\n' "$label" "$scope" "$cpus" "$mode" "${heads[*]}" "$(date -u +%FT%TZ)"
while :; do
  while [ -n "$(ps -C wineserver -o pid= 2>/dev/null)" ]; do
    printf 'WAIT_WINESERVER %s\n' "$(date -u +%FT%TZ)"; sleep 300 & wait $!
  done
  cmd=(env S286_PHASE=held bash "$0" "$label" "$scope" "$cpus" "$mode" "${heads[@]}")
  for (( i=${#locks[@]}-1; i>=0; i-- )); do cmd=(flock -w 21600 "${locks[i]}" "${cmd[@]}"); done
  "${cmd[@]}" &
  child=$!
  while [ -d "/proc/$child" ]; do printf 'TAKE_ALIVE label=%s %s\n' "$label" "$(date -u +%FT%TZ)"; sleep 60 & wait $!; done
  wait "$child"; rc=$?; child=
  [ "$rc" -eq 75 ] && continue
  break
done
printf 'TAKE_END label=%s exit=%s %s\n' "$label" "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
exit "$rc"
