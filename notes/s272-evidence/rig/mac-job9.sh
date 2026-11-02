#!/bin/bash
# s272 mac job 9 (run by build-at.sh in the build worktree, behind the Mac
# lock): at one commit, the statfs rows alone, os_surface_lanes whole (the
# pty row beside them), fmt and clippy on wolf_driver's tests.
set -u
export PATH=$HOME/.cargo/bin:$PATH
B=/Users/mfwolffe/lanes/s272/build
cd "$B" || exit 1
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ) $(rustc --version)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step build cargo build -j4 -p wolf_driver --bin wolf -p wolf_rt
step churn-row cargo test -j4 -p wolf_driver --test os_surface_lanes fs_statfs_holds_while_a_writer_churns -- --nocapture --exact
step plain-row cargo test -j4 -p wolf_driver --test os_surface_lanes fs_statfs_matches_gnu_stat_f -- --nocapture --exact
for i in 1 2 3; do step os-surface-all-$i cargo test -j4 -p wolf_driver --test os_surface_lanes -- --test-threads=4; done
step fmt cargo fmt --all --check
step clippy-driver cargo clippy -j4 -p wolf_driver --all-targets -- -D warnings
echo "clean: $(git status --porcelain | wc -l) paths"
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
