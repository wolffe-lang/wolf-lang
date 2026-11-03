# s278: the container's environment (adapted by copy from s277's gate-env.sh).
# Sourced inside the container. Prints nothing.
export HOME=/lane/scratch/home TMPDIR=/lane/tmp XDG_CACHE_HOME=/lane/scratch/cache
mkdir -p "$HOME" "$TMPDIR" "$XDG_CACHE_HOME"
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
unset CARGO_TARGET_DIR
case "${S278_BLOCK:-}" in
  A) export WOLF_TEST_CPUS=0-3 ;;
  B) export WOLF_TEST_CPUS=6-9 ;;
  *) echo "s278: no block" >&2; exit 2 ;;
esac
export RUST_TEST_NOCAPTURE=1
export LUPIN=/inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1
ulimit -c 0
