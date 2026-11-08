#!/usr/bin/env bash
# s286: the two terminal rows that failed at posix_openpt, at the base and at
# the head, each its own take of the Mac lock (mac-locked.sh: the 8 GiB gate),
# with the 60 s turn between them. Each worktree is pinned to its commit.
#   mac-term-pair.sh BASE_WT HEAD_WT LOG
set -u
base_wt=$1; head_wt=$2; log=$3
export PATH=$HOME/.cargo/bin:$PATH CARGO_INCREMENTAL=0 TMPDIR=$LANE/tmp
run() {
  local wt=$1
  echo "== $(git -C "$wt" log --oneline -1) in $wt $(date -u +%TZ)"
  cd "$wt" || return 2
  bash $LANE/rig/mac-locked.sh cargo test -j4 --locked -p wolf_rt --lib -- \
    term::tests::the_three_raw_bits term::tests::the_pty_helper_answers_its_own_pair --nocapture
  echo "== exit=$? $(date -u +%TZ)"
}
{
  run "$base_wt"
  sleep 65
  run "$head_wt"
  echo "PAIR_DONE $(date -u +%TZ)"
} > "$log" 2>&1
