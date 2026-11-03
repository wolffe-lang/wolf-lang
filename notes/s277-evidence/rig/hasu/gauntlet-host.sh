#!/usr/bin/env bash
# s277: the gauntlet's host side, run BY the org's gauntlet recorder (in
# the hasu clone at the final head): four chunks, each its own take of the
# block's two locks (the block name outside, one flock -w 21600 each; at
# most 2 h a take), each a fresh r41-toolchain container (the taskset shim
# built as the container's root, the chunk dropped to the lane uid under a
# foreign pid 1; bash stays pid 1). Only the containers' step lines reach
# the record; the waits are logged beside it.
# usage: gauntlet-host.sh <A|B> <expected-sha>
set -u
block=$1 sha=$2
case $block in A) cpus=0-5 off=0 ;; B) cpus=6-11 off=6 ;; *) exit 2 ;; esac
L=/home/mfwolffe/lanes
side=$L/s277/records/gauntlet-$sha.side.log
export PATH=$L/hasu-env/bin:$L/hasu-env/profile/bin:$PATH
for chunk in ${S277_CHUNKS:-prep lint test rest}; do
  printf 'S277 chunk %s waiting for block %s at %s\n' "$chunk" "$block" "$(date -u +%FT%TZ)" >> "$side"
  flock -w 21600 $L/hasu-block-$block.lock flock -w 21600 $L/hasu-$block.lock \
    podman run --rm --name "s277-g-$chunk" --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
      --userns keep-id:uid=1000,gid=1000 --user 0 -e S277_BLOCK=$block -e S277_CPUS=$cpus \
      -v "$L/s277:/lane:rw" -v "$L/r41/inputs:/inputs:ro" -w /lane/wolf-lang localhost/r41-toolchain:1 \
      bash -c 'if ! cc -O2 -static -DOFFSET='"$off"' /lane/rig/taskset-shim.c -o /usr/bin/taskset > /lane/records/shim-build.log 2>&1; then printf "START shim\nEND shim exit=90 secs=0\n"; exit 90; fi; /usr/bin/taskset.real -c '"$cpus"' setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs bash /lane/rig/gauntlet-chunk.sh "$@"; exit $?' bash "$chunk" "$sha"
  rc=$?
  printf 'S277 chunk %s END exit=%s at %s\n' "$chunk" "$rc" "$(date -u +%FT%TZ)" >> "$side"
  [ "$rc" -eq 0 ] || exit "$rc"
done
exit 0
