#!/usr/bin/env bash
# s277: the plant's steps, run BY the org's gauntlet recorder (a copy) in the
# plant worktree on its branch, behind the Mac lock (taken outside the
# record). Every line is inside a START/END step; the rows step is expected
# red, by name (`FAIL race_handle_walk`, `FAIL open_at_nofollow`).
set -uo pipefail
source /Users/mfwolffe/lanes/s277/rig/gauntlet-record.sh
export PATH=$HOME/.cargo/bin:$PATH CARGO_INCREMENTAL=0 LUPIN=/Users/mfwolffe/.local/bin/lupin
head_ok() { git rev-parse HEAD && git symbolic-ref --short HEAD && git log --oneline -2 && git diff --stat HEAD~1 HEAD; }
gauntlet_step plant-head head_ok || exit $?
gauntlet_step plant-build cargo build --locked -j4 -p wolf_driver -p wolf_rt || exit $?
gauntlet_step plant-rows cargo test --locked -j4 -p wolf_driver --test descriptor_walk_lanes -- --nocapture --test-threads=1 the_race_row every_refusal_by_name || exit $?
