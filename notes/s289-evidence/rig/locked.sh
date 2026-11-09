#!/bin/bash
# s289: run one command behind the Mac build lock (the lane rules).
# Adapted by copy from s285's rig (locked.sh, sha256 c84feb39...).
# The 60 s turn: never re-take within 60 s of this lane's last release.
# TERM, INT and HUP exit with their own nonzero codes, so a stopped take never
# reads as a success.
# usage: locked.sh <cmd> [args...]
LANE=${LANE:-$LANE}
LOCK=$HOME/lanes/mac-build.lock
LAST=$LANE/last-release
if [ -f "$LAST" ]; then
  gap=$(( $(date +%s) - $(cat "$LAST") ))
  if [ "$gap" -lt 60 ]; then
    echo "s289: last release ${gap}s ago; waiting $((60 - gap))s for the turn"
    sleep $((60 - gap))
  fi
fi
trap 'echo "s289: TERM while waiting"; exit 143' TERM
trap 'echo "s289: INT while waiting"; exit 130' INT
trap 'echo "s289: HUP while waiting"; exit 129' HUP
until mkdir "$LOCK" 2>/dev/null; do
  holder=$(cat "$LOCK/pid" 2>/dev/null)
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    echo "s289: lock holder pid $holder is dead; removing the stale lock"
    rm -rf "$LOCK"
    continue
  fi
  echo "s289: $(date +%H:%M:%S) mac build lock held by $(cat "$LOCK/lane" 2>/dev/null) pid $holder; waiting"
  sleep 30
done
echo $$ > "$LOCK/pid"
echo s289 > "$LOCK/lane"
cleanup() { rm -rf "$LOCK"; date +%s > "$LAST"; }
trap cleanup EXIT
trap 'echo "s289: TERM inside the lock"; exit 143' TERM
trap 'echo "s289: INT inside the lock"; exit 130' INT
trap 'echo "s289: HUP inside the lock"; exit 129' HUP
free_bytes() {
  vm_stat | awk '/Pages free/ {f=$3} /Pages inactive/ {i=$3} /Pages speculative/ {s=$3} END {gsub(/\./,"",f); gsub(/\./,"",i); gsub(/\./,"",s); print (f+i+s)*16384}'
}
need=$((8 * 1024 * 1024 * 1024))
while :; do
  fb=$(free_bytes)
  if [ "$fb" -ge "$need" ]; then break; fi
  echo "s289: $(date +%H:%M:%S) only $((fb / 1048576)) MiB free+inactive+speculative (< 8 GiB); waiting"
  sleep 30
done
disk=$(df -g $HOME | awk 'NR==2 {print $4}')
if [ "$disk" -lt 15 ]; then
  echo "s289: only ${disk} GB free on the Mac (< 15 GB); refusing to build"
  exit 75
fi
echo "s289: lock taken $(date +%H:%M:%S), $(( $(free_bytes) / 1048576 )) MiB free, ${disk} GB disk; running: $*"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4
export PATH=$HOME/.cargo/bin:$PATH
export TMPDIR=$LANE/tmp
unset CARGO_TARGET_DIR
"$@"
rc=$?
echo "s289: END exit=$rc $(date +%H:%M:%S)"
exit $rc
