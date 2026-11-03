#!/usr/bin/env bash
# Reaping root PID 1 calls this child, then drops workload credentials.
set -uo pipefail
cc -O2 -static ci/r42/taskset-shim.c -o /usr/bin/taskset || exit $?
job=(ci/r42/wolf-steps.sh "$@")
if [[ "${1:?}" = validation-* ]]; then
    job=("${R42_RECORDER:?}" --output "${3:?}/local-record.log" -- bash ci/r42/validation-steps.sh "$@")
fi
/usr/bin/taskset.real -c "${R42_CPU_FOUR:?}" setpriv \
    --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all \
    --inh-caps=-all --ambient-caps=-all --no-new-privs \
    setsid --fork --wait bash -c 'bash "$@"; exit $?' bash \
    "${job[@]}"
rc=$?
exit "$rc"
