#!/bin/bash
# s281 mac job 4 (run by build-at.sh in the build worktree, behind the lock):
# the head before the note. Every probe of the base measurement again (the
# sweep's answers after the fix); fmt; clippy on the two crates the lane
# changed; the corpus snapshots without an update; wolf_wir's own tests; the
# release-parity row over the whole run-phase corpus.
set -u
L=/Users/mfwolffe/lanes/s281
export LUPIN=/Users/mfwolffe/.local/bin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
head_info() { git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V && "$LUPIN" --version; }
step head head_info
step build cargo build --locked -p wolf_driver -p wolf_rt
step probes-head "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "$L"/probe/*/main.lu
step fmt cargo fmt --all --check
step clippy cargo clippy --locked -p wolf_wir -p wolf_driver --all-targets -- -D warnings
step snap-lex cargo test --locked -p wolf_lex --test corpus_snapshots
step snap-parse cargo test --locked -p wolf_parse --test corpus_decls
step wolf-wir cargo test --locked -p wolf_wir
step release-parity cargo test --locked -p wolf_driver --test release_native -- --test-threads=1 release_tier_matches_debug_tier_on_the_corpus
echo "clean: $(git status --porcelain --untracked-files=no | wc -l)"
echo "== done $(date -u +%FT%TZ)"
