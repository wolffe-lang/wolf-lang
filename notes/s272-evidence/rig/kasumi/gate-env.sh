# s272: the gauntlet's environment on hasu or kasumi (K: slot 8-11; adapted by copy from s273's gate-env.sh and
# inner.sh, themselves copied from s271's). Sourced inside the container.
export HOME=/lane/scratch/home TMPDIR=/lane/scratch/tmp XDG_CACHE_HOME=/lane/scratch/cache
mkdir -p "$HOME" "$TMPDIR" "$XDG_CACHE_HOME"
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
unset CARGO_TARGET_DIR
case "${S272_BLOCK:-}" in
  A) export WOLF_TEST_CPUS=0-3 ;;
  B) export WOLF_TEST_CPUS=6-9 ;;
  K) export WOLF_TEST_CPUS=0-3 CARGO_BUILD_JOBS=4 ;;
  *) echo "s272: no block" >&2; exit 2 ;;
esac
export RUST_TEST_NOCAPTURE=1
export LUPIN=/inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 WOLF_RT_NONE_REQUIRE=1
export WOLF_TEST_NATIVE_REQUIRE=1 WOLF_STATFS_REQUIRE_GNU=1
export WOLF_DIST_STD_SRC=/lane/wolf-std
export WOLF_STATIC_ENTRY_MUSL_RT=/lane/wolf-lang/target/dist-rt/x86_64-unknown-linux-musl/release/libwolf_rt.a
export WOLF_STATIC_ENTRY_SYSROOT=/lane/wolf-lang/target/static-entry-bundle/sysroot WOLF_STATIC_ENTRY_REQUIRE=1
export WOLF_TEST_LIGATURE_RT=$WOLF_STATIC_ENTRY_MUSL_RT LIGATURE_SYSROOT=$WOLF_STATIC_ENTRY_SYSROOT
export WOLF_TEST_CLEF=/lane/wolf-lang/target/test-clef/target/clef WOLF_TEST_CLEF_REQUIRE=1
export WOLF_TEST_LIGATURE=/lane/wolf-lang/target/test-ligature/target/ligature WOLF_TEST_LIGATURE_REQUIRE=1
ulimit -c 0
