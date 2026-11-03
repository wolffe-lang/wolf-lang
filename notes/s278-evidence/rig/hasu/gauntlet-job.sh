#!/usr/bin/env bash
# s278: the linux record's job, detached (adapted by copy from s277's
# gauntlet-job.sh): the org's gauntlet recorder (a copy) writes a NEW output
# path from the hasu clone at the final head; a pid file, a log and an exit
# marker beside it.
# usage: gauntlet-job.sh <A|B> <sha>
set -u
block=$1 sha=$2
R=/home/mfwolffe/lanes/s278/records
cd /home/mfwolffe/lanes/s278/wolf-lang || exit 2
echo $$ > "$R/gauntlet-$sha.pid"
rm -f "$R/gauntlet-$sha.exit"
bash /home/mfwolffe/lanes/s278/rig/gauntlet-record.sh --output "$R/green-linux-$sha.log" -- bash /home/mfwolffe/lanes/s278/rig/gauntlet-host.sh "$block" "$sha" > "$R/gauntlet-$sha.log" 2>&1
echo "exit=$?" > "$R/gauntlet-$sha.exit"
