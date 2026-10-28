set -uo pipefail
source /lane/scripts/s270-env.sh
cd /lane/wolf-lang
git apply --check /lane/scratch/s270c-patches/940-fix.patch || exit 3
git apply /lane/scratch/s270c-patches/940-fix.patch
grep -c 'while close_writes(&mut locals, &order, true) {}' crates/wolf_wir/src/midend/interproc.rs
cp crates/wolf_wir/src/midend/interproc.rs /lane/scratch/s270c-patches/interproc.fixed.rs
sed -i 's/^    while close_writes(&mut locals, &order, true) {}$/    \/\/ s270 check: widening removed/' crates/wolf_wir/src/midend/interproc.rs
grep -n 's270 check: widening removed' crates/wolf_wir/src/midend/interproc.rs
printf '== fix WITHOUT the widening\n'
cargo test --locked -p wolf_wir --test interproc 2>&1 | grep -E '^test |test result|left:|right:'
printf 'NO_WIDEN_EXIT %s\n' "${PIPESTATUS[0]}"
cp /lane/scratch/s270c-patches/interproc.fixed.rs crates/wolf_wir/src/midend/interproc.rs
git diff --stat
printf '== whole fix\n'
bash /lane/scripts/s270c-tests-inner.sh green
