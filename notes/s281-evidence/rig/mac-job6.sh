#!/bin/bash
# s281 mac job 6 (run by build-at.sh in the build worktree, behind the lock):
# at the head with the parse-tree snapshots. insta never writes here
# (INSTA_UPDATE=no) and nothing stops at the first red binary (--no-fail-fast):
# the whole wolf_parse suite (every corpus-fed snapshot set it has); wolf_driver's
# corpus-walking binaries and the four-machine rows; and every crate the hasu
# record's test step never reached after it stopped at wolf_parse's
# ambiguity_trees (wolf_pkg, wolf_query, wolf_rt, wolf_sema, wolf_span,
# wolf_wir, xtask).
set -u
L=/Users/mfwolffe/lanes/s281
export LUPIN=/Users/mfwolffe/.local/bin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1 INSTA_UPDATE=no
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V'
step build cargo build --locked -p wolf_driver -p wolf_rt
step wolf-parse cargo test --locked -p wolf_parse --no-fail-fast
step wolf-driver-corpus cargo test --locked -p wolf_driver --no-fail-fast --test bare_entry --test checked_tasks_lanes --test debug_native --test fmt_refusals --test fs_change_lanes --test ids_lanes --test incremental --test int_truth_lanes --test json_dup_lanes --test stale_global_lanes --test str_list_fs_native --test four_machine_regressions --test release_native
step unreached cargo test --locked --no-fail-fast -p wolf_pkg -p wolf_query -p wolf_rt -p wolf_sema -p wolf_span -p wolf_wir -p xtask
echo "untracked or changed: $(git status --porcelain --untracked-files=all | wc -l)"; git status --porcelain --untracked-files=all | head
echo "== done $(date -u +%FT%TZ)"
