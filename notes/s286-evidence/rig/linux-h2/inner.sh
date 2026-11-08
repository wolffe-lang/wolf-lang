#!/usr/bin/env bash
# s286, inside the container as uid 1000: one measurement take. Every step
# prints START/END at column 0 with its output indented; a measurement take
# runs every step and its own exit is the first nonzero step's (a red row
# is the expected answer at the base, so the caller reads each END).
#   modes: red | green | full   (S286_MODE; heads in S286_HEADS, each
#   mounted at /w/<its first 8 hex>, the same path in every take, so a
#   target built in one take is fresh in the next)
set -uo pipefail
export PATH=/opt/s263-rust/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export CARGO_HOME=/lane/cargo-home RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=8 RUST_TEST_THREADS=4
export TMPDIR=/lane/tmp/${S286_LABEL:?} HOME=/lane/home XDG_CACHE_HOME=/lane/home/.cache
export WOLF_SYSCALL_REQUIRE_STRACE=1
unset CARGO_TARGET_DIR WOLF_STD LUPIN_STD WOLF_RT_LIB MALLOC_ARENA_MAX MALLOC_ARENA_TEST GLIBC_TUNABLES
mkdir -p "$TMPDIR" "$HOME"
read -r -a heads <<< "$S286_HEADS"
wd() { printf '/w/%s' "${heads[$1]:0:8}"; }
first_rc=0
step() {
  local name=$1; shift
  local started=$SECONDS
  printf 'START %s\n' "$name"
  "$@" 2>&1 | sed 's/^/  /'
  local rc=${PIPESTATUS[0]}
  printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"
  [ "$rc" -ne 0 ] && [ "$first_rc" -eq 0 ] && first_rc=$rc
  return 0
}
env_step() {
  printf 'label=%s mode=%s heads=%s\n' "$S286_LABEL" "$S286_MODE" "$S286_HEADS"
  printf 'affinity=%s nproc=%s kernel=%s\n' "$(taskset -pc $$ | sed 's/.*: //')" "$(nproc)" "$(uname -r)"
  ldd --version | head -1
  strace -V | head -1
  grep -E 'SigBlk|SigIgn|CapEff|NoNewPrivs' /proc/self/status
  printf 'PID1 uid=%s workload uid=%s\n' "$(awk '/^Uid:/{print $2}' /proc/1/status)" "$(id -u)"
  local h
  for h in "${heads[@]}"; do
    printf '/w/%s: %s (expected %s)\n' "${h:0:8}" "$(git -C "/w/${h:0:8}" rev-parse HEAD)" "$h"
    test "$(git -C "/w/${h:0:8}" rev-parse HEAD)" = "$h" || return 1
  done
}
build_head() {
  cd "$(wd "$1")" || return 2
  cargo build --locked -p wolf_driver -p wolf_rt || return $?
  cargo test --locked -p wolf_driver --test signal_gaps_lanes --no-run || return $?
  cargo test --locked -p wolf_rt --test arena_limit --no-run || return $?
}
# The probe: arenas held, plain and set-user-ID, with each user setting.
probe_step() {
  local exe env count line
  for exe in /opt/s286/arena-probe /opt/s286/arena-probe-suid; do
    for env in none MALLOC_ARENA_MAX=2 GLIBC_TUNABLES=glibc.malloc.arena_max=2; do
      if [ "$env" = none ]; then out=$("$exe" 2>&1); else out=$(env "$env" "$exe" 2>&1); fi
      count=$(printf '%s\n' "$out" | grep -c '^Arena ')
      line=$(printf '%s\n' "$out" | grep '^PROBE')
      printf 'probe %s %s: arenas=%s | %s\n' "${exe##*/}" "$env" "$count" "$line"
    done
  done
  for cpus in 0-3 0-3,8-11; do
    out=$(taskset.real -c "$cpus" /opt/s286/arena-probe 2>&1)
    printf 'probe arena-probe taskset %s: arenas=%s | %s\n' "$cpus" "$(printf '%s\n' "$out" | grep -c '^Arena ')" "$(printf '%s\n' "$out" | grep '^PROBE')"
  done
}
# The system calls around one get_nprocs(): the probe calls it once.
nprocs_calls() {
  strace -qq -o "$TMPDIR/nprocs.trace" /opt/s286/arena-probe > /dev/null 2>&1 || true
  grep -n -A4 'cpu/online' "$TMPDIR/nprocs.trace" | head -12
}
# At the fix: the calls around the settle's own get_nprocs() in the row's
# built sleepers, and hello's whole count (the start-up gate's input).
fix_calls() {
  local d; d="$(wd "$1")/target/tmp/signal_gaps_lanes/cpu1051"
  strace -f -qq -o "$TMPDIR/sleepers-calls.trace" "$d/sleepers-native" || return $?
  grep -n -A4 'cpu/online' "$TMPDIR/sleepers-calls.trace" | head -12
  strace -qq -o "$TMPDIR/hello-calls.trace" "$d/hello-native" > /dev/null || return $?
  echo "hello-native system calls, execve to exit: $(wc -l < "$TMPDIR/hello-calls.trace")"
  grep -c 'cpu/online' "$TMPDIR/hello-calls.trace"
  return 0
}
arena_row() { cd "$(wd "$1")" && cargo test --locked -p wolf_rt --test arena_limit; }
cpu_row() {
  cd "$(wd "$1")" && cargo test --locked -p wolf_driver --test signal_gaps_lanes -- --exact \
    glibc_reads_the_cpu_count_only_before_any_worker_runs --nocapture
}
race_row() {
  local w=$1 n=$2 i rc bad=0
  cd "$(wd "$w")" || return 2
  for i in $(seq 1 "$n"); do
    cargo test --locked -q -p wolf_driver --test signal_gaps_lanes -- --exact \
      a_descriptor_wait_makes_no_descriptor_while_the_program_runs --nocapture > "$TMPDIR/row-$w-$i.log" 2>&1
    rc=$?
    grep -h 'REPORT race781' "$TMPDIR/row-$w-$i.log" | sed "s/^/run $i: /"
    echo "run $i: test exit=$rc"
    [ "$rc" -ne 0 ] && bad=$((bad+1))
  done
  echo "race781 row runs=$n red_runs=$bad affinity=$(taskset -pc $$ | sed 's/.*: //')"
  [ "$bad" -eq 0 ]
}
# The row's fixture, built by the head's own debug wolf (the binaries the
# row builds), then run with descriptor 1 closed under each CPU set, the
# row's own check on each run.
race_direct() {
  local w=$1 n=$2; shift 2
  local d="$TMPDIR/race-$w" fx="$(wd "$w")/crates/wolf_driver/tests/fixtures/signal_gaps/race781.lu" tier cpus i bad total=0
  mkdir -p "$d"
  if [ ! -x "$d/race-release" ]; then
    "$(wd "$w")/target/debug/wolf" build "$fx" -o "$d/race-native" --no-cache || return $?
    "$(wd "$w")/target/debug/wolf" build "$fx" -o "$d/race-release" --no-cache --release || return $?
  fi
  for cpus in "$@"; do
    for tier in native release; do
      bad=0
      for i in $(seq 1 "$n"); do
        rm -f "$d/o" "$d/o.waits"
        taskset.real -c "$cpus" sh -c 'exec 1>&-; exec "$1" "$2"' sh "$d/race-$tier" "$d/o" < /dev/null
        local rc=$? line; line=$(cat "$d/o" 2>/dev/null)
        local ok=${line##*ok }; ok=${ok%%[!0-9]*}
        local waits=${line#waits }; waits=${waits%% *}
        local ready=${line#* ready }; ready=${ready%%,*}
        local asks=${line#*asked }; asks=${asks%%,*}
        local verdict=green
        if [ "$rc" -ne 0 ] || [ -z "$ok" ] || [ "$ok" != 0 ] || [ "$waits" != "$ready" ] || [ "${waits:-0}" -lt 1000 ] || [ "${asks:-0}" -lt 1000 ]; then
          verdict=red; bad=$((bad+1))
        fi
        echo "race $tier cpus=$cpus run $i: exit=$rc $line $verdict"
      done
      echo "RACE head=${heads[$w]:0:8} tier=$tier cpus=$cpus runs=$n red=$bad"
      total=$((total+bad))
    done
  done
  [ "$total" -eq 0 ]
}
[ "$S286_MODE" = plant ] || step env env_step
case "$S286_MODE" in
  red)
    step build build_head 0
    step probe probe_step
    step nprocs-calls nprocs_calls
    step arena-row arena_row 0
    step cpu-row cpu_row 0
    step race-row race_row 0 10
    ;;
  green)
    step build build_head 0
    step arena-row arena_row 0
    step cpu-row cpu_row 0
    step fix-calls fix_calls 0
    step race-row race_row 0 3
    step race-direct-8 race_direct 0 100 0-3,8-11
    ;;
  full)
    # A no-op where the slot takes built these heads already (the same
    # mount paths); a build of its own where this take came first.
    step build-red build_head 0
    step build-green build_head 1
    step race-direct-red race_direct 0 10 0-5,8-13 0-15
    step race-direct-green race_direct 1 100 0-5,8-13 0-15
    step race-row-red race_row 0 1
    step race-row-green race_row 1 3
    ;;
  plant)
    # heads: GREEN PLANT (the plant checkout is a clone of the plant branch,
    # mounted like any head); the record's FAIL lines stay at column 0.
    cd "$(wd 1)" || exit 2
    exec bash /lane/rig/gauntlet-record.sh --output "/lane/records/plant-${heads[1]}.record" \
      --base "${heads[0]}" --plant a_descriptor_wait_makes_no_descriptor_while_the_program_runs \
      glibc_reads_the_cpu_count_only_before_any_worker_runs arena_limit \
      -- bash /lane/rig/plant-inner.sh "${heads[0]}" "${heads[1]}" \
      a_descriptor_wait_makes_no_descriptor_while_the_program_runs \
      glibc_reads_the_cpu_count_only_before_any_worker_runs arena_limit
    ;;
  *) echo "unknown mode $S286_MODE"; exit 2 ;;
esac
echo "INNER first_rc=$first_rc"
exit "$first_rc"
