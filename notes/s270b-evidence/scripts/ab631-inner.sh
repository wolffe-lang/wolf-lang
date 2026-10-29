#!/usr/bin/env bash
# s270b: the #631 A/B — region_copyout_lanes::a_loop_keeping_a_scalar_is_flat alone, five times
# at the head (/lane/wolf-lang) and five at trunk d2753b48 (/lane/base), the gauntlet's env.
set -uo pipefail
source /lane/scripts/gauntlet-env.sh
for tree in wolf-lang base; do
  cd "/lane/$tree"
  printf 's270b AB %s head=%s %s\n' "$tree" "$(git rev-parse HEAD 2>/dev/null || cat /lane/records/base-head.txt)" "$(date -u +%FT%TZ)"
  cargo build --locked -q -p wolf_driver -p wolf_rt 2>&1 | tail -3
  cargo test --locked -q -p wolf_driver --test region_copyout_lanes --no-run 2>&1 | tail -2
  for i in 1 2 3 4 5; do
    out=$(cargo test --locked -q -p wolf_driver --test region_copyout_lanes a_loop_keeping_a_scalar_is_flat -- --exact --nocapture 2>&1)
    rc=$?
    printf '%s\n' "$out" | grep -E 'copy_int: VmHWM|grew|test result' | sed "s/^/AB $tree run$i: /"
    printf 'AB %s run%s rc=%s\n' "$tree" "$i" "$rc"
  done
done
printf 's270b AB DONE %s\n' "$(date -u +%FT%TZ)"
