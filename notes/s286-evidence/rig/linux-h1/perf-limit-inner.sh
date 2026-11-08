#!/usr/bin/env bash
# s286, inside the perf container as uid 1000 (perf-take3.sh), on one block:
# worker_spawn_median read faster after the settle by more than its margin
# (perf2, 9 runs; perf3, 30). Three variants interleaved, 30 rounds: the base
# runtime as it is, the base runtime with the limit set by the user's own
# variable (MALLOC_ARENA_MAX=48, 8 per CPU of this six-CPU block: glibc's own
# policy with a limit set), and the settled runtime. If the base with the
# variable reads like the settled runtime, the gain is glibc's behaviour with
# a nonzero limit.
set -uo pipefail
export PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export CARGO_HOME=/lane/cargo-home RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6 HOME=/lane/home XDG_CACHE_HOME=/lane/home/.cache
unset CARGO_TARGET_DIR MALLOC_ARENA_MAX MALLOC_ARENA_TEST GLIBC_TUNABLES
b=${S286_BEFORE:0:8}; a=${S286_AFTER:0:8}
run() {
  local tag=$1 h=$2 r=$3; shift 3
  cd /w/$h || return 2
  env "$@" cargo test --release --locked -q -p wolf_rt --test spawn_bench 2>&1 | grep '^spawn_bench:' | sed "s/^/p$r $tag /"
}
for r in $(seq 1 30); do
  case $((r % 3)) in
    0) run base "$b" "$r" A=1; run baselimit "$b" "$r" MALLOC_ARENA_MAX=48; run settled "$a" "$r" A=1 ;;
    1) run baselimit "$b" "$r" MALLOC_ARENA_MAX=48; run settled "$a" "$r" A=1; run base "$b" "$r" A=1 ;;
    2) run settled "$a" "$r" A=1; run base "$b" "$r" A=1; run baselimit "$b" "$r" MALLOC_ARENA_MAX=48 ;;
  esac
done
echo "LIMIT_READ done"
