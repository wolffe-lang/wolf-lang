#!/usr/bin/env bash
# s274 items 3 and 5: the gauntlet at the head, scoped to a tests-only change (the
# contract: no full gauntlet; CI's tier pr is the verdict on the rest), ending in
# the proof. Inside the container (inner.sh's env), one block held for the batch.
# Every step prints `END <step> exit=N`; the batch stops at the first nonzero step.
# Steps: head (the tree is the head, clean), build (wolf and the runtime, their
# sha256 against batch 1's), fmt, clippy (the test target, -D warnings), suite
# (the whole region_copyout_lanes binary, the gauntlet's env, lupin 0.1.58),
# plant (its test binary built from its own commit), proof (the rigs below).
# Proof rigs, as in item 1: load4 first, interleaving the head row (HEAD_N),
# trunk's row (every 2nd turn) and the plant's row (every PLANT_EVERY-th turn);
# then rest6, load6 and rest4 with the head row (OTHER_N) and the plant (every
# PLANT_EVERY-th). The proof step is red if the head row is red once or the
# plant's row is green once.
# usage: proof-inner.sh <head sha> <plant sha> [HEAD_N] [OTHER_N] [PLANT_EVERY]
set -uo pipefail
head=${1:?head sha}
plant=${2:?plant sha}
HEAD_N=${3:-220}
OTHER_N=${4:-50}
PLANT_EVERY=${5:-11}
cd /lane/wolf-lang
rows=/lane/records/proof-rows-${head:0:8}.log
step() {
  local name=$1
  shift
  printf '== %s %s\n' "$name" "$(date -u +%FT%TZ)"
  "$@"
  local rc=$?
  printf 'END %s exit=%s %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
  if [ "$rc" -ne 0 ]; then
    printf 'S274_GAUNTLET_RED %s head=%s step=%s exit=%s\n' "$(date -u +%FT%TZ)" "$head" "$name" "$rc"
    exit "$rc"
  fi
}
at() {
  git checkout -q --detach "$1" &&
    test "$(git rev-parse HEAD)" = "$1" &&
    git diff --quiet && git diff --cached --quiet
}
copy_bin() {  # copy_bin <label> <test target>: build it at this commit, copy it out
  local label=$1 t=$2
  cargo test --locked --no-run -p wolf_driver --test "$t" --message-format=json > "/lane/scratch/no-run-$label.json" || return 3
  mkdir -p "/lane/bins/$label"
  python3 - "/lane/scratch/no-run-$label.json" "/lane/bins/$label/$t" "$t" <<'PY'
import json, shutil, sys
path, dest, name = sys.argv[1:]
exe = [m["executable"] for m in map(json.loads, open(path))
       if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == name]
if len(exe) != 1:
    sys.exit(f"S274: {len(exe)} executables for {name}")
shutil.copy2(exe[0], dest)
print(f"S274_BIN {dest} <- {exe[0]}")
PY
}
build() {
  cargo build --locked -p wolf_driver -p wolf_rt || return 3
  sha256sum target/debug/wolf target/debug/libwolf_rt.a | tee /lane/scratch/head-tools.sha | sed 's/^/S274_SHA /'
  grep -E 'S274_SHA [0-9a-f]+  target/debug/(wolf|libwolf_rt\.a)$' /lane/records/m1.log | sed 's/^S274_SHA //' | sort -u > /lane/scratch/trunk-tools.sha
  if cmp -s <(sort /lane/scratch/head-tools.sha) /lane/scratch/trunk-tools.sha; then
    echo "S274_TOOLS identical to batch 1's (trunk 7332772f): the rows differ only in the test binary"
  else
    echo "S274_TOOLS differ from batch 1's"; cat /lane/scratch/trunk-tools.sha; return 4
  fi
  copy_bin "head-${head:0:8}" region_copyout_lanes
}
suite() {
  # The gauntlet's env (s273's gate-env.sh, the parts this binary reads).
  LUPIN=/lane/inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin \
  WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1 RUST_TEST_NOCAPTURE=1 \
    /usr/bin/taskset.real -c "$S274_FOUR" "/lane/bins/head-${head:0:8}/region_copyout_lanes" 2>&1 |
    tee /lane/scratch/suite.log | grep -E '^(test |SKIP|copy_int|copy_str|no_region|test result)'
  local rc=${PIPESTATUS[0]}
  echo "S274_SUITE SKIP lines: $(grep -c '^SKIP' /lane/scratch/suite.log)"
  return "$rc"
}
plant_bin() {
  at "$plant" || return 2
  copy_bin "plant-${plant:0:8}" region_copyout_lanes
  local rc=$?
  at "$head" || return 2
  return "$rc"
}

