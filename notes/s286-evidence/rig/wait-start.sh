#!/usr/bin/env bash
# s286: wait (at most N minutes) until any of the lane's takes holds its
# locks or ends, printing one line a minute.
#   wait-start.sh N
n=${1:-9}
for i in $(seq 1 "$n"); do
  s=$(ssh H2 bash -s <<'EOF'
cd $LANE
for l in red-c09042b8-sat8 green-8fd5a8ef-sat8 full-c09042b8-8fd5a8ef-sat16; do
  grep -q LOCKS_ACQUIRED "logs/$l.log" && printf '%s started; ' "$l"
  [ -e "records/$l.exit" ] && printf '%s ended %s; ' "$l" "$(cat records/$l.exit)"
done
EOF
)
  h=$(ssh H1 bash -s <<'EOF'
cd $LANE
l=perf2-c09042b8-8fd5a8ef-B
grep -q LOCKS_ACQUIRED "logs/$l.log" && printf '%s started; ' "$l"
[ -e "records/$l.exit" ] && printf '%s ended %s; ' "$l" "$(cat records/$l.exit)"
EOF
)
  echo "$(date -u +%TZ) H2: ${s:-waiting} | H1: ${h:-waiting}"
  if [ -n "$s$h" ] && [ "${2:-}" != all ]; then exit 0; fi
  sleep 60
done
