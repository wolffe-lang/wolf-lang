#!/bin/bash
# s282 mac measure job (run by build-at.sh in the build worktree at the head,
# behind the lock): wolf at the head and at the base (build-base, ed19904c),
# both copied out; then every .lu file of the org's wolf repositories and of
# wolf-std (at the compiler's STD-PIN and at its trunk) at the resolve rung on
# both binaries (tools/measure-org.py); then the compiler's binder rows on the
# four machines with lupin built from the s282 branch.
set -u
L=$LANE
SHA=$(git rev-parse --short=8 HEAD)
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; }
step head bash -c 'git rev-parse HEAD && git log --oneline -1 && rustc -V'
step build-head cargo build --locked -p wolf_driver -p wolf_rt || exit 1
mkdir -p "$L/records/wolf-$SHA"; cp target/debug/wolf "$L/records/wolf-$SHA/wolf"
step build-base bash -c "cd $L/build-base && git rev-parse HEAD && cargo build --locked -p wolf_driver -p wolf_rt" || exit 1
mkdir -p "$L/records/wolf-ed19904c"; cp "$L/build-base/target/debug/wolf" "$L/records/wolf-ed19904c/wolf"
O=$L/org
step measure-base python3 -I "$L/tools/measure-org.py" "$L/records/wolf-ed19904c/wolf" "$O/wolf-std-5b3a5994/std" "$L/records/measure-base-ed19904c.json" $DIRS
step measure-head python3 -I "$L/tools/measure-org.py" "$L/records/wolf-$SHA/wolf" "$O/wolf-std-5b3a5994/std" "$L/records/measure-head-$SHA.json" $DIRS
ROWS="lints/binder_variant_let lints/binder_variant_for lints/binder_variant_else lints/binder_variant_imported/main lints/binder_variant_controls conc/select_arm_bare_variant conc/select_arm_lowercase_binds"
files=(); for r in $ROWS; do files+=("$PWD/corpus/$r.lu"); done
step rows-with-s282-lupin env LUPIN="$L/records/lupin-d76649c8/lupin" "$L/rig/probe-all.sh" "$PWD/target/debug/wolf" "${files[@]}"
echo "== done $(date -u +%FT%TZ)"
