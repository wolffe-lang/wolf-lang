#!/usr/bin/env bash
# s270b: a standalone local clone for the bisect (its .git is a directory, so the
# container's paths work), and the untouched witness tree it builds.
set -euo pipefail
base=/home/mfwolffe/lanes/s270b
if [ ! -d "$base/bisect" ]; then
  git clone --quiet "$base/wolf-lang" "$base/bisect"
fi
git -C "$base/bisect" cat-file -t cf0c13d38479f444b69bb8356ad5709d07eff323
git -C "$base/bisect" cat-file -t 5adf3d91f75b3545ff8d38e5b6693b0b9f367a05
rm -rf "$base/witnesses/yew-bisect"
mkdir -p "$base/witnesses/yew-bisect"
for d in fl util unicode; do cp -r "$base/yew/src/$d" "$base/witnesses/yew-bisect/$d"; done
rm -f "$base/records/bisect-steps.txt"
echo ready
