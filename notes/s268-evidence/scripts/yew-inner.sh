set -euo pipefail
lane=/home/mfwolffe/lanes/s268
mkdir -p "$lane/yew-input" "$lane/yew-control" "$lane/yew-inline" "$lane/evidence/yew"
tar -xf "$lane/yew-input.tar" -C "$lane/yew-input"
for variant in control inline; do
    cp -a "$lane/yew-input/src/." "$lane/yew-$variant/"
    find "$lane/yew-$variant" -maxdepth 1 -type f -name '*.lu' -delete
    find "$lane/yew-$variant" -type f -name '*_test.lu' -delete
    cp "$lane/yew-input/tools/fl-headless/main.lu" "$lane/yew-$variant/main.lu"
done
python3 -I "$lane/scripts/inline-exec.py" "$lane/yew-control/fl/vm.lu" "$lane/yew-inline/fl/vm.lu"
export WOLF_RT_LIB="$lane/baseline/libwolf_rt.a"
export YEW_RUNTIME_DIR="$lane/yew-input/runtime"
for variant in control inline; do
    "$lane/baseline/wolf" build --release --no-cache --codegen-report "$lane/yew-$variant/main.lu" -o "$lane/yew-$variant/run" > "$lane/evidence/yew/$variant-codegen.txt" 2>&1
    "$lane/baseline/wolf" build --release --no-cache --emit=llvm-ir "$lane/yew-$variant/main.lu" -o "$lane/evidence/yew/$variant.ll"
    objdump -d "$lane/yew-$variant/run" > "$lane/evidence/yew/$variant.asm"
    nm -S --size-sort "$lane/yew-$variant/run" > "$lane/evidence/yew/$variant-symbols.txt"
done
for run in 1 2 3; do
    for variant in control inline; do
        echo "variant=$variant run=$run"
        "$lane/yew-$variant/run" fib27
    done
done
for variant in control inline; do
    set +e
    perf stat -e cycles,instructions,branches,branch-misses,cache-references,cache-misses "$lane/yew-$variant/run" fib27 > "$lane/evidence/yew/$variant-perf.txt" 2>&1
    status=$?
    set -e
    echo "perf variant=$variant status=$status"
    cat "$lane/evidence/yew/$variant-perf.txt"
done
