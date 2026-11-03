#!/bin/bash
# s277 mac job 1 (run by build-at.sh in the build worktree, behind the lock):
# the base compiler on the strengthened bit-32 row, then the targeted rows at
# the head. Every step runs and prints `END <step> exit=N`.
set -u
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
E=/Users/mfwolffe/lanes/s277/ev/base-row
rm -rf "$E"; mkdir -p "$E"
base_row() {
  cd "$E" && /Users/mfwolffe/lanes/s277/ev/base-wolf/wolf-9f569214 conform-run /Users/mfwolffe/lanes/s277/build/corpus/fs/open_directory.lu --checked --json; local rc=$?; cd /Users/mfwolffe/lanes/s277/build; return $rc
}
step base-bit32-row base_row
export LUPIN=/Users/mfwolffe/.local/bin/lupin
step build cargo build --locked -j4 -p wolf_driver -p wolf_rt
step rt-unit cargo test --locked -j4 -p wolf_rt --lib -- fs_at fs_gaps fs::tests
step sema cargo test --locked -j4 -p wolf_sema --test prelude_names --test sandbox_s277
step drv-tables cargo test --locked -j4 -p wolf_driver --test str_list_fs_native --test freestanding_target
step drv-cap cargo test --locked -j4 -p wolf_driver --bin wolf builtin_cap
step lanes cargo test --locked -j4 -p wolf_driver --test descriptor_walk_lanes -- --nocapture --test-threads=2
step gaps-lanes cargo test --locked -j4 -p wolf_driver --test fs_gaps_lanes -- --test-threads=2
step clippy cargo clippy --locked -j4 -p wolf_rt -p wolf_mem -p wolf_sema -p wolf_wir -p wolf_driver -p wolf_codegen_clif --all-targets -- -D warnings
step check-windows cargo check --locked -j4 -p wolf_rt -p wolf_mem --target x86_64-pc-windows-msvc
step check-freebsd cargo check --locked -j4 -p wolf_rt -p wolf_mem --target x86_64-unknown-freebsd
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
