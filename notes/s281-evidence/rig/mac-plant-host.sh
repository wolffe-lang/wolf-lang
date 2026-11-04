#!/bin/bash
# s281: the plant record's host side, run BY the org's gauntlet recorder (a
# copy) in the plant worktree on its branch (adapted by copy from s278's
# mac-gauntlet-host.sh): one take of the Mac lock (lock-run.sh: the 60 s
# turn, the memory and disk gates). Only the chunk's step lines reach the
# record; the lock's own lines go to a side log beside it.
# usage: mac-plant-host.sh <expected-sha>
set -u
sha=$1
L=/Users/mfwolffe/lanes/s281
side=$L/records/plant-$sha.side.log
printf 'S281 plant waiting for the Mac lock at %s\n' "$(date -u +%FT%TZ)" >> "$side"
"$L/rig/lock-run.sh" bash "$L/rig/mac-plant-chunk.sh" "$sha" 2>> "$side"
rc=$?
printf 'S281 plant END exit=%s at %s\n' "$rc" "$(date -u +%FT%TZ)" >> "$side"
exit "$rc"
