set -uo pipefail
source /lane/scripts/s270-env.sh
export WOLF_STD=/lane/inputs/wolf/wolf-0.2.35-x86_64-unknown-linux-gnu/std
cd /lane/wolf-lang
printf 's270c BISECT %s head %s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
test -z "$(git status --porcelain)" || { echo "DIRTY CLONE"; git status --short; exit 3; }
python3 -I /lane/scripts/s270c-knob.py /lane/wolf-lang || exit 3
cargo build --locked -q -p wolf_driver -p wolf_rt
rc=$?
printf 'KNOB_BUILD_EXIT %s\n' "$rc"
mkdir -p /lane/scratch/s270c-bisect
cp target/debug/wolf /lane/scratch/s270c-bisect/wolf
git checkout -- crates/wolf_wir/src/midend/mod.rs crates/wolf_wir/src/midend/wp.rs
git status --short
test "$rc" = 0 || exit 3
wolf=/lane/scratch/s270c-bisect/wolf
probe() {
 cd /lane/witnesses/pack-rec
 WOLF_MIDEND_SKIP="$1" "$wolf" build --no-cache --release src/decoder_probe.lu -o /lane/scratch/s270c-bisect/probe > /lane/scratch/s270c-bisect/probe.log 2>&1 || { echo "probe BUILD FAIL"; tail -n 5 /lane/scratch/s270c-bisect/probe.log; return; }
 /lane/scratch/s270c-bisect/probe | grep -E '^(space|down):' | tr '\n' ' '
 echo
}
row() {
 cd /lane/scratch/s270c-red
 WOLF_MIDEND_SKIP="$1" "$wolf" build --no-cache --release v6/main.lu -o /lane/scratch/s270c-bisect/v6 > /lane/scratch/s270c-bisect/v6.log 2>&1 || { echo "v6 BUILD FAIL"; tail -n 5 /lane/scratch/s270c-bisect/v6.log; return; }
 /lane/scratch/s270c-bisect/v6 | tr '\n' ' '
 echo
}
for skip in none simplify inline memopt rangeopt sink coalesce licm hdrprom loopcse dedup interproc crossinline; do
 printf 'SKIP %-12s probe: ' "$skip"; probe "$skip"
 printf 'SKIP %-12s v6:    ' "$skip"; row "$skip"
done
printf 's270c BISECT DONE %s\n' "$(date -u +%FT%TZ)"
