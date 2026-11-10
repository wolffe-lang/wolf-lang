#!/usr/bin/env bash
# s290: `test_cpus_helper` at trunk and at the head, in ONE block take, under the
# image's own taskset translator (never replaced), in take.sh's container shape.
# The image's translator maps every pin into CPUs 6-11, so the host file names
# block B. Each head is recorded (records/helper-<sha>.record).
#   helper-take.sh run LABEL TRUNK_WORK TRUNK_SHA HEAD_WORK HEAD_SHA
set -uo pipefail
base=${LANE:?}
source "$base/host.env"
export TMPDIR=$base/tmp
mode=$1; shift
if [ "$mode" = outer ]; then
  deadline=$(( $(date +%s) + 3600 ))
  printf 'LOCK_ACQUIRED block %s\n' "$(date -u +%FT%TZ)"
  exec flock -w 3600 "$RIG_LOCK_SLOT" flock -w 1800 "$RIG_LOCK_LIGHT" bash "$0" inner "$deadline" "$@"
elif [ "$mode" = inner ]; then
  deadline=$1; label=$2; tw=$3; ts=$4; hw=$5; hs=$6
  budget=$(( deadline - $(date +%s) ))
  [ "$budget" -gt 600 ] || exit 124
  printf 'LOCKS_ACQUIRED block,slot,light %s budget=%s cpus=%s\n' "$(date -u +%FT%TZ)" "$budget" "$RIG_CPUS"
  exec "$RIG_PODMAN" run --rm --init --timeout "$budget" --name "s290-$label" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
    -e RIG_DESC="$RIG_DESC" -e RIG_CPUS="$RIG_CPUS" \
    -v "$base:/lane:rw" -v "$base/$tw:/trunk:rw" -v "$base/$hw:/work:rw" -w /work "${RIG_IMAGE:?}" bash -e -c '
    exec /usr/bin/taskset.real -c "$RIG_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash /lane/rig/helper-inner.sh "$0" "$1"
  ' "$ts" "$hs"
fi
label=$1
marker=$base/records/$label.exit
printf '%s\n' "$$" > "$base/records/$label.pid"
child=
on_signal() { printf '%s\n' "$1" > "$marker"; printf 'TAKE_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT
printf 'QUEUED %s label=%s args=%s\n' "$(date -u +%FT%TZ)" "$label" "${*:2}"
flock -w 43200 "$RIG_LOCK_BLOCK" bash "$0" outer "$@" &
child=$!
wait "$child"
rc=$?
child=
printf 'JOB_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
exit "$rc"
