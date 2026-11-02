#!/usr/bin/env bash
# s272: kasumi slot 8-11's final run, inside r41-toolchain as the lane uid (the
# slot's lock held outside for the whole run). In order:
#   1. the plant's named row at the plant's commit (expected red by name), on
#      kasumi's btrfs (free and available differ there: the plant shows);
#   2. prep at the final head (s273's prep-inner, copied);
#   3. the gauntlet at the final head: CI_STEPS read from xtask/src/main.rs at
#      that head, one `END <step> exit=N` each, stopping at the first red
#      (s273's gauntlet-inner, copied).
# A nonzero prep or gauntlet step is the run's exit code; the plant's is a
# measurement.
# usage: final-inner.sh <final-sha> <plant-sha|->
set -uo pipefail
source /lane/scripts/gate-env.sh
final=${1:?final sha} plant=${2:?plant sha}
cd /lane/wolf-lang || exit 2
printf 'S272_FINAL_START %s block=%s cpus=%s scratch=%s\n' "$(date -u +%FT%TZ)" "$S272_BLOCK" "$S272_CPUS" "$(stat -f -c %T /lane/wolf-lang)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
pid1_uid=$(awk '/^Uid:/{print $2}' /proc/1/status)
printf 'pid1 uid=%s self uid=%s\n' "$pid1_uid" "$(id -u)"
test "$pid1_uid" != "$(id -u)" || { echo "S272_REFUSED running as pid 1's uid"; exit 2; }
rustc --version
R=/lane/records/final
mkdir -p "$R"

# The CPUs: WOLF_TEST_CPUS must be exactly this process's affinity (the slot
# held), and the shim must run a list inside the slot as asked and move 0-3
# into it, refusing any other: the run stops here otherwise (11cfa83f's first
# kasumi gauntlet carried hasu's 0-3 into slot 8-11, and atomic_witness refused
# it by name).
own=$(awk '/^Cpus_allowed_list/{print $2}' /proc/self/status)
echo "== STEP cpus $(date -u +%T) WOLF_TEST_CPUS=${WOLF_TEST_CPUS:-unset} affinity=$own slot=$S272_CPUS"
cpus_ok=1
[ "${WOLF_TEST_CPUS:-}" = "$own" ] && [ "$own" = "$S272_CPUS" ] || cpus_ok=0
lo=${S272_CPUS%-*}
for want in "$S272_CPUS:$S272_CPUS" "$lo:$lo" "0-3:$S272_CPUS" "0:$lo"; do
  req=${want%%:*} exp=${want#*:}
  got=$(taskset -c "$req" awk '/^Cpus_allowed_list/{print $2}' /proc/self/status 2>&1)
  echo "  taskset -c $req -> $got (want $exp)"
  [ "$got" = "$exp" ] || cpus_ok=0
done
if taskset -c 12 true 2>/dev/null; then echo "  taskset -c 12 ran (want a refusal)"; cpus_ok=0; else echo "  taskset -c 12 refused"; fi
echo "END cpus exit=$((1 - cpus_ok))"
[ "$cpus_ok" = 1 ] || { echo "S272_REFUSED the CPU list"; exit 2; }

if [ "$plant" = - ]; then
  echo "SKIP plant (red by name at this plant in the run before: records/final-11cfa83f-cpus)"
elif git checkout -q --detach "$plant" && [ "$(git rev-parse HEAD)" = "$plant" ]; then
  echo "== STEP plant $(date -u +%T) head=$(git rev-parse HEAD)"
  cargo build --locked -p wolf_driver -p wolf_rt > "$R/plant-build.log" 2>&1
  cargo test --locked -p wolf_driver --test os_surface_lanes fs_statfs_matches_gnu_stat_f -- --nocapture --exact > "$R/plant.log" 2>&1
  prc=$?
  grep -E "^SKIP|no triple of|: available wolf=|panicked at|^test |test result" "$R/plant.log" | head -12
  echo "END plant exit=$prc (the plant: nonzero expected, red by name)"
else
  echo "END plant exit=2 (checkout)"
fi

git checkout -q --detach "$final" || { echo "S272_REFUSED checkout $final"; exit 2; }
test "$(git rev-parse HEAD)" = "$final" || { echo "S272_REFUSED head != $final"; exit 2; }
git diff --exit-code >/dev/null || { echo "S272_REFUSED dirty tree"; exit 2; }
git diff --cached --exit-code >/dev/null || { echo "S272_REFUSED dirty index"; exit 2; }

echo "== STEP prep $(date -u +%T)"
bash /lane/scripts/prep-inner.sh "$final"
rc=$?
echo "END prep exit=$rc $(date -u +%FT%TZ)"
[ $rc -eq 0 ] || exit $rc

bash /lane/scripts/gauntlet-inner.sh "$final"
rc=$?
git diff --exit-code --stat || { echo "S272_GAUNTLET_DIRTIED the tree"; [ $rc -ne 0 ] || rc=3; }
printf 'S272_FINAL_END %s exit=%s\n' "$(date -u +%FT%TZ)" "$rc"
exit $rc