# One run of the head's row (or the plant's: the same instrument): its report
# lines folded into `legs=<tier>:<B/turn>:<anon a>:<anon b>:<VmHWM a>:<VmHWM b>`.
head_row() {  # head_row <label> <bin> <rig> <cpus> <k>
  local out rc legs
  out=$(/usr/bin/taskset.real -c "$4" "$2" --exact a_loop_keeping_a_scalar_is_flat --nocapture 2>&1)
  rc=$?
  legs=$(printf '%s\n' "$out" | sed -n 's/^copy_int (release=\(false\|true\)): anonymous \([0-9]*\) kB at 20000 turns, \([0-9]*\) kB at 40000, one process: \(-\?[0-9.]*\) B\/turn (VmHWM \([0-9]*\) -> \([0-9]*\) kB)$/\1:\4:\2:\3:\5:\6/p' |
    sed 's/^false:/native:/; s/^true:/release:/' | paste -sd, -)
  printf 'S274_HEAD label=%s rig=%s k=%s exit=%s legs=%s\n' "$1" "$3" "$5" "$rc" "$legs" | tee -a "$rows"
  [ -n "$legs" ] || printf '%s\n' "$out" | sed 's/^/  S274_HEAD_NOLEGS /'
}
trunk_row() {  # trunk_row <rig> <cpus> <k>
  local out rc legs
  out=$(/usr/bin/taskset.real -c "$2" /lane/bins/trunk/region_copyout_lanes --exact a_loop_keeping_a_scalar_is_flat --nocapture 2>&1)
  rc=$?
  legs=$(printf '%s\n' "$out" | sed -n 's/^copy_int: VmHWM \([0-9]*\) kB at 20000 turns, \([0-9]*\) kB at 40000$/\1:\2/p' | paste -sd, -)
  printf 'S274_ROW label=trunk rig=%s k=%s exit=%s legs=%s\n' "$1" "$3" "$rc" "$legs" | tee -a "$rows"
}
loadpid=
start_load() {
  stop=/lane/scratch/load-proof-$1.stop
  rm -f "$stop"
  (
    while [ ! -e "$stop" ]; do
      t0=$(date +%s)
      out=$(/usr/bin/taskset.real -c "$2" /lane/bins/trunk/element_places_lanes -q 2>&1)
      rc=$?
      printf 'S274_LOAD rig=%s rc=%s secs=%s %s\n' "$1" "$rc" "$(( $(date +%s) - t0 ))" "$(printf '%s\n' "$out" | grep -E '^test result' | tail -1)"
    done
  ) &
  loadpid=$!
  printf 'S274_LOAD_START rig=%s pid=%s cpus=%s %s\n' "$1" "$loadpid" "$2" "$(date -u +%FT%TZ)"
  sleep 20
}
stop_load() {
  [ -n "$loadpid" ] || return 0
  touch "$stop"
  wait "$loadpid"
  printf 'S274_LOAD_STOP pid=%s %s\n' "$loadpid" "$(date -u +%FT%TZ)"
  loadpid=
}
proof() {
  local H="/lane/bins/head-${head:0:8}/region_copyout_lanes" P="/lane/bins/plant-${plant:0:8}/region_copyout_lanes"
  local rig cpus n k
  for rig in load4 rest6 load6 rest4; do
    case "$rig" in *6) cpus=$S274_SIX ;; *4) cpus=$S274_FOUR ;; esac
    if [ "$rig" = load4 ]; then n=$HEAD_N; else n=$OTHER_N; fi
    printf 'S274_RIG_START %s rig=%s cpus=%s loadavg=%s\n' "$(date -u +%FT%TZ)" "$rig" "$cpus" "$(cut -d' ' -f1-3 /proc/loadavg)"
    case "$rig" in load*) start_load "$rig" "$cpus" ;; esac
    for k in $(seq 1 "$n"); do
      head_row head "$H" "$rig" "$cpus" "$k"
      if [ "$rig" = load4 ] && [ $((k % 2)) -eq 0 ]; then trunk_row "$rig" "$cpus" "$k"; fi
      if [ $((k % PLANT_EVERY)) -eq 0 ]; then head_row plant "$P" "$rig" "$cpus" "$k"; fi
    done
    case "$rig" in load*) stop_load ;; esac
    printf 'S274_RIG_END %s rig=%s loadavg=%s\n' "$(date -u +%FT%TZ)" "$rig" "$(cut -d' ' -f1-3 /proc/loadavg)"
  done
  local hr hn pr pn tr tn
  hn=$(grep -c '^S274_HEAD label=head ' "$rows")
  hr=$(grep '^S274_HEAD label=head ' "$rows" | grep -vc ' exit=0 ')
  pn=$(grep -c '^S274_HEAD label=plant ' "$rows")
  pr=$(grep '^S274_HEAD label=plant ' "$rows" | grep -vc ' exit=0 ')
  tn=$(grep -c '^S274_ROW label=trunk rig=load4 ' "$rows")
  tr=$(grep '^S274_ROW label=trunk rig=load4 ' "$rows" | grep -vc ' exit=0 ')
  printf 'S274_PROOF head red %s/%s, plant red %s/%s, trunk (load4) red %s/%s\n' "$hr" "$hn" "$pr" "$pn" "$tr" "$tn"
  [ "$hr" -eq 0 ] && [ "$pr" -eq "$pn" ] && [ "$pn" -gt 0 ]
}

rm -f "$rows"
printf 'S274_GAUNTLET_START %s head=%s plant=%s\n' "$(date -u +%FT%TZ)" "$head" "$plant"
step head at "$head"
step build build
step fmt cargo fmt --all --check
step clippy cargo clippy --locked -p wolf_driver --test region_copyout_lanes -- -D warnings
step suite suite
step plant plant_bin
step proof proof
printf 'S274_GAUNTLET_GREEN %s head=%s\n' "$(date -u +%FT%TZ)" "$head"
