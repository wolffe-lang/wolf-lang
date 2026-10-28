#!/usr/bin/env bash
# s270b plant: the gate rows on the plant checkout (/lane/plant), red by name expected.
# The plant is the line's code head with ONE deliberate break: the plain push's copy site is
# minted in the list's own region (where s255 put the growth) instead of the ambient region.
set -uo pipefail
source /lane/scripts/env.sh
export WOLF_PAIRING_REQUIRE_SIBLING=1 RUST_TEST_NOCAPTURE=1
cd /lane/plant
printf 's270b PLANT %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git log --oneline -3
cargo fmt --all --check > /lane/logs/plant-fmt.log 2>&1
printf 'PLANT rustfmt rc=%s\n' "$?"
cargo build --locked -q -p wolf_driver -p wolf_rt 2>&1 | tail -3
cargo test -p wolf_driver --test region_outer_write_lanes -- --test-threads=2 > /lane/logs/plant-outer-write.log 2>&1
printf 'PLANT outer-write rc=%s\n' "$?"
grep -E '^test .*(ok|FAILED)$|panicked at|test result' /lane/logs/plant-outer-write.log
cargo xtask corpus > /lane/logs/plant-corpus.log 2>&1
printf 'PLANT corpus rc=%s\n' "$?"
grep -E 'region_outer_write_push' /lane/logs/plant-corpus.log | grep -v '\[phase' | head -20
tail -5 /lane/logs/plant-corpus.log
printf 's270b PLANT DONE %s\n' "$(date -u +%FT%TZ)"
