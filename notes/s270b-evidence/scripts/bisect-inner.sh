#!/usr/bin/env bash
# git bisect between v0.2.35 (good) and trunk 5adf3d91 (bad) for the witness's trap(bounds)
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/bisect
git bisect reset > /dev/null 2>&1 || true
git bisect start 5adf3d91f75b3545ff8d38e5b6693b0b9f367a05 cf0c13d38479f444b69bb8356ad5709d07eff323
git bisect run bash /lane/scripts/bisect-step.sh
git bisect log
git bisect reset
