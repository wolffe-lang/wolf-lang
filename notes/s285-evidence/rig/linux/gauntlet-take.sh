#!/usr/bin/env bash
# One H1 take: H1-block-X.lock outside, then H1-X.lock, then H1-X-light.lock
# (a perf job holds its block's light lock too). The block's six CPUs inside the
# container; trunk's taskset shim maps the tests' "0-3" and "0" onto the block.
# At most 2 h per take from the outer lock. Never holds a lock idle.
#   gauntlet-take.sh run BLOCK LABEL WORK EXPECTED SHARD
set -uo pipefail
base=$LANE
podman=$LANES/H1-env/bin/podman
export TMPDIR=$base/tmp
cpus_of() { case $1 in A) echo "0-5 0-3 0";; B) echo "6-11 6-9 6";; C) echo "12-17 12-15 12";; *) return 1;; esac; }
mode=$1; shift
if [ "$mode" = outer ]; then
  block=$1
  deadline=$(( $(date +%s) + 6900 ))
  printf 'OUTER_ACQUIRED H1-block-%s %s\n' "$block" "$(date -u +%FT%TZ)"
  exec flock -w 3600 "$LANES/H1-$block.lock" flock -w 1800 "$LANES/H1-$block-light.lock" bash "$0" inner "$deadline" "$@"
elif [ "$mode" = inner ]; then
  deadline=$1; block=$2; label=$3; work=$4; expected=$5; shard=$6
  read -r cpus four one <<< "$(cpus_of "$block")"
  budget=$(( deadline - $(date +%s) ))
  [ "$budget" -gt 600 ] || exit 124
  printf 'LOCKS_ACQUIRED H1-block-%s,H1-%s,H1-%s-light %s budget=%s cpus=%s host_kernel=%s\n' "$block" "$block" "$block" "$(date -u +%FT%TZ)" "$budget" "$cpus" "$(uname -r)"
  exec "$podman" run --rm --timeout "$budget" --name "s285-$label" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
    -e S285_TAKE_LABEL="$label" -e S285_HOST_DESC="H1 block=$block" -e S285_CPUS="$cpus" -e R42_CPU_FOUR="$four" -e R42_CPU_ONE="$one" -e R42_TASKSET_LOG="/lane/records/$label-taskset.log" \
    -v "$base:/lane:rw" -v "$base:$base:rw" -v "$base/$work:/work:rw" -w /work localhost/r41-toolchain:1 bash -e -c '
    /usr/bin/taskset.real -c "$S285_CPUS" cc -O2 -static /work/ci/r42/taskset-shim.c -o /usr/bin/taskset
    /usr/bin/taskset.real -c "$S285_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs setsid --fork --wait bash /lane/rig/gauntlet-inner.sh record "$@"
    exit $?
  ' bash "$expected" "$shard"
fi
block=$1; label=$2; work=$3; expected=$4; shard=$5
cpus_of "$block" > /dev/null || exit 2
marker=$base/records/$label.exit
printf '%s\n' "$$" > "$base/records/$label.pid"
child=
on_signal() { printf '%s\n' "$1" > "$marker"; printf 'TAKE_STOPPED signal-exit=%s %s\n' "$1" "$(date -u +%FT%TZ)"; [ -n "$child" ] && kill -TERM "$child" 2>/dev/null; exit "$1"; }
trap 'on_signal 143' TERM; trap 'on_signal 130' INT; trap 'on_signal 129' HUP
printf 'QUEUED %s head=%s shard=%s block=%s\n' "$(date -u +%FT%TZ)" "$expected" "$shard" "$block"
flock -w 21600 "$LANES/H1-block-$block.lock" bash "$0" outer "$block" "$label" "$work" "$expected" "$shard" &
child=$!
wait "$child"
rc=$?
printf 'JOB_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
exit "$rc"
