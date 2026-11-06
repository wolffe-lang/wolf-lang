#!/bin/bash
# s282 mac re-measure job, after the binder rule became case-blind (run by
# build-at.sh in the build worktree at the head, behind the lock): wolf at
# the head, copied out; every .lu file of the org's wolf repositories and of
# wolf-std at the resolve rung on it (tools/measure-org.py; the base's run,
# measure-base-ed19904c.json, stands: the base binary did not change); the
# case-blind syntactic census (tools/variant-census.py); the W0801 and E0806
# comparison of the compiler and lupin built from the s282 branch over the
# whole corpus and lupin's mirror fixtures (tools/w0801-diff.py); and the
# compiler's binder rows on the four machines with that lupin.
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
LUPIN=$(ls -d "$L"/records/lupin-b0b521c*/lupin | head -1)
FIX=$(ls -d "$L"/build-interp-b0b521c*/tests/binder_variant_s282 | head -1)
STD=$L/org/wolf-std-5b3a5994/std
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c "git rev-parse HEAD && git log --oneline -1 && git status --porcelain --untracked-files=no | wc -l && rustc -V && $LUPIN --version && echo fixtures $FIX"
step build-head cargo build --locked -p wolf_driver -p wolf_rt || { echo "== build failed: stopping"; exit 1; }
mkdir -p "$L/records/wolf-$SHA"; cp target/debug/wolf "$L/records/wolf-$SHA/wolf"
O=$L/org
DIRS="$O/aspen $O/boreutils $O/burrow $O/clef $O/cub $O/den $O/fenrir $O/fugue $O/jobq $O/lair $O/$PRIVATE_NAME $O/ligature $O/lobo $O/lope $O/mk $O/muzzle $O/pack $O/pax $O/pelt $O/scent $O/sniff $O/spell $O/tomlq $O/trail $O/tree-sitter-wolf $O/wolf-book $O/wolf-lsp $O/wolf-web $O/wolf-std-5b3a5994 $O/wolf-std"
step measure-head python3 -I "$L/tools/measure-org.py" "$L/records/wolf-$SHA/wolf" "$STD" "$L/records/measure-head-$SHA.json" $DIRS
step census python3 -I "$L/tools/variant-census.py" $DIRS "$PWD/corpus"
step w0801-corpus python3 -I "$L/tools/w0801-diff.py" "$L/records/wolf-$SHA/wolf" "$LUPIN" "$STD" "$PWD/corpus"
step w0801-mirror python3 -I "$L/tools/w0801-diff.py" "$L/records/wolf-$SHA/wolf" "$LUPIN" "$STD" "$FIX"
ROWS="lints/binder_variant_let lints/binder_variant_for lints/binder_variant_else lints/binder_variant_imported/main lints/binder_variant_lowercase lints/binder_variant_controls conc/select_arm_bare_variant conc/select_arm_lowercase_variant conc/select_arm_lowercase_binds"
files=(); for r in $ROWS; do files+=("$PWD/corpus/$r.lu"); done
step rows-with-s282-lupin env LUPIN="$LUPIN" "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "${files[@]}"
step w0801-probes python3 -I "$L/tools/w0801-diff.py" "$L/records/wolf-$SHA/wolf" "$LUPIN" "$STD" "$L/probe"
E=(); for d in b16_lowercase_variant b32_else_lowercase_variant b33_else_variant_is_tag b34_type_alias_enum; do E+=("$L/probe/$d/main.lu"); done
step edge-probes env LUPIN="$LUPIN" "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "${E[@]}"
render() { for f in "${E[@]}"; do echo "## $(basename "$(dirname "$f")")"; "$PWD/target/debug/wolf" build "$f" -o "$TMPDIR/probe-bin" 2>&1 | head -40; echo "   (build exit ${PIPESTATUS[0]})"; "$LUPIN" run "$f" 2>&1 | head -20; echo "   (lupin exit ${PIPESTATUS[0]})"; done; }
step edge-render render
echo "== done $(date -u +%FT%TZ)"
