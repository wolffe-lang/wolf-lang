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
# usage: final-inner.sh <final-sha> <plant-sha>
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

if git checkout -q --detach "$plant" && [ "$(git rev-parse HEAD)" = "$plant" ]; then
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
