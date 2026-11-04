#!/usr/bin/env bash
# Each invocation is one independently recorded, deadline-bounded take.
set -uo pipefail
mode=${1:?}; head=${2:?}; records=${3:?}
source "${R42_RECORDER:?org recorder copy required}"
[[ "$(git rev-parse HEAD)" = "$head" ]] || exit 2
root=$PWD
inputs=/lane/inputs/wolf-controls-20261103
lupin=$(find "$inputs/lupin-0.1.59" -type f -name lupin -print -quit)
previous=$(find "$inputs/lupin-0.1.58" -type f -name lupin -print -quit)
control=$(find "$inputs/wolf-0.2.36" -type f -name wolf -print -quit)
export LUPIN="$lupin" WOLF_PAIRING_REQUIRE_SIBLING=1
export WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1
export WOLF_SYSCALL_REQUIRE_STRACE=1 RUST_TEST_NOCAPTURE=1
export WOLF_RT_LIB="$root/target/debug/libwolf_rt.a"
export WOLF_STD=/lane/builds/wolf-std-5b3a59942b2ee69b99682544341ce8a12c984e9e/std
export LUPIN_STD="$WOLF_STD"
indent() { "$@" 2>&1 | sed 's/^/  /'; return "${PIPESTATUS[0]}"; }
build_current() {
    cargo build --locked -p wolf_driver -p wolf_rt || return $?
    python3 -I ci/r42/assert-build-stamp.py "$root/target/debug/wolf" "$head"
}
identity() {
    git rev-parse HEAD || return $?
    git status --porcelain --untracked-files=no || return $?
    rustc -V && cargo -V && clang --version && strace --version || return $?
    cat /etc/os-release || return $?
    grep -E 'Cpus_allowed_list|SigBlk|SigIgn|CapEff' /proc/self/status || return $?
    printf 'PID1 uid=%s workload uid=%s\n' "$(awk '/^Uid:/{print $2}' /proc/1/status)" "$(id -u)"
    [[ "$(awk '/^Uid:/{print $2}' /proc/1/status)" != "$(id -u)" ]]
}
gauntlet_step identity indent identity || exit $?
gauntlet_step metadata indent python3 -I ci/r42/release-posture.py || exit $?
gauntlet_step proxy-tests indent python3 -I ci/r42/compiler-proxy-test.py || exit $?
gauntlet_step pruner-tests indent python3 -I ci/r42/prune-completed-tests-test.py || exit $?
case "$mode" in
    validation-plant)
        export LUPIN="$previous"
        gauntlet_step compile-plant-test indent cargo test --locked -p wolf_driver --test os_surface_lanes --no-run || exit $?
        plant_case() {
            cargo test --locked -p wolf_driver --test os_surface_lanes published_pairing_keeps_the_os_surface_pre_mirror -- --nocapture > "$records/plant-test.log" 2>&1
            local rc=$?
            sed 's/^/  /' "$records/plant-test.log"
            if ((rc != 0)) && grep -q '^test published_pairing_keeps_the_os_surface_pre_mirror \.\.\. FAILED$' "$records/plant-test.log"; then
                echo 'FAIL published_pairing_keeps_the_os_surface_pre_mirror the deliberate false mirror requirement rejected the published .58 control'
            fi
            return "$rc"
        }
        gauntlet_step pre-mirror-plant plant_case || exit $?
        ;;
    validation-prep)
        gauntlet_step build indent build_current || exit $?
        export GITHUB_ENV="$records/tool-inputs.env"
        cache="/lane/inputs/entry-bundle-$head"
        gauntlet_step entry-bundle indent python3 -I ci/fetch-entry-bundle "$cache" || exit $?
        gauntlet_step musl-runtime indent cargo xtask dist-rt --target x86_64-unknown-linux-musl || exit $?
        export WOLF_STATIC_ENTRY_MUSL_RT="$root/target/dist-rt/x86_64-unknown-linux-musl/release/libwolf_rt.a"
        set -a; source "$GITHUB_ENV"; set +a
        gauntlet_step real-link-inputs indent python3 -I ci/fetch-test-link-bundle "$cache" || exit $?
        gauntlet_step real-clef indent bash ci/build-test-clef || exit $?
        gauntlet_step real-ligature indent bash ci/build-test-ligature || exit $?
        printf 'WOLF_STATIC_ENTRY_MUSL_RT=%s\n' "$WOLF_STATIC_ENTRY_MUSL_RT" >> "$GITHUB_ENV"
        ;;
    validation-debug|validation-release)
        envfile="/lane/records/wolf-validation-prep-$head/tool-inputs.env"
        [[ -f "$envfile" ]] || exit 2
        set -a; source "$envfile"; set +a
        if [[ "$mode" = validation-release ]]; then
            gauntlet_step prune-debug-tests indent python3 -I ci/r42/prune-completed-tests.py "$root" debug "/lane/records/wolf-validation-debug-$head/local-record.log" "/lane/records/wolf-validation-debug-$head.exit" "$records/pruned-debug.txt" || exit $?
            release_shim() {
                mkdir -p /lane/scripts/release-cargo &&
                    cp ci/r42/release-cargo/cargo /lane/scripts/release-cargo/cargo &&
                    chmod 755 /lane/scripts/release-cargo/cargo &&
                    cmp ci/r42/release-cargo/cargo /lane/scripts/release-cargo/cargo
            }
            gauntlet_step release-shim indent release_shim || exit $?
            export PATH=/lane/scripts/release-cargo:$PATH
        fi
        gauntlet_step compiler-build-stamp indent python3 -I ci/r42/assert-build-stamp.py "$root/target/debug/wolf" "$head" || exit $?
        gauntlet_step "${mode#validation-}-xtask-ci" indent cargo xtask ci || exit $?
        gauntlet_step compiler-build-stamp-after-ci indent python3 -I ci/r42/assert-build-stamp.py "$root/target/debug/wolf" "$head" || exit $?
        ;;
    validation-constrained)
        gauntlet_step prune-release-tests indent python3 -I ci/r42/prune-completed-tests.py "$root" release "/lane/records/wolf-validation-release-$head/local-record.log" "/lane/records/wolf-validation-release-$head.exit" "$records/pruned-release.txt" || exit $?
        gauntlet_step constrained-runtime indent taskset -c 0-3 cargo test --locked -p wolf_rt -- --nocapture || exit $?
        ;;
    validation-startup)
        gauntlet_step build indent build_current || exit $?
        gauntlet_step startup-bench-gates indent cargo xtask bench-gates || exit $?
        ;;
    validation-std-tests|validation-std-docs|validation-std-published-tests|validation-std-published-docs)
        gauntlet_step build indent build_current || exit $?
        std=/lane/builds/wolf-std-5b3a59942b2ee69b99682544341ce8a12c984e9e
        std_identity() {
            [[ "$(git -C "$std" rev-parse HEAD)" = 5b3a59942b2ee69b99682544341ce8a12c984e9e ]] &&
                git -C "$std" diff --exit-code HEAD
        }
        gauntlet_step fixed-std indent std_identity || exit $?
        export R42_DIFF_A="$root/target/debug/wolf" R42_DIFF_B="$lupin"
        if [[ "$mode" = validation-std-published-* ]]; then
            export R42_DIFF_A="$control" R42_DIFF_B="$previous"
            export WOLF_RT_LIB="$(dirname "$control")/libwolf_rt.a"
        fi
        export R42_DIFF_TIER=passthrough R42_DIFF_LOG="$records/protocol.jsonl"
        mkdir -p "$records/bin"
        for role in A B; do
            printf '#!/usr/bin/env bash\nexec /usr/bin/python3 -I %q %q "$@"\n' "$root/ci/r42/compiler-proxy.py" "$role" > "$records/bin/proxy-$role"
            chmod 755 "$records/bin/proxy-$role"
        done
        export WOLF_BIN="$records/bin/proxy-A" LUPIN_BIN="$records/bin/proxy-B"
        command=std-test
        [[ "$mode" = *-docs ]] && command=doc-examples
        std_gate() {
            (cd "$std" && cargo xtask "$command")
            local rc=$?
            if [[ -f "$std/target/std-test-divergences.jsonl" ]]; then
                cp "$std/target/std-test-divergences.jsonl" "$records/std-test-divergences.jsonl" || return $?
            fi
            return "$rc"
        }
        gauntlet_step "$command" indent std_gate || exit $?
        ;;
    validation-differ-compiler-checked|validation-differ-compiler-native|validation-differ-compiler-release)
        gauntlet_step build indent build_current || exit $?
        export R42_DIFF_A="$root/target/debug/wolf" R42_DIFF_B="$control"
        export R42_DIFF_RT_A="$root/target/debug/libwolf_rt.a"
        export R42_DIFF_RT_B="$(dirname "$control")/libwolf_rt.a"
        export R42_DIFF_TIER="${mode##*-}" R42_DIFF_LOG="$records/protocol.jsonl"
        a="/usr/bin/python3 -I $root/ci/r42/compiler-proxy.py A"
        b="/usr/bin/python3 -I $root/ci/r42/compiler-proxy.py B"
        gauntlet_step "compiler-$R42_DIFF_TIER-differ" indent cargo xtask differ "$a" "$b" --triage || exit $?
        ;;
    validation-differ-interp-checked|validation-differ-interp-native)
        gauntlet_step build indent build_current || exit $?
        tier="${mode##*-}"
        gauntlet_step "interpreter-$tier-control" indent cargo xtask differ "$root/target/debug/wolf" "$lupin" --triage "--$tier" --control "$previous" || exit $?
        ;;
    *) exit 2 ;;
esac
gauntlet_step clean indent git diff --exit-code HEAD || exit $?
