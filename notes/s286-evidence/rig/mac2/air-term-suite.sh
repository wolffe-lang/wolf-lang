#!/usr/bin/env bash
# s286: the runtime's lib suite whole (where two terminal rows failed at
# posix_openpt in an early run), at the base and at the head, each its own
# take of the Air lock (air-lock-run.sh), with the 60 s turn
# between them. Each worktree is pinned to its commit. Every test line is
# kept, then the terminal rows and the verdict are repeated at the end.
#   mac-term-suite.sh BASE_WT HEAD_WT LOG
set -u
base_wt=$1; head_wt=$2; log=$3
export TMPDIR=$LANE/tmp
run() {
  local wt=$1 out rc
  out=$LANE/tmp/suite-$(git -C "$wt" rev-parse --short=8 HEAD).log
  echo "== $(git -C "$wt" log --oneline -1) in $wt $(date -u +%TZ)"
  cd "$wt" || return 2
  bash $LANE/rig/air-lock-run.sh cargo test -j4 --locked -p wolf_rt --lib > "$out" 2>&1
  rc=$?
  cat "$out"
  echo "== terminal rows and verdict:"
  grep -E '^test term::|posix_openpt|test result' "$out" | sort | uniq -c
  echo "== exit=$rc $(date -u +%TZ)"
}
{
  run "$base_wt"
  sleep 65
  run "$head_wt"
  echo "SUITE_DONE $(date -u +%TZ)"
} > "$log" 2>&1
