#!/usr/bin/env bash
# s286, inside the perf container as uid 1000, on one block's six CPUs: the
# cost rows at BEFORE (the runtime as trunk has it) and AFTER (the settle).
# Builds both, then six rounds in the order ABBAAB, each round t1's family B
# (list_alloc, b3_churn; 10 runs) and the pool's benches (spawn_bench,
# chan_bench; release, 3 runs each). Every number is kept raw; `bench diff`
# applies the protocol's significance rule (3xMAD and the 2% floor) to each
# round's before/after pair.
set -uo pipefail
export PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export CARGO_HOME=/lane/cargo-home RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export TMPDIR=/lane/tmp/${S286_LABEL:?} HOME=/lane/home XDG_CACHE_HOME=/lane/home/.cache
unset CARGO_TARGET_DIR WOLF_STD LUPIN_STD WOLF_RT_LIB MALLOC_ARENA_MAX MALLOC_ARENA_TEST GLIBC_TUNABLES
out=/lane/records/$S286_LABEL
mkdir -p "$TMPDIR" "$HOME" "$out"
first_rc=0
step() {
  local name=$1; shift
  local started=$SECONDS
  printf 'START %s\n' "$name"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"
  [ "$rc" -ne 0 ] && [ "$first_rc" -eq 0 ] && first_rc=$rc
  return "$rc"
}
b=${S286_BEFORE:0:8}; a=${S286_AFTER:0:8}
envinfo() {
  printf 'before=%s after=%s affinity=%s kernel=%s\n' "$S286_BEFORE" "$S286_AFTER" "$(taskset -pc $$ | sed 's/.*: //')" "$(uname -r)"
  ldd --version | head -1; clang --version | head -1; command -v perf || echo "no perf: wall time only"
  for h in "$S286_BEFORE" "$S286_AFTER"; do test "$(git -C "/w/${h:0:8}" rev-parse HEAD)" = "$h" || return 1; done
}
build() {
  cd "/w/$1" || return 2
  cargo build --locked -p wolf_driver -p wolf_rt || return $?
  cargo build --release --locked -p wolf_rt || return $?
  cargo test --release --locked -p wolf_rt --test spawn_bench --test chan_bench --no-run || return $?
  cargo xtask bench --track=t1 --runs=3 --kernels=list_alloc,b3_churn --out="$TMPDIR/warm-$1.jsonl" || return $?
}
round() {
  local r=$1 h=$2 i
  cd "/w/$h" || return 2
  cargo xtask bench --track=t1 --runs=10 --kernels=list_alloc,b3_churn --out="$out/t1-r$r-$h.jsonl" || return $?
  for i in 1 2 3; do
    cargo test --release --locked -q -p wolf_rt --test spawn_bench 2>&1 | grep '^spawn_bench:' | sed "s/^/r$r $h /" | tee -a "$out/pool-benches.txt"
    cargo test --release --locked -q -p wolf_rt --test chan_bench 2>&1 | grep '^chan_bench:' | sed "s/^/r$r $h /" | tee -a "$out/pool-benches.txt"
  done
}
step env envinfo || exit $?
step build-before build "$b" || exit $?
step build-after build "$a" || exit $?
r=0
for h in "$b" "$a" "$a" "$b" "$b" "$a"; do
  r=$((r+1))
  step "round-$r-$h" round "$r" "$h" || exit $?
done
diffs() {
  cd "/w/$a" || return 2
  local r
  for r in 1 3 5; do
    local x y
    case $r in 1) x=1; y=2;; 3) x=4; y=3;; 5) x=5; y=6;; esac
    echo "== pair rounds $x (before) and $y (after)"
    cargo xtask bench diff "$out/t1-r$x-$b.jsonl" "$out/t1-r$y-$a.jsonl"
  done
}
step bench-diff diffs
echo "PERF first_rc=$first_rc"
exit "$first_rc"
