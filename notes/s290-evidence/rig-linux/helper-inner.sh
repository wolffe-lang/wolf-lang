#!/usr/bin/env bash
# s290, inside helper-take.sh's container: `test_cpus_helper` in /trunk, then in
# /work (the head), each in its own checkout and target, each recorded. The
# image's translator logs any pin it moves to /work/notes/s263-evidence/; the
# count is printed after each run (0: every pin stayed where it asked).
set -uo pipefail
export PATH=/lane/tool-bin:/opt/s263-rust/bin:$PATH
export CARGO_HOME=/lane/cargo-home RUSTUP_HOME=/opt/s263-rustup RUSTUP_TOOLCHAIN=1.97.1
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=4 TMPDIR=/lane/tmp HOME=/lane/home
unset WOLF_STD CARGO_TARGET_DIR WOLF_TEST_CPUS
tlog=/work/notes/s263-evidence/taskset-translations.log
if [ "${1:-}" = steps ]; then
  step() { name=$1; shift; started=$SECONDS; printf 'START %s\n' "$name"; "$@" 2>&1 | sed 's/^/  /'; rc=${PIPESTATUS[0]}; printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"; return "$rc"; }
  step env bash -c 'printf "rig=%s affinity=%s kernel=%s pid1=%s head=%s\n" "${RIG_DESC:-unknown}" "$(taskset.real -pc $$ | sed "s/.*: //")" "$(uname -r)" "$(cat /proc/1/comm)" "$(git rev-parse HEAD)"; head -n 1 /usr/bin/taskset; sed -n "/^ALLOWED/p" /usr/lib/s263-taskset-translate.py' || exit $?
  before=$(cat "$tlog" 2>/dev/null | wc -l)
  step test-cpus-helper cargo test --locked -p wolf_driver --test test_cpus_helper -- --nocapture --test-threads=1 || rc=$?
  after=$(cat "$tlog" 2>/dev/null | wc -l)
  step translations bash -c "echo \"pins the translator moved during this run: $((after - before))\"; test $((after - before)) -eq 0" || rc=${rc:-$?}
  exit "${rc:-0}"
fi
ts=$1; hs=$2
worst=0
for pair in "/trunk $ts" "/work $hs"; do
  set -- $pair
  cd "$1" || exit 1
  test "$(git rev-parse HEAD)" = "$2" || exit 1
  bash /lane/rig/gauntlet-record.sh --output "/lane/records/helper-$2.record" -- bash /lane/rig/helper-inner.sh steps
  rc=$?; [ "$rc" -gt "$worst" ] && worst=$rc
done
exit "$worst"
