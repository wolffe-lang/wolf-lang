#!/bin/bash
# s267: fmt, clippy (deny warnings) and the codegen crate's shim tests at the head, in ctr-g.
set -u
cd /home/mfwolffe/lanes/s267/wolf-lang
echo "lint head=$(git rev-parse HEAD) start=$(date -u +%FT%TZ)"
cargo fmt --all -- --check; f=$?; echo "fmt=$f"
cargo clippy --locked --workspace --all-targets -- -D warnings > /home/mfwolffe/lanes/s267/evidence/clippy.log 2>&1; c=$?
grep -E '^(error|warning)' /home/mfwolffe/lanes/s267/evidence/clippy.log | head -20; echo "clippy=$c"
cargo test --locked -p wolf_codegen_clif --test entry_shim --test exec_smoke 2>&1 | grep -E '^test |test result'; t=${PIPESTATUS[0]}; echo "shim-tests=$t"
[ $f = 0 ] && [ $c = 0 ] && [ $t = 0 ]
