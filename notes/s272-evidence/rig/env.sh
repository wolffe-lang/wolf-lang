# s272: the container's environment (adapted by copy from r41's wolf-env.sh).
export HOME=/lane/scratch/home TMPDIR=/lane/scratch/tmp XDG_CACHE_HOME=/lane/scratch/cache
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
unset CARGO_TARGET_DIR
export RUST_TEST_NOCAPTURE=1
export LUPIN=/lane/inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_CPUS=0-3 WOLF_STATFS_REQUIRE_GNU=1
ulimit -c 0
