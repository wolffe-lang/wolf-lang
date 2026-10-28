#!/usr/bin/env bash
# one git-bisect step in /lane/bisect: is yew's untouched witness at (1024 bytes, 2 elements)
# exit(0) on the native tier (good, 0) or trap(bounds) (bad, 1)? Anything else skips (125).
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/bisect
head=$(git rev-parse --short=10 HEAD)
if ! cargo build --locked -q -p wolf_driver -p wolf_rt > /lane/scratch/tmp/bisect-build.log 2>&1; then
  echo "BISECT $head build-failed: skip"; exit 125
fi
export WOLF_STD=/lane/inputs/wolf035/wolf-0.2.35-x86_64-unknown-linux-gnu/std
w=/lane/witnesses/yew-bisect
sed -e 's/100000/1024/g' -e 's/99999/1023/g' -e 's/0\.\.64 +/0..2 +/' /lane/yew/notes/yw07b-evidence/stdlib/scratch-once/main.lu > "$w/main.lu"
v=$(/lane/bisect/target/debug/wolf conform-run --json --native "$w/main.lu" 2>/dev/null | python3 -I -c 'import json,sys; print(json.loads(sys.stdin.read().strip().splitlines()[-1])["verdict"])' 2>/dev/null)
echo "BISECT $head verdict=$v"
echo "$head $v" >> /lane/records/bisect-steps.txt
case "$v" in
  "exit(0)") exit 0;;
  "trap(bounds)") exit 1;;
  *) exit 125;;
esac
