set -euo pipefail
lane=/home/mfwolffe/lanes/s268
mkdir -p "$lane/evidence" "$lane/scratch/home" "$lane/scratch/tmp" "$lane/scratch/cache" "$lane/baseline"
cp -a --reflink=auto /home/mfwolffe/lanes/s267/cargo-home "$lane/cargo-home"
echo "s268 baseline waiting for CPUs 4-7 $(date -u +%FT%TZ)"
exec flock /home/mfwolffe/lanes/kasumi-cores-4-7.lock taskset -c 4-7 podman run --rm --name s268-baseline --network none \
  --userns=keep-id --user 0:0 --init --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 --memory 12g --cpuset-cpus 4-7 \
  -v /usr/lib:/usr/lib:ro -v /usr/share:/usr/share:ro -v /usr/include:/usr/include:ro -v /var/lib/pacman:/var/lib/pacman:ro \
  -v /home/mfwolffe/.rustup:/opt/s268-rustup:ro -v "$lane:$lane" \
  -v /home/mfwolffe/lanes/s267/wolf-std:/opt/s268-std:ro \
  -w "$lane/wolf-lang" -e HOME="$lane/scratch/home" -e TMPDIR="$lane/scratch/tmp" -e XDG_CACHE_HOME="$lane/scratch/cache" \
  -e RUSTUP_HOME=/opt/s268-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
  -e PATH=/opt/s268-rustup/toolchains/1.97.1-x86_64-unknown-linux-gnu/bin:/usr/bin:/bin \
  -e CARGO_HOME="$lane/cargo-home" -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 -e CARGO_NET_OFFLINE=true \
  -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 -e WOLF_STD=/opt/s268-std/std \
  localhost/s267-gauntlet:latest \
  setpriv --reuid=1000 --regid=1000 --clear-groups --bounding-set=-all --inh-caps=-all --ambient-caps=-all --no-new-privs \
  /usr/bin/taskset.real -c 4-7 s266-exec-clean bash /home/mfwolffe/lanes/s268/scripts/baseline-inner.sh
