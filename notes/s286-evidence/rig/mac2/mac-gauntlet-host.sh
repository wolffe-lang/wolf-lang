#!/bin/bash
# s286: the macOS record's host side, run BY the org's gauntlet recorder (a
# copy) in the build worktree at the final head: each chunk is its own take
# of the Mac lock (lock-run.sh: the 60 s turn, the memory and disk gates), at
# most 2 h a take. Only the chunks' step lines reach the record; the lock's
# own lines go to a side log beside it.
# usage: mac-gauntlet-host.sh <expected-sha> <chunk>...
set -u
sha=$1; shift
L=$LANE
side=$L/records/mac-$sha.side.log
for chunk in "$@"; do
  printf 'S286 chunk %s waiting for the Mac lock at %s\n' "$chunk" "$(date -u +%FT%TZ)" >> "$side"
  "$L/rig/air-lock-run.sh" bash "$L/rig/mac-gauntlet-chunk.sh" "$chunk" "$sha" 2>> "$side"
  rc=$?
  printf 'S286 chunk %s END exit=%s at %s\n' "$chunk" "$rc" "$(date -u +%FT%TZ)" >> "$side"
  [ "$rc" -eq 0 ] || exit "$rc"
done
exit 0
