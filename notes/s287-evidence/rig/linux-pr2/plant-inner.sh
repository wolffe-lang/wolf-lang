#!/usr/bin/env bash
# The s287 plant, recorded (adapted by copy from s285's
# plant-inner.sh): one commit on the green head that drops signature
# elaboration's check of types that contain themselves by value (wolf-lang#1062).
# Each row runs in its own process, and a red row prints an unindented FAIL
# line naming it, inside the nonzero plant-rows step. The controls are rows
# the plant does not touch: recursion through containers, a match binding assigned, a used alias loop.
set -uo pipefail
source /lane/rig/gauntlet-env.sh
cd /work || exit 1
mode=$1; green=$2; plant=$3; shift 3
test "$(git rev-parse HEAD)" = "$plant" || exit 1
test "$(git rev-parse HEAD^)" = "$green" || exit 1
if [ "$mode" = record ]; then
  exec bash /lane/rig/gauntlet-record.sh --output "/lane/records/plant-$plant.record" --base "$green" --plant "$@" -- bash "$0" steps "$green" "$plant" "$@"
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
step plant-head bash -c 'git symbolic-ref --short HEAD 2>/dev/null || git log --format=%H -1; git log --oneline -2; git diff --stat HEAD^ HEAD; git diff HEAD^ HEAD' || exit $?
step env-std-plant bash -c 'if test -n "${WOLF_STD+x}"; then echo "WOLF_STD is set: $WOLF_STD"; exit 1; fi; printf "WOLF_STD=unset WOLF_DIST_STD_SRC=%s LUPIN=%s\n" "$WOLF_DIST_STD_SRC" "$LUPIN"' || exit $?
step plant-build bash -c 'cargo build --locked -p wolf_driver -p wolf_rt && cargo test --locked -p wolf_driver --test front_end_rulings_lanes --no-run' || exit $?
step plant-controls bash -c 'for row in recursion_through_containers_is_legal assigning_a_match_binding_is_e0410 a_used_alias_loop_keeps_one_e0610; do cargo test --locked -p wolf_driver --test front_end_rulings_lanes "$row" -- --exact --test-threads=1 || exit 1; done' || exit $?
printf 'START plant-rows\n'
started=$SECONDS
worst=0
for row in "$@"; do
  log=/lane/tmp/plant-row-$row.log
  cargo test --locked -p wolf_driver --test front_end_rulings_lanes "$row" -- --exact --nocapture --test-threads=1 > "$log" 2>&1
  rc=$?
  sed 's/^/  /' "$log"
  if [ "$rc" -ne 0 ]; then
    why=$(grep -m1 -A2 'panicked at' "$log" | grep -m1 -E 'left|assert' | sed 's/^ *//')
    printf 'FAIL %s exit=%s: %s\n' "$row" "$rc" "$why"
    worst=$rc
  else
    printf '  PASS %s\n' "$row"
  fi
done
printf 'END plant-rows exit=%s secs=%s\n' "$worst" "$((SECONDS-started))"
exit "$worst"
