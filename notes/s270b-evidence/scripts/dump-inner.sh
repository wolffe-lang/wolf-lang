#!/usr/bin/env bash
# dump one probe's region inference and mem record: dump.sh <file-under-/lane> <kind>
set -uo pipefail
source /lane/scripts/env.sh
/lane/wolf-lang/target/debug/wolf conform-run --json --phase=mem "--dump=$2" "/lane/$1"
echo "rc=$?"
