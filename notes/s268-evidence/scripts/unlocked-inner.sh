set -euo pipefail
unset WOLF_STD
cargo test -p wolf_driver --test scope_handle_param -- --nocapture
WOLF=/home/mfwolffe/lanes/s268/wolf-lang/target/debug/wolf OUT=/home/mfwolffe/lanes/s268/runner-check sh bench/dispatch/run.sh
