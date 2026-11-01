#!/usr/bin/env bash
# s274: inside the container, as the lane uid (copied from s273's inner.sh).
set -euo pipefail
export HOME=/lane/scratch/home TMPDIR=/lane/scratch/tmp XDG_CACHE_HOME=/lane/scratch/cache
mkdir -p "$HOME" "$TMPDIR" "$XDG_CACHE_HOME" /lane/records
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
unset CARGO_TARGET_DIR
case "${S274_BLOCK:-}" in
  A) export S274_SIX=0-5 S274_FOUR=0-3 ;;
  B) export S274_SIX=6-11 S274_FOUR=6-9 ;;
  *) echo "s274: no block" >&2; exit 2 ;;
esac
ulimit -c 0
printf 's274 block=%s cpus=%s six=%s four=%s date=%s kernel=%s\n' "$S274_BLOCK" "$S274_CPUS" "$S274_SIX" "$S274_FOUR" "$(date -u +%FT%TZ)" "$(uname -r)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
pid1_uid=$(awk '/^Uid:/{print $2}' /proc/1/status)
printf 'pid1 uid=%s self uid=%s\n' "$pid1_uid" "$(id -u)"
test "$pid1_uid" != "$(id -u)"
rustc --version
exec "$@"
