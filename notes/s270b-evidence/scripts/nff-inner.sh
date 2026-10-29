#!/usr/bin/env bash
# s270b: every `cargo xtask ci` step at the checked-out head, in xtask ci's order, WITHOUT
# stopping at a red (xtask ci has no --no-fail-fast). Each step is the exact `cargo` command
# xtask/src/main.rs's CI_STEPS names; `test` adds --no-fail-fast. One exit per step is recorded.
set -uo pipefail
source /lane/scripts/gauntlet-env.sh
cd /lane/wolf-lang
printf 's270b NFF START %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git status --short
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
printf 'pid1 uid: '; awk '/^Uid:/{print $2}' /proc/1/status
printf 'self uid: '; id -u
pre() { local name=$1; shift; printf '\n== PRE %s %s\n' "$name" "$(date -u +%FT%TZ)"; "$@"; local rc=$?; printf '== PRE %s EXIT %s\n' "$name" "$rc"; return "$rc"; }
pre build cargo build --locked -p wolf_driver -p wolf_rt || exit 10
pre dist-rt cargo xtask dist-rt --target x86_64-unknown-linux-musl || exit 11
pre clef ci/build-test-clef || exit 12
pre ligature env WOLF=/lane/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu/wolf ci/build-test-ligature || exit 13
red=0
summary=""
step() {
  local name=$1; shift
  printf '\n== xtask ci (nff): %s %s\n' "$name" "$(date -u +%FT%TZ)"
  cargo "$@"
  local rc=$?
  printf '== STEP %s EXIT %s %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
  summary="$summary $name=$rc"
  [ "$rc" = 0 ] || red=$((red + 1))
}
step fmt fmt --all --check
step fmt-fixpoint xtask fmt-fixpoint
step clippy clippy --workspace --all-targets -- -D warnings
step rt-none xtask rt-none
step test test --workspace --no-fail-fast
step fuzz-check check --manifest-path fuzz/Cargo.toml
step deps-check xtask deps-check
step corpus xtask corpus
step abi-check xtask abi-check
step debug-check xtask debug-check
step midend-rate xtask midend-rate
step bench-gates xtask bench-gates
step spec-extract xtask spec-extract --check
step conformance xtask conformance
step print-gate xtask print-gate
step diag-catalog xtask diag-catalog --check
step doc-catalog xtask doc-catalog --check
step fmt-lu xtask fmt-lu
step fmt-fuzz xtask fmt-fuzz --ci
step audit-surface xtask audit-surface
step dist-smoke xtask dist
step differ-self xtask differ --self
step lane-coverage xtask lane-coverage
printf '\nNFF_SUMMARY%s\n' "$summary"
printf 'NFF_RED_STEPS %s\n' "$red"
git status --short
git diff --exit-code > /dev/null; printf 'NFF_TREE_CLEAN %s\n' "$?"
printf 's270b NFF END %s\n' "$(date -u +%FT%TZ)"
[ "$red" = 0 ]
