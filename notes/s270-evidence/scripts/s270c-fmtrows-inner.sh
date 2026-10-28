set -uo pipefail
source /lane/scripts/s270c-gauntlet-env.sh
cd /lane/wolf-lang
for f in corpus/rows/mixed_payload_display.lu corpus/typecheck/enum_str_bare_choice.lu; do
 target/debug/wolf fmt "$f"
 printf 'FMT %s EXIT %s\n' "$f" "$?"
done
git diff > /lane/scratch/s270c-patches/fmt-rows.patch
cat /lane/scratch/s270c-patches/fmt-rows.patch
cargo xtask fmt-lu > /lane/scratch/s270c-quick-fmt-lu2.log 2>&1
printf 'XTASK fmt-lu EXIT %s\n' "$?"
tail -n 3 /lane/scratch/s270c-quick-fmt-lu2.log
cargo test --locked -q -p wolf_driver --test four_machine_regressions 2>&1 | grep -E 'test result|FAILED'
printf 'FOUR_MACHINE_EXIT %s\n' "${PIPESTATUS[0]}"
git checkout -- corpus/rows/mixed_payload_display.lu corpus/typecheck/enum_str_bare_choice.lu
git status --short
