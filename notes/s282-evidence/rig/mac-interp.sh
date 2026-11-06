#!/bin/bash
# s282 mac lupin job (run by build-at-interp.sh in the wolf-interp build
# worktree, behind the lock): the mirror rows at a commit, and with
# S282_EXTRA=1 the targeted gates (fmt, clippy, the library tests, the lint
# mirror rows) and a release build of lupin kept for the compiler's
# four-machine rows (records/lupin-<sha>).
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step mirror-rows cargo test --locked --test binder_variant_s282 -- --nocapture --test-threads=1
if [ "${S282_EXTRA:-0}" = 1 ]; then
  step fmt cargo fmt --check
  step clippy cargo clippy --locked --all-targets -- -D warnings
  step lib-tests cargo test --locked --lib
  step lint-rows cargo test --locked --test lint_is61
  step release-build cargo build --release --locked
  mkdir -p "$L/records/lupin-$SHA"
  cp target/release/lupin "$L/records/lupin-$SHA/lupin"
  step lupin-version "$L/records/lupin-$SHA/lupin" --version
fi
echo "== done $(date -u +%FT%TZ)"
