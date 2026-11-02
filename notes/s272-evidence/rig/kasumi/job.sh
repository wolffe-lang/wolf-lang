#!/usr/bin/env bash
# s272: a detached kasumi job (run under setsid -f): pid file, log, exit marker;
# the slot's one lock taken ONCE for the whole chain, one long wait. The chain
# gets the slot as its first argument.
# usage: job.sh <slot:4-7|8-11> <name> <chain-script> [args...]
set -u
slot=$1 name=$2; shift 2
d=/home/mfwolffe/lanes/s272/records/$name
mkdir -p "$d"
rm -f "$d/exit.marker"
echo $$ > "$d/pid"
{
  printf 'S272 %s waiting for kasumi slot %s at %s\n' "$name" "$slot" "$(date -u +%FT%TZ)"
  flock -w 21600 "/home/mfwolffe/lanes/kasumi-cores-$slot.lock" bash -c 'slot=$1; shift; printf "S272 acquired kasumi slot %s at %s\n" "$slot" "$(date -u +%FT%TZ)"; df -h /home | tail -1; bash "$1" "$slot" "${@:2}"' bash "$slot" "$@"
  rc=$?
  printf 'S272 %s chain END exit=%s at %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
} > "$d/log" 2>&1
echo "exit=$rc" > "$d/exit.marker"
