#!/usr/bin/env bash
# The s280 plant, recorded: one commit on the green head that drops remove's
# index rebuild. Each row runs in its own process (a panic inside an extern "C"
# entry point aborts the whole test binary), and a red row prints an
# unindented FAIL line naming it, inside the nonzero plant-rows step.
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
step env-std-plant bash -c 'if test -n "${WOLF_STD+x}"; then echo "WOLF_STD is set: $WOLF_STD"; exit 1; fi; printf "WOLF_STD=unset WOLF_DIST_STD_SRC=%s\n" "$WOLF_DIST_STD_SRC"' || exit $?
step plant-build bash -c 'cargo test --locked -p wolf_rt --lib --no-run && cargo test --locked -p wolf_rt_none --test hook_contract --no-run' || exit $?
# Rows the plant does not touch: the break is in removal, not a build break.
step plant-controls bash -c 'for row in map::tests::int_keys_round_trip map::tests::indexed_integer_collisions_survive_rehash map::tests::indexed_clear_reuses_storage_and_forgets_old_keys; do cargo test --locked -p wolf_rt --lib "$row" -- --exact || exit 1; done' || exit $?
printf 'START plant-rows\n'
started=$SECONDS
worst=0
# The declared rows, then one undeclared freestanding probe (reported either way).
for row in "$@" hook_contract::map_index_growth_removal_and_copy_obey_the_allocator_hook; do
  log=/lane/tmp/plant-row-${row//:/_}.log
  case "$row" in
    hook_contract::*) cargo test --locked -p wolf_rt_none --test hook_contract "${row#hook_contract::}" -- --exact --nocapture > "$log" 2>&1 ;;
    *) cargo test --locked -p wolf_rt --lib "$row" -- --exact --nocapture > "$log" 2>&1 ;;
  esac
  rc=$?
  sed 's/^/  /' "$log"
  if [ "$rc" -ne 0 ]; then
    why=$(grep -m1 -A1 'panicked at' "$log" | tail -1)
    sig=$(grep -m1 -o 'signal: [0-9]*, [A-Z]*' "$log" || true)
    printf 'FAIL %s exit=%s: %s%s\n' "$row" "$rc" "$why" "${sig:+ ($sig)}"
    worst=$rc
  else
    printf '  PASS %s\n' "$row"
  fi
done
printf 'END plant-rows exit=%s secs=%s\n' "$worst" "$((SECONDS-started))"
exit "$worst"
