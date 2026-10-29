#!/usr/bin/env bash
# s270b: the diag-catalog step alone, at the checked-out head
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/wolf-lang
printf 's270b DIAG-CHECK %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git status --short
cargo xtask diag-catalog --check
printf 'DIAG_CHECK_EXIT %s\n' "$?"
