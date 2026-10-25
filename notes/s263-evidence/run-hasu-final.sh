#!/usr/bin/env bash
set -eu
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
ulimit -c 0
lane=$HOME/lanes/s263
# Complete pinned toolchain is image-owned; no host Rust/Nix sysroot is used.
exec flock "$HOME/lanes/hasu-B.lock" podman run --rm --memory 12g --ulimit core=0:0 \
 -v "$lane:/work" -w /work \
 -e CARGO_HOME=/work/.cargo-home -e CARGO_TARGET_DIR=/work/target \
 -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=6 \
 -e RUSTUP_HOME=/opt/s263-rustup \
 -e PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
 localhost/s263-toolchain:final-1.97.1 taskset -c 6-11 "$@"
