#!/usr/bin/env bash
# s286 on H1: the cost rows, before and after, interleaved, in one take of
# one block holding all three of its locks (block, inner, light: a perf job
# holds the light lock too, so nothing runs beside it).
#   perf-take.sh LABEL BLOCK BEFORE AFTER
# Writes records/LABEL.pid, records/LABEL.exit; the caller redirects the log.
set -uo pipefail
base=$LANE
lanes=$HOME/lanes
podman=$lanes/H1-env/bin/podman
export PATH=$lanes/H1-env/bin:$lanes/H1-env/profile/bin:$PATH
export TMPDIR=$base/tmp
label=$1; block=$2; before=$3; after=$4
case $block in A) cpus=0-5;; B) cpus=6-11;; *) echo "perf rows run on A or B only"; exit 2;; esac
marker=$base/records/$label.exit
if [ "${S286_PHASE:-}" = held ]; then
  printf 'LOCKS_ACQUIRED H1-block-%s,H1-%s,H1-%s-light %s cpus=%s kernel=%s\n' "$block" "$block" "$block" "$(date -u +%FT%TZ)" "$cpus" "$(uname -r)"
  ctr=
  stop_inner() { [ -n "$ctr" ] && kill -TERM "$ctr" 2>/dev/null && wait "$ctr" 2>/dev/null; exit "$1"; }
  trap 'stop_inner 143' TERM; trap 'stop_inner 130' INT; trap 'stop_inner 129' HUP
  timeout --foreground --kill-after=30 6900 \
  "$podman" run --rm --name "s286-$label" --memory 12g --memory-swap 12g --pids-limit 16384 \
    --ulimit nproc=32768:32768 --ulimit core=0 --stop-timeout 10 \
    --userns keep-id:uid=1000,gid=1000 --user 0 -v "$base:/lane:rw" -v "$base:$base:rw" \
    -v "$base/perf-${before:0:8}:/w/${before:0:8}:rw" -v "$base/perf-${after:0:8}:/w/${after:0:8}:rw" \
    -e S286_LABEL="$label" -e S286_CPUS="$cpus" -e S286_BEFORE="$before" -e S286_AFTER="$after" \
    -w /lane localhost/r41-toolchain:1 bash -c '
      /usr/bin/taskset.real -c "$S286_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups \
        --inh-caps=-all --ambient-caps=-all setsid --fork --wait bash /lane/rig/perf-inner.sh
      exit $?' &
  ctr=$!
  wait "$ctr"; rc=$?; ctr=
  printf 'LOCKS_RELEASING rc=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
  exit "$rc"
fi
[ -e "$marker" ] && { echo "refuse: $marker exists"; exit 2; }
for h in "$before" "$after"; do
  [ "$(git -C "$base/perf-${h:0:8}" rev-parse HEAD)" = "$h" ] || { echo "refuse: perf-${h:0:8} is not at $h"; exit 2; }
done
printf '%s\n' "$$" > "$base/records/$label.pid"
child=
on_signal() { printf '%s\n' "$1" > "$marker"; printf 'TAKE_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
printf 'QUEUED label=%s block=%s before=%s after=%s %s\n' "$label" "$block" "$before" "$after" "$(date -u +%FT%TZ)"
flock -w 21600 "$lanes/H1-block-$block.lock" flock -w 21600 "$lanes/H1-$block.lock" \
  flock -w 21600 "$lanes/H1-$block-light.lock" env S286_PHASE=held bash "$0" "$@" &
child=$!
while [ -d "/proc/$child" ]; do printf 'TAKE_ALIVE label=%s %s\n' "$label" "$(date -u +%FT%TZ)"; sleep 60 & wait $!; done
wait "$child"; rc=$?; child=
printf 'TAKE_END label=%s exit=%s %s\n' "$label" "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
exit "$rc"
