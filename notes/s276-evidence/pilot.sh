#!/usr/bin/env bash
# s276 pilot: characterize original rows, then each original-regression shape.
set -euo pipefail
base=${1:?baseline sha}; plant=${2:?plant sha}
for pair in "base $base" "plant $plant"; do
  read -r label sha <<< "$pair"
  cd "/lane/$label"
  test "$(git rev-parse HEAD)" = "$sha"
  test -z "$(git status --porcelain --untracked-files=no)"
  cargo build --locked -p wolf_driver -p wolf_rt
  for target in cwd_region_lanes region_outer_write_lanes map_pairs_walk_lanes checked_budget element_places_lanes; do
    cargo test --locked -p wolf_driver --test "$target" --no-run --message-format=json > "/lane/records/$label-$target-build.json"
    python3 -I - "/lane/records/$label-$target-build.json" "$target" "/lane/records/$label-$target.bin" <<'PY'
import json,sys
records=[json.loads(x) for x in open(sys.argv[1])]
exe=[r['executable'] for r in records if r.get('reason')=='compiler-artifact' and r.get('executable') and r['target']['name']==sys.argv[2]]
assert len(exe)==1,exe
open(sys.argv[3],'w').write(exe[0]+'\n')
PY
  done
  for spec in 'cwd_region_lanes a_cwd_asked_in_a_region_keeps_nothing_a_turn' 'region_outer_write_lanes the_turn_region_runs_flat_on_the_native_tier' 'region_outer_write_lanes the_turn_region_runs_flat_on_the_release_tier' 'map_pairs_walk_lanes a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_natively' 'map_pairs_walk_lanes a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_on_the_checked_machine' 'checked_budget the_reduction_runs_flat_on_the_checked_tier' 'checked_budget a_str_doubled_by_plus_meets_the_byte_budget_before_the_host_does'; do
    read -r target row <<< "$spec"
    exe=$(cat "/lane/records/$label-$target.bin")
    set +e
    "$exe" --exact "$row" --nocapture > "/lane/records/pilot-$label-$row.log" 2>&1
    rc=$?
    set -e
    cat "/lane/records/pilot-$label-$row.log"
    printf 'S276_PILOT label=%s row=%s exit=%s %s\n' "$label" "$row" "$rc" "$(date -u +%FT%TZ)"
  done
done
