set -euo pipefail
lane=/home/mfwolffe/lanes/s269
printf 's269 strict preflight %s\n' "$(date -u +%FT%TZ)"
grep -E '^(Pid|PPid|Uid|SigBlk|SigIgn|Cpus_allowed_list):' /proc/self/status
ps -o pid,ppid,uid,args -p 1
"$LUPIN" --version
sha256sum "$LUPIN" /opt/s269-ligature /opt/s269-clef-tree/target/clef /opt/s269-musl-rt.a
sha256sum "$LUPIN" /opt/s269-ligature /opt/s269-clef-tree/target/clef /opt/s269-musl-rt.a
cargo fmt --all
cargo build --locked -p wolf_driver -p wolf_rt -j4
cargo test --locked -p wolf_driver --bin wolf status::tests -- --nocapture
cargo test --locked -p wolf_driver --test status_lanes -- --nocapture
cargo test --locked -p wolf_driver --test outcome_surface a_fine_program_passes_on_the_default_lane -- --nocapture
cargo clippy --locked -p wolf_driver -p wolf_span --all-targets -- -D warnings
python3 -I "$lane/scripts/startup-baseline.py" > "$lane/evidence/startup-baseline.log" 2>&1
