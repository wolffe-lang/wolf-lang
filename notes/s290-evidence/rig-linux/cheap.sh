#!/usr/bin/env bash
# The cheap gates before any block take (s290): a light slot's two CPUs and
# 6 GiB, the slot's CPUs set inside the container, an init as its pid 1.
# Run under the host's light-slot wrapper, which takes the slot's lock and
# exports LIGHT_CPUS and LIGHT_MEM.
#   cheap.sh WORK EXPECTED [STD_WORK STD_EXPECTED]
# With the wolf-std arguments, followon.sh runs after the cheap gates in
# the same hold (the PAX fixture, wolf-std's rows, the PAX boot leg).
set -uo pipefail
base=${LANE:?}
source "$base/host.env"
work=$1; expected=$2
export TMPDIR=$base/tmp
test "$(git -C "$base/$work" rev-parse HEAD)" = "$expected" || exit 1
"$RIG_PODMAN" run --rm --init --name "s290-cheap-${expected:0:8}" --memory "${LIGHT_MEM:?}" --pids-limit 8192 --ulimit nproc=16384:16384 --ulimit core=0 --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
  -e RIG_CPUS="${LIGHT_CPUS:?}" -e RIG_JOBS=2 -e RIG_DESC="light slot" \
  -v "$base:/lane:rw" -v "$base/$work:/work:rw" -w /work "${RIG_IMAGE:?}" bash -e -c '
  exec /usr/bin/taskset.real -c "$RIG_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash /lane/rig/cheap-inner.sh record "$0"
' "$expected"
rc=$?
printf 'CHEAP_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
if [ $# -ge 4 ]; then
  bash "$base/rig/followon.sh" "$work" "$expected" "$3" "$4"
  printf 'FOLLOWON_END exit=%s %s\n' "$?" "$(date -u +%FT%TZ)"
fi
exit "$rc"
