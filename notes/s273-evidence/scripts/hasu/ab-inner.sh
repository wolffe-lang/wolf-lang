#!/usr/bin/env bash
# s273: one row, trunk against the head, interleaved five times each, inside the gauntlet container (S273_TEST_BIN, S273_TEST_NAME, S273_TEST_GREP).
# inside the gauntlet's container on block A (the orchestrator's step 1).
set -uo pipefail
S273_TEST_BIN=${1:?test binary}
S273_TEST_NAME=${2:?test name}
S273_TEST_GREP=${3:?report grep}
S273_RUNS=${4:-5}
S273_FIRST=${5:-trunk}
source /lane/scripts/gate-env.sh
T=/lane/wolf-lang-trunk
H=/lane/wolf-lang
test "$(git -C "$T" rev-parse HEAD)" = b16353e8ae616ff3a35bd27adc56b727c50df345 || { echo "S273_AB trunk tree is not b16353e8"; exit 2; }
test "$(git -C "$H" rev-parse HEAD)" = d07e868742826d346f354b3b7152781eb6650df5 || { echo "S273_AB head tree is not d07e8687"; exit 2; }
for tree in "$T" "$H"; do
  printf 'S273_AB_BUILD %s %s\n' "$(date -u +%FT%TZ)" "$tree"
  (cd "$tree" && cargo build --locked -p wolf_driver -p wolf_rt && cargo test --locked --no-run -p wolf_driver --test ${S273_TEST_BIN}) || { echo "S273_AB build failed in $tree"; exit 3; }
done
for i in $(seq 1 ${S273_RUNS:-5}); do
  if [ "$S273_FIRST" = head ]; then order="head:$H trunk:$T"; else order="trunk:$T head:$H"; fi
  for pair in $order; do
    name=${pair%%:*}
    tree=${pair#*:}
    out=$(cd "$tree" && cargo test --locked -q -p wolf_driver --test ${S273_TEST_BIN} -- --exact ${S273_TEST_NAME} --nocapture 2>&1)
    rc=$?
    printf 'S273_AB run=%s tree=%s exit=%s %s\n' "$i" "$name" "$rc" "$(date -u +%FT%TZ)"
    printf '%s\n' "$out" | grep -E "${S273_TEST_GREP}|test result" | sed "s/^/  [$name $i] /"
  done
done
printf 'S273_AB_DONE %s\n' "$(date -u +%FT%TZ)"
