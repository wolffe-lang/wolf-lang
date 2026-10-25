#!/bin/bash
# s265 item 2: build the compiler and runtime at each landing commit of
# 0.2.35 (the dist build's crates and stamp, one target directory), build
# the hello witness on the release tier with it, keep only the witness.
# Runs INSIDE the lane container (ctr). Args: commit list file.
set -uo pipefail
# Absolute: inside the container HOME is the lane's scratch home, so
# $HOME/lanes/... named a directory that does not exist (phase 2b's
# first run, 01:07 UTC).
L=/home/mfwolffe/lanes/s265
W=$L/wolf-lang
B=$L/bisect
mkdir -p $B
while read -r c label; do
  [ -z "$c" ] && continue
  [ -e "$B/hello-$c" ] && { echo "SKIP $c $label (built)"; continue; }
  echo "=== $c $label $(date -u +%T)"
  git -C $W checkout -q --detach "$c" || { echo "checkout $c FAILED"; exit 1; }
  [ "$(git -C $W rev-parse --short=8 HEAD)" = "$c" ] || { echo "HEAD is not $c"; exit 1; }
  t0=$(date +%s)
  if ! (cd $W && WOLF_COMMIT=$(git rev-parse --short=7 HEAD) cargo build --offline --release \
          -p wolf_driver -p wolf_rt --quiet); then
    echo "BUILD $c FAILED"; exit 1
  fi
  t1=$(date +%s)
  "$L/target/release/wolf" --version | head -1
  "$L/target/release/wolf" build "$W/corpus/hello.lu" -o "$B/hello-$c" --release || { echo "WITNESS $c FAILED"; exit 1; }
  [ "$("$B/hello-$c")" = "hello, wolf" ] || { echo "WITNESS $c wrong output"; exit 1; }
  sha256sum "$B/hello-$c"
  echo "built $c in $((t1 - t0)) s; target $(du -sh $L/target | cut -f1); free $(df -h --output=avail $L | tail -1)"
done < "$1"
echo "BISECT BUILDS DONE $(date -u +%T)"
