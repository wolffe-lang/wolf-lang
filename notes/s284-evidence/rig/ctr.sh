#!/usr/bin/env bash
# s284: one command in localhost/r41-toolchain:1 as the lane uid (rootless
# podman, keep-id), --memory 12g, bash kept as the container's pid 1
# (`; exit $?`, h1-env README). The lane directory is mounted at its own
# path so a git worktree's .git file still resolves inside. Copied by value
# from s275's scripts/H1/ctr.sh.
# usage: ctr.sh <net|nonet> <workdir> <cmd...>
L=/home/mfwolffe/lanes/s284
net=${1:?net or nonet}
dir=${2:?workdir}
shift 2
export PATH=$HOME/lanes/h1-env/bin:$HOME/lanes/h1-env/profile/bin:$PATH
mkdir -p "$L/scratch/home" "$L/tmp" "$L/cargo"
nf=()
[ "$net" = nonet ] && nf=(--network none)
exec podman run --rm "${nf[@]}" --userns=keep-id -u "$(id -u):$(id -g)" \
    --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
    -v "$L:$L" -w "$dir" \
    -e HOME="$L/scratch/home" -e TMPDIR="$L/tmp" -e CARGO_HOME="$L/cargo" \
    -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=6 -e CARGO_TERM_COLOR=never \
    -e RUSTUP_HOME=/opt/s263-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
    -e PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    localhost/r41-toolchain:1 bash -c 'ulimit -c 0; "$@"; exit $?' ctr "$@"
