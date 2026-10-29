#!/usr/bin/env bash
# s270b chain 2, detached: the plant's local red run, the #919 repair probe, then the hasu
# clone moved to the pushed head and the full gauntlet there. Each container step takes
# block A for itself. pid file, log, exit marker (records/chain2.{pid,exit}).
set -uo pipefail
base=/home/mfwolffe/lanes/s270b
echo $$ > "$base/records/chain2.pid"
rm -f "$base/records/chain2.exit"
printf 's270b CHAIN2 pid %s start %s\n' "$$" "$(date -u +%FT%TZ)"
bash "$base/scripts/container.sh" plant /lane/scripts/plant-inner.sh > "$base/logs/plant.log" 2>&1
printf 's270b CHAIN2 plant exit %s %s\n' "$?" "$(date -u +%FT%TZ)"
bash "$base/scripts/container.sh" probe-f /lane/scripts/py-inner.sh probe.py probe-f-head.jsonl probes/f > "$base/logs/probe-f.log" 2>&1
printf 's270b CHAIN2 probe-f exit %s %s\n' "$?" "$(date -u +%FT%TZ)"
want=$(cat "$base/records/gauntlet-head.txt")
git -C "$base/wolf-lang" fetch --quiet origin s270b
git -C "$base/wolf-lang" checkout --quiet -B s270b FETCH_HEAD
git -C "$base/wolf-lang" reset --quiet --hard FETCH_HEAD
git -C "$base/wolf-lang" clean -fdq -e target
have=$(git -C "$base/wolf-lang" rev-parse HEAD)
printf 's270b CHAIN2 clone at %s (want %s) status:\n' "$have" "$want"
git -C "$base/wolf-lang" status --short
if [ "$have" != "$want" ]; then
  echo "s270b CHAIN2 head mismatch: no gauntlet"; echo 9 > "$base/records/chain2.exit"; exit 9
fi
rm -f "$base/records/gauntlet.pid" "$base/records/gauntlet.exit"
bash "$base/scripts/gauntlet-job.sh" > "$base/logs/gauntlet.log" 2>&1
rc=$?
printf 's270b CHAIN2 gauntlet exit %s %s\n' "$rc" "$(date -u +%FT%TZ)"
echo "$rc" > "$base/records/chain2.exit"
