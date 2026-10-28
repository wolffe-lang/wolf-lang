#!/usr/bin/env bash
# s270b gauntlet job: run detached by gauntlet-launch.sh. pid file, log, exit marker.
set -uo pipefail
base=/home/mfwolffe/lanes/s270b
echo $$ > "$base/records/gauntlet.pid"
rm -f "$base/records/gauntlet.exit"
printf 's270b GAUNTLET JOB pid %s start %s head %s\n' "$$" "$(date -u +%FT%TZ)" "$(git -C "$base/wolf-lang" rev-parse HEAD)"
bash "$base/scripts/container.sh" gauntlet /lane/scripts/gauntlet-inner.sh
rc=$?
echo "$rc" > "$base/records/gauntlet.exit"
printf 's270b GAUNTLET JOB end %s exit %s\n' "$(date -u +%FT%TZ)" "$rc"
exit "$rc"
