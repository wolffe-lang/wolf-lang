#!/usr/bin/env bash
# s272: one run in r41-toolchain (loaded from hasu) on a kasumi slot, 4-7 or
# 8-11 (adapted by copy from s272's hasu container.sh): the slot's one lock name
# is taken by the CALLER (job.sh), never here; the taskset shim
# (taskset-shim-slot.c) runs a list inside the slot as asked and moves a list
# inside 0-3 into the slot; the command dropped to the lane uid under a foreign
# pid 1 (bash stays pid 1). The lane directory is /lane, the lane's own copy of
# r41's inputs /inputs (read-only).
# usage: container.sh <slot:4-7|8-11> <name> <tmpfs-scratch:0|1> <inner-script> [args...]
set -uo pipefail
slot=$1 name=$2 tmpfs=$3; shift 3
case $slot in 4-7|8-11) lo=${slot%-*} ;; *) echo "slot 4-7 or 8-11"; exit 2 ;; esac
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
printf 'S272 %s container start at %s (slot %s, tmpfs scratch %s)\n' "$name" "$(date -u +%FT%TZ)" "$slot" "$tmpfs"
podman run --rm --name "s272-$name" --cpuset-cpus "$slot" --memory 12g --pids-limit 16384 \
  --ulimit nproc=32768:32768 --ulimit core=0 \
  --userns keep-id:uid=1000,gid=1000 --user 0 -e S272_BLOCK=K -e S272_CPUS="$slot" \
  -v "$L/s272:/lane:rw" -v "$L/s272/inputs:/inputs:ro" "${extra[@]}" -w /lane/wolf-lang localhost/r41-toolchain:1 \
  bash -c 'echo "S272 in the container at $(date -u +%FT%TZ)"; cc -O2 -static -DSLOT_LO='"$lo"' -DSLOT_N=4 /lane/scripts/taskset-shim-slot.c -o /usr/bin/taskset || exit 90; /usr/bin/taskset.real -c '"$slot"' setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash "$@"; exit $?' bash "$@"
rc=$?
printf 'S272 %s END exit=%s at %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
exit $rc
