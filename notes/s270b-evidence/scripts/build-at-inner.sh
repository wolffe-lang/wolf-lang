#!/usr/bin/env bash
# build wolf_driver + wolf_rt in a checkout under /lane: build-at-inner.sh <dir-under-/lane>
set -euo pipefail
source /lane/scripts/env.sh
cd "/lane/$1"
printf 's270b BUILD %s %s head=%s\n' "$1" "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
cargo build --locked -p wolf_driver -p wolf_rt 2>&1 | tail -3
ls -la target/debug/wolf
printf 's270b BUILD DONE %s\n' "$(date -u +%FT%TZ)"
