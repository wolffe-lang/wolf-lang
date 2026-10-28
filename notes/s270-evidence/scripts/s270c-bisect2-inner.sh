set -uo pipefail
source /lane/scripts/s270-env.sh
export WOLF_STD=/lane/inputs/wolf/wolf-0.2.35-x86_64-unknown-linux-gnu/std
cp /lane/wolf-lang/target/debug/libwolf_rt.a /lane/scratch/s270c-bisect/libwolf_rt.a
wolf=/lane/scratch/s270c-bisect/wolf
grep -c WOLF_MIDEND_SKIP "$wolf"
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
