#!/bin/bash
# s272 mac job 2 (run by build-at.sh inside the build worktree, behind the
# Mac lock): the regenerated files (anchors, prelude.json, the corpus
# snapshots, wolf fmt's form of the new rows) copied out, then the targeted
# rows. Every step runs and prints `END <step> exit=N`; the worktree is put
# back clean at the end.
set -u
B=/Users/mfwolffe/lanes/s272/build
OUT=/Users/mfwolffe/lanes/s272/ev/mac2
rm -rf "$OUT"; mkdir -p "$OUT/snaps" "$OUT/fmt"
cd "$B" || exit 1
echo "head $(git rev-parse HEAD) $(date -u +%FT%TZ)"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step build cargo build -j4 -p wolf_driver --bin wolf -p xtask
step cargo-fmt env PATH=$HOME/.cargo/bin:$PATH cargo fmt --all --check
step spec-extract cargo xtask spec-extract
cp spec/anchors.json "$OUT/anchors.json"
step prelude-json sh -c 'target/debug/wolf prelude --json > '"$OUT"'/prelude.json'
cp "$OUT/prelude.json" spec/prelude.json
for f in corpus/fs/statfs_rows.lu corpus/os/chroot_rows.lu corpus/os/priority_rows.lu corpus/os/signal_action_rows.lu corpus/os/term_lf_rows.lu crates/wolf_driver/tests/fixtures/os_surface/*.lu; do
  d="$OUT/fmt/$(echo "$f" | tr / _)"
  cp "$f" "$d"
  target/debug/wolf fmt "$d" > /dev/null 2>&1
  if cmp -s "$f" "$d"; then echo "fmt-same $f"; else echo "fmt-differs $f"; diff -u "$f" "$d" | head -40; fi
done
step snap-lex env INSTA_UPDATE=always cargo test -j4 -p wolf_lex --test corpus_snapshots
step snap-parse env INSTA_UPDATE=always cargo test -j4 -p wolf_parse --test corpus_decls
step snap-wir env INSTA_UPDATE=always cargo test -j4 -p wolf_wir --test lower_corpus
git status --porcelain > "$OUT/status.txt"
git status --porcelain | awk '{print $2}' | grep '\.snap$' | while read -r f; do mkdir -p "$OUT/snaps/$(dirname "$f")"; cp "$f" "$OUT/snaps/$f"; done
step rt-unit cargo test -j4 -p wolf_rt --lib -- statfs procattr term::tests signal::tests::the_action_query
step sema cargo test -j4 -p wolf_sema --test prelude_names --test sandbox_s272 --test host_table_spec
step drv-tables cargo test -j4 -p wolf_driver --test str_list_fs_native --test freestanding_target --test prelude_list
step drv-cap cargo test -j4 -p wolf_driver --bin wolf builtin_cap
step lanes env LUPIN=/Users/mfwolffe/.local/bin/lupin cargo test -j4 -p wolf_driver --test os_surface_lanes -- --nocapture --test-threads=2
step clippy env PATH=$HOME/.cargo/bin:$PATH cargo clippy -j4 -p wolf_rt -p wolf_mem -p wolf_sema -p wolf_wir -p wolf_driver -p wolf_codegen_clif --all-targets -- -D warnings
step check-windows env PATH=$HOME/.cargo/bin:$PATH cargo check -j4 -p wolf_rt -p wolf_mem --target x86_64-pc-windows-msvc
step check-freebsd env PATH=$HOME/.cargo/bin:$PATH cargo check -j4 -p wolf_rt -p wolf_mem --target x86_64-unknown-freebsd
step check-linux env PATH=$HOME/.cargo/bin:$PATH cargo check -j4 -p wolf_rt -p wolf_mem --target x86_64-unknown-linux-gnu
git checkout -q -- . && git clean -fdq
echo "clean: $(git status --porcelain | wc -l) paths"
echo "ALL rc=$rc_all $(date -u +%FT%TZ)"
exit $rc_all
