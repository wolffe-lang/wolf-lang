#!/usr/bin/env bash
# s277: compile checks and the runtime's unit rows at a commit, linux gnu and
# musl, inside r41-toolchain. Every step prints its END; all steps run.
set -u
source /lane/rig/env.sh
want=${1:?sha}
cd /lane/wolf-lang || exit 2
test "$(git rev-parse HEAD)" = "$(git rev-parse "$want")" || { echo "head is not $want"; exit 2; }
echo "head $(git rev-parse HEAD) cpus $(grep Cpus_allowed_list /proc/self/status)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step check-gnu cargo check --locked -p wolf_rt -p wolf_mem -p wolf_sema -p wolf_wir -p wolf_codegen_clif -p wolf_driver --all-targets
step check-musl cargo check --locked -p wolf_rt -p wolf_mem --target x86_64-unknown-linux-musl
step rt-unit cargo test --locked -p wolf_rt --lib fs_at
step clippy cargo clippy --locked -p wolf_rt -p wolf_mem -p wolf_sema -p wolf_wir -p wolf_codegen_clif -p wolf_driver --all-targets -- -D warnings
echo "ALL rc=$rc_all"
exit $rc_all
