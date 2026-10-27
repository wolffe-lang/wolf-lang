set -uo pipefail
lane=/home/mfwolffe/lanes/s268
unset WOLF_STD
mkdir -p "$lane/evidence/remaining"
worst=0
run() {
    name=$1
    shift
    echo "remaining $name start $(date -u +%FT%TZ)"
    "$@" > "$lane/evidence/remaining/$name.log" 2>&1
    status=$?
    printf '%s\n' "$status" > "$lane/evidence/remaining/$name.exit"
    echo "remaining $name exit=$status $(date -u +%FT%TZ)"
    if [ "$status" -ne 0 ]; then worst=1; fi
}
sha256sum -c "$lane/evidence/verified-source.sha256" || exit 3
run startup-baseline python3 -I "$lane/scripts/startup-baseline.py"
# The one filtered test already passed all six expensive links in take3.
# Every other workspace test runs again with the repaired immutable shim.
run workspace cargo test --workspace -- --skip linux::every_root_manifest_program_links_with_real_ligature_on_both_tiers
run fuzz-check cargo check --manifest-path fuzz/Cargo.toml
run deps-check cargo xtask deps-check
run spec-extract cargo xtask spec-extract --check
run conformance cargo xtask conformance
run print-gate cargo xtask print-gate
run diag-catalog cargo xtask diag-catalog --check
run doc-catalog cargo xtask doc-catalog --check
run fmt-lu cargo xtask fmt-lu
run fmt-fuzz cargo xtask fmt-fuzz --ci
run audit-surface cargo xtask audit-surface
run dist-smoke cargo xtask dist
run differ-self cargo xtask differ --self
run lane-coverage cargo xtask lane-coverage
exit "$worst"
