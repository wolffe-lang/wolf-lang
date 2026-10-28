#!/usr/bin/env bash
# s270b: rustfmt the workspace and `wolf fmt` the four run rows, then show what moved
set -uo pipefail
source /lane/scripts/env.sh
cd /lane/wolf-lang
cargo fmt --all
for f in corpus/memory/region_outer_write_push_copy_call.lu corpus/memory/region_outer_write_push_copy_control.lu corpus/memory/region_outer_write_push_copy_over.lu corpus/memory/region_outer_write_push_copy_under.lu; do
  cp "$f" "/lane/scratch/tmp/$(basename "$f").before"
  target/debug/wolf fmt "$f"
  echo "== $f"
  diff "/lane/scratch/tmp/$(basename "$f").before" "$f"
done
target/debug/wolf fmt --check corpus/memory/region_outer_write_push_*.lu && echo "fmt-check clean"
cargo fmt --all --check && echo "rustfmt clean"
