#!/usr/bin/env bash
# s270b focused checks before the gauntlet: every step's exit code is printed by name
set -uo pipefail
source /lane/scripts/env.sh
export WOLF_PAIRING_REQUIRE_SIBLING=1 RUST_TEST_NOCAPTURE=1
cd /lane/wolf-lang
printf 's270b FOCUSED %s\n' "$(date -u +%FT%TZ)"
step() { local name=$1; shift; "$@" > "/lane/logs/focused-$name.log" 2>&1; local rc=$?; printf 'STEP %s rc=%s\n' "$name" "$rc"; }
step fmt cargo fmt --all --check
step clippy cargo clippy --workspace --all-targets -- -D warnings
step mem-tests cargo test -p wolf_mem
step outer-write cargo test -p wolf_driver --test region_outer_write_lanes -- --test-threads=2
step fmt-fixpoint cargo xtask fmt-fixpoint
step fmt-lu cargo xtask fmt-lu
step corpus cargo xtask corpus
step diag-catalog cargo xtask diag-catalog --check
step spec-extract cargo xtask spec-extract --check
printf 's270b FOCUSED DONE %s\n' "$(date -u +%FT%TZ)"
