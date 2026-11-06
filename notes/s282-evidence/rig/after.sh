#!/bin/bash
# s282: wait (printing each minute) until every marker named exists, then run
# the command. usage: after.sh <marker>... -- <cmd> [args...]
set -u
m=(); while [ "$1" != "--" ]; do m+=("$1"); shift; done; shift
while :; do
  missing=0; for f in "${m[@]}"; do [ -e "$f" ] || missing=1; done
  [ $missing = 0 ] && break
  echo "s282: $(date -u +%T) waiting for ${m[*]}"; sleep 60
done
echo "s282: $(date -u +%T) markers present: $(for f in "${m[@]}"; do printf '%s=%s ' "$(basename "$f")" "$(cat "$f")"; done)"
exec "$@"
