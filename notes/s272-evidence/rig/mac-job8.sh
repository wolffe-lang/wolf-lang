#!/bin/bash
# s272 mac job 8 (run by build-at.sh in the build worktree, behind the Mac
# lock): the statfs rows one at a time (the churn row, then the plain row)
# at the red commit (where build-at.sh left the worktree), then the same at
# the fix's commit (the worktree moved, detached and clean), with fmt and
# clippy on wolf_driver's tests there.
set -u
export PATH=$HOME/.cargo/bin:$PATH
B=/Users/mfwolffe/lanes/s272/build
FIX=13818328
cd "$B" || exit 1
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
rows() {
  echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ) $(rustc --version)"
  step build cargo build -j4 -p wolf_driver --bin wolf -p wolf_rt
  step churn-row cargo test -j4 -p wolf_driver --test os_surface_lanes fs_statfs_holds_while_a_writer_churns -- --nocapture --exact
  step plain-row cargo test -j4 -p wolf_driver --test os_surface_lanes fs_statfs_matches_gnu_stat_f -- --nocapture --exact
}
rows
red_rc=$rc_all
rc_all=0
git checkout -q --detach "$FIX" || { echo "checkout $FIX failed"; exit 2; }
[ -z "$(git status --porcelain --untracked-files=no)" ] || { echo "not clean at $FIX"; exit 2; }
rows
step os-surface-all cargo test -j4 -p wolf_driver --test os_surface_lanes -- --test-threads=2
step fmt cargo fmt --all --check
step clippy-driver cargo clippy -j4 -p wolf_driver --all-targets -- -D warnings
echo "clean: $(git status --porcelain | wc -l) paths"
echo "ALL red-commit rc=$red_rc (nonzero expected: the churn row) fix rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
