set -euo pipefail
lane=/home/mfwolffe/lanes/s269
cd "$lane/plant-heartbeat"
cargo fmt --all --check
set +e
cargo test --locked -p wolf_driver --test outcome_surface a_fine_program_passes_on_the_default_lane -- --nocapture
result=$?
set -e
printf '%s\n' "$result" > "$lane/evidence/plant-heartbeat-test.exit"
test "$result" = 101
printf 'PASS plant witness: existing a_fine_program_passes_on_the_default_lane is RED, exit101\n'
