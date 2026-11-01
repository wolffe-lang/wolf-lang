#!/bin/bash
# s273: run named steps in the current (build) tree, printing END <step> exit=N
# for each and stopping at the first nonzero. usage: steps.sh <step>...
# Steps: build, clippy-touched, t-pairswalk, t-lanes, t-mem, t-wir, corpus,
#        conformance, differ, lane-coverage, t-driver-region
export CARGO_INCREMENTAL=0
# rustup's proxy first: it honours rust-toolchain.toml (1.97.1); Homebrew's cargo does not.
export PATH="$HOME/.cargo/bin:$PATH"
run() {
  local name=$1
  shift
  echo "=== $name: $*"
  "$@"
  local rc=$?
  echo "END $name exit=$rc"
  return $rc
}
for s in "$@"; do
  case $s in
    fmt) run fmt cargo fmt --all --check || exit 1 ;;
    build) run build cargo build -j4 -p wolf_driver -p wolf_rt || exit 1 ;;
    clippy-touched) run clippy-touched cargo clippy -j4 -p wolf_mem -p wolf_wir -p wolf_driver --all-targets -- -D warnings || exit 1 ;;
    t-pairswalk) run t-pairswalk cargo test -j4 -p wolf_mem --test pairswalk || exit 1 ;;
    t-lanes) run t-lanes cargo test -j4 -p wolf_driver --test map_pairs_walk_lanes -- --test-threads=1 --nocapture || exit 1 ;;
    t-mem) run t-mem cargo test -j4 -p wolf_mem || exit 1 ;;
    t-wir) run t-wir cargo test -j4 -p wolf_wir || exit 1 ;;
    corpus) run corpus cargo xtask corpus || exit 1 ;;
    conformance) run conformance cargo xtask conformance || exit 1 ;;
    differ) run differ cargo xtask differ --self || exit 1 ;;
    lane-coverage) run lane-coverage cargo xtask lane-coverage || exit 1 ;;
    t-driver-region) run t-driver-region cargo test -j4 -p wolf_driver --test region_outer_write_lanes --test checked_budget --test cwd_region_lanes || exit 1 ;;
    *) echo "unknown step $s"; exit 2 ;;
  esac
done
