#!/bin/bash
# s272: run one command behind the Mac build lock (the-lane-rules-1031.md).
# Adapted by copy from s273's locked.sh.
# usage: locked.sh <cmd> [args...]
LOCK=/Users/mfwolffe/lanes/mac-build.lock
until mkdir "$LOCK" 2>/dev/null; do
  holder=$(cat "$LOCK/pid" 2>/dev/null)
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    echo "s272: lock holder pid $holder is dead; removing the stale lock"
    rm -rf "$LOCK"
    continue
  fi
  echo "s272: $(date +%H:%M:%S) mac build lock held by $(cat "$LOCK/lane" 2>/dev/null) pid $holder; waiting"
  sleep 30
done
echo $$ > "$LOCK/pid"
echo s272 > "$LOCK/lane"
cleanup() { rm -rf "$LOCK"; date +%s > /Users/mfwolffe/lanes/s272/last-release; }
trap cleanup EXIT
free_bytes() {
  vm_stat | awk '/Pages free/ {f=$3} /Pages inactive/ {i=$3} /Pages speculative/ {s=$3} END {gsub(/\./,"",f); gsub(/\./,"",i); gsub(/\./,"",s); print (f+i+s)*16384}'
}
need=$((8 * 1024 * 1024 * 1024))
while :; do
  fb=$(free_bytes)
  if [ "$fb" -ge "$need" ]; then break; fi
  echo "s272: $(date +%H:%M:%S) only $((fb / 1048576)) MiB free+inactive+speculative (< 8 GiB); waiting"
  sleep 30
done
echo "s272: lock taken, $(( $(free_bytes) / 1048576 )) MiB free; running: $*"
export CARGO_INCREMENTAL=0
"$@"
rc=$?
echo "s272: END exit=$rc"
exit $rc
