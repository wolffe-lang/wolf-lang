#!/usr/bin/env bash
# s286 (root3: root.sh running inner3.sh), the container's entry as its root: the taskset shim, the
# set-user-ID probe's copies (made here because only root can set the
# bit), then the workload as uid 1000 under taskset on the take's CPUs.
set -uo pipefail
/usr/bin/taskset.real -c "$S286_CPUS" cc -O2 -static /lane/rig/taskset-shim.c -o /usr/bin/taskset || exit $?
mkdir -p /opt/s286
/usr/bin/taskset.real -c "$S286_CPUS" cc -O2 -pthread /lane/rig/arena-probe.c -o /opt/s286/arena-probe || exit $?
cp /opt/s286/arena-probe /opt/s286/arena-probe-suid
chown 0:0 /opt/s286/arena-probe-suid
chmod 4755 /opt/s286/arena-probe-suid
chmod 0755 /opt/s286/arena-probe
# The workload: the lane's uid, no capabilities. no_new_privs stays off so
# the set-user-ID copy runs set-user-ID (the probe reads AT_SECURE itself).
# No exec: this bash stays pid 1 and reaps (setsid as pid 1 does not).
/usr/bin/taskset.real -c "$S286_CPUS" setpriv --reuid=1000 --regid=1000 --clear-groups \
  --inh-caps=-all --ambient-caps=-all setsid --fork --wait bash /lane/rig/inner3.sh
exit $?
