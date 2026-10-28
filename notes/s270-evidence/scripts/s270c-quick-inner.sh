set -uo pipefail
source /lane/scripts/s270c-gauntlet-env.sh
cd /lane/wolf-lang
git rev-parse --short=8 HEAD
for g in "corpus" "fmt-lu" "conformance" "spec-extract --check" "diag-catalog --check"; do
 cargo xtask $g > "/lane/scratch/s270c-quick-${g%% *}.log" 2>&1
 rc=$?
 printf 'XTASK %s EXIT %s\n' "$g" "$rc"
 if test "$rc" != 0; then tail -n 25 "/lane/scratch/s270c-quick-${g%% *}.log"; fi
done
grep -iE 'mut_relay_push|mut_param_replaced|enum_str_bare|mixed_payload' /lane/scratch/s270c-quick-*.log | head -20
tail -n 3 /lane/scratch/s270c-quick-corpus.log
