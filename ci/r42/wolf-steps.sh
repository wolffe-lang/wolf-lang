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
elif [[ "$mode" = pairing-red-restore || "$mode" = green || "$mode" = postrebase ]]; then
    export WOLF_PAIRING_REQUIRE_SIBLING=1
    if [[ "$mode" = postrebase ]]; then
        step postrebase-metadata python3 -I ci/r42/release-posture.py || exit $?
        step postrebase-userland cat /etc/os-release || exit $?
        step postrebase-build cargo build --locked -p wolf_driver -p wolf_rt || exit $?
        export WOLF_RT_LIB="$PWD/target/debug/libwolf_rt.a"
    fi
    step focused-format cargo fmt --all --check || exit $?
    step compile-focused-tests cargo test -p wolf_driver --test os_surface_lanes --test descriptor_walk_lanes --test editor_calls_lanes --no-run || exit $?
    for version in 0.1.58 0.1.59; do
        export LUPIN
        LUPIN=$(find "$inputs/lupin-$version" -type f -name lupin -print -quit)
        [[ "$LUPIN" = /* && -x "$LUPIN" ]] || exit 2
        for gate in os_surface_lanes descriptor_walk_lanes; do
            step "focused-$version-$gate" cargo test -p wolf_driver --test "$gate" published_pairing_ -- --nocapture || exit $?
        done
        if [[ "$mode" = pairing-red-restore && "$version" = 0.1.59 ]]; then
            name="red-$version-editor-restore"
            step "$name" cargo test -p wolf_driver --test editor_calls_lanes published_pairing_ -- --nocapture
            rc=$?
            printf '%s\n' "$rc" > "$records/$name.exit"
            if ((rc == 0)) || ! grep -q 'test result: FAILED. 0 passed; 1 failed;' "$records/$name.log"; then
                echo 'FAIL expected the named published restore regression'
                exit 1
            fi
            echo 'END restore-red-sensitivity exit=0'
        else
            step "focused-$version-editor-restore" cargo test -p wolf_driver --test editor_calls_lanes published_pairing_ -- --nocapture || exit $?
        fi
    done
    if [[ "$mode" = postrebase ]]; then
        export LUPIN WOLF_TEST_NATIVE_REQUIRE=1
        LUPIN=$(find "$inputs/lupin-0.1.59" -type f -name lupin -print -quit)
        step postrebase-host-corpus-expectations cargo test --locked -p wolf_driver --test os_surface_lanes -- --nocapture --test-threads=1 the_root_rows_on_every_host the_signal_action_rows_on_every_host || exit $?
        step postrebase-clean git diff --exit-code HEAD || exit $?
    fi
elif [[ "$mode" = plant ]]; then
    export WOLF_PAIRING_REQUIRE_SIBLING=1 LUPIN
    LUPIN=$(find "$inputs/lupin-0.1.58" -type f -name lupin -print -quit)
    [[ "$LUPIN" = /* && -x "$LUPIN" ]] || exit 2
    step compile-plant-tests cargo test -p wolf_driver --test os_surface_lanes --test descriptor_walk_lanes --no-run || exit $?
    for gate in os_surface_lanes descriptor_walk_lanes; do
        name="plant-$gate"
        step "$name" cargo test -p wolf_driver --test "$gate" published_pairing_keeps -- --nocapture
        rc=$?
        printf '%s\n' "$rc" > "$records/$name.exit"
        if ((rc == 0)) || ! grep -q 'test result: FAILED. 0 passed; 1 failed;' "$records/$name.log"; then
            echo "FAIL expected named plant red in $gate"
            exit 1
        fi
    done
    echo 'END plant-sensitivity exit=0'
else
    exit 2
fi
