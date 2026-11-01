#!/usr/bin/env bash
# s274 item 1: measure, before any change. Inside the container (inner.sh's env),
# one block held for the batch. At one commit:
#   - build wolf, the runtime and the test binaries; copy the test binaries to
#     /lane/bins/<label>/ (they name /lane/wolf-lang/target/debug/wolf, which the
#     batch records by sha256);
#   - cut the #631 witnesses from the test file (probe.py) and build them;
#   - on each rig: the row itself K times; the probe variants M times per tier
#     (a 20,000-turn and a 40,000-turn process each time); the sibling rows S times.
# Rigs: rest6 (the block's six CPUs), load6 (the same beside the load), rest4
# (four of the block's CPUs, the CI runner's four vCPUs), load4 (four beside the
# load). The load is `cargo test -p wolf_driver --test element_places_lanes`, its
# test binary run directly (default threads: the CPUs it is given) in a loop on
# the same CPUs, stopped by a stop file at the end of the rig.
# usage: measure-inner.sh <sha> <label> [rigs] [K] [M] [S] [sibling rigs]
set -uo pipefail
sha=${1:?commit}
label=${2:?label}
rigs=${3:-rest6 load6 rest4 load4}
K=${4:-100}
M=${5:-100}
S=${6:-20}
sibrigs=${7:-rest6 load4}
cd /lane/wolf-lang
git checkout -q --detach "$sha" || { echo "S274_REFUSED checkout $sha"; exit 2; }
test "$(git rev-parse HEAD)" = "$sha" || { echo "S274_REFUSED head $(git rev-parse HEAD) != $sha"; exit 2; }
git diff --quiet && git diff --cached --quiet || { echo "S274_REFUSED dirty tree"; exit 2; }
printf 'S274_BUILD_START %s sha=%s\n' "$(date -u +%FT%TZ)" "$sha"
cargo build --locked -p wolf_driver -p wolf_rt || { echo "S274_BUILD_RED cargo build"; exit 3; }
tests="region_copyout_lanes element_places_lanes cwd_region_lanes region_outer_write_lanes map_pairs_walk_lanes"
targs=""
for t in $tests; do targs="$targs --test $t"; done
cargo test --locked --no-run -p wolf_driver $targs --message-format=json > /lane/scratch/no-run-$label.json || { echo "S274_BUILD_RED cargo test --no-run"; exit 3; }
bins=/lane/bins/$label
rm -rf "$bins"; mkdir -p "$bins"
python3 - "/lane/scratch/no-run-$label.json" "$bins" $tests <<'PY' || exit 3
import json, shutil, sys, os
path, bins, want = sys.argv[1], sys.argv[2], set(sys.argv[3:])
got = {}
for line in open(path):
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] in want and "test" in m["target"]["kind"]:
        got[m["target"]["name"]] = m["executable"]
missing = want - set(got)
if missing:
    sys.exit(f"S274_BUILD_RED no executable for {sorted(missing)}")
for name, exe in sorted(got.items()):
    shutil.copy2(exe, os.path.join(bins, name))
    print(f"S274_BIN {name} <- {exe}")
PY
sha256sum target/debug/wolf target/debug/libwolf_rt.a "$bins"/* | sed 's/^/S274_SHA /'
printf 'S274_BUILD_GREEN %s sha=%s\n' "$(date -u +%FT%TZ)" "$sha"

# The witnesses, cut from this commit's test file, built once per tier.
pw=/lane/scratch/probe-$label
rm -rf "$pw"
python3 /lane/scripts/probe.py crates/wolf_driver/tests/region_copyout_lanes.rs "$pw" || exit 3
wolf=/lane/wolf-lang/target/debug/wolf
for kind in row probe; do
  for v in flat leak str ctl; do
    (cd "$pw/$kind/$v" && "$wolf" build main.lu -o w-native && "$wolf" build --release main.lu -o w-release) || { echo "S274_BUILD_RED witness $kind/$v"; exit 3; }
  done
done
sha256sum "$pw"/*/*/main.lu "$pw"/*/*/w-* | sed 's/^/S274_SHA /'
# The row's own witness, as the row writes it, must be the cut row/flat source.
"$bins/region_copyout_lanes" --exact a_loop_keeping_a_scalar_is_flat --nocapture > /lane/scratch/row-once-$label.log 2>&1
cmp target/tmp/region_copyout_lanes/copy_int/main.lu "$pw/row/flat/main.lu" && echo "S274_WITNESS row/flat is the row's own source"

