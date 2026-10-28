#!/usr/bin/env bash
# print the rendered report `wolf build` gives one file: report-inner.sh <file-under-/lane>
set -uo pipefail
source /lane/scripts/env.sh
mkdir -p /lane/scratch/tmp/report
/lane/wolf-lang/target/debug/wolf build "/lane/$1" -o /lane/scratch/tmp/report/out --no-cache
echo "rc=$?"
