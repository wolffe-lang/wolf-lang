#!/usr/bin/env bash
# The cheap gates' steps inside the container (s290): fmt, clippy as CI runs
# it, the build, the freestanding archive, the test files the change touched,
# the corpus row on the checked machine and natively; recorded. Every step
# runs, a red one included, and the take exits with the worst.
set -uo pipefail
export PATH=/lane/tool-bin:/opt/s263-rust/bin:$PATH
export CARGO_HOME=/lane/cargo-home RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=${RIG_JOBS:-2} TMPDIR=/lane/tmp HOME=/lane/home
export RUST_TEST_NOCAPTURE=1 RUST_TEST_THREADS=${RIG_JOBS:-2}
unset WOLF_STD CARGO_TARGET_DIR
export LUPIN=/lane/inputs/lupin060/unpacked/lupin-0.1.60-x86_64-unknown-linux-gnu/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1
cd /work || exit 1
mode=$1; expected=$2
test "$(git rev-parse HEAD)" = "$expected" || exit 1
if [ "$mode" = record ]; then
  exec bash /lane/rig/gauntlet-record.sh --output "/lane/records/cheap-$expected.record" -- bash "$0" steps "$expected"
fi
step() {
  name=$1; shift
  started=$SECONDS
  printf 'START %s\n' "$name"
  "$@" 2>&1 | sed 's/^/  /'
  rc=${PIPESTATUS[0]}
  printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"
  return "$rc"
}
row() { # the corpus row on one machine: conform-run's JSON verdict
  printf '%s\n' "$*"
  "$@"
}
worst=0
run() { step "$@"; rc=$?; [ "$rc" -gt "$worst" ] && worst=$rc; return 0; }
run env bash -c 'printf "rig=%s affinity=%s jobs=%s kernel=%s pid1=%s\n" "${RIG_DESC:-unknown}" "$(taskset -pc $$ | sed "s/.*: //")" "$CARGO_BUILD_JOBS" "$(uname -r)" "$(cat /proc/1/comm)"; rustc --version; "$LUPIN" --version'
run fmt cargo fmt --all --check
run clippy cargo clippy --locked --workspace --all-targets -- -D warnings
run build cargo build --locked -p wolf_driver -p wolf_rt
run rt-none cargo xtask rt-none
run session-rows bash -c 'set -e; for m in "" --checked; do echo "== conform-run --json $m"; target/debug/wolf conform-run corpus/os/session_rows.lu --json $m; echo; done; echo "== WOLF_NATIVE=1 conform-run --json"; WOLF_NATIVE=1 target/debug/wolf conform-run corpus/os/session_rows.lu --json; echo'
run driver-tests cargo test --locked --no-fail-fast -p wolf_driver --test session_lanes --test freestanding_target --test str_list_fs_native --test prelude_list -- --test-threads=1
run sema-tests cargo test --locked --no-fail-fast -p wolf_sema --test host_table_spec --test prelude_names --test prelude_census --test sandbox_s290
run snapshot-tests bash -c 'set -e; cargo test --locked -p wolf_lex --test corpus_snapshots; cargo test --locked -p wolf_parse --test corpus_decls; cargo test --locked -p wolf_wir --test lower_corpus'
run spec-extract cargo xtask spec-extract --check
exit "$worst"
