#!/bin/bash
set -euo pipefail
lane=/home/mfwolffe/lanes/s269
job=${1:?inner job}
exec 9>/home/mfwolffe/lanes/kasumi-cores-8-11.lock
until flock -n 9; do
    printf 's269 %s waiting for CPUs8-11 %s\n' "$job" "$(date -u +%FT%TZ)"
    sleep 30
done
df -h /home
du -sh "$lane"
used=$(du -sm "$lane" | cut -f1)
[ "$used" -lt 20480 ] || { echo 's269 exceeds 20GB ceiling' >&2; exit 2; }
snapshot=$(mktemp "$lane/scratch/taskset.XXXXXX")
cp "$lane/scripts/taskset.sh" "$snapshot"
chmod 755 "$snapshot"
printf 's269 %s acquired CPUs8-11 shim=%s %s\n' "$job" "$snapshot" "$(date -u +%FT%TZ)"
exec taskset -c 8-11 podman run --rm --name "s269-$job" --network none \
    --userns=keep-id --user 0:0 --init --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --memory 12g --cpuset-cpus 8-11 \
    -v /usr/lib:/usr/lib:ro -v /usr/share:/usr/share:ro -v /usr/include:/usr/include:ro -v /var/lib/pacman:/var/lib/pacman:ro \
    -v /home/mfwolffe/.rustup:/opt/s269-rustup:ro -v "$lane:$lane" -v "$snapshot:/usr/bin/taskset:ro" \
    -v /home/mfwolffe/lanes/s267/bundle/stage/sysroot:/opt/s269-sysroot:ro \
    -v /home/mfwolffe/lanes/s267/bundle/stage/libwolf_rt.a:/opt/s269-musl-rt.a:ro \
    -v /home/mfwolffe/lanes/ld02b/ligature/target/ligature:/opt/s269-ligature:ro \
    -v /home/mfwolffe/lanes/ld02b/clef:/opt/s269-clef-tree:ro \
    -w "$lane/wolf-lang" -e HOME="$lane/scratch/home" -e TMPDIR="$lane/scratch/tmp" -e XDG_CACHE_HOME="$lane/scratch/cache" \
    -e RUSTUP_HOME=/opt/s269-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
    -e PATH=/opt/s269-rustup/toolchains/1.97.1-x86_64-unknown-linux-gnu/bin:/usr/bin:/bin \
    -e CARGO_HOME="$lane/cargo-home" -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 -e CARGO_NET_OFFLINE=true \
    -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 -e CARGO_PROFILE_RELEASE_DEBUG=0 \
    -e WOLF_DIST_STD_SRC="$lane/wolf-std" \
    -e LUPIN="$lane/toolchain/lupin-extracted/lupin-0.1.58-x86_64-unknown-linux-gnu/lupin" \
    -e WOLF_PAIRING_REQUIRE_SIBLING=1 -e WOLF_SYSCALL_REQUIRE_STRACE=1 -e WOLF_INTERRUPT_REQUIRE_QEMU=1 \
    -e WOLF_RT_NONE_REQUIRE=1 -e WOLF_TEST_NATIVE_REQUIRE=1 -e WOLF_TEST_CPUS=8-11 -e RUST_TEST_NOCAPTURE=1 \
    -e WOLF_STATIC_ENTRY_SYSROOT=/opt/s269-sysroot -e WOLF_STATIC_ENTRY_MUSL_RT=/opt/s269-musl-rt.a -e WOLF_STATIC_ENTRY_REQUIRE=1 \
    -e WOLF_TEST_LIGATURE=/opt/s269-ligature -e WOLF_TEST_LIGATURE_REQUIRE=1 -e WOLF_TEST_LIGATURE_RT=/opt/s269-musl-rt.a \
    -e LIGATURE_SYSROOT=/opt/s269-sysroot -e WOLF_TEST_CLEF=/opt/s269-clef-tree/target/clef -e WOLF_TEST_CLEF_REQUIRE=1 \
    localhost/s267-gauntlet:latest \
    setsid --fork --wait setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs \
    /usr/bin/taskset.real -c 8-11 s266-exec-clean bash "$lane/scripts/$job-inner.sh"
