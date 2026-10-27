#!/bin/sh
# s268/yw08 witnesses. WOLF and OUT are explicit; never reuse cached objects
# across development compilers that share the +dev.unknown stamp.
set -eu
: "${WOLF:?absolute path to the compiler under measurement}"
: "${OUT:?directory for this measurement's binaries}"
root=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
mkdir -p "$OUT"
"$WOLF" --version
for tier in native release; do
    for bench in dense-match token-leaf; do
        if [ "$tier" = release ]; then
            "$WOLF" build --no-cache --release "$root/$bench/main.lu" -o "$OUT/$bench-$tier"
        else
            "$WOLF" build --no-cache "$root/$bench/main.lu" -o "$OUT/$bench-$tier"
        fi
        for run in 1 2 3; do
            printf 'tier=%s bench=%s run=%s\n' "$tier" "$bench" "$run"
            "$OUT/$bench-$tier"
        done
    done
done
