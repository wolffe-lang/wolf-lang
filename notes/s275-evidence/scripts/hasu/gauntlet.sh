#!/usr/bin/env bash
# s275 job "gauntlet" (hasu, the block the orchestrator assigns): the full
# gauntlet at the final head named in gauntlet.sha, in /lane/wolf-lang
# checked out at that commit: prep (the inputs, each read twice; the musl
# runtime, the bundles, clef and ligature), then `cargo xtask ci`'s own
# steps in its order, each with END <step> exit=N, stopping at the first
# nonzero (s273's gauntlet-inner.sh). Run by scripts/launch.sh, which holds
# both of the block's lock names for the whole job.
# usage: gauntlet.sh <A|B>
set -u
L=/home/mfwolffe/lanes/s275
blk=${1:?block}
head=$(cat "$L/gauntlet.sha")
echo "BEGIN gauntlet $(date -Iseconds) block $blk head $head"
git -C "$L/wolf-lang" fetch -q origin s275 || { echo "REFUSED fetch"; exit 2; }
git -C "$L/wolf-lang" checkout -q --detach "$head" || { echo "REFUSED checkout"; exit 2; }
test "$(git -C "$L/wolf-lang" rev-parse HEAD)" = "$head" || { echo "REFUSED head"; exit 2; }
mkdir -p "$L/records"
bash "$L/scripts/ctr-g.sh" "$blk" bash /lane/scripts/prep-inner.sh "$head"
code=$?
echo "END prep exit=$code $(date -Iseconds)"
if [ "$code" -eq 0 ]; then
    bash "$L/scripts/ctr-g.sh" "$blk" bash /lane/scripts/gauntlet-inner.sh "$head"
    code=$?
fi
echo "END gauntlet exit=$code $(date -Iseconds)"
exit "$code"
