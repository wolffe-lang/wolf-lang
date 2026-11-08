#!/usr/bin/env bash
# s286, inside the perf container as uid 1000 (perf-take2.sh), on one block:
# the one row outside its margin in perf2, b3_churn's PGO lane (28 ms before,
# 29 ms after: one tick of the kernel's 1 ms clock), read again at ten times
# the ops so a tick is 0.35%, before and after interleaved (ABBA, 20 rounds),
# with the gated wolf lane beside it. Each run prints the kernel's own ns
# and the child's user CPU time. Also the binaries' and the PGO profiles'
# digests, since each head's PGO lane was built from its own training run.
# Then the pool's benches again, 30 runs a head in ABBA order (perf2 had 9,
# and one row, worker_spawn_median, read faster after by more than its margin).
set -uo pipefail
export PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export CARGO_HOME=/lane/cargo-home RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6 HOME=/lane/home XDG_CACHE_HOME=/lane/home/.cache
unset CARGO_TARGET_DIR MALLOC_ARENA_MAX MALLOC_ARENA_TEST GLIBC_TUNABLES
b=${S286_BEFORE:0:8}; a=${S286_AFTER:0:8}
ops=7900000
for h in "$b" "$a"; do
  echo "== $h"; sha256sum /w/$h/target/bench-t1/b3_churn_wolf /w/$h/target/bench-t1/b3_churn_wolf_pgo /w/$h/target/bench-t1/b3_churn.wprof
done
one() {
  local h=$1 lane=$2 r=$3 out
  TIMEFORMAT='%U'
  { time out=$(/w/$h/target/bench-t1/b3_churn_$lane "$ops"); } 2> "$TMPDIR/t"
  echo "r$r $h $lane user_s=$(cat "$TMPDIR/t") $out"
}
mkdir -p "$TMPDIR"
for r in $(seq 1 20); do
  if [ $((r % 2)) -eq 1 ]; then order="$b $a"; else order="$a $b"; fi
  for h in $order; do
    one "$h" wolf "$r"
    one "$h" wolf_pgo "$r"
  done
done
echo "PGO_READ done"
pool() {
  local h=$1 r=$2
  cd /w/$h || return 2
  cargo test --release --locked -q -p wolf_rt --test spawn_bench 2>&1 | grep '^spawn_bench:' | sed "s/^/p$r $h /"
  cargo test --release --locked -q -p wolf_rt --test chan_bench 2>&1 | grep '^chan_bench:' | sed "s/^/p$r $h /"
}
for r in $(seq 1 30); do
  if [ $((r % 4)) -eq 1 ] || [ $((r % 4)) -eq 0 ]; then order="$b $a"; else order="$a $b"; fi
  for h in $order; do pool "$h" "$r"; done
done
echo "POOL_READ done"
