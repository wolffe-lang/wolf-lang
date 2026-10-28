#!/usr/bin/env bash
set -euo pipefail
source /lane/scripts/env.sh
cd /lane/wolf-lang
printf 's270b BUILD %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
rustc --version
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
printf 'pid1 uid '; awk '/^Uid:/{print $2}' /proc/1/status
printf 'test uid '; id -u
cargo build --locked -p wolf_driver -p wolf_rt
ls -la target/debug/wolf
printf 's270b BUILD DONE %s\n' "$(date -u +%FT%TZ)"
