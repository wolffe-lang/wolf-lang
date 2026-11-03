#!/usr/bin/env bash
# Run one immutable compiler snapshot, with both lock names and one deadline.
set -uo pipefail
mode=${1:?observe, red, pairing-red-restore, green, postrebase or plant}
head=${2:?full head}
base=/home/mfwolffe/lanes/r42
[[ "$head" =~ ^[0-9a-f]{40}$ ]] || exit 2
case "$mode" in observe|red|pairing-red-restore|green|postrebase|plant) ;; *) exit 2 ;; esac
name="wolf-$mode-$head"
snapshot="$base/builds/$name"
records="$base/records/$name"
marker="$base/records/$name.exit"
[[ -f "$snapshot/ci/r42/wolf-work.sh" && ! -e "$marker" ]] || exit 2
mkdir -p "$records" "$base/scratch/wolf-home" "$base/scratch/wolf-tmp" "$base/scratch/wolf-cache" "$base/cargo"
echo $$ > "$base/records/$name.pid"
size=$(du -sk "$base" | awk '{print $1}')
((size < 9500000)) || exit 3
export PATH=/home/mfwolffe/lanes/hasu-env/bin:/home/mfwolffe/lanes/hasu-env/profile/bin:$PATH
a=$(ps -eo args | grep -c '[f]lock .*hasu-block-A.lock' || true)
b=$(ps -eo args | grep -c '[f]lock .*hasu-block-B.lock' || true)
block=A; four=0-3; one=0
if (( b <= a )); then block=B; four=6-9; one=6; fi
echo "SUBMIT $name waiters_A=$a waiters_B=$b block=$block cpus=$four size_KiB=$size $(date -u +%FT%TZ)"
flock -w 21600 --close "/home/mfwolffe/lanes/hasu-block-$block.lock" \
    bash "$snapshot/ci/r42/lock-budget.sh" outer "/home/mfwolffe/lanes/hasu-$block.lock" \
    podman --rm --name "r42-wolf-$mode-${head:0:12}" \
    --memory 12g --pids-limit 16384 --ulimit nproc=32768:32768 --ulimit core=0 \
    --stop-timeout 10 --userns keep-id:uid=1000,gid=1000 --user 0 \
    -v "$base:/lane:rw" -v "$base:$base:rw" -w "/lane/builds/$name" \
    -e PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    -e RUSTUP_HOME=/opt/s263-rustup -e RUSTUP_TOOLCHAIN=1.97.1 \
    -e CARGO_HOME=/lane/cargo -e CARGO_INCREMENTAL=0 -e CARGO_BUILD_JOBS=4 \
    -e CARGO_PROFILE_DEV_DEBUG=0 -e CARGO_PROFILE_TEST_DEBUG=0 -e RUST_TEST_THREADS=4 \
    -e RUST_TEST_NOCAPTURE=1 -e "WOLF_TEST_CPUS=$four" \
    -e HOME=/lane/scratch/wolf-home -e TMPDIR=/lane/scratch/wolf-tmp \
    -e XDG_CACHE_HOME=/lane/scratch/wolf-cache \
    -e "R42_CPU_FOUR=$four" -e "R42_CPU_ONE=$one" \
    -e "R42_TASKSET_LOG=/lane/records/$name/taskset-translations.log" \
    -e R42_LOCK_DEADLINE localhost/r41-toolchain:1 bash -c '
        unset CARGO_TARGET_DIR
        budget=$(bash ci/r42/lock-budget.sh work "$(date +%s)" "$R42_LOCK_DEADLINE") || exit $?
        timeout --foreground --kill-after=5 "$budget" bash ci/r42/wolf-work.sh "$@"
        rc=$?
        exit "$rc"
    ' bash "$mode" "$head" "/lane/records/$name" &
locker=$!
echo "$locker" > "$base/records/$name.locker.pid"
while [[ -d "/proc/$locker" ]]; do
    echo "R42_WOLF_JOB_ALIVE name=$name locker=$locker $(date -u +%FT%TZ)"
    sleep 60
done
wait "$locker"
rc=$?
echo "R42_WOLF_JOB_EXIT name=$name exit=$rc $(date -u +%FT%TZ)"
printf '%s\n' "$rc" > "$marker"
du -sk "$base"
exit "$rc"
