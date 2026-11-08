#!/bin/bash
# s287 mac rows job (run by build-at.sh in the build worktree, behind the
# lock; adapted by copy from s285's mac-rows.sh): build; the named rows on
# the four machines (probe-all); the four-machine lanes test, filtered by
# $TESTS; the anchors gate; and, with FULL=1, the corpus and conformance
# steps whole. ROWS names corpus paths without `.lu`.
set -u
L=$LANE
export LUPIN=$L/inputs/lupin060/unpacked/lupin-0.1.60-aarch64-apple-darwin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
head_info() { { git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l; } 2>/dev/null || echo "tree of $(cat .s287-sha)"; rustc -V && "$LUPIN" --version; uname -a; }
step head head_info
step build cargo build --locked -p wolf_driver -p wolf_rt || { echo "== build failed: stopping"; exit 1; }
if [ -n "${ROWS:-}" ]; then
  files=(); for r in $ROWS; do files+=("$PWD/corpus/$r.lu"); done
  step rows-everywhere "$L/rig/probe-all.sh" "${CARGO_TARGET_DIR:-$PWD/target}/debug/wolf" "${files[@]}"
fi
step rows-test cargo test --locked -p wolf_driver --test front_end_rulings_lanes -- --test-threads=1 ${TESTS:-}
step anchors cargo xtask spec-extract --check
if [ "${FULL:-0}" = 1 ]; then
  step corpus cargo xtask corpus
  step conformance cargo xtask conformance
fi
if [ -n "${EXTRA:-}" ]; then
  step extra bash -c "$EXTRA"
fi
echo "== done $(date -u +%FT%TZ)"
