set -uo pipefail
lane=/home/mfwolffe/lanes/s268
echo "gauntlet3 source cd60d930 start $(date -u +%FT%TZ)"
grep -E 'Cpus_allowed_list|SigBlk|SigIgn|NoNewPrivs' /proc/self/status
sha256sum -c "$lane/evidence/verified-source.sha256" || exit 3
sha256sum /opt/s268-ligature /opt/s268-clef-tree/target/clef /opt/s268-musl-rt.a
sha256sum /opt/s268-ligature /opt/s268-clef-tree/target/clef /opt/s268-musl-rt.a
cargo xtask ci > "$lane/evidence/gauntlet3.log" 2>&1
status=$?
printf '%s\n' "$status" > "$lane/evidence/gauntlet3.exit"
echo "gauntlet3 exit=$status end $(date -u +%FT%TZ)"
exit "$status"
