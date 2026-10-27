set -uo pipefail
lane=/home/mfwolffe/lanes/s268
echo "s268 source cd60d930 (code b0b6c72a); started $(date -u +%FT%TZ)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn|NoNewPrivs' /proc/self/status
p1=$(awk '/^Uid:/{print $2}' /proc/1/status)
echo "pid1 uid=$p1 self=$(id -u)"
[ "$p1" != "$(id -u)" ] || exit 3
sha256sum crates/wolf_codegen_clif/src/dense.rs crates/wolf_codegen_clif/src/translate.rs crates/wolf_codegen_clif/tests/dense_match.rs crates/wolf_driver/tests/dense_match_lanes.rs crates/wolf_wir/tests/token_inline.rs > "$lane/evidence/verified-source.sha256"
sha256sum /opt/s268-lupin /opt/s268-musl-rt.a
sha256sum /opt/s268-lupin /opt/s268-musl-rt.a
cargo xtask ci > "$lane/evidence/gauntlet.log" 2>&1
gauntlet=$?
printf '%s\n' "$gauntlet" > "$lane/evidence/gauntlet.exit"
echo "gauntlet exit=$gauntlet $(date -u +%FT%TZ)"
worst=$gauntlet
for tier in checked native release; do
    unset WOLF_CHECKED WOLF_NATIVE WOLF_RELEASE
    case "$tier" in checked) export WOLF_CHECKED=1 ;; native) export WOLF_NATIVE=1 ;; release) export WOLF_RELEASE=1 ;; esac
    target/debug/xtask differ "$lane/wolf-lang/target/debug/wolf" "$lane/baseline/wolf" > "$lane/evidence/differ-$tier.log" 2>&1
    result=$?
    printf '%s\n' "$result" > "$lane/evidence/differ-$tier.exit"
    echo "differ $tier exit=$result $(date -u +%FT%TZ)"
    if [ "$result" -ne 0 ]; then worst=1; fi
done
unset WOLF_CHECKED WOLF_NATIVE WOLF_RELEASE
cargo build --release -p wolf_rt > "$lane/evidence/runtime-release.log" 2>&1
runtime=$?
if [ "$runtime" -eq 0 ]; then
    python3 -I "$lane/scripts/kernels.py" > "$lane/evidence/kernels.log" 2>&1
    kernels=$?
else
    kernels=$runtime
fi
printf '%s\n' "$kernels" > "$lane/evidence/kernels.exit"
echo "kernels exit=$kernels $(date -u +%FT%TZ)"
if [ "$kernels" -ne 0 ]; then worst=1; fi
exit "$worst"
