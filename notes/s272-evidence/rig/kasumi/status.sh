#!/usr/bin/env bash
# s272: a kasumi job, for the watch: every END/S272/LOOP line so far, the last
# step, the marker.
d=/home/mfwolffe/lanes/s272/records/${1:?job}
grep -E '^(END |S272|REPORT the churn|== STEP|.*agreed in triple [1-9]|.*no triple of|test result)' "$d/log" 2>/dev/null | grep -v '^S272 in the container'
printf 'STATUS %s last=[%s] marker=[%s]\n' "$(date -u +%T)" \
  "$(grep -E '^== ' "$d/log" 2>/dev/null | tail -1 | cut -c1-60)" "$(cat "$d/exit.marker" 2>/dev/null)"
