#!/bin/bash
# s272 m1, inside ubuntu:24.04 (as the --user given): the probe on three
# file systems and GNU's answers beside it.
set -u
cd /m1
echo "== id: $(id)"
./probe / /m1 /dev/shm /proc
for p in / /m1 /dev/shm /proc; do echo "gnu-stat-f $p: $(stat -f -c '%s %S %b %f %a %c %d %l %t %i' "$p")"; done
stat -f /m1 | head -3
grep -E ' / | /m1 | /dev/shm | /proc ' /proc/self/mountinfo
nice -n -1 true; echo "gnu nice -n -1 status=$?"
nice -n 2 nice; echo "gnu nice -n 2 status=$?"
chroot /m1 /probe 2>&1 | head -2; echo "gnu chroot status=$?"
exit 0
