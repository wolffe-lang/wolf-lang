#!/bin/bash
# s267: `xtask differ <head's wolf> <trunk's wolf>` on the three tiers (s265's
# shape), WOLF_STD the std pin. Exit per tier in evidence/differ-<tier>.exit
# (1: divergences, each named in the note; 2: a fault).
set -u
L=/home/mfwolffe/lanes/s267
cd $L/wolf-lang
export WOLF_STD=$L/wolf-std/std
A=$CARGO_TARGET_DIR/debug/wolf; B=$L/trunk-wolf/wolf
echo "differ head=$(git rev-parse HEAD) start=$(date -u +%FT%TZ)"
sha256sum "$A" "$B"; "$A" --version | head -1; "$B" --version | head -1
worst=0
for tier in checked native release; do
  unset WOLF_CHECKED WOLF_NATIVE WOLF_RELEASE
  case $tier in checked) export WOLF_CHECKED=1;; native) export WOLF_NATIVE=1;; release) export WOLF_RELEASE=1;; esac
  "$CARGO_TARGET_DIR/debug/xtask" differ "$A" "$B" > $L/evidence/differ-$tier.log 2>&1
  rc=$?; echo "$rc" > $L/evidence/differ-$tier.exit
  echo "differ $tier exit=$rc"; tail -4 $L/evidence/differ-$tier.log
  [ $rc -le 1 ] || worst=$rc
done
echo "differ end=$(date -u +%FT%TZ)"
exit $worst
