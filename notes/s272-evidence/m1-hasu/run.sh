#!/bin/bash
# s272 m1 (block A, both lock names held by the caller): build the probe
# in r41-toolchain, then run it in throwaway ubuntu:24.04 containers as a
# user and as root, with and without SYS_NICE, pinned to block A.
set -u
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
D=/home/mfwolffe/lanes/s272/m1
cd $D || exit 1
echo "== start $(date -u +%FT%TZ)"
podman run --rm --network none --memory 12g -v $D:/m1 -w /m1 localhost/r41-toolchain:1 \
  /usr/bin/taskset.real -c 0-5 cc -O0 -o probe probe.c
echo "END build exit=$?"
chmod +x inner.sh
for who in 4242:4242 0:0; do
  for cap in none SYS_NICE; do
    capflag=""; [ $cap = SYS_NICE ] && capflag="--cap-add SYS_NICE"
    echo "=== as $who cap=$cap"
    podman run --rm --network none --memory 12g --user $who $capflag -v $D:/m1:ro localhost/r41-toolchain:1 \
      /usr/bin/taskset.real -c 0-5 /m1/inner.sh
    echo "END case $who $cap exit=$?"
  done
done
echo "== host GNU stat (9.11) on hasu itself, for the record"
for p in / /dev/shm /boot; do echo "host-gnu-stat-f $p: $(stat -f -c '%s %S %b %f %a %c %d %l %t %i %T' "$p")"; done
echo "== end $(date -u +%FT%TZ)"
