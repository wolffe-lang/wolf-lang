#!/usr/bin/env bash
# s272: a detached kasumi job (run under setsid -f): pid file, log, exit marker;
# the slot 8-11 lock taken ONCE for the whole chain, one long wait.
# usage: job.sh <name> <chain-script> [args...]
set -u
name=$1; shift
d=/home/mfwolffe/lanes/s272/records/$name
mkdir -p "$d"
rm -f "$d/exit.marker"
echo $$ > "$d/pid"
{
  printf 'S272 %s waiting for kasumi slot 8-11 at %s\n' "$name" "$(date -u +%FT%TZ)"
  flock -w 21600 /home/mfwolffe/lanes/kasumi-cores-8-11.lock bash -c 'printf "S272 acquired kasumi slot 8-11 at %s\n" "$(date -u +%FT%TZ)"; df -h /home | tail -1; bash "$@"' bash "$@"
  rc=$?
  printf 'S272 %s chain END exit=%s at %s\n' "$name" "$rc" "$(date -u +%FT%TZ)"
} > "$d/log" 2>&1
echo "exit=$rc" > "$d/exit.marker"
