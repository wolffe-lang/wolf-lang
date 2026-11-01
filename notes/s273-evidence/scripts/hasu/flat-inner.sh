#!/usr/bin/env bash
# s273: wolf-lang#631's row, trunk against the head, interleaved five times each,
# inside the gauntlet's container on block A (the orchestrator's step 1).
set -uo pipefail
source /lane/scripts/gate-env.sh
T=/lane/wolf-lang-trunk
H=/lane/wolf-lang
test "$(git -C "$T" rev-parse HEAD)" = b16353e8ae616ff3a35bd27adc56b727c50df345 || { echo "S273_FLAT trunk tree is not b16353e8"; exit 2; }
test "$(git -C "$H" rev-parse HEAD)" = d07e868742826d346f354b3b7152781eb6650df5 || { echo "S273_FLAT head tree is not d07e8687"; exit 2; }
for tree in "$T" "$H"; do
  printf 'S273_FLAT_BUILD %s %s\n' "$(date -u +%FT%TZ)" "$tree"
  (cd "$tree" && cargo build --locked -p wolf_driver -p wolf_rt && cargo test --locked --no-run -p wolf_driver --test region_copyout_lanes) || { echo "S273_FLAT build failed in $tree"; exit 3; }
done
for i in 1 2 3 4 5; do
  for pair in "trunk:$T" "head:$H"; do
    name=${pair%%:*}
    tree=${pair#*:}
    out=$(cd "$tree" && cargo test --locked -q -p wolf_driver --test region_copyout_lanes -- --exact a_loop_keeping_a_scalar_is_flat --nocapture 2>&1)
    rc=$?
    printf 'S273_FLAT run=%s tree=%s exit=%s %s\n' "$i" "$name" "$rc" "$(date -u +%FT%TZ)"
    printf '%s\n' "$out" | grep -E "copy_int: VmHWM|grew .* B/turn|test result" | sed "s/^/  [$name $i] /"
  done
done
printf 'S273_FLAT_DONE %s\n' "$(date -u +%FT%TZ)"
