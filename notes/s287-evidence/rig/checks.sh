#!/bin/bash
# s287: the lane's own checks at a commit: fmt, clippy on the crates it
# touches (all targets, -D warnings), the wolf_sema and wolf_diag suites, the
# wolf_driver lanes test, the catalogs' checks, the anchors gate, the corpus,
# the corpus walkers (wolf_fmt's corpus identity among them) and conformance. Every step prints its END line; the take does not stop.
set -u
export LUPIN=$LANE/inputs/lupin060/unpacked/lupin-0.1.60-aarch64-apple-darwin/lupin
export WOLF_PAIRING_REQUIRE_SIBLING=1 WOLF_TEST_NATIVE_REQUIRE=1
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; return $rc; }
echo "tree of $(cat .s287-sha 2>/dev/null || git rev-parse HEAD)"
step fmt cargo fmt --all --check
step clippy cargo clippy --locked -p wolf_diag -p wolf_sema -p wolf_driver --all-targets -- -D warnings
step build cargo build --locked -p wolf_driver -p wolf_rt
step diag-tests cargo test --locked -p wolf_diag
step sema-tests env INSTA_UPDATE=no cargo test --locked -p wolf_sema
step lanes-test cargo test --locked -p wolf_driver --test front_end_rulings_lanes -- --test-threads=1
step diag-catalog-check cargo xtask diag-catalog --check
step doc-catalog-check cargo xtask doc-catalog --check
step anchors cargo xtask spec-extract --check
step corpus cargo xtask corpus
# The corpus walkers outside wolf_driver (see walkers.sh): the gauntlet's
# test step runs them, and these checks once missed a row the formatter
# rewrites because they did not.
step fmt-properties env INSTA_UPDATE=no cargo test --locked -p wolf_fmt --test properties
step corpus-walkers env INSTA_UPDATE=no cargo test --locked --no-fail-fast -p wolf_fmt -p wolf_lex -p wolf_parse -p wolf_wir
step conformance cargo xtask conformance
echo "== done $(date -u +%FT%TZ)"
