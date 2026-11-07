#!/usr/bin/env bash
# The s285 plant, recorded (adapted by copy from s280's and s282's
# plant-inner.sh): one commit on the green head that drops the checker's
# literal-pattern half (wolf-lang#1058). Each row runs in its own process,
# and a red row prints an unindented FAIL line naming it, inside the nonzero
# plant-rows step. The controls are rows the plant does not touch: the
# parser's signed-literal refusal and the in-range edge row.
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
step plant-build bash -c 'cargo build --locked -p wolf_driver -p wolf_rt && cargo test --locked -p wolf_driver --test literal_pattern_lanes --no-run' || exit $?
step plant-controls bash -c 'for row in a_signed_literal_pattern_is_refused_by_name every_primitives_edge_literal_matches_on_every_tier; do cargo test --locked -p wolf_driver --test literal_pattern_lanes "$row" -- --exact --test-threads=1 || exit 1; done' || exit $?
printf 'START plant-rows\n'
started=$SECONDS
worst=0
for row in "$@"; do
  log=/lane/tmp/plant-row-$row.log
  cargo test --locked -p wolf_driver --test literal_pattern_lanes "$row" -- --exact --nocapture --test-threads=1 > "$log" 2>&1
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
