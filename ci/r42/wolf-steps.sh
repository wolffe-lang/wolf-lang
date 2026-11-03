#!/usr/bin/env bash
set -uo pipefail
mode=${1:?}; head=${2:?}; records=${3:?}
[[ "$(git rev-parse HEAD)" = "$head" ]] || exit 2
step() {
    name=$1; shift
    echo "BEGIN $name $(date -u +%FT%TZ)"
    "$@" > "$records/$name.log" 2>&1
    rc=$?
    cat "$records/$name.log"
    echo "END $name exit=$rc"
    return "$rc"
}
inputs=/lane/inputs/wolf-controls-20261103
if [[ "$mode" = observe ]]; then
    step published-observations python3 -I ci/r42/observe-published.py "$PWD" "$inputs" "$records/published" || exit $?
elif [[ "$mode" = red ]]; then
    export LUPIN
    LUPIN=$(find "$inputs/lupin-0.1.59" -type f -name lupin -print -quit)
    [[ "$LUPIN" = /* && -x "$LUPIN" ]] || exit 2
    export WOLF_PAIRING_REQUIRE_SIBLING=1
    # A compile/setup failure must not count as the named regression's red.
    step compile-red-tests cargo test -p wolf_driver --test os_surface_lanes --test descriptor_walk_lanes --no-run || exit $?
    failed=0
    for gate in os_surface_lanes descriptor_walk_lanes; do
        step "red-$gate" cargo test -p wolf_driver --test "$gate" published_pairing_keeps -- --nocapture
        rc=$?
        printf '%s\n' "$rc" > "$records/red-$gate.exit"
        if ((rc == 0)); then
            echo "FAIL expected named red in $gate"
            failed=1
        elif ! grep -q 'test result: FAILED. 0 passed; 1 failed;' "$records/red-$gate.log"; then
            echo "FAIL not the expected one-test red in $gate"
            failed=1
        fi
    done
    echo "END red-sensitivity exit=$failed"
    exit "$failed"
else
    exit 2
fi
