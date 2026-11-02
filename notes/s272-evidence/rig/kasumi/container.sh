#!/usr/bin/env bash
# s272: one run in r41-toolchain (loaded from hasu) on kasumi's slot 8-11
# (adapted by copy from s272's hasu container.sh): the slot's one lock name is
# taken by the CALLER (job.sh), never here; the taskset shim moves lists within
# 0-5 by 8; the command dropped to the lane uid under a foreign pid 1 (bash
# stays pid 1). The lane directory is /lane, the lane's own copy of r41's
# inputs /inputs (read-only).
# usage: container.sh <name> <tmpfs-scratch:0|1> <inner-script> [args...]
set -uo pipefail
name=$1 tmpfs=$2; shift 2
L=/home/mfwolffe/lanes
extra=()
# The mount point made by the lane user, never by podman (a directory podman
# makes for a mount belongs to the container's root, and the lane uid cannot
# write target/ beside it).
mkdir -p "$L/s272/wolf-lang/target/tmp"
if [ "$tmpfs" = 1 ]; then
  # The test binaries' scratch (CARGO_TARGET_TMPDIR = target/tmp) on a
  # private tmpfs: a file system that counts an unsynced write at once.
  extra=(--tmpfs /lane/wolf-lang/target/tmp:rw,size=3g,mode=1777)
fi
printf 'S272 %s container start at %s (tmpfs scratch %s)\n' "$name" "$(date -u +%FT%TZ)" "$tmpfs"
podman run --rm --name "s272-$name" --cpuset-cpus 8-11 --memory 12g --pids-limit 16384 \
  --ulimit nproc=32768:32768 --ulimit core=0 \
  --userns keep-id:uid=1000,gid=1000 --user 0 -e S272_BLOCK=K -e S272_CPUS=8-11 \
  -v "$L/s272:/lane:rw" -v "$L/s272/inputs:/inputs:ro" "${extra[@]}" -w /lane/wolf-lang localhost/r41-toolchain:1 \
  bash -c 'echo "S272 in the container at $(date -u +%FT%TZ)"; cc -O2 -static -DOFFSET=8 /lane/scripts/taskset-shim.c -o /usr/bin/taskset || exit 90; /usr/bin/taskset.real -c 8-11 setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash "$@"; exit $?' bash "$@"
rc=$?
printf 'S272 %s END exit=%s at %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
exit $rc
