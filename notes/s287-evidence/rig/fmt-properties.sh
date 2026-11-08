#!/bin/bash
# s287: wolf_fmt's corpus identity and properties alone, at a commit.
set -u
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
echo "tree of $(cat .s287-sha 2>/dev/null || git rev-parse HEAD)"
step fmt-properties env INSTA_UPDATE=no cargo test --locked -p wolf_fmt --test properties
