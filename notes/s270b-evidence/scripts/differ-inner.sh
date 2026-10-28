#!/usr/bin/env bash
# s270b: the compiler control over the corpus — head (A) against base trunk 5adf3d91 (B) — on
# the checked machine, native and release. Raw differ statuses are kept: a named move is the
# claim, never a zero-divergence claim.
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/wolf-lang
unset WOLF_STD
printf 's270b DIFFER START %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
for mode in checked native release; do
  export S270B_PROXY_TIER=$mode
  export S270B_PROTOCOL_LOG="/lane/records/differ-$mode.jsonl"
  rm -f "$S270B_PROTOCOL_LOG"
  flag=; [ "$mode" = checked ] && flag=--checked; [ "$mode" = native ] && flag=--native
  cargo xtask differ /lane/scripts/proxy-wolf-A /lane/scripts/proxy-wolf-B ${flag:+$flag} > "/lane/logs/differ-$mode.log" 2>&1
  rc=$?; echo "$rc" > "/lane/records/differ-$mode.exit"
  printf 'differ %s exit %s %s\n' "$mode" "$rc" "$(date -u +%FT%TZ)"
  tail -25 "/lane/logs/differ-$mode.log"
done
printf 's270b DIFFER END %s\n' "$(date -u +%FT%TZ)"
