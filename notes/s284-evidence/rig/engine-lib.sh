# s284: the host side of rig/engine.sh, sourced by a take that holds its
# block's locks. engine_up starts `podman system service` (h1-env's podman,
# the lane's own config and storage) on a socket of this take's own under
# the lane directory, records its pid, and waits for the socket; engine_down
# stops that pid and only it. ENGINE_FLAGS are the gauntlet container's extra
# `podman run` flags and environment for fork_lanes.
S284_STORE_PODMAN=/nix/store/a4x9z75ghw7jrd7kv4c4rrv6dz0ayfsp-podman-5.8.9/bin/podman
engine_up() { # $1 label, $2 the block's cpus
    local L=/home/mfwolffe/lanes/s284 i
    mkdir -p "$L/run"
    ENGINE_SOCK=$L/run/podman-$1.sock
    rm -f "$ENGINE_SOCK"
    "$HOME/lanes/h1-env/bin/podman" system service --time=0 "unix://$ENGINE_SOCK" >"$L/logs/engine-$1.log" 2>&1 </dev/null &
    ENGINE_PID=$!
    echo "$ENGINE_PID" >"$L/run/podman-$1.pid"
    for i in $(seq 1 100); do
        [ -S "$ENGINE_SOCK" ] && break
        sleep 0.1
    done
    [ -S "$ENGINE_SOCK" ] || { echo "ENGINE refused: no socket after 10 s (pid $ENGINE_PID)"; return 1; }
    echo "ENGINE up pid=$ENGINE_PID sock=$ENGINE_SOCK cpus=$2"
    ENGINE_FLAGS=(-v /nix/store:/nix/store:ro
        -e "S284_PODMAN=$S284_STORE_PODMAN" -e "S284_PODMAN_SOCK=$ENGINE_SOCK" -e "S284_ENGINE_CPUS=$2"
        -e "WOLF_FORK_ENGINE=$L/rig/engine.sh" -e WOLF_FORK_REQUIRE_CONTAINER=1
        -e WOLF_FORK_IMAGE=docker.io/library/ubuntu:24.04)
}
engine_down() {
    [ -n "${ENGINE_PID:-}" ] || return 0
    kill -TERM "$ENGINE_PID" 2>/dev/null
    wait "$ENGINE_PID" 2>/dev/null
    echo "ENGINE down pid=$ENGINE_PID"
    rm -f "$ENGINE_SOCK"
    ENGINE_PID=
}
