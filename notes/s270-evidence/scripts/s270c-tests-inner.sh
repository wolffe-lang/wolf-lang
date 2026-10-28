set -uo pipefail
source /lane/scripts/s270-env.sh
export WOLF_PAIRING_REQUIRE_SIBLING=1
cd /lane/wolf-lang
stage=${1:?stage}
printf 's270c TESTS stage=%s %s head=%s\n' "$stage" "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git status --short
cargo test --locked -p wolf_wir --test interproc 2>&1 | grep -E '^test |test result|panicked|left:|right:|assert'
printf 'WIR_INTERPROC_EXIT %s\n' "${PIPESTATUS[0]}"
cargo test --locked -p wolf_driver --test four_machine_regressions -- --nocapture 2>&1 | grep -E '^test |test result|panicked|SKIP|stdout_inline|lupin|release' | cut -c1-400
printf 'FOUR_MACHINE_EXIT %s\n' "${PIPESTATUS[0]}"
printf 's270c TESTS DONE %s\n' "$(date -u +%FT%TZ)"
