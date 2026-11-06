#!/usr/bin/env bash
# s284 (copied by value from s280's scripts3, rows rewritten): the plant,
# recorded. One commit on the green head that puts the second look back in
# the drain thread's `back_from_poll` (the claim that finds a fork's hold looks
# at the state again and goes to work unless that look reads the hold too).
# Each named row runs in its own cargo invocation, and a red row prints an
# unindented FAIL line naming it, inside the nonzero plant-rows step. The
# fork_lanes rows run through the take's engine (rig/engine.sh), as in the
# green gauntlet.
set -uo pipefail
source /lane/scripts3/gauntlet-env.sh
cd /work || exit 1
mode=$1; green=$2; plant=$3; shift 3
test "$(git rev-parse HEAD)" = "$plant" || exit 1
test "$(git rev-parse HEAD^)" = "$green" || exit 1
if [ "$mode" = record ]; then
  exec bash /lane/scripts3/gauntlet-record.sh --output "/lane/records/plant-$plant.record" --base "$green" --plant "$@" -- bash "$0" steps "$green" "$plant" "$@"
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
step plant-head bash -c 'git symbolic-ref --short HEAD; git log --oneline -2; git diff --stat HEAD^ HEAD; git diff HEAD^ HEAD' || exit $?
step env-std-plant bash -c 'if test -n "${WOLF_STD+x}"; then echo "WOLF_STD is set: $WOLF_STD"; exit 1; fi; printf "WOLF_STD=unset WOLF_DIST_STD_SRC=%s WOLF_FORK_ENGINE=%s WOLF_FORK_REQUIRE_CONTAINER=%s\n" "$WOLF_DIST_STD_SRC" "${WOLF_FORK_ENGINE:-unset}" "${WOLF_FORK_REQUIRE_CONTAINER:-unset}"' || exit $?
step plant-build bash -c 'cargo build --locked -p wolf_driver -p wolf_rt && cargo test --locked -p wolf_rt --lib --no-run && cargo test --locked -p wolf_driver --test fork_lanes --no-run' || exit $?
# Rows the plant does not reach: the reactor's twin (its own file), s275's
# three rows, and a fork from main through the same engine.
step plant-controls bash -c 'for row in reactor::tests::s284_a_release_in_the_claims_gap_never_lets_a_fork_claim_a_working_reactor_thread signal::tests::s275_a_poll_interrupted_in_a_forks_hold_keeps_the_drain_thread signal::tests::s275_a_transient_poll_failure_keeps_the_drain_thread signal::tests::s275_a_drain_thread_that_ends_is_made_again; do cargo test --locked -p wolf_rt --lib "$row" -- --exact || exit 1; done; cargo test --locked -p wolf_driver --test fork_lanes a_fork_from_main -- --exact --nocapture' || exit $?
printf 'START plant-rows\n'
started=$SECONDS
worst=0
for row in "$@"; do
  log=/lane/tmp/plant-row-${row//:/_}.log
  case "$row" in
    # R-s284-4: a local take that must see the churn row red asks for 400 runs (it
    # misses about 0.7% at this host's measured rate; it stops at its first stall).
    fork_lanes::*) printf '  WOLF_FORK_CHURN_RUNS=400 for %s\n' "$row"; WOLF_FORK_CHURN_RUNS=400 cargo test --locked -p wolf_driver --test fork_lanes "${row#fork_lanes::}" -- --exact --nocapture > "$log" 2>&1 ;;
    *) cargo test --locked -p wolf_rt --lib "$row" -- --exact --nocapture > "$log" 2>&1 ;;
  esac
  rc=$?
  sed 's/^/  /' "$log"
  if [ "$rc" -ne 0 ]; then
    why=$(grep -m1 -A3 'panicked at' "$log" | tail -3 | tr '\n' ' ' | cut -c1-300)
    printf 'FAIL %s exit=%s: %s\n' "$row" "$rc" "$why"
    worst=$rc
  else
    printf '  PASS %s\n' "$row"
  fi
done
printf 'END plant-rows exit=%s secs=%s\n' "$worst" "$((SECONDS-started))"
exit "$worst"
