#!/usr/bin/env bash
# s286: print one line a minute until the lane's takes change state from what
# was seen at the start (a take holds its locks, or ends), at most N minutes.
#   wait-change.sh N
n=${1:-9}
look() {
  ssh H2 bash -s <<'EOF'
cd $LANE
for f in logs/*.log; do
  case $f in logs/sensors-*) continue;; esac
  l=$(basename "$f" .log)
  s=queued; grep -q LOCKS_ACQUIRED "$f" && s=running; [ -e "records/$l.exit" ] && s="ended:$(cat records/$l.exit)"
  printf '%s=%s ' "$l" "$s"
done
EOF
  ssh H1 bash -s <<'EOF'
cd $LANE
for f in logs/*.log; do
  l=$(basename "$f" .log)
  s=queued; grep -q LOCKS_ACQUIRED "$f" && s=running; [ -e "records/$l.exit" ] && s="ended:$(cat records/$l.exit)"
  printf '%s=%s ' "$l" "$s"
done
EOF
}
first=$(look)
echo "$(date -u +%TZ) $first"
for i in $(seq 1 "$n"); do
  sleep 60
  now=$(look)
  echo "$(date -u +%TZ) $now"
  [ "$now" = "$first" ] || exit 0
done
