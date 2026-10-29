#!/usr/bin/env bash
# s270b: regenerate the diagnostics catalogue and show what moved
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/wolf-lang
printf 's270b DIAG %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git status --short
cargo xtask diag-catalog > /lane/logs/diag-catalog-write.log 2>&1
printf 'DIAG write rc=%s\n' "$?"
tail -3 /lane/logs/diag-catalog-write.log
git status --short
git diff --stat
git diff
cargo xtask diag-catalog --check > /lane/logs/diag-catalog-check.log 2>&1
printf 'DIAG check rc=%s\n' "$?"
tail -3 /lane/logs/diag-catalog-check.log
