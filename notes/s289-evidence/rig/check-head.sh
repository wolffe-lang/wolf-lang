#!/bin/bash
# s289: the fix's checks at a commit (behind the lock, by at-commit.sh): the
# build, the four-machine test, the sema and diag unit tests, the
# regenerated catalog, and a copy of the binary for the probes.
set -u
L=${LANE:-$LANE}
export LUPIN=$L/inputs/lupin060/x/lupin
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return 0; }
step head bash -c 'git rev-parse HEAD && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step build cargo build --locked -p wolf_driver -p wolf_rt
sha=$(git rev-parse --short=8 HEAD)
out=$L/bin-$sha
mkdir -p "$out"
step copy bash -c "cp -p target/debug/wolf '$out/wolf' && cp -p target/debug/libwolf_rt.a '$out/libwolf_rt.a' && sH3m -a 256 '$out/wolf'"
step lanes cargo test --locked -p wolf_driver --test "${LANES_TEST:-diagnostics_sweep_lanes}" -- --test-threads=4
if [ -n "${EXTRA_TESTS:-}" ]; then step test-extra cargo test --locked ${EXTRA_TESTS}; fi
step fmt cargo fmt --all --check
step clippy cargo clippy --locked -p wolf_sema -p wolf_driver -p wolf_diag -p wolf_pkg --all-targets -- -D warnings
if [ -n "${CORPUS:-}" ]; then step corpus cargo xtask corpus; step conformance cargo xtask conformance; fi
step diag-catalog cargo xtask diag-catalog
step catalog-diff git status --porcelain
echo "== done $(date -u +%FT%TZ)"
