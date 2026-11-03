#!/usr/bin/env bash
# s278: the linux record's chunk at the final head, inside r41-toolchain as
# the lane uid (adapted by copy from s277's gauntlet-chunk.sh): one lock take,
# at most 2 h. Every line it prints is inside a `START`/`END <step> exit=N
# secs=S` pair (the org's gauntlet recorder, sourced, is the step function),
# each step's own output indented two spaces; it stops at the first nonzero
# step. The corpus step is `cargo xtask ci`'s own (`cargo xtask corpus`), its
# refusal lines repeated at column 0 as `FAIL <file> …`; then the two
# os_surface_lanes rows the lane changed, on linux.
# usage: gauntlet-chunk.sh corpus <expected-sha>
set -uo pipefail
exec 2>&1
source /lane/rig/env.sh
source /lane/rig/gauntlet-record.sh
chunk=${1:?chunk} expected=${2:?sha}
cd /lane/wolf-lang || exit 2
indent() { "$@" 2>&1 | sed 's/^/  /'; return "${PIPESTATUS[0]}"; }
head_ok() {
  test "$(git rev-parse HEAD)" = "$expected" && test -z "$(git status --porcelain --untracked-files=no)" &&
    git rev-parse HEAD && git log --oneline -1 && uname -srm && rustc -V && cargo -V && "$LUPIN" --version &&
    grep -E 'Cpus_allowed_list|SigBlk|SigIgn|CapEff' /proc/self/status &&
    printf 'pid1 uid=%s self uid=%s block=%s tests on %s\n' "$(awk '/^Uid:/{print $2}' /proc/1/status)" "$(id -u)" "$S278_BLOCK" "$WOLF_TEST_CPUS" &&
    test "$(awk '/^Uid:/{print $2}' /proc/1/status)" != "$(id -u)"
}
gauntlet_step "$chunk-head" indent head_ok || exit $?
corpus_step() {
  local log rc
  log=$(mktemp "$TMPDIR/s278-corpus.XXXXXX") || return 1
  cargo xtask corpus > "$log" 2>&1; rc=$?
  sed 's/^/  /' "$log"
  grep -E '^corpus: [^ ]+\.lu: ' "$log" | sed -E 's/^corpus: ([^ ]+\.lu): (.*)$/FAIL \1 \2/'
  rm -f "$log"
  return $rc
}
case $chunk in
  corpus)
    gauntlet_step build indent cargo build --locked -p wolf_driver -p wolf_rt || exit $?
    gauntlet_step corpus corpus_step || exit $?
    gauntlet_step os-surface-rows indent cargo test --locked -p wolf_driver --test os_surface_lanes -- --nocapture --test-threads=1 the_root_rows_on_every_host the_signal_action_rows_on_every_host || exit $?
    ;;
  *) exit 2 ;;
esac
gauntlet_step "$chunk-clean" indent bash -c 'git diff --exit-code --stat && test -z "$(git status --porcelain --untracked-files=no)"' || exit $?
exit 0
