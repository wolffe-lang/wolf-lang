#!/usr/bin/env bash
# One take on a shared Linux host (adapted by copy from s289's gauntlet-take.sh
# and plant-take.sh): three locks in order, the block's, its slot's and its
# light slot's (a heavy take holds the light slot too, so nothing runs beside
# it); the block's CPUs set INSIDE the container (the host delegates no cpuset
# controller); an init as the container's pid 1, which reaps; at most 2 h from
# the first lock. The host's own names (RIG_PODMAN, RIG_LOCK_BLOCK,
# RIG_LOCK_SLOT, RIG_LOCK_LIGHT, RIG_CPUS, RIG_DESC, RIG_IMAGE: the Rust
# toolchain image) come from $LANE/host.env,
# which is not part of the record.
#   take.sh run gauntlet LABEL WORK EXPECTED SHARD
#   take.sh run plant LABEL WORK GREEN PLANT ROW...   (PLANT_TEST, PLANT_CONTROLS set)
set -uo pipefail
base=${LANE:?}
source "$base/host.env"
export TMPDIR=$base/tmp
mode=$1; shift
if [ "$mode" = outer ]; then
  deadline=$(( $(date +%s) + 7200 ))
  printf 'LOCK_ACQUIRED block %s\n' "$(date -u +%FT%TZ)"
  exec flock -w 3600 "$RIG_LOCK_SLOT" flock -w 1800 "$RIG_LOCK_LIGHT" bash "$0" inner "$deadline" "$@"
elif [ "$mode" = inner ]; then
  deadline=$1; kind=$2; label=$3; work=$4; shift 4
  budget=$(( deadline - $(date +%s) ))
  [ "$budget" -gt 600 ] || exit 124
  printf 'LOCKS_ACQUIRED block,slot,light %s budget=%s cpus=%s host_kernel=%s\n' "$(date -u +%FT%TZ)" "$budget" "$RIG_CPUS" "$(uname -r)"
  case $kind in
    gauntlet) script=/lane/rig/gauntlet-inner.sh; extra=(-e RIG_KIND=gauntlet) ;;
    plant) script=/lane/rig/plant-inner.sh; extra=(-e PLANT_TEST="${PLANT_TEST:?}" -e PLANT_CONTROLS="${PLANT_CONTROLS:?}") ;;
    *) exit 2 ;;
  esac
  exec "$RIG_PODMAN" run --rm --init --timeout "$budget" --name "s290-$label" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
    -e RIG_TAKE_LABEL="$label" -e RIG_DESC="$RIG_DESC" -e RIG_CPUS="$RIG_CPUS" -e RIG_JOBS="${RIG_JOBS:-4}" "${extra[@]}" \
    -v "$base:/lane:rw" -v "$base/$work:/work:rw" -w /work "${RIG_IMAGE:?}" bash -e -c '
    exec /usr/bin/taskset.real -c "$RIG_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash "$0" record "$@"
  ' "$script" "$@"
fi
kind=$1; label=$2
marker=$base/records/$label.exit
printf '%s\n' "$$" > "$base/records/$label.pid"
child=
on_signal() { printf '%s\n' "$1" > "$marker"; printf 'TAKE_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT
printf 'QUEUED %s kind=%s label=%s args=%s\n' "$(date -u +%FT%TZ)" "$kind" "$label" "${*:3}"
flock -w 21600 "$RIG_LOCK_BLOCK" bash "$0" outer "$@" &
child=$!
wait "$child"
rc=$?
child=
printf 'JOB_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
exit "$rc"
