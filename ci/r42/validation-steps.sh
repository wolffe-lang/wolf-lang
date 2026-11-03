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
indent() { "$@" 2>&1 | sed 's/^/  /'; return "${PIPESTATUS[0]}"; }
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
case "$mode" in
    validation-prep)
        gauntlet_step build indent cargo build --locked -p wolf_driver -p wolf_rt || exit $?
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
    validation-debug)
        envfile="/lane/records/wolf-validation-prep-$head/tool-inputs.env"
        [[ -f "$envfile" ]] || exit 2
        set -a; source "$envfile"; set +a
        gauntlet_step debug-xtask-ci indent cargo xtask ci || exit $?
        ;;
    validation-std-tests|validation-std-docs)
        gauntlet_step build indent cargo build --locked -p wolf_driver -p wolf_rt || exit $?
        std=/lane/builds/wolf-std-5b3a59942b2ee69b99682544341ce8a12c984e9e
        std_identity() {
            [[ "$(git -C "$std" rev-parse HEAD)" = 5b3a59942b2ee69b99682544341ce8a12c984e9e ]] &&
                git -C "$std" diff --exit-code HEAD
        }
        gauntlet_step fixed-std indent std_identity || exit $?
        export WOLF_BIN="$root/target/debug/wolf" LUPIN_BIN="$lupin"
        command=std-test
        [[ "$mode" = validation-std-docs ]] && command=doc-examples
        std_gate() { (cd "$std" && cargo xtask "$command"); }
        gauntlet_step "$command" indent std_gate || exit $?
        ;;
    validation-differ-compiler-checked|validation-differ-compiler-native|validation-differ-compiler-release)
        gauntlet_step build indent cargo build --locked -p wolf_driver -p wolf_rt || exit $?
        export R42_DIFF_A="$root/target/debug/wolf" R42_DIFF_B="$control"
        export R42_DIFF_TIER="${mode##*-}" R42_DIFF_LOG="$records/protocol.jsonl"
        a="/usr/bin/python3 -I $root/ci/r42/compiler-proxy.py A"
        b="/usr/bin/python3 -I $root/ci/r42/compiler-proxy.py B"
        gauntlet_step "compiler-$R42_DIFF_TIER-differ" indent cargo xtask differ "$a" "$b" --triage || exit $?
        ;;
    validation-differ-interp-checked|validation-differ-interp-native)
        gauntlet_step build indent cargo build --locked -p wolf_driver -p wolf_rt || exit $?
        tier="${mode##*-}"
        gauntlet_step "interpreter-$tier-control" indent cargo xtask differ "$root/target/debug/wolf" "$lupin" --triage "--$tier" --control "$previous" || exit $?
        ;;
    *) exit 2 ;;
esac
gauntlet_step clean indent git diff --exit-code HEAD || exit $?
