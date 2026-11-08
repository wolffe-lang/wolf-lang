#!/bin/bash
# s287: the tests that walk the whole corpus outside wolf_driver, every test
# of the front-end crates that read it (wolf_fmt's corpus identity and
# properties, wolf_lex, wolf_parse, wolf_wir), with insta's update off and
# no fail-fast, so one red binary does not hide the next. The gauntlet's
# test step runs them; the Mac checks did not, and missed a row the
# formatter rewrites (wolf_fmt --test properties).
set -u
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
echo "tree of $(cat .s287-sha 2>/dev/null || git rev-parse HEAD)"
step fmt-properties env INSTA_UPDATE=no cargo test --locked -p wolf_fmt --test properties
step corpus-walkers env INSTA_UPDATE=no cargo test --locked --no-fail-fast -p wolf_fmt -p wolf_lex -p wolf_parse -p wolf_wir
echo "== walkers done $(date -u +%FT%TZ)"
