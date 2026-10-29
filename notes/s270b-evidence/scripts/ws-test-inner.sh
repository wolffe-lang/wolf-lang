#!/usr/bin/env bash
# s270b: every test binary once, no fail-fast, nothing written by insta — the list of every red
set -uo pipefail
source /lane/scripts/gauntlet-env.sh
export INSTA_UPDATE=no
cd /lane/wolf-lang
printf 's270b WS-TEST START %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git status --short
cargo test --workspace --no-fail-fast > /lane/logs/ws-test.log 2>&1
rc=$?
printf 'WS_TEST_EXIT %s\n' "$rc"
grep -E '^error: test failed|^     Running|test result: FAILED|^test .* FAILED$' /lane/logs/ws-test.log | grep -B1 -E 'FAILED|error: test failed' | head -80
printf 's270b WS-TEST END %s\n' "$(date -u +%FT%TZ)"
exit "$rc"
