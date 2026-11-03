#!/bin/bash
# s278: run one command behind the Mac build lock (the lane rules).
# Adapted by copy from s277's notes/s277-evidence/rig/locked.sh.
# The 60 s turn: never re-take within 60 s of this lane's last release.
# usage: locked.sh <cmd> [args...]
LOCK=/Users/mfwolffe/lanes/mac-build.lock
LAST=/Users/mfwolffe/lanes/s278/last-release
if [ -f "$LAST" ]; then
  gap=$(( $(date +%s) - $(cat "$LAST") ))
  if [ "$gap" -lt 60 ]; then
    echo "s278: last release ${gap}s ago; waiting $((60 - gap))s for the turn"
    sleep $((60 - gap))
  fi
fi
until mkdir "$LOCK" 2>/dev/null; do
  holder=$(cat "$LOCK/pid" 2>/dev/null)
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    echo "s278: lock holder pid $holder is dead; removing the stale lock"
    rm -rf "$LOCK"
    continue
  fi
  echo "s278: $(date +%H:%M:%S) mac build lock held by $(cat "$LOCK/lane" 2>/dev/null) pid $holder; waiting"
  sleep 30
done
echo $$ > "$LOCK/pid"
echo s278 > "$LOCK/lane"
cleanup() { rm -rf "$LOCK"; date +%s > "$LAST"; }
trap cleanup EXIT
free_bytes() {
  vm_stat | awk '/Pages free/ {f=$3} /Pages inactive/ {i=$3} /Pages speculative/ {s=$3} END {gsub(/\./,"",f); gsub(/\./,"",i); gsub(/\./,"",s); print (f+i+s)*16384}'
}
need=$((8 * 1024 * 1024 * 1024))
while :; do
  fb=$(free_bytes)
  if [ "$fb" -ge "$need" ]; then break; fi
  echo "s278: $(date +%H:%M:%S) only $((fb / 1048576)) MiB free+inactive+speculative (< 8 GiB); waiting"
  sleep 30
done
disk=$(df -g /Users/mfwolffe | awk 'NR==2 {print $4}')
if [ "$disk" -lt 15 ]; then
  echo "s278: only ${disk} GB free on the Mac (< 15 GB); refusing to build"
  exit 75
fi
echo "s278: lock taken $(date +%H:%M:%S), $(( $(free_bytes) / 1048576 )) MiB free, ${disk} GB disk; running: $*"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4
export PATH=$HOME/.cargo/bin:$PATH
export TMPDIR=/Users/mfwolffe/lanes/s278/tmp
unset CARGO_TARGET_DIR
"$@"
rc=$?
echo "s278: END exit=$rc $(date +%H:%M:%S)"
exit $rc
