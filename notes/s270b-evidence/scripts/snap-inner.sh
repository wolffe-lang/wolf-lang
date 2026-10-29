#!/usr/bin/env bash
# s270b: the seven rows' snapshots in the three corpus-walking suites, written by insta in the
# container at the rebased head, then read back with INSTA_UPDATE=no. Prints every changed path
# and the full diff of every tracked file that changed (only the ledger may).
set -uo pipefail
source /lane/scripts/gauntlet-env.sh
cd /lane/wolf-lang
printf 's270b SNAP %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git status --short
for t in "wolf_lex corpus_snapshots" "wolf_parse corpus_decls" "wolf_wir lower_corpus"; do
  set -- $t
  INSTA_UPDATE=always cargo test -q -p "$1" --test "$2" > "/lane/logs/snap-write-$2.log" 2>&1
  printf 'SNAP write %s/%s rc=%s\n' "$1" "$2" "$?"
done
echo '--- changed paths'
git status --short
echo '--- tracked diff'
git diff
for t in "wolf_lex corpus_snapshots" "wolf_parse corpus_decls" "wolf_wir lower_corpus"; do
  set -- $t
  INSTA_UPDATE=no cargo test -q -p "$1" --test "$2" > "/lane/logs/snap-check-$2.log" 2>&1
  printf 'SNAP check %s/%s rc=%s\n' "$1" "$2" "$?"
done
printf 's270b SNAP DONE %s\n' "$(date -u +%FT%TZ)"
