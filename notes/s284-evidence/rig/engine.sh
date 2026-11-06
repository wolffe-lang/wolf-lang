#!/bin/sh
# s284: the container engine fork_lanes is handed inside a H1 gauntlet
# container (WOLF_FORK_ENGINE), where no engine runs: the host's own rootless
# podman (h1-env's, its /nix/store binary mounted read-only) as a remote
# client of `podman system service` on the lane's socket. Every container it
# starts is one of fork_lanes' throwaway ones, exactly as CI's docker starts
# them (no network, NO init: the image's `sh` is the namespace's process 1),
# with two additions the host's lane rules ask: `--memory 4g`, and the image's
# command run under `taskset -c $S284_ENGINE_CPUS` (the block's CPUs; taskset
# execs `sh`, which stays process 1), since H1 delegates no cpuset.
#   env: S284_PODMAN (the store binary), S284_PODMAN_SOCK, S284_ENGINE_CPUS,
#        WOLF_FORK_IMAGE (default docker.io/library/ubuntu:24.04)
img=${WOLF_FORK_IMAGE:-docker.io/library/ubuntu:24.04}
cpus=${S284_ENGINE_CPUS:?S284_ENGINE_CPUS}
sub=$1
shift
set -- "$sub" "$@"
if [ "$sub" = run ]; then
    first=1
    put=0
    for a do
        if [ "$first" = 1 ]; then
            set --
            first=0
        fi
        set -- "$@" "$a"
        if [ "$a" = run ] && [ "$#" = 1 ]; then
            set -- "$@" --memory 4g
        elif [ "$a" = "$img" ] && [ "$put" = 0 ]; then
            set -- "$@" taskset -c "$cpus"
            put=1
        fi
    done
fi
exec "${S284_PODMAN:?S284_PODMAN}" --remote --url "unix://${S284_PODMAN_SOCK:?S284_PODMAN_SOCK}" "$@"
