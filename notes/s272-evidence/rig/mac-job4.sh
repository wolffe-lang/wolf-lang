#!/bin/bash
# s272 mac job 4 (run by build-at.sh in the build worktree, behind the Mac
# lock): the rows CI went red on and every row of this lane that runs on
# lupin, with the pairing's lupin required — before the push.
set -u
export PATH=$HOME/.cargo/bin:$PATH
B=/Users/mfwolffe/lanes/s272/build
cd "$B" || exit 1
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ) $(rustc --version)"
export LUPIN=/Users/mfwolffe/.local/bin/lupin WOLF_PAIRING_REQUIRE_SIBLING=1
echo "lupin: $($LUPIN --version)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step build cargo build -j4 -p wolf_driver --bin wolf
step cargo-fmt cargo fmt --all --check
step editor-calls cargo test -j4 -p wolf_driver --test editor_calls_lanes -- --nocapture --test-threads=2
step os-surface cargo test -j4 -p wolf_driver --test os_surface_lanes -- --nocapture --test-threads=2
step term-editor-rows cargo test -j4 -p wolf_driver --test term_editor_lanes the_terminal_rows_on_every_machine -- --nocapture --exact
step lex-snapshots cargo test -j4 -p wolf_lex --test corpus_snapshots
step clippy-driver cargo clippy -j4 -p wolf_driver --all-targets -- -D warnings
echo "clean: $(git status --porcelain | wc -l) paths"
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
