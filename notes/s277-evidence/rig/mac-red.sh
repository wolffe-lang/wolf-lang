#!/bin/bash
# s277 mac job (run by build-at.sh in the build worktree, behind the lock): the
# lanes test at the red commit — every row expected red (E0301), the bit-32
# row red by its own answer.
set -u
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ)"
export LUPIN=/Users/mfwolffe/.local/bin/lupin
cargo test --locked -j4 -p wolf_driver --test descriptor_walk_lanes -- --nocapture --test-threads=2 2>&1
rc=$?
echo "END red-lanes exit=$rc"
exit 0
