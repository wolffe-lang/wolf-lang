#!/usr/bin/env bash
# s270b: fresh clones and inputs on hasu (run as: ssh hasu bash -s < setup.sh)
set -euo pipefail
base=/home/mfwolffe/lanes/s270b
mkdir -p "$base"/{scripts,logs,records,scratch/home,scratch/tmp,scratch/cache,cargo,inputs,witnesses}
cd "$base"
if [ ! -d wolf-lang ]; then
  git clone --quiet https://github.com/wolffe-lang/wolf-lang.git wolf-lang
fi
git -C wolf-lang checkout --quiet -B s270b 5adf3d91f75b3545ff8d38e5b6693b0b9f367a05
git -C wolf-lang rev-parse HEAD
git -C wolf-lang branch --show-current
if [ ! -d yew ]; then
  git clone --quiet https://github.com/wolffe-lang/yew.git yew
fi
git -C yew fetch --quiet origin yw07b-front
git -C yew checkout --quiet --detach bf7ab4a3472dc7bdf03e2090e7521703e5b790d6
git -C yew rev-parse HEAD
ls yew/notes/yw07b-evidence/stdlib/scratch-once/ yew/notes/yw07b-evidence/stdlib/scratch-escape/
df -h /home | tail -1
