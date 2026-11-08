#!/usr/bin/env bash
# s286 on H1 (by copy from s285, s280 and s282), inside the container (one block of six CPUs).
export PATH=/lane/tool-bin:/opt/s263-rust/bin:$PATH
export CARGO_HOME=/lane/cargo-home RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 TMPDIR=/lane/tmp HOME=/lane/home
unset CARGO_TARGET_DIR
export RUST_TEST_NOCAPTURE=1 RUST_TEST_THREADS=4
# R-s280-2: no global WOLF_STD. conform-run falls back to WOLF_STD when a
# fixture has no adjacent std, so g1's export advanced corpus/strings.lu and
# corpus/typecheck/method_home_no_std.lu from resolve to wir. CI sets none.
unset WOLF_STD
# The std at the head's STD-PIN (0.2.38: d4a4d4ca), for dist-smoke.
export WOLF_DIST_STD_SRC=/lane/wolf-std-d4a4d4ca
export LUPIN=/lane/inputs/lupin060/unpacked/lupin-0.1.60-x86_64-unknown-linux-gnu/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_SYSCALL_REQUIRE_STRACE=1 WOLF_RT_NONE_REQUIRE=1
export WOLF_TEST_NATIVE_REQUIRE=1 WOLF_STATIC_ENTRY_REQUIRE=1
# CI's own dist-rt step exports this path.
export WOLF_STATIC_ENTRY_MUSL_RT=/work/target/dist-rt/x86_64-unknown-linux-musl/release/libwolf_rt.a
export WOLF_TEST_CLEF_REQUIRE=1 WOLF_TEST_LIGATURE_REQUIRE=1
# Every other path is the one the head's own setup scripts wrote, the way CI's
# GITHUB_ENV carries them (the prep take ran them with GITHUB_ENV set to this
# file): WOLF_STATIC_ENTRY_SYSROOT, WOLF_TEST_CLEF, WOLF_TEST_LIGATURE,
# WOLF_TEST_LIGATURE_RT, LIGATURE_SYSROOT. Nothing here hard-codes them.
ci_env=/work/target/s286-ci-env
if [ -f "$ci_env" ]; then set -a; . "$ci_env"; set +a; fi
