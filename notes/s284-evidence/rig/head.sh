#!/usr/bin/env bash
# s284 job "head": at <sha>, the drain binaries on both cargo profiles
# (build-at.sh, worktree b-<sha>), then wolf_rt's rows and suites (rows.sh,
# worktree r-<sha>), each worktree's target fresh. Every step prints END.
# usage: head.sh <A|B> <sha>
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
sha=${2:?sha}
case $blk in A) SIX=0-5 ;; B) SIX=6-11 ;; *) echo "block must be A or B"; exit 2 ;; esac
echo "LOCKS_ACQUIRED $(date -u +%FT%TZ) block=$blk sha=$sha"
bash $L/rig/build-at.sh "$sha" "$SIX" || exit $?
exec bash $L/rig/rows.sh "$blk" "$sha"
