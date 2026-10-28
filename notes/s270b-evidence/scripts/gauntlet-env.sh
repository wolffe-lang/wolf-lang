export HOME=/lane/scratch/home TMPDIR=/lane/scratch/tmp XDG_CACHE_HOME=/lane/scratch/cache
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
unset CARGO_TARGET_DIR WOLF_STD WOLF_MIDEND WOLF_MIDEND_SKIP
export RUST_TEST_NOCAPTURE=1
export LUPIN=/lane/inputs/lupin/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 WOLF_RT_NONE_REQUIRE=1 WOLF_TEST_NATIVE_REQUIRE=1 WOLF_TEST_CPUS=0-3
export WOLF_DIST_STD_SRC=/lane/wolf-std
export WOLF_STATIC_ENTRY_MUSL_RT=/lane/wolf-lang/target/dist-rt/x86_64-unknown-linux-musl/release/libwolf_rt.a
export WOLF_STATIC_ENTRY_SYSROOT=/lane/inputs/s267/unpacked/sysroot WOLF_STATIC_ENTRY_REQUIRE=1
export WOLF_TEST_LIGATURE_RT=$WOLF_STATIC_ENTRY_MUSL_RT LIGATURE_SYSROOT=$WOLF_STATIC_ENTRY_SYSROOT
export WOLF_TEST_CLEF=/lane/wolf-lang/target/test-clef/target/clef WOLF_TEST_CLEF_REQUIRE=1
export WOLF_TEST_LIGATURE=/lane/wolf-lang/target/test-ligature/target/ligature WOLF_TEST_LIGATURE_REQUIRE=1
ulimit -c 0
