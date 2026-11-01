#!/usr/bin/env bash
# s275: one command in localhost/r41-toolchain:1 as the lane uid (rootless
# podman, keep-id), --memory 12g, bash kept as the container's pid 1
# (`; exit $?`, hasu-env README). The lane directory is mounted at its own
# path so a git worktree's .git file still resolves inside.
# usage: ctr.sh <net|nonet> <workdir> <cmd...>
L=/home/mfwolffe/lanes/s275
net=${1:?net or nonet}
dir=${2:?workdir}
shift 2
export PATH=$HOME/lanes/hasu-env/bin:$HOME/lanes/hasu-env/profile/bin:$PATH
mkdir -p "$L/scratch/home" "$L/scratch/tmp"
nf=()
[ "$net" = nonet ] && nf=(--network none)
exec podman run --rm "${nf[@]}" --userns=keep-id -u "$(id -u):$(id -g)" \
    --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
    -v "$L:$L" -w "$dir" \
    -e HOME="$L/scratch/home" -e TMPDIR="$L/scratch/tmp" -e CARGO_HOME="$L/cargo" \
    -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=6 -e CARGO_TERM_COLOR=never \
    -e RUSTUP_HOME=/opt/s263-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
    -e PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    localhost/r41-toolchain:1 bash -c 'ulimit -c 0; "$@"; exit $?' ctr "$@"
