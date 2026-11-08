#!/usr/bin/env bash
# s286: an event stream of the lane's takes on H2 and H1: one line
# when any take changes state (queued, running, ended:N), and a heartbeat
# line every five minutes. Read-only.
look() {
  ssh -o ConnectTimeout=20 H2 bash -s <<'EOF' 2>/dev/null
cd $LANE
for f in logs/*.log; do
  case $f in logs/sensors-*) continue;; esac
  l=$(basename "$f" .log)
  s=queued; grep -q LOCKS_ACQUIRED "$f" && s=running; [ -e "records/$l.exit" ] && s="ended:$(cat records/$l.exit)"
  printf 'sat:%s=%s\n' "$l" "$s"
done
EOF
  ssh -o ConnectTimeout=20 H1 bash -s <<'EOF' 2>/dev/null
cd $LANE
for f in logs/*.log; do
  l=$(basename "$f" .log)
  s=queued; grep -q LOCKS_ACQUIRED "$f" && s=running; [ -e "records/$l.exit" ] && s="ended:$(cat records/$l.exit)"
  printf 'H1:%s=%s\n' "$l" "$s"
done
EOF
}
prev=$(look)
echo "$(date -u +%TZ) START $(echo $prev)"
n=0
while true; do
  sleep 60
  cur=$(look)
  [ -z "$cur" ] && continue
  changed=$(comm -13 <(echo "$prev" | sort) <(echo "$cur" | sort) | tr '\n' ' ')
  [ -n "$changed" ] && echo "$(date -u +%TZ) CHANGE $changed"
  prev=$cur
  n=$((n + 1))
  [ $((n % 5)) -eq 0 ] && echo "$(date -u +%TZ) heartbeat"
done
