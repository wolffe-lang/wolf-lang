#!/usr/bin/env bash
# s292 (adapted by copy from s290's), in the same light-slot hold as the
# cheap gates, after them: (1) in the toolchain image, the PAX fixture and
# its helper built static with the head's own wolf (rows-inner.sh); (2) in
# an image with QEMU, the PAX boot leg on pax's px111 kernel from a fresh
# clone with no change to it (pax-leg.sh). Each recorded.
#   followon.sh WORK EXPECTED
set -uo pipefail
base=${LANE:?}
source "$base/host.env"
work=$1; expected=$2
export TMPDIR=$base/tmp
test "$(git -C "$base/$work" rev-parse HEAD)" = "$expected" || exit 1
mkdir -p "$base/pax-leg"
"$RIG_PODMAN" run --rm --init --name "s292-rows-${expected:0:8}" --memory "${LIGHT_MEM:?}" --pids-limit 4096 --ulimit nproc=8192:8192 --ulimit core=0 --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
  -e RIG_CPUS="${LIGHT_CPUS:?}" -v "$base:/lane:rw" -v "$base/$work:/work:rw" -w /work "${RIG_IMAGE:?}" bash -e -c '
  exec /usr/bin/taskset -c "$RIG_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash /lane/rig/rows-inner.sh record "$0"
' "$expected"
rc=$?
printf 'ROWS_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
"$RIG_PODMAN" run --rm --init --name "s292-pax-${expected:0:8}" --memory "${LIGHT_MEM:?}" --pids-limit 4096 --ulimit core=0 --stop-timeout 10 --device /dev/kvm --userns keep-id:uid=1000,gid=1000 --user 0 \
  -e RIG_CPUS="${LIGHT_CPUS:?}" -v "$base:/lane:rw" -w /lane/pax-leg "${RIG_PAX_IMAGE:?}" bash -e -c '
  exec /usr/bin/taskset -c "$RIG_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash /lane/rig/pax-leg.sh record "$0"
' "$expected"
prc=$?
printf 'PAX_END exit=%s %s\n' "$prc" "$(date -u +%FT%TZ)"
[ "$rc" = 0 ] && [ "$prc" = 0 ]
