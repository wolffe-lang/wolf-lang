#!/bin/bash
set -u
L=/home/mfwolffe/lanes/ld02b
echo $$ > "$L/evidence/final.pid"
echo "ld02b final head=$(git -C "$L/wolf-lang" rev-parse HEAD) start=$(date -u +%FT%TZ)"
if [ "$(cat "$L/evidence/check.exit")" != 0 ]; then echo 'fast check is not green'; echo 3 > "$L/evidence/final.exit"; exit 3; fi
size=$(du -sm "$L" | cut -f1)
if [ "$size" -ge 10240 ]; then echo "lane too large: $size MiB"; echo 3 > "$L/evidence/final.exit"; exit 3; fi
LD02B_JOB=gauntlet "$L/scripts/ctr" bash "$L/scripts/gauntlet.sh"
gauntlet=$?
echo "gauntlet wrapper exit=$gauntlet; bench-gates startup 23 is known #952, never waived; inspect gauntlet.log"
# A separate container invocation releases and reacquires the CPU lock.
LD02B_JOB=after-bench "$L/scripts/ctr" bash "$L/scripts/after-bench.sh"
after=$?
echo "after-bench wrapper exit=$after"
du -sh "$L"
rc=$gauntlet
[ "$after" -gt "$rc" ] && rc=$after
echo "$rc" > "$L/evidence/final.exit"
echo "ld02b final exit=$rc end=$(date -u +%FT%TZ)"
exit "$rc"
