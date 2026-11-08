#!/usr/bin/env bash
# s286's plant, recorded on H2 (8 CPUs, slot A; the race row is red only
# with 8 or more): one commit on the green head whose one hunk makes the
# settle's decision leave glibc's limit alone, so neither mallopt call is
# made. Each named row runs in its own process; a red row prints an
# unindented FAIL line naming it, inside the nonzero plant-rows step. The
# controls are rows the plant does not touch.
#   plant-inner.sh GREEN PLANT ROW...   (cwd: the plant checkout)
set -uo pipefail
green=$1; plant=$2; shift 2
test "$(git rev-parse HEAD)" = "$plant" || exit 1
test "$(git rev-parse HEAD^)" = "$green" || exit 1
step() {
  local name=$1; shift
  local started=$SECONDS
  printf 'START %s\n' "$name"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"
  return "$rc"
}
step plant-head bash -c 'git symbolic-ref --short HEAD; git log --oneline -2; git diff --stat HEAD^ HEAD; git diff HEAD^ HEAD; printf "affinity=%s kernel=%s\n" "$(taskset -pc $$ | sed "s/.*: //")" "$(uname -r)"; ldd --version | head -1' || exit $?
step plant-build bash -c 'cargo build --locked -p wolf_driver -p wolf_rt && cargo test --locked -p wolf_driver --test signal_gaps_lanes --no-run && cargo test --locked -p wolf_rt --test arena_limit --no-run' || exit $?
step plant-controls bash -c 'cargo test --locked -p wolf_rt --lib arena::tests && cargo test --locked -p wolf_driver --test signal_gaps_lanes -- --exact signal_zero_has_words_on_every_host' || exit $?
printf 'START plant-rows\n'
started=$SECONDS
worst=0
for row in "$@"; do
  log=$TMPDIR/plant-row-$row.log
  case $row in
    arena_limit) cargo test --locked -p wolf_rt --test arena_limit > "$log" 2>&1 ;;
    *) cargo test --locked -p wolf_driver --test signal_gaps_lanes -- --exact "$row" --nocapture > "$log" 2>&1 ;;
  esac
  rc=$?
  sed 's/^/  /' "$log"
  if [ "$rc" -ne 0 ]; then
    why=$(grep -m1 -E '#1051|#781|left:|assertion' "$log" | sed 's/^ *//' | cut -c1-200)
    printf 'FAIL %s exit=%s: %s\n' "$row" "$rc" "$why"
    worst=$rc
  else
    printf '  PASS %s\n' "$row"
  fi
done
printf 'END plant-rows exit=%s secs=%s\n' "$worst" "$((SECONDS-started))"
exit "$worst"
