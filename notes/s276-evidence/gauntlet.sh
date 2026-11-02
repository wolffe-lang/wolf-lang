#!/usr/bin/env bash
# s276 final-head gauntlet; every step is fail-fast and prints its exit.
# Baseline/plant pilot must already be complete. No checkout of a live branch.
set -uo pipefail
head=${1:?final head}; plant=${2:?plant head}
step() {
  local name=$1 rc; shift
  printf 'BEGIN %s %s\n' "$name" "$(date -u +%FT%TZ)"
  "$@"; rc=$?
  printf 'END %s exit=%s\n' "$name" "$rc"
  if [ "$rc" -ne 0 ]; then exit "$rc"; fi
}
checkout() {
  cd /lane/base || return
  git checkout -q --detach "$head" && test "$(git rev-parse HEAD)" = "$head" &&
    test -z "$(git status --porcelain --untracked-files=no)"
}
build() {
  cargo build --locked -p wolf_driver -p wolf_rt || return
  sha256sum target/debug/wolf target/debug/libwolf_rt.a
  for target in cwd_region_lanes region_outer_write_lanes map_pairs_walk_lanes checked_budget element_places_lanes; do
    cargo test --locked -p wolf_driver --test "$target" --no-run --message-format=json > "/lane/records/head-$target-build.json" || return
    python3 -I - "/lane/records/head-$target-build.json" "$target" "/lane/records/head-$target.bin" <<'PY'
import json,sys
records=[json.loads(x) for x in open(sys.argv[1])]
exe=[r['executable'] for r in records if r.get('reason')=='compiler-artifact' and r.get('executable') and r['target']['name']==sys.argv[2]]
assert len(exe)==1,exe
open(sys.argv[3],'w').write(exe[0]+'\n')
PY
    test "$?" = 0 || return 2
  done
}
suite() {
  for target in cwd_region_lanes region_outer_write_lanes map_pairs_walk_lanes checked_budget; do
    "$(cat "/lane/records/head-$target.bin")" --nocapture > "/lane/records/suite-$target.log" 2>&1
    rc=$?
    cat "/lane/records/suite-$target.log"
    printf 'S276_SUITE target=%s exit=%s SKIPs=%s\n' "$target" "$rc" "$(grep -c SKIP "/lane/records/suite-$target.log" || true)"
    if [ "$rc" -ne 0 ]; then return "$rc"; fi
  done
}
proof() {
  local rc=0 row target label count i out got loadpid
  test "$(git -C /lane/plant rev-parse HEAD)" = "$plant" || return 2
  rm -f /lane/records/load-stop
  (
    k=0
    while [ ! -f /lane/records/load-stop ]; do
      "$(cat /lane/records/head-element_places_lanes.bin)" > /lane/records/load-latest.log 2>&1 || {
        cp /lane/records/load-latest.log /lane/records/load-failed.log; exit 1;
      }
      k=$((k + 1)); printf '%s\n' "$k" > /lane/records/load-count
    done
  ) &
  loadpid=$!
  printf 'S276_LOAD pid=%s four=%s %s\n' "$loadpid" "$S276_FOUR" "$(date -u +%FT%TZ)"
  cat /proc/stat > /lane/records/cpu-before.txt
  for label in head plant; do
    count=200; if [ "$label" = plant ]; then count=20; fi
    for spec in 'cwd_region_lanes a_cwd_asked_in_a_region_keeps_nothing_a_turn' 'region_outer_write_lanes the_turn_region_runs_flat_on_the_native_tier' 'region_outer_write_lanes the_turn_region_runs_flat_on_the_release_tier' 'map_pairs_walk_lanes a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_natively' 'map_pairs_walk_lanes a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_on_the_checked_machine' 'checked_budget the_reduction_runs_flat_on_the_checked_tier' 'checked_budget a_str_doubled_by_plus_meets_the_byte_budget_before_the_host_does'; do
      read -r target row <<< "$spec"
      # The budget-refusal sibling is a separate control, not one of the four leak plants.
      if [ "$label" = plant ] && [ "$row" = a_str_doubled_by_plus_meets_the_byte_budget_before_the_host_does ]; then continue; fi
      for ((i=1; i<=count; i++)); do
        out="/lane/records/proof-$label-$row-$i.log"
        "$(cat "/lane/records/$label-$target.bin")" --exact "$row" --nocapture > "$out" 2>&1
        got=$?
        printf 'S276_ROW label=%s row=%s run=%s exit=%s %s\n' "$label" "$row" "$i" "$got" "$(date -u +%FT%TZ)" >> /lane/records/proof-rows.log
        if { [ "$label" = head ] && [ "$got" -ne 0 ]; } || { [ "$label" = plant ] && [ "$got" -eq 0 ]; }; then
          cat "$out"; rc=1; break 3
        fi
        # A plant is red only by the named assertion, not a crash or build refusal.
        if [ "$label" = plant ] && ! grep -q "test $row ... FAILED" "$out"; then cat "$out"; rc=1; break 3; fi
        if grep -q SKIP "$out"; then cat "$out"; rc=1; break 3; fi
        if [ "$((i % 10))" = 0 ]; then
          printf 'S276_PROGRESS label=%s row=%s run=%s/%s load_iterations=%s %s\n' "$label" "$row" "$i" "$count" "$(cat /lane/records/load-count 2>/dev/null || echo 0)" "$(date -u +%FT%TZ)"
        fi
      done
    done
  done
  touch /lane/records/load-stop
  wait "$loadpid" || rc=1
  cat /proc/stat > /lane/records/cpu-after.txt
  test ! -f /lane/records/load-failed.log || rc=1
  test "$(cat /lane/records/load-count)" -gt 0 || rc=1
  printf 'S276_PROOF head=%s plant=%s exit=%s load_iterations=%s\n' "$head" "$plant" "$rc" "$(cat /lane/records/load-count)"
  return "$rc"
}
step checkout checkout
step build build
step fmt cargo fmt --all --check
step clippy cargo clippy --locked -p wolf_driver --test cwd_region_lanes --test region_outer_write_lanes --test map_pairs_walk_lanes --test checked_budget -- -D warnings
step suite suite
step proof proof
