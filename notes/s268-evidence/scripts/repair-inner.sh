set -uo pipefail
lane=/home/mfwolffe/lanes/s268
echo "parked repair start $(date -u +%FT%TZ)"
stat -c '%a %n' /usr/bin/taskset
test -x /usr/bin/taskset || exit 3
/usr/bin/taskset -c 0-3 sh -c 'grep Cpus_allowed_list /proc/self/status'
cargo test -p wolf_driver --test parked_tasks -- --nocapture > "$lane/evidence/parked-repair.log" 2>&1
parked=$?
printf '%s\n' "$parked" > "$lane/evidence/parked-repair.exit"
echo "parked repair exit=$parked $(date -u +%FT%TZ)"
if [ "$parked" -ne 0 ]; then exit "$parked"; fi
# Earlier ci invocations stopped in test: these later shards were unrun.
cargo xtask ci --shard gates > "$lane/evidence/post-test-gates.log" 2>&1
gates=$?
printf '%s\n' "$gates" > "$lane/evidence/post-test-gates.exit"
echo "post-test gates exit=$gates $(date -u +%FT%TZ)"
cargo xtask ci --shard coverage > "$lane/evidence/post-test-coverage.log" 2>&1
coverage=$?
printf '%s\n' "$coverage" > "$lane/evidence/post-test-coverage.exit"
echo "post-test coverage exit=$coverage $(date -u +%FT%TZ)"
if [ "$gates" -ne 0 ] || [ "$coverage" -ne 0 ]; then exit 1; fi
cargo xtask ci > "$lane/evidence/gauntlet4.log" 2>&1
status=$?
printf '%s\n' "$status" > "$lane/evidence/gauntlet4.exit"
echo "gauntlet4 exit=$status $(date -u +%FT%TZ)"
exit "$status"
