# s277: the container's environment (adapted by copy from s272's gate-env.sh).
export HOME=/lane/scratch/home TMPDIR=/lane/tmp XDG_CACHE_HOME=/lane/scratch/cache
mkdir -p "$HOME" "$TMPDIR" "$XDG_CACHE_HOME"
export PATH=/opt/s263-rust/bin:$PATH RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_HOME=/lane/cargo CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=6
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
unset CARGO_TARGET_DIR
export RUST_TEST_NOCAPTURE=1
export LUPIN=/inputs/lupin058/lupin-0.1.58-x86_64-unknown-linux-gnu.tar.gz/unpacked/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin
ulimit -c 0
