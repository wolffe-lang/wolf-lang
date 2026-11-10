#!/usr/bin/env bash
# One take of the s292 gauntlet inside the container (adapted by copy from
# s290's). Takes: prep (fresh target, pins, CI's setup, every test binary
# compiled), then the unchanged xtask CI shards build, gates and coverage, in
# the same head's target.
set -uo pipefail
source /lane/rig/gauntlet-env.sh
cd /work || exit 1
mode=$1; expected=$2; shard=$3
test "$(git rev-parse HEAD)" = "$expected" || exit 1
if [ "$mode" = record ]; then
  exec bash /lane/rig/gauntlet-record.sh --output "/lane/records/gauntlet-$expected-${RIG_TAKE_LABEL:?}.record" -- bash "$0" steps "$expected" "$shard"
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
# Every take shows, inside its record, that no global WOLF_STD is set, and
# where it ran (the container's pid 1 is an init that reaps).
step "env-std-$shard" bash -c 'if test -n "${WOLF_STD+x}"; then echo "WOLF_STD is set: $WOLF_STD"; exit 1; fi; printf "WOLF_STD=unset WOLF_DIST_STD_SRC=%s LUPIN=%s\n" "$WOLF_DIST_STD_SRC" "$LUPIN"; printf "rig=%s affinity=%s jobs=%s test_threads=%s kernel=%s pid1=%s\n" "${RIG_DESC:-unknown}" "$(taskset -pc $$ | sed "s/.*: //")" "$CARGO_BUILD_JOBS" "$RUST_TEST_THREADS" "$(uname -r)" "$(cat /proc/1/comm)"' || exit $?
if [ "$shard" != prep ]; then
  # Every later take starts by proving the setup scripts' paths are real files.
  step "env-paths-$shard" bash -c 'rc=0; for v in WOLF_STATIC_ENTRY_MUSL_RT WOLF_STATIC_ENTRY_SYSROOT WOLF_TEST_CLEF WOLF_TEST_LIGATURE WOLF_TEST_LIGATURE_RT LIGATURE_SYSROOT; do p=${!v:-}; if test -n "$p" && test -e "$p"; then printf "%s=%s\n" "$v" "$p"; else printf "MISSING %s=%s\n" "$v" "$p"; rc=1; fi; done; exit $rc' || exit $?
fi
if [ "$shard" = prep ]; then
  step prep-build bash /lane/rig/gauntlet-prep.sh || exit $?
  # The build shard's test step runs `cargo test --workspace` with this same
  # environment and target; compiling it here keeps that take under 2 h.
  step prep-test-binaries cargo test --workspace --no-run || exit $?
  exit 0
fi
exec python3 -I /lane/rig/ci-step-stream.py "$shard"
