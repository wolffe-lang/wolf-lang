#!/bin/bash
# s287: regenerate spec/anchors.json at a commit (by build-at.sh, behind the
# lock), copy it out to $LANE/tmp/anchors-<sha>.json, diff the key sets both
# ways against the commit's own, and put the worktree back clean.
set -u
SHA=$(git rev-parse --short=8 HEAD)
cp spec/anchors.json "$LANE/tmp/anchors-before-$SHA.json"
cargo xtask spec-extract || exit 1
cp spec/anchors.json "$LANE/tmp/anchors-$SHA.json"
python3 -I - "$LANE/tmp/anchors-before-$SHA.json" "$LANE/tmp/anchors-$SHA.json" <<'PY'
import json, sys
a = set(json.load(open(sys.argv[1]))["anchors"]); b = set(json.load(open(sys.argv[2]))["anchors"])
print(f"anchors: {len(a)} -> {len(b)}; added {sorted(b - a)}; removed {sorted(a - b)}")
sys.exit(1 if a - b else 0)
PY
rc=$?
git checkout -q -- spec/anchors.json
exit $rc
