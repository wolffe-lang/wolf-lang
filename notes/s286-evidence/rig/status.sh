#!/usr/bin/env bash
# s286: one read-only look at every take of the lane on every host.
ssh H2 bash -s <<'EOF'
cd $LANE || exit 1
echo "== H2 $(date -u +%TZ) wineserver=[$(ps -C wineserver -o pid= | tr -d ' ')]"
for f in logs/*.log; do
  case $f in logs/sensors-*) continue;; esac
  l=$(basename "$f" .log)
  printf '%s: exit=%s | %s | %s\n' "$l" "$(cat records/$l.exit 2>/dev/null || echo -)" \
    "$(grep -v TAKE_ALIVE "$f" | tail -1 | cut -c1-110)" "$(tail -1 "$f" | cut -c1-60)"
done
ps -eo pid,etime,args | grep '[f]lock .*H2-A.lock' | cut -c1-80
EOF
ssh H1 bash -s <<'EOF'
cd $LANE || exit 1
echo "== H1 $(date -u +%TZ)"
for f in logs/*.log; do
  l=$(basename "$f" .log)
  printf '%s: exit=%s | %s | %s\n' "$l" "$(cat records/$l.exit 2>/dev/null || echo -)" \
    "$(grep -v TAKE_ALIVE "$f" | tail -1 | cut -c1-110)" "$(tail -1 "$f" | cut -c1-60)"
done
for b in A B; do printf '%s block waiters=%s\n' "$b" "$(ps -eo args | grep -c "[f]lock .*H1-block-$b.lock")"; done
EOF
