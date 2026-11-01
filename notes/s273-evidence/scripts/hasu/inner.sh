#!/usr/bin/env bash
# s273: inside the container, as the lane uid (copied from s271's inner.sh).
set -euo pipefail
export HOME=/lane/scratch/home TMPDIR=/lane/scratch/tmp XDG_CACHE_HOME=/lane/scratch/cache
mkdir -p "$HOME" "$TMPDIR" "$XDG_CACHE_HOME" /lane/records
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
unset CARGO_TARGET_DIR
case "${S273_BLOCK:-}" in
  A) export WOLF_TEST_CPUS=0-3 ;;
  B) export WOLF_TEST_CPUS=6-9 ;;
  *) echo "s273: no block" >&2; exit 2 ;;
esac
export WOLF_TEST_NATIVE_REQUIRE=1
ulimit -c 0
printf 's273 block=%s cpus=%s head=%s date=%s\n' "$S273_BLOCK" "$S273_CPUS" "$(git rev-parse HEAD)" "$(date -u +%FT%TZ)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
pid1_uid=$(awk '/^Uid:/{print $2}' /proc/1/status)
printf 'pid1 uid=%s self uid=%s\n' "$pid1_uid" "$(id -u)"
test "$pid1_uid" != "$(id -u)"
rustc --version
exec "$@"
