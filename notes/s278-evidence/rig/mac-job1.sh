#!/bin/bash
# s278 mac job 1 (run by build-at.sh in the build worktree, behind the lock):
# the measurement at the base, every step printing `END <step> exit=N` and
# every step run. The C black box (probe.c); the two rows on every machine
# (the corpus step's own native conform-run, the checked machine, the native
# and release binaries, lupin); then the corpus step whole, expected red by
# the two rows' lines (CI's nightly at 0d6b3a64, `1247 file(s), 2 bad`).
set -u
L=/Users/mfwolffe/lanes/s278
T=$L/tmp/job1
rm -rf "$T"; mkdir -p "$T/probe"
export LUPIN=/Users/mfwolffe/.local/bin/lupin
export WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
head_info() {
  git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l &&
  sw_vers && uname -a && rustc -V && cargo -V && cc --version | head -1 &&
  "$LUPIN" --version && id && ps -o pid,sigmask,sigignore,sigcatch -p $$
}
step head head_info
step build cargo build --locked -p wolf_driver -p wolf_rt
probe() {
  cc -O1 -Wall -o "$T/probe/probe" "$L/rig/probe.c" || return 1
  cp "$T/probe/probe" "$L/ev/probe/probe-darwin-arm64"
  ( cd "$T/probe" && ./probe )
}
step probe probe
W=$PWD/target/debug/wolf
row_everywhere() {
  local r=$1 f=corpus/os/$1.lu rc
  echo "-- corpus step's leg: WOLF_NATIVE=1 wolf conform-run $f --json"
  WOLF_NATIVE=1 "$W" conform-run "$f" --json; echo "@status $?"
  echo "-- checked: wolf conform-run $f --checked --json"
  "$W" conform-run "$f" --checked --json; echo "@status $?"
  for tier in native release; do
    local extra=; [ $tier = release ] && extra=--release
    echo "-- $tier: wolf build $f -o $T/$r-$tier --no-cache $extra"
    "$W" build "$f" -o "$T/$r-$tier" --no-cache $extra; rc=$?
    echo "@build-status $rc"
    [ $rc -eq 0 ] || return $rc
    "$T/$r-$tier"; echo "@status $?"
  done
  echo "-- lupin: $LUPIN conform-run $f --json"
  "$LUPIN" conform-run "$f" --json; echo "@status $?"
}
step rows-signal_action row_everywhere signal_action_rows
step rows-chroot row_everywhere chroot_rows
step corpus cargo xtask corpus
echo "== done $(date -u +%FT%TZ)"
exit 0
