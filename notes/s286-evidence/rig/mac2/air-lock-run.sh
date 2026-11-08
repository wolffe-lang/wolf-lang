#!/bin/bash
# s286 on the Air: run one command behind the Air's own build lock
# (~/lanes/mac-build.lock, a directory: mkdir to take), by copy from the
# main Mac's lock-run.sh. Its own lines go to stderr, so a record sees only
# its steps. The 60 s turn after this lane's last release; the 8 GiB memory
# gate; the Air's 25 GB disk floor. The lane's own toolchain (rustup 1.97.1
# under ~/lanes/s286).
# usage: air-lock-run.sh <cmd> [args...]
L=$LANE
LOCK=$LANES/mac-build.lock
LAST=$L/last-release
if [ -f "$LAST" ]; then
  gap=$(( $(date +%s) - $(cat "$LAST") ))
  if [ "$gap" -lt 60 ]; then
    echo "s286: last release ${gap}s ago; waiting $((60 - gap))s for the turn" >&2
    sleep $((60 - gap))
  fi
fi
until mkdir "$LOCK" 2>/dev/null; do
  holder=$(cat "$LOCK/pid" 2>/dev/null)
  if [ -n "$holder" ] && ! kill -0 "$holder" 2>/dev/null; then
    echo "s286: lock holder pid $holder is dead; removing the stale lock" >&2
    rm -f "$LOCK/pid" "$LOCK/lane"; rmdir "$LOCK"
    continue
  fi
  echo "s286: $(date +%H:%M:%S) air build lock held by $(cat "$LOCK/lane" 2>/dev/null) pid $holder; waiting" >&2
  sleep 30
done
echo $$ > "$LOCK/pid"
echo s286 > "$LOCK/lane"
cleanup() { rm -f "$LOCK/pid" "$LOCK/lane"; rmdir "$LOCK"; date +%s > "$LAST"; }
trap cleanup EXIT
trap 'exit 143' TERM; trap 'exit 130' INT; trap 'exit 129' HUP
free_bytes() {
  vm_stat | awk '/Pages free/ {f=$3} /Pages inactive/ {i=$3} /Pages speculative/ {s=$3} END {gsub(/\./,"",f); gsub(/\./,"",i); gsub(/\./,"",s); print (f+i+s)*16384}'
}
need=$((8 * 1024 * 1024 * 1024))
while :; do
  fb=$(free_bytes)
  if [ "$fb" -ge "$need" ]; then break; fi
  echo "s286: $(date +%H:%M:%S) only $((fb / 1048576)) MiB free+inactive+speculative (< 8 GiB); waiting" >&2
  sleep 30
done
disk=$(df -g $HOME | awk 'NR==2 {print $4}')
if [ "$disk" -lt 25 ]; then
  echo "s286: only ${disk} GB free on the Air (< 25 GB); refusing to build" >&2
  exit 75
fi
echo "s286: air lock taken $(date +%H:%M:%S), $(( $(free_bytes) / 1048576 )) MiB free, ${disk} GB disk; running: $*" >&2
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4
export RUSTUP_HOME=$L/rustup CARGO_HOME=$L/cargo
export PATH=$L/cargo/bin:$PATH
export TMPDIR=$L/tmp
unset CARGO_TARGET_DIR
"$@"
