#!/usr/bin/env bash
# s278: a linux pre-check at a commit (not the record): the two rows on every
# machine and the two os_surface_lanes rows, inside r41-toolchain as the lane
# uid, pinned to the block. Every step runs and prints `END <step> exit=N`.
# usage: precheck-inner.sh <sha>
set -u
source /lane/rig/env.sh
want=${1:?sha}
cd /lane/wolf-lang || exit 2
git fetch -q origin s278 && git checkout -q --detach "$want" || exit 2
test "$(git rev-parse HEAD)" = "$(git rev-parse "$want")" || { echo "head is not $want"; exit 2; }
echo "head $(git rev-parse HEAD) $(grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status | tr '\n' ' ')"
T=/lane/tmp/precheck; rm -rf "$T"; mkdir -p "$T"
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step build cargo build --locked -p wolf_driver -p wolf_rt
W=$PWD/target/debug/wolf
row_everywhere() {
  local r=$1 f=corpus/os/$1.lu rc
  echo "-- corpus step's leg: WOLF_NATIVE=1 wolf conform-run $f --json"
  WOLF_NATIVE=1 "$W" conform-run "$f" --json; echo "@status $?"
  echo "-- checked: wolf conform-run $f --checked --json"
  "$W" conform-run "$f" --checked --json; echo "@status $?"
  for tier in native release; do
    local extra=; [ $tier = release ] && extra=--release
    "$W" build "$f" -o "$T/$r-$tier" --no-cache $extra; rc=$?
    echo "-- $tier build-status $rc"
    [ $rc -eq 0 ] || return $rc
    "$T/$r-$tier"; echo "@status $?"
  done
  echo "-- lupin"
  "$LUPIN" conform-run "$f" --json; echo "@status $?"
}
step rows-signal_action row_everywhere signal_action_rows
step rows-chroot row_everywhere chroot_rows
step os-surface-rows cargo test --locked -p wolf_driver --test os_surface_lanes -- --nocapture --test-threads=1 the_root_rows_on_every_host the_signal_action_rows_on_every_host
echo "== done $(date -u +%FT%TZ)"
