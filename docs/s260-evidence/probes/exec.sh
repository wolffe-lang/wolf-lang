#!/usr/bin/env bash
# Run one built program (or a machine) inside the container.
# exec.sh <dir> <cmd...>  — cwd is the dir, mounted at /w; the release
# toolchains at /tc and the head's dist at /dist, read-only.
set -uo pipefail
dir="$1"; shift
root="$HOME/lanes/s260"
img=docker.io/library/archlinux:base-devel
podman run --rm --network none --cpuset-cpus 0-3 -v "$dir:/w" -v "$root/tc:/tc:ro" \
  -v "$root/wolf-lang/target/dist:/dist:ro" -w /w "$img" \
  bash -c 'ulimit -c 0; grep -E "SigBlk|SigIgn" /proc/self/status | tr "\n" " "; echo; timeout 20 "$@"; echo "rc=$?"' s260 "$@"
