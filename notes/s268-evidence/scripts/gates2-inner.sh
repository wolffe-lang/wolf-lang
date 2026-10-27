set -uo pipefail
lane=/home/mfwolffe/lanes/s268
unset WOLF_STD
echo "gates2 without WOLF_STD start $(date -u +%FT%TZ)"
sha256sum -c "$lane/evidence/verified-source.sha256" || exit 3
cargo xtask ci --shard gates > "$lane/evidence/post-test-gates2.log" 2>&1
gates=$?
printf '%s\n' "$gates" > "$lane/evidence/post-test-gates2.exit"
echo "gates2 exit=$gates $(date -u +%FT%TZ)"
exit "$gates"
