#!/usr/bin/env bash
# s290, in the same light-slot hold as the cheap gates, after them: (1) in
# the toolchain image, the PAX fixture built static with the head's own
# wolf (glibc, `cc -static`, as pax's tools/mkedwolf builds edwolf) and
# wolf-std's session rows run on the head's wolf (checked and native)
# and on lupin 0.1.60; (2) in an image with QEMU, the PAX boot leg from a
# fresh pax clone with no change to it (pax-leg.sh). Each recorded.
#   followon.sh WORK EXPECTED STD_WORK STD_EXPECTED
set -uo pipefail
base=${LANE:?}
source "$base/host.env"
work=$1; expected=$2; swork=$3; sexpected=$4
export TMPDIR=$base/tmp
test "$(git -C "$base/$work" rev-parse HEAD)" = "$expected" || exit 1
test "$(git -C "$base/$swork" rev-parse HEAD)" = "$sexpected" || exit 1
mkdir -p "$base/pax-leg"
"$RIG_PODMAN" run --rm --init --name "s290-rows-${expected:0:8}" --memory "${LIGHT_MEM:?}" --pids-limit 4096 --ulimit nproc=8192:8192 --ulimit core=0 --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
  -e RIG_CPUS="${LIGHT_CPUS:?}" -v "$base:/lane:rw" -v "$base/$work:/work:rw" -v "$base/$swork:/std:rw" -w /work "${RIG_IMAGE:?}" bash -e -c '
  exec /usr/bin/taskset.real -c "$RIG_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash /lane/rig/rows-inner.sh record "$0" "$1"
' "$expected" "$sexpected"
rc=$?
printf 'ROWS_END exit=%s %s\n' "$rc" "$(date -u +%FT%TZ)"
"$RIG_PODMAN" run --rm --init --name "s290-pax-${expected:0:8}" --memory "${LIGHT_MEM:?}" --pids-limit 4096 --ulimit core=0 --stop-timeout 10 --device /dev/kvm --userns keep-id:uid=1000,gid=1000 --user 0 \
  -e RIG_CPUS="${LIGHT_CPUS:?}" -v "$base:/lane:rw" -w /lane/pax-leg "${RIG_PAX_IMAGE:?}" bash -e -c '
  exec /usr/bin/taskset -c "$RIG_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash /lane/rig/pax-leg.sh record "$0"
' "$expected"
prc=$?
printf 'PAX_END exit=%s %s\n' "$prc" "$(date -u +%FT%TZ)"
[ "$rc" = 0 ] && [ "$prc" = 0 ]
