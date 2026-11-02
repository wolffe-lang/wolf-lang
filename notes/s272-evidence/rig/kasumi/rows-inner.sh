#!/usr/bin/env bash
# s272: the statfs rows at one or more commits, inside r41-toolchain on
# kasumi's slot 8-11: for each <label> <sha>, the build, then the churn row and
# the plain row, each alone, one `END <step> exit=N` line each (a measurement:
# the red commit's churn row is expected nonzero).
# usage: rows-inner.sh [--all=N] <label> <sha> [<label> <sha> ...]
set -uo pipefail
source /lane/scripts/gate-env.sh
case "${1:-}" in --all=*) S272_ALL=${1#--all=}; shift ;; esac
cd /lane/wolf-lang || exit 2
printf 'S272_ROWS_START %s block=%s scratch=%s\n' "$(date -u +%FT%TZ)" "$S272_BLOCK" "$(stat -f -c %T /lane/wolf-lang/target/tmp 2>/dev/null || echo none)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status
pid1_uid=$(awk '/^Uid:/{print $2}' /proc/1/status)
printf 'pid1 uid=%s self uid=%s\n' "$pid1_uid" "$(id -u)"
test "$pid1_uid" != "$(id -u)" || { echo "S272_REFUSED running as pid 1's uid"; exit 2; }
rustc --version
stat --version | head -1
mkdir -p /lane/records/rows
while [ $# -ge 2 ]; do
  label=$1 sha=$2; shift 2
  git checkout -q --detach "$sha" && test "$(git rev-parse HEAD)" = "$sha" || { echo "END $label-checkout exit=2"; continue; }
  echo "== STEP $label-build $(date -u +%T) head=$(git rev-parse HEAD)"
  blog=/lane/records/rows/$label-build.log
  cargo build --locked -p wolf_driver -p wolf_rt > "$blog" 2>&1
  b=$?
  cargo test --locked -p wolf_driver --test os_surface_lanes --no-run >> "$blog" 2>&1
  t=$?
  tail -3 "$blog"
  echo "END $label-build exit=$((b | t))"
  for row in fs_statfs_holds_while_a_writer_churns fs_statfs_matches_gnu_stat_f; do
    log=/lane/records/rows/$label-$row-$(stat -f -c %T /lane/wolf-lang/target/tmp).log
    echo "== STEP $label-$row $(date -u +%T)"
    cargo test --locked -p wolf_driver --test os_surface_lanes "$row" -- --nocapture --exact > "$log" 2>&1
    rc=$?
    grep -E "^SKIP|REPORT statfs paths|REPORT the churn|agreed in triple|attempt [0-9]+ outside|panicked at|: (free|available|free_files|blocks|1024-blocks|iused|ifree) wolf=|no triple of|test result" "$log" | head -40
    echo "END $label-$row exit=$rc"
  done
  # --all=N: os_surface_lanes whole, N times (the rows beside the pty
  # and container rows, as the gauntlet runs them).
  for i in $(seq 1 "${S272_ALL:-0}"); do
    log=/lane/records/rows/$label-all-$i-$(stat -f -c %T /lane/wolf-lang/target/tmp).log
    echo "== STEP $label-all-$i $(date -u +%T)"
    cargo test --locked -p wolf_driver --test os_surface_lanes -- --nocapture > "$log" 2>&1
    rc=$?
    grep -E "^SKIP|agreed in triple [1-9]|no triple of|panicked at|^test .* (ok|FAILED)$|test result" "$log" | head -30
    echo "END $label-all-$i exit=$rc"
  done
done
printf 'S272_ROWS_END %s\n' "$(date -u +%FT%TZ)"
