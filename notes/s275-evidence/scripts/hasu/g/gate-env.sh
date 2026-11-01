# s275: the gauntlet's environment (copied from s273's, itself s271's).
export RUST_TEST_NOCAPTURE=1
export LUPIN=/inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 WOLF_RT_NONE_REQUIRE=1
export WOLF_TEST_NATIVE_REQUIRE=1
export WOLF_DIST_STD_SRC=/lane/wolf-std
export WOLF_STATIC_ENTRY_MUSL_RT=/lane/wolf-lang/target/dist-rt/x86_64-unknown-linux-musl/release/libwolf_rt.a
export WOLF_STATIC_ENTRY_SYSROOT=/lane/wolf-lang/target/static-entry-bundle/sysroot WOLF_STATIC_ENTRY_REQUIRE=1
export WOLF_TEST_LIGATURE_RT=$WOLF_STATIC_ENTRY_MUSL_RT LIGATURE_SYSROOT=$WOLF_STATIC_ENTRY_SYSROOT
export WOLF_TEST_CLEF=/lane/wolf-lang/target/test-clef/target/clef WOLF_TEST_CLEF_REQUIRE=1
export WOLF_TEST_LIGATURE=/lane/wolf-lang/target/test-ligature/target/ligature WOLF_TEST_LIGATURE_REQUIRE=1