cpus_of() { case "$1" in *6) echo "$S274_SIX" ;; *4) echo "$S274_FOUR" ;; esac; }
loadpid=
start_load() {
  local rig=$1 cpus=$2
  stop=/lane/scratch/load-$label-$rig.stop
  rm -f "$stop"
  (
    while [ ! -e "$stop" ]; do
      t0=$(date +%s)
      out=$(/usr/bin/taskset.real -c "$cpus" "$bins/element_places_lanes" -q 2>&1)
      rc=$?
      printf 'S274_LOAD rig=%s rc=%s secs=%s %s\n' "$rig" "$rc" "$(( $(date +%s) - t0 ))" "$(printf '%s\n' "$out" | grep -E '^test result' | tail -1)"
    done
  ) &
  loadpid=$!
  printf 'S274_LOAD_START rig=%s pid=%s cpus=%s %s\n' "$rig" "$loadpid" "$cpus" "$(date -u +%FT%TZ)"
}
stop_load() {
  [ -n "$loadpid" ] || return 0
  touch "$stop"
  wait "$loadpid"
  printf 'S274_LOAD_STOP pid=%s %s\n' "$loadpid" "$(date -u +%FT%TZ)"
  loadpid=
}

for rig in $rigs; do
  cpus=$(cpus_of "$rig")
  printf 'S274_RIG_START %s label=%s rig=%s cpus=%s loadavg=%s\n' "$(date -u +%FT%TZ)" "$label" "$rig" "$cpus" "$(cut -d' ' -f1-3 /proc/loadavg)"
  case "$rig" in load*) start_load "$rig" "$cpus"; sleep 20 ;; esac
  # (i) the row itself, K times: each run measures release=false then release=true
  # and stops at the first red leg.
  for k in $(seq 1 "$K"); do
    out=$(/usr/bin/taskset.real -c "$cpus" "$bins/region_copyout_lanes" --exact a_loop_keeping_a_scalar_is_flat --nocapture 2>&1)
    rc=$?
    legs=$(printf '%s\n' "$out" | sed -n 's/^copy_int: VmHWM \([0-9]*\) kB at 20000 turns, \([0-9]*\) kB at 40000$/\1:\2/p' | paste -sd, -)
    printf 'S274_ROW label=%s rig=%s k=%s exit=%s legs=%s\n' "$label" "$rig" "$k" "$rc" "$legs"
  done
  # (ii) the probe variants: per m, every variant and tier, a 20,000- and a
  # 40,000-turn process (interleaved, so every variant sees the same conditions).
  for m in $(seq 1 "$M"); do
    for v in flat leak str ctl; do
      case "$v" in str|ctl) [ "$m" -le 30 ] || continue ;; esac
      for tier in native release; do
        for n in 20000 40000; do
          out=$(/usr/bin/taskset.real -c "$cpus" "$pw/probe/$v/w-$tier" "$n" 2>&1)
          rc=$?
          printf 'S274_PROBE label=%s rig=%s v=%s tier=%s m=%s n=%s exit=%s %s\n' "$label" "$rig" "$v" "$tier" "$m" "$n" "$rc" "$(printf '%s\n' "$out" | grep -E '^(mark|hwm=)' | paste -sd' ' -)"
        done
      done
    done
  done
  # (iii) the sibling rows, S times, on the sibling rigs.
  case " $sibrigs " in
    *" $rig "*)
      for s in $(seq 1 "$S"); do
        for row in \
          "region_copyout_lanes the_loop_frees_its_scratch_keeping_only_its_result" \
          "region_copyout_lanes the_control_grows_without_a_region" \
          "cwd_region_lanes a_cwd_asked_in_a_region_keeps_nothing_a_turn" \
          "region_outer_write_lanes the_turn_region_runs_flat_on_the_native_tier" \
          "region_outer_write_lanes the_turn_region_runs_flat_on_the_release_tier" \
          "map_pairs_walk_lanes a_string_keyed_map_walked_by_get_keeps_a_linear_resident_set_natively"; do
          set -- $row
          out=$(/usr/bin/taskset.real -c "$cpus" "$bins/$1" --exact "$2" --nocapture 2>&1)
          rc=$?
          printf 'S274_SIB label=%s rig=%s s=%s bin=%s row=%s exit=%s\n' "$label" "$rig" "$s" "$1" "$2" "$rc"
          printf '%s\n' "$out" | grep -E '^(REPORT|copy_str: VmHWM|no_region: VmHWM)' | sed "s/^/  S274_SIBLINE rig=$rig s=$s /"
        done
      done
      ;;
  esac
  case "$rig" in load*) stop_load ;; esac
  printf 'S274_RIG_END %s label=%s rig=%s loadavg=%s\n' "$(date -u +%FT%TZ)" "$label" "$rig" "$(cut -d' ' -f1-3 /proc/loadavg)"
done
printf 'S274_MEASURE_DONE %s label=%s sha=%s\n' "$(date -u +%FT%TZ)" "$label" "$sha"
