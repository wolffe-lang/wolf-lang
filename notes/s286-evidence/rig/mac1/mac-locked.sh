#!/usr/bin/env bash
# s286: run one command behind the Mac build lock, with the 8 GiB gate.
#   mac-locked.sh <cmd...>
# The lock is the directory $LANES/mac-build.lock (mkdir).
# A holder whose pid is dead is cleared at once; otherwise wait 30 s and
# print. The 8 GiB gate: free + inactive + speculative pages x 16384.
# The 60 s turn: never re-take within 60 s of this lane's last release
# (the release time is kept in last-release).
set -u
lock=$LANES/mac-build.lock
last=$LANE/last-release
if [ -f "$last" ]; then
  gap=$(( $(date +%s) - $(cat "$last") ))
  if [ "$gap" -lt 60 ]; then
    echo "mac-locked: last release ${gap}s ago; waiting $((60 - gap))s for the turn"
    sleep $((60 - gap))
  fi
fi
while ! mkdir "$lock" 2>/dev/null; do
  holder=$(cat "$lock/pid" 2>/dev/null || true)
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    echo "mac-locked: holder pid $holder is dead; clearing $(date -u +%TZ)"
    rm -rf "$lock"
    continue
  fi
  echo "mac-locked: waiting on $(cat "$lock/lane" 2>/dev/null || echo '?') pid ${holder:-?} $(date -u +%TZ)"
  sleep 30
done
echo $$ > "$lock/pid"
echo s286 > "$lock/lane"
trap 'rm -rf "$lock"; date +%s > "$last"' EXIT
trap 'exit 143' TERM; trap 'exit 130' INT; trap 'exit 129' HUP
while :; do
  pages=$(vm_stat | awk '/Pages free/ {f=$3} /Pages inactive/ {i=$3} /Pages speculative/ {s=$3} END {gsub(/\./,"",f); gsub(/\./,"",i); gsub(/\./,"",s); print f+i+s}')
  bytes=$((pages * 16384))
  if [ "$bytes" -ge $((8 * 1024 * 1024 * 1024)) ]; then
    echo "mac-locked: gate ok, $((bytes / 1048576)) MiB available $(date -u +%TZ)"
    break
  fi
  echo "mac-locked: gate waits, $((bytes / 1048576)) MiB available $(date -u +%TZ)"
  sleep 30
done
"$@"
rc=$?
echo "mac-locked: exit=$rc $(date -u +%TZ)"
exit "$rc"
