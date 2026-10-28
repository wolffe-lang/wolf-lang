set -uo pipefail
lane=/home/mfwolffe/lanes/s269
unset WOLF_STD WOLF_STATUS_AFTER WOLF_CHECKED WOLF_NATIVE WOLF_RELEASE
printf 's269 final-head gauntlet source=%s start=%s\n' "$(git rev-parse HEAD)" "$(date -u +%FT%TZ)"
test "$(git branch --show-current)" = s269 || exit 3
test "$(git rev-parse HEAD)" = "$(cat "$lane/evidence/final-head")" || exit 3
sha256sum -c "$lane/evidence/final-source.sha256" > "$lane/evidence/gauntlet-source-before.log" 2>&1 || exit 3
grep -E '^(Pid|PPid|Uid|SigBlk|SigIgn|Cpus_allowed_list):' /proc/self/status
ps -o pid,ppid,uid,args -p 1
cargo xtask ci > "$lane/evidence/gauntlet.log" 2>&1
result=$?
printf '%s\n' "$result" > "$lane/evidence/gauntlet-cargo.exit"
sha256sum -c "$lane/evidence/final-source.sha256" > "$lane/evidence/gauntlet-source-after.log" 2>&1
source_result=$?
printf 's269 cargo xtask ci exit=%s source-recheck=%s end=%s\n' "$result" "$source_result" "$(date -u +%FT%TZ)"
if [ "$source_result" -ne 0 ]; then exit 3; fi
exit "$result"
