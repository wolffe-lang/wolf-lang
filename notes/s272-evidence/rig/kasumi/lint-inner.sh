#!/usr/bin/env bash
# s272: fmt and clippy at one commit, inside r41-toolchain on kasumi's slot 8-11.
# usage: lint-inner.sh <sha>
set -uo pipefail
source /lane/scripts/gate-env.sh
cd /lane/wolf-lang || exit 2
git checkout -q --detach "$1" && test "$(git rev-parse HEAD)" = "$1" || { echo "END checkout exit=2"; exit 2; }
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ) $(rustc --version)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step fmt cargo fmt --all --check
step clippy cargo clippy --locked --workspace --all-targets -- -D warnings
exit $rc_all
