set -euo pipefail
lane=/home/mfwolffe/lanes/s268
# Freeze the executable input outside the rsync-managed scripts directory.
# Updating an unrelated script must never change a running container's shim.
snapshot=$(mktemp "$lane/scratch/taskset.XXXXXX")
cp "$lane/scripts/taskset-shim.sh" "$snapshot"
chmod 755 "$snapshot"
test -x "$snapshot"
echo "taskset snapshot=$snapshot"
echo "s268 verification waiting for CPUs 4-7 $(date -u +%FT%TZ)"
exec flock /home/mfwolffe/lanes/kasumi-cores-4-7.lock taskset -c 4-7 podman run --rm --name s268-verification --network none \
  --userns=keep-id --user 0:0 --init --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --memory 12g --cpuset-cpus 4-7 \
  -v /usr/lib:/usr/lib:ro -v /usr/share:/usr/share:ro -v /usr/include:/usr/include:ro -v /var/lib/pacman:/var/lib/pacman:ro \
  -v /home/mfwolffe/.rustup:/opt/s268-rustup:ro -v "$lane:$lane" \
  -v "$snapshot:/usr/bin/taskset:ro" \
  -v /home/mfwolffe/lanes/s267/wolf-std:/opt/s268-std:ro \
  -v /home/mfwolffe/lanes/s267/toolchain/lupin/lupin-0.1.56-x86_64-unknown-linux-gnu/lupin:/opt/s268-lupin:ro \
  -v /home/mfwolffe/lanes/s267/bundle/stage/sysroot:/opt/s268-sysroot:ro \
  -v /home/mfwolffe/lanes/s267/bundle/stage/libwolf_rt.a:/opt/s268-musl-rt.a:ro \
  -v /home/mfwolffe/lanes/ld02b/ligature/target/ligature:/opt/s268-ligature:ro \
  -v /home/mfwolffe/lanes/ld02b/clef:/opt/s268-clef-tree:ro \
  -w "$lane/wolf-lang" -e HOME="$lane/scratch/home" -e TMPDIR="$lane/scratch/tmp" -e XDG_CACHE_HOME="$lane/scratch/cache" \
  -e RUSTUP_HOME=/opt/s268-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
  -e PATH=/opt/s268-rustup/toolchains/1.97.1-x86_64-unknown-linux-gnu/bin:/usr/bin:/bin \
  -e CARGO_HOME="$lane/cargo-home" -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 -e CARGO_NET_OFFLINE=true \
  -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 \
  -e WOLF_DIST_STD_SRC=/opt/s268-std \
  -e LUPIN=/opt/s268-lupin -e WOLF_PAIRING_REQUIRE_SIBLING=1 -e WOLF_SYSCALL_REQUIRE_STRACE=1 \
  -e WOLF_INTERRUPT_REQUIRE_QEMU=1 -e WOLF_RT_NONE_REQUIRE=1 -e WOLF_TEST_NATIVE_REQUIRE=1 \
  -e RUST_TEST_NOCAPTURE=1 -e S268_TASKSET_LOG="$lane/evidence/taskset.log" \
  -e WOLF_STATIC_ENTRY_SYSROOT=/opt/s268-sysroot -e WOLF_STATIC_ENTRY_MUSL_RT=/opt/s268-musl-rt.a -e WOLF_STATIC_ENTRY_REQUIRE=1 \
  -e WOLF_TEST_LIGATURE=/opt/s268-ligature -e WOLF_TEST_LIGATURE_REQUIRE=1 \
  -e WOLF_TEST_LIGATURE_RT=/opt/s268-musl-rt.a -e LIGATURE_SYSROOT=/opt/s268-sysroot \
  -e WOLF_TEST_CLEF=/opt/s268-clef-tree/target/clef -e WOLF_TEST_CLEF_REQUIRE=1 \
  localhost/s267-gauntlet:latest \
  setsid --fork --wait setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs \
  /usr/bin/taskset.real -c 4-7 s266-exec-clean bash "$lane/scripts/${S268_VERIFY_JOB:-verify}-inner.sh"
