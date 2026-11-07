#!/bin/bash
# s285: each issue probe's verbatim answer on every tier: the rendered stderr
# and the conform-run record, for the trunk binary, the branch binary and
# lupin 0.1.59.
set -u
L=$LANE
LUPIN=$L/inputs/lupin060/unpacked/lupin-0.1.60-aarch64-apple-darwin/lupin
for f in "$@"; do
  echo "######## $(basename "$(dirname "$f")")"; cat "$f"
  for w in "$L/base-bin/wolf:trunk" "$L/records/wolf-ceb23098/wolf:branch"; do
    for m in --checked --native --release; do
      echo "=== wolf(${w##*:}) conform-run --json $m"
      "${w%%:*}" conform-run --json $m "$f" 2>"$L/tmp/vb.err"; echo; sed -n '1,12p' "$L/tmp/vb.err"
    done
  done
  echo "=== lupin conform-run --json"; "$LUPIN" conform-run --json "$f" 2>"$L/tmp/vb.err"; echo; sed -n '1,12p' "$L/tmp/vb.err"
  echo "=== lupin run"; "$LUPIN" run "$f"; echo "(exit $?)"
done
