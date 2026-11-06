#!/usr/bin/env bash
# s284 job "rows": wolf_rt's lib tests at <sha> (a fresh worktree and target),
# under four of the block's CPUs, in r41-toolchain:1:
#   rows-build   cargo test --no-run (the test binary)
#   s284-rows    the two s284_ rows, each alone (`--exact`); a red row prints
#                an unindented `FAIL <row>` line inside this nonzero step
#   signal-suite every signal::tests row (s275's included)
#   reactor-suite every reactor::tests row
#   fork-suite   every fork:: and task:: row of the lib (the hold's neighbours)
# Every step prints END <step> exit=N; the job goes on past a red step so a
# red commit shows every row's verdict.
# usage: rows.sh <A|B> <sha>
set -uo pipefail
L=/home/mfwolffe/lanes/s284
blk=${1:?block}
sha=${2:?sha}
case $blk in A) SIX=0-5 FOUR=0-3 ;; B) SIX=6-11 FOUR=6-9 ;; *) echo "block must be A or B"; exit 2 ;; esac
W=$L/r-$sha
C="bash $L/rig/ctr.sh"
echo "LOCKS_ACQUIRED $(date -u +%FT%TZ) block=$blk six=$SIX four=$FOUR sha=$sha kernel=$(uname -r)"
[ -d "$W" ] || git -C "$L/wolf-lang" worktree add -q --detach "$W" "$sha" || { echo "REFUSED worktree"; exit 2; }
test "$(git -C "$W" rev-parse HEAD)" = "$sha" || { echo "REFUSED head"; exit 2; }
git -C "$W" diff --quiet || { echo "REFUSED dirty"; exit 2; }
if [ -e "$W/target" ]; then echo "REFUSED: $W/target exists, not a fresh target for this head"; exit 2; fi
step() {
    name=$1
    shift
    echo "START $name $(date -u +%FT%TZ)"
    "$@" 2>&1 | sed 's/^/  /'
    rc=${PIPESTATUS[0]}
    echo "END $name exit=$rc $(date -u +%FT%TZ)"
    return "$rc"
}
step rows-build $C net "$W" taskset.real -c "$SIX" cargo test --locked -p wolf_rt --lib --no-run || exit 3
rows="signal::tests::s284_a_release_in_the_claims_gap_never_lets_a_fork_claim_a_working_drain_thread reactor::tests::s284_a_release_in_the_claims_gap_never_lets_a_fork_claim_a_working_reactor_thread"
echo "START s284-rows $(date -u +%FT%TZ)"
worst=0
for row in $rows; do
    log=$L/tmp/row-$sha-${row##*::}.log
    $C nonet "$W" taskset.real -c "$FOUR" cargo test --locked --offline -p wolf_rt --lib "$row" -- --exact --nocapture --test-threads=1 >"$log" 2>&1
    rc=$?
    sed 's/^/  /' "$log"
    if [ "$rc" -ne 0 ]; then
        why=$(grep -m1 -A2 'panicked at' "$log" | tail -2 | tr '\n' ' ' | cut -c1-240)
        echo "FAIL $row exit=$rc: $why"
        worst=$rc
    else
        echo "  PASS $row"
    fi
done
echo "END s284-rows exit=$worst $(date -u +%FT%TZ)"
step signal-suite $C nonet "$W" taskset.real -c "$FOUR" cargo test --locked --offline -p wolf_rt --lib signal::tests
step reactor-suite $C nonet "$W" taskset.real -c "$FOUR" cargo test --locked --offline -p wolf_rt --lib reactor::tests
step fork-suite $C nonet "$W" taskset.real -c "$FOUR" cargo test --locked --offline -p wolf_rt --lib fork
echo "ROWS_END $sha $(date -u +%FT%TZ)"
