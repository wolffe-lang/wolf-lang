#!/usr/bin/env bash
# s284 job "forklanes": fork_lanes alone at <sha>, a fresh worktree f-<sha> and
# target, in r41-toolchain:1 with the take's engine (rig/engine.sh through the
# host's podman service): every fork witness in its own throwaway container,
# as CI runs them, WOLF_FORK_REQUIRE_CONTAINER=1. A trial before the gauntlet,
# and the new churn row's timing. Steps print END <step> exit=N. The target is
# removed after the take.
# usage: forklanes.sh <A|B> <sha>
set -uo pipefail
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
sha=${2:?sha}
case $blk in A) SIX=0-5 ;; B) SIX=6-11 ;; *) echo "block must be A or B"; exit 2 ;; esac
W=$L/f-$sha
export PATH=$HOME/lanes/H1-env/bin:$HOME/lanes/H1-env/profile/bin:$PATH
source "$L/rig/engine-lib.sh"
echo "LOCKS_ACQUIRED $(date -u +%FT%TZ) block=$blk six=$SIX sha=$sha kernel=$(uname -r)"
[ -d "$W" ] || git -C "$L/wolf-lang" worktree add -q --detach "$W" "$sha" || { echo "REFUSED worktree"; exit 2; }
test "$(git -C "$W" rev-parse HEAD)" = "$sha" || { echo "REFUSED head"; exit 2; }
if [ -e "$W/target" ]; then echo "REFUSED: $W/target exists, not a fresh target for this head"; exit 2; fi
trap engine_down EXIT
engine_up "forklanes-${sha:0:8}" "$SIX" || exit 1
step() {
    name=$1
    shift
    echo "START $name $(date -u +%FT%TZ)"
    "$@" 2>&1 | sed 's/^/  /'
    rc=${PIPESTATUS[0]}
    echo "END $name exit=$rc $(date -u +%FT%TZ)"
    return "$rc"
}
run() {
    podman run --rm --userns=keep-id -u "$(id -u):$(id -g)" --memory 12g --pids-limit 16384 \
        --ulimit nproc=32768:32768 --ulimit core=0 -v "$L:$L" "${ENGINE_FLAGS[@]}" -w "$W" \
        -e HOME="$L/scratch/home" -e TMPDIR="$L/tmp" -e CARGO_HOME="$L/cargo" \
        -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=6 -e CARGO_TERM_COLOR=never -e RUST_TEST_THREADS=4 \
        -e RUSTUP_HOME=/opt/s263-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
        -e PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
        localhost/r41-toolchain:1 taskset.real -c "$SIX" bash -c '"$@"; exit $?' run "$@"
}
# The driver links the libwolf_rt.a beside target/debug/wolf; `cargo test` alone
# does not build that archive (a first take skipped every build for want of it).
step fork-lanes-build run bash -c 'cargo build --locked -p wolf_driver -p wolf_rt && cargo test --locked -p wolf_driver --test fork_lanes --no-run' || exit 3
step fork-lanes run bash -c 'WOLF_TEST_NATIVE_REQUIRE=1 cargo test --locked --offline -p wolf_driver --test fork_lanes -- --nocapture > "$TMPDIR/forklanes-'"${sha:0:8}"'.out" 2>&1; rc=$?; cat "$TMPDIR/forklanes-'"${sha:0:8}"'.out"; if grep -q "SKIP: environment cannot build" "$TMPDIR/forklanes-'"${sha:0:8}"'.out"; then echo "a tier did not build: not a pass"; exit 1; fi; exit $rc'
rc=$?
rm -rf "$W/target" && git -C "$L/wolf-lang" worktree remove "$W" && echo "PRUNED $W"
echo "FORKLANES_END $sha rc=$rc $(date -u +%FT%TZ)"
exit "$rc"
