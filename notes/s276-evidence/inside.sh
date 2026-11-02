#!/usr/bin/env bash
# s276: committed container entry; never build a live tree.
set -euo pipefail
export HOME=/lane/scratch/home TMPDIR=/lane/scratch/tmp XDG_CACHE_HOME=/lane/scratch/cache
mkdir -p "$HOME" "$TMPDIR" "$XDG_CACHE_HOME"
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export LUPIN=/lane/inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1 RUST_TEST_NOCAPTURE=1
unset CARGO_TARGET_DIR
case "$S276_BLOCK" in A) four=0-3 ;; B) four=6-9 ;; *) exit 2 ;; esac
export S276_FOUR=$four
printf 'S276_ENV %s block=%s four=%s kernel=%s\n' "$(date -u +%FT%TZ)" "$S276_BLOCK" "$four" "$(uname -r)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
printf 'pid1 uid=%s self uid=%s\n' "$(awk '/^Uid:/{print $2}' /proc/1/status)" "$(id -u)"
test "$(awk '/^Uid:/{print $2}' /proc/1/status)" != "$(id -u)"
rustc --version
exec /usr/bin/taskset.real -c "$four" bash "$@"
