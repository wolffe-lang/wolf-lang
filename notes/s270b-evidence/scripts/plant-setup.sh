#!/usr/bin/env bash
# s270b: a standalone clone of the plant branch for the local red run
set -euo pipefail
base=/home/mfwolffe/lanes/s270b
if [ ! -d "$base/plant" ]; then
  git clone --quiet --branch s270b-plant-copy-in-list-region https://github.com/wolffe-lang/wolf-lang.git "$base/plant"
fi
git -C "$base/plant" fetch --quiet origin s270b-plant-copy-in-list-region
git -C "$base/plant" checkout --quiet -B s270b-plant-copy-in-list-region FETCH_HEAD
git -C "$base/plant" log --oneline -2
