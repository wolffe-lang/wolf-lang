#!/usr/bin/env bash
# One H2 take (adapted by copy from s285's gauntlet-take.sh, with
# H2's rules from s286's take.sh): wait for no wineserver, then
# H2-A.lock outside and H2-B.lock inside (one job on the host),
# slot A's CPUs (0-3,8-11) inside the container, a sensor log beside it.
# A wineserver seen inside the locks releases them at once (exit 75) and the
# take queues again. At most 2 h per take from the inner lock.
#   gauntlet-take.sh run LABEL WORK EXPECTED SHARD
set -uo pipefail
base=$LANE/pr2
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
  label=$1; work=$2; expected=$3; shard=$4
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
    -e S287_TAKE_LABEL="$label" -e S287_HOST_DESC="H2 slot=A" -e S287_CPUS="$cpus" -e R42_CPU_FOUR=0-3 -e R42_CPU_ONE=0 -e R42_TASKSET_LOG="/lane/records/$label-taskset.log" \
    -v "$base:/lane:rw" -v "$LANES:$LANES:rw" -v "$base/$work:/work:rw" -w /work localhost/r41-toolchain:1 bash -e -c '
    /usr/bin/taskset.real -c "$S287_CPUS" cc -O2 -static /work/ci/r42/taskset-shim.c -o /usr/bin/taskset
    /usr/bin/taskset.real -c "$S287_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/rig/gauntlet-inner.sh record "$@"
    exit $?
  ' bash "$expected" "$shard"
  rc=$?
  kill "$sensor" 2>/dev/null; wait "$sensor" 2>/dev/null
  printf 'LOCKS_RELEASING rc=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
  exit "$rc"
fi
label=$1; work=$2; expected=$3; shard=$4
marker=$base/records/$label.exit
printf '%s\n' "$$" > "$base/records/$label.pid"
child=
on_signal() { printf '%s\n' "$1" > "$marker"; printf 'TAKE_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
printf 'QUEUED %s head=%s shard=%s\n' "$(date -u +%FT%TZ)" "$expected" "$shard"
while :; do
  wine_wait
  flock -w 21600 $LANES/H2-A.lock flock -w 21600 $LANES/H2-B.lock bash "$0" inner "$label" "$work" "$expected" "$shard" &
  child=$!
  wait "$child"
  rc=$?
  child=
  [ "$rc" -eq 75 ] || break
done
printf 'JOB_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
exit "$rc"
