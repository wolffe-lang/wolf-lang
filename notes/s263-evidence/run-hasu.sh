#!/usr/bin/env bash
set -eu
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
ulimit -c 0
lane=$HOME/lanes/s263
cargo_bin=$(readlink -f "$(command -v cargo)")
rustc_bin=$(readlink -f "$(command -v rustc)")
rustdoc_bin=$(readlink -f "$(command -v rustdoc)")
clippy_bin=$(readlink -f "$(command -v cargo-clippy)")
rustfmt_bin=$(readlink -f "$(command -v rustfmt)")
container_path=$(dirname "$cargo_bin"):$(dirname "$rustc_bin"):$(dirname "$clippy_bin"):$(dirname "$rustfmt_bin"):/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
exec flock "$HOME/lanes/hasu-B.lock" podman run --rm --memory 12g -v /nix:/nix:ro -v "$lane:/work" -w /work \
 -e CARGO_HOME=/work/.cargo-home -e CARGO_TARGET_DIR=/work/target -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=6 \
 -e RUSTC="$rustc_bin" -e RUSTDOC="$rustdoc_bin" -e PATH="$container_path" \
 localhost/s263-toolchain:1.97.1 taskset -c 6-11 "$@"
