set -u
lane=/home/mfwolffe/lanes/s269
printf '%s\n' "$$" > "$lane/evidence/gauntlet.pid"
printf 's269 gauntlet job started pid=%s source=%s %s\n' "$$" "$(cat "$lane/evidence/final-head")" "$(date -u +%FT%TZ)"
bash "$lane/scripts/ctr-final.sh" gauntlet > "$lane/evidence/gauntlet-container.log" 2>&1
result=$?
printf '%s\n' "$result" > "$lane/evidence/gauntlet.exit"
printf 's269 gauntlet job finished exit=%s %s\n' "$result" "$(date -u +%FT%TZ)"
exit "$result"
