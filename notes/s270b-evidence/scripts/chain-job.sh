#!/usr/bin/env bash
# s270b measurement chain, detached: census, corpus differ, the repaired witness at the
# remaining sizes. Each step takes block A for itself (container.sh), in order.
set -uo pipefail
base=/home/mfwolffe/lanes/s270b
echo $$ > "$base/records/chain.pid"
rm -f "$base/records/chain.exit"
printf 's270b CHAIN pid %s start %s\n' "$$" "$(date -u +%FT%TZ)"
bash "$base/scripts/container.sh" census /lane/scripts/py-inner.sh census.py census.jsonl > "$base/logs/census.log" 2>&1
printf 's270b CHAIN census exit %s %s\n' "$?" "$(date -u +%FT%TZ)"
bash "$base/scripts/container.sh" differ /lane/scripts/differ-inner.sh > "$base/logs/differ.log" 2>&1
printf 's270b CHAIN differ exit %s %s\n' "$?" "$(date -u +%FT%TZ)"
bash "$base/scripts/container.sh" yew-copy2 /lane/scripts/py-inner.sh repro2.py yew-copy.jsonl yew-copy head=/lane/wolf-lang/target/debug/wolf,base=/lane/base/target/debug/wolf,w035=/lane/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu/wolf 1024,33 1024,64 100000,2 100000,64 > "$base/logs/yew-copy2.log" 2>&1
printf 's270b CHAIN yew-copy2 exit %s %s\n' "$?" "$(date -u +%FT%TZ)"
echo 0 > "$base/records/chain.exit"
printf 's270b CHAIN end %s\n' "$(date -u +%FT%TZ)"
