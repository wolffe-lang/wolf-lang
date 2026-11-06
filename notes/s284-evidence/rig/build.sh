#!/usr/bin/env bash
# s284 job "build": build-at.sh on the block's six CPUs, under launch.sh's locks.
# usage: build.sh <A|B> <sha> [ref-to-fetch]
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
case $blk in A) SIX=0-5 ;; B) SIX=6-11 ;; *) echo "block must be A or B"; exit 2 ;; esac
echo "LOCKS_ACQUIRED $(date -u +%FT%TZ) block=$blk cpus=$SIX host_kernel=$(uname -r)"
exec bash "$L/rig/build-at.sh" "${2:?sha}" "$SIX" "${3:-}"
