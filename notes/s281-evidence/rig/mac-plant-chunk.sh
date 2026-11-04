#!/bin/bash
# s281: the plant's steps behind the Mac lock, in the plant worktree on its
# branch (adapted by copy from s278's mac-gauntlet-chunk.sh, chunk `plant`).
# Every line is inside a `START`/`END <step> exit=N secs=S` pair (the org's
# gauntlet recorder, sourced, is the step function), each step's own output
# indented two spaces; it stops at the first nonzero step. The rows step is
# the four-machine test rows, expected red by name: each `test <row> ...
# FAILED` line is repeated at column 0 as `FAIL <row>`.
# usage: mac-plant-chunk.sh <expected-sha>
set -u
exec 2>&1
L=/Users/mfwolffe/lanes/s281
source "$L/rig/gauntlet-record.sh"
expected=${1:?sha}
export LUPIN=/Users/mfwolffe/.local/bin/lupin
export WOLF_TEST_NATIVE_REQUIRE=1 WOLF_PAIRING_REQUIRE_SIBLING=1
indent() { "$@" 2>&1 | sed 's/^/  /'; return "${PIPESTATUS[0]}"; }
head_ok() {
  test "$(git rev-parse HEAD)" = "$expected" || { echo "head is not $expected"; return 1; }
  test -z "$(git status --porcelain --untracked-files=no)" || { echo "tracked checkout is dirty"; return 1; }
  git rev-parse HEAD && git symbolic-ref --short HEAD && git log --oneline -2 && git diff --stat HEAD~1 HEAD &&
    git diff HEAD~1 HEAD && sw_vers && rustc -V && "$LUPIN" --version
}
rows_step() {
  local log rc
  log=$(mktemp "$TMPDIR/s281-rows.XXXXXX") || return 1
  cargo test --locked -p wolf_driver --test four_machine_regressions -- --test-threads=1 > "$log" 2>&1; rc=$?
  sed 's/^/  /' "$log"
  grep -E '^test [A-Za-z0-9_]+ \.\.\. FAILED$' "$log" | sed -E 's/^test ([A-Za-z0-9_]+) \.\.\. FAILED$/FAIL \1/'
  rm -f "$log"
  return $rc
}
gauntlet_step plant-head indent head_ok || exit $?
gauntlet_step plant-build indent cargo build --locked -p wolf_driver -p wolf_rt || exit $?
gauntlet_step plant-rows rows_step || exit $?
gauntlet_step plant-clean indent bash -c 'git diff --exit-code --stat && test -z "$(git status --porcelain --untracked-files=no)"' || exit $?
exit 0
