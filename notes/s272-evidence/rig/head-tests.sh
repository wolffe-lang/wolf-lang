#!/usr/bin/env bash
# s272: targeted rows at the checkout's head, inside r41-toolchain on block A.
# Every step runs and prints `END <step> exit=N` (a targeted run, not the gauntlet).
source /lane/scripts/env.sh
cd /lane/wolf-lang || exit 1
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn' /proc/self/status; id -u
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step build cargo build --locked -p wolf_driver -p wolf_rt
step rt-unit cargo test --locked -p wolf_rt --lib -- statfs procattr term::tests signal::tests::the_action_query
step sema cargo test --locked -p wolf_sema --test prelude_names --test sandbox_s272 --test host_table_spec
step drv-tables cargo test --locked -p wolf_driver --test str_list_fs_native --test freestanding_target
step drv-cap cargo test --locked -p wolf_driver --bin wolf builtin_cap
step lanes cargo test --locked -p wolf_driver --test os_surface_lanes -- --nocapture --test-threads=2
exit $rc_all
