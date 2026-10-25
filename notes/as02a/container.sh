#!/bin/bash
# as02a's container: kasumi CPUs 4-7 under the lane lock (RULES §4): the
# taskset shim (the image's /usr/bin/taskset), --pids-limit 16384 and
# nproc 32768, and a foreign pid 1 (the container's init as the userns
# root, a subordinate uid) with the command dropped to the lane's uid.
# Never --pid=host.
set -euo pipefail
lane="$HOME/lanes/as02a"
name="as02a-${AS02A_JOB:-job}"
echo "as02a waiting for the CPUs 4-7 lock ($(date -u +%FT%TZ)); job $name: $*"
exec flock "$HOME/lanes/kasumi-cores-4-7.lock" podman run --rm --name "$name" --network none \
    --userns=keep-id --user 0:0 --init --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
    --memory 12g --cpuset-cpus 4-7 \
    -v /usr/lib:/usr/lib:ro -v /usr/share:/usr/share:ro -v /usr/include:/usr/include:ro \
    -v /var/lib/pacman:/var/lib/pacman:ro \
    -v "$HOME/.rustup:/opt/as02a-rustup:ro" -v "$lane:$lane" -w "$lane/wolf-lang" \
    -e HOME="$lane/scratch/home" -e TMPDIR="$lane/scratch/tmp" -e XDG_CACHE_HOME="$lane/scratch/cache" \
    -e RUSTUP_HOME=/opt/as02a-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
    -e PATH=/opt/as02a-rustup/toolchains/1.97.1-x86_64-unknown-linux-gnu/bin:/usr/bin:/bin \
    -e CARGO_HOME="$lane/cargo-home" -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 -e CARGO_NET_OFFLINE=true \
    -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 \
    -e S266_TASKSET_LOG="$lane/evidence/taskset-translations.log" \
    -e WOLF_TEST_CLEF="$lane/clef/target/clef" -e WOLF_TEST_CLEF_REQUIRE=1 \
    -e LUPIN="$lane/downloads/lupin/lupin-0.1.56-x86_64-unknown-linux-gnu/lupin" \
    -e WOLF_PAIRING_REQUIRE_SIBLING=1 -e WOLF_SYSCALL_REQUIRE_STRACE=1 \
    -e WOLF_INTERRUPT_REQUIRE_QEMU=1 -e WOLF_RT_NONE_REQUIRE=1 -e WOLF_TEST_NATIVE_REQUIRE=1 \
    -e WOLF_DIST_STD_SRC="$lane/wolf-std" -e RUST_TEST_NOCAPTURE=1 \
    ${AS02A_ENV:+$AS02A_ENV} \
    localhost/s266-gauntlet:latest \
    setpriv --reuid="$(id -u)" --regid="$(id -g)" --clear-groups --bounding-set=-all \
    --inh-caps=-all --ambient-caps=-all --no-new-privs \
    taskset -c 4-7 s266-exec-clean bash -c 'ulimit -c 0; exec "$@"' bash "$@"
