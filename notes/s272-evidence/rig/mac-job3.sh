#!/bin/bash
# s272 mac job 3 (run by build-at.sh in the build worktree, behind the Mac
# lock): the targeted rows at the head, clippy and the cross-target checks
# under the pinned 1.97.1 (rustup's proxies first on PATH for those).
set -u
B=/Users/mfwolffe/lanes/s272/build
cd "$B" || exit 1
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
R=$HOME/.cargo/bin
step build cargo build -j4 -p wolf_driver --bin wolf -p xtask
step cargo-fmt env PATH=$R:$PATH cargo fmt --all --check
step spec-extract-check cargo xtask spec-extract --check
step rt-unit cargo test -j4 -p wolf_rt --lib -- statfs procattr term::tests signal::tests::the_action_query
step sema cargo test -j4 -p wolf_sema --test prelude_names --test sandbox_s272 --test host_table_spec
step lex-parse-wir cargo test -j4 -p wolf_lex --test corpus_snapshots -p wolf_parse --test corpus_decls -p wolf_wir --test lower_corpus
step drv-tables cargo test -j4 -p wolf_driver --test str_list_fs_native --test freestanding_target --test prelude_list
step drv-cap cargo test -j4 -p wolf_driver --bin wolf builtin_cap
step lanes env LUPIN=/Users/mfwolffe/.local/bin/lupin cargo test -j4 -p wolf_driver --test os_surface_lanes -- --nocapture --test-threads=2
step clippy env PATH=$R:$PATH cargo clippy -j4 -p wolf_rt -p wolf_mem -p wolf_sema -p wolf_wir -p wolf_driver -p wolf_codegen_clif --all-targets -- -D warnings
step check-windows-rt env PATH=$R:$PATH cargo check -j4 -p wolf_rt --target x86_64-pc-windows-msvc
step check-freebsd env PATH=$R:$PATH cargo check -j4 -p wolf_rt -p wolf_mem --target x86_64-unknown-freebsd
step check-linux env PATH=$R:$PATH cargo check -j4 -p wolf_rt -p wolf_mem --target x86_64-unknown-linux-gnu
echo "clean: $(git status --porcelain | wc -l) paths"
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
