set -euo pipefail
lane=/home/mfwolffe/lanes/s269
cd "$lane/plant-live"
cargo fmt --all --check
set +e
cargo test --locked -p wolf_driver --test status_lanes non_tty_never_receives_live_controls_even_with_immediate_heartbeat -- --nocapture
result=$?
set -e
printf '%s\n' "$result" > "$lane/evidence/plant-live-test.exit"
test "$result" = 101
printf 'PASS plant witness: non_tty_never_receives_live_controls_even_with_immediate_heartbeat is RED, exit101\n'
