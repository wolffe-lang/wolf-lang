#!/usr/bin/env bash
# s272: block A's one run, inside r41-toolchain as the lane uid, both lock names
# held outside for the whole run. In order:
#   1. probe: eight threads open pty pairs (ptyprobe.c), unlocked and locked;
#   2. stress at the red row's commit and at the fix's: the pty helper's row
#      10 runs, wolf_rt's term tests 20 runs, head4's rt-unit filter 10 runs;
#   3. prep at the final head (s273's prep-inner, copied);
#   4. the gauntlet at the final head: CI_STEPS read from xtask/src/main.rs at
#      that head, one `END <step> exit=N` each, stopping at the first red
#      (s273's gauntlet-inner, copied).
# Steps 1-2 are measurements (their counts are the result, the red commit's
# expected nonzero); a nonzero prep or gauntlet step is the run's exit code.
# usage: blocka-inner.sh <red-sha> <fix-sha> <final-sha>
set -uo pipefail
source /lane/scripts/gate-env.sh
red=${1:?red sha} fix=${2:?fix sha} final=${3:?final sha}
cd /lane/wolf-lang || exit 2
printf 'S272_BLOCKA_START %s block=%s cpus=%s\n' "$(date -u +%FT%TZ)" "$S272_BLOCK" "$S272_CPUS"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
pid1_uid=$(awk '/^Uid:/{print $2}' /proc/1/status)
printf 'pid1 uid=%s self uid=%s\n' "$pid1_uid" "$(id -u)"
test "$pid1_uid" != "$(id -u)" || { echo "S272_REFUSED running as pid 1's uid"; exit 2; }
rustc --version
R=/lane/records/blocka
mkdir -p "$R"

echo "== STEP probe $(date -u +%T)"
cc -O2 -o "$R/ptyprobe" /lane/scripts/ptyprobe.c -lpthread
prc=$?
if [ $prc -eq 0 ]; then
  for m in 0 1 2; do for r in 1 2 3; do "$R/ptyprobe" $m 1000 2>&1 | tail -3; done; done
fi
echo "END probe exit=$prc"

stress() {
  local label=$1 sha=$2
  git checkout -q --detach "$sha" || { echo "END stress-$label exit=2 (checkout)"; return; }
  test "$(git rev-parse HEAD)" = "$sha" || { echo "END stress-$label exit=2 (head)"; return; }
  echo "== STEP stress-$label $(date -u +%T) head=$(git rev-parse HEAD)"
  cargo test --locked -q -p wolf_rt --lib --no-run 2>&1 | tail -2
  local name runs filter fails i log
  for spec in "pair-row|10|term::tests::the_pty_helper_answers_its_own_pair --exact" \
              "term-tests|20|term::tests" \
              "head4-filter|10|statfs procattr term::tests signal::tests::the_action_query"; do
    IFS='|' read -r name runs filter <<< "$spec"
    fails=0
    for i in $(seq 1 "$runs"); do
      log=$R/stress-$label-$name-$i.log
      # shellcheck disable=SC2086
      if ! cargo test --locked -q -p wolf_rt --lib -- $filter > "$log" 2>&1; then
        fails=$((fails + 1))
        grep -E "panicked|FAILED|left:|right:" "$log" | head -6 | sed "s/^/  run $i: /"
      fi
    done
    echo "LOOP $label $name fails=$fails of $runs"
  done
  echo "END stress-$label exit=0"
}
stress red "$red"
stress fix "$fix"

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
printf 'S272_BLOCKA_END %s exit=%s\n' "$(date -u +%FT%TZ)" "$rc"
exit $rc
