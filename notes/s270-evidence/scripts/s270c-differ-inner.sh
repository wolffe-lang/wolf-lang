set -uo pipefail
source /lane/scripts/s270-env.sh
export WOLF_STD=/lane/wolf-std/std
cd /lane/wolf-lang
printf 's270c DIFFER START %s head=%s\n' "$(date -u +%FT%TZ)" "$(git rev-parse HEAD)"
git status --short
cargo build --locked -q -p wolf_driver -p wolf_rt
printf 'HEAD_BUILD_EXIT %s\n' "$?"
(cd /lane/wolf-base && cargo build --locked -q -p wolf_driver -p wolf_rt)
printf 'BASE_BUILD_EXIT %s\n' "$?"
/lane/wolf-lang/target/debug/wolf --version | head -1
/lane/wolf-base/target/debug/wolf --version | head -1
grep -c 'wolf-lang#940' /lane/wolf-lang/crates/wolf_wir/src/midend/interproc.rs
grep -c 'wolf-lang#940' /lane/wolf-base/crates/wolf_wir/src/midend/interproc.rs
for tier in checked native release; do
 export S270C_TIER=$tier
 export S270C_PROTOCOL_LOG=/lane/records/s270c-differ-$tier.jsonl
 rm -f "$S270C_PROTOCOL_LOG"
 cargo xtask differ /lane/scripts/s270c-proxy-A /lane/scripts/s270c-proxy-B > "/lane/logs/s270c-differ-$tier.log" 2>&1
 rc=$?
 echo "$rc" > "/lane/records/s270c-differ-$tier.exit"
 printf 'DIFFER %s exit %s at %s\n' "$tier" "$rc" "$(date -u +%FT%TZ)"
 grep -E '^differ: [0-9]+ file|run-rung coverage' "/lane/logs/s270c-differ-$tier.log"
done
python3 -I /lane/scripts/s270c-differ-compare.py /lane/records checked native release
printf 's270c DIFFER END %s\n' "$(date -u +%FT%TZ)"
