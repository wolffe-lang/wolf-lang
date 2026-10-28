set -uo pipefail
source /lane/scripts/s270-env.sh
export WOLF_PAIRING_REQUIRE_SIBLING=1
cd /lane/wolf-lang
test -z "$(git status --porcelain)" || { echo "clone not clean"; git status --short; exit 3; }
for b in 940-relay 950-payload 735-zero; do
 git checkout -q --detach "origin/s270-plant-$b"
 printf '\n== PLANT s270-plant-%s at %s (%s)\n' "$b" "$(git rev-parse --short=8 HEAD)" "$(date -u +%FT%TZ)"
 git log --oneline -1
 cargo test --locked -p wolf_wir --test interproc 2>&1 | grep -E '^test .*(FAILED|ok)$|test result|left:|right:'
 printf 'PLANT %s WIR_INTERPROC_EXIT %s\n' "$b" "${PIPESTATUS[0]}"
 cargo test --locked -p wolf_driver --test four_machine_regressions 2>&1 | grep -E '^test .*(FAILED|ok)$|test result|panicked at|assertion|stdout_inline|cannot compile|ICE' | cut -c1-300
 printf 'PLANT %s FOUR_MACHINE_EXIT %s\n' "$b" "${PIPESTATUS[0]}"
done
git checkout -q s270
git status -sb | head -1
git rev-parse --short=8 HEAD
cargo build --locked -q -p wolf_driver -p wolf_rt
printf 'HEAD_REBUILD_EXIT %s\n' "$?"
