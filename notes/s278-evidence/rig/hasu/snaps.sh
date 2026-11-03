#!/usr/bin/env bash
# s278: at a commit, the formatter's form of the two rows (copies, diffed)
# and the corpus snapshots (insta's update), copied out to
# /lane/records/snaps-<sha>/; the checkout put back clean (adapted by copy
# from s277's snaps.sh).
# usage: snaps.sh <sha>
set -u
source /lane/rig/env.sh
want=${1:?sha}
cd /lane/wolf-lang || exit 2
git fetch -q origin s278 && git checkout -q --detach "$want" || exit 2
test "$(git rev-parse HEAD)" = "$(git rev-parse "$want")" || { echo "head is not $want"; exit 2; }
OUT=/lane/records/snaps-$want
rm -rf "$OUT"; mkdir -p "$OUT/fmt"
rc_all=0
step() { local n=$1; shift; echo "== STEP $n $(date -u +%T)"; "$@"; local rc=$?; echo "END $n exit=$rc"; [ $rc -eq 0 ] || rc_all=1; }
step build cargo build --locked -p wolf_driver -p wolf_rt
fmt_form() {
  local r rc=0
  for r in signal_action_rows chroot_rows; do
    cp corpus/os/$r.lu "$OUT/fmt/$r.lu"
    echo "-- wolf fmt --check corpus/os/$r.lu"
    target/debug/wolf fmt --check corpus/os/$r.lu; echo "@check-status $?"
    target/debug/wolf fmt "$OUT/fmt/$r.lu"; echo "@fmt-status $?"
    diff -u corpus/os/$r.lu "$OUT/fmt/$r.lu" && echo "-- $r: already the formatter's form" || rc=1
  done
  return $rc
}
fmt_form; echo "END fmt-form exit=$? (1 = the formatter re-lays a row; its form is in $OUT/fmt)"
step snap-lex env INSTA_UPDATE=always cargo test --locked -p wolf_lex --test corpus_snapshots
step snap-parse env INSTA_UPDATE=always cargo test --locked -p wolf_parse --test corpus_decls
step snap-wir env INSTA_UPDATE=always cargo test --locked -p wolf_wir --test lower_corpus
git status --porcelain > "$OUT/status.txt"
cat "$OUT/status.txt"
git status --porcelain | awk '{print $2}' | grep '\.snap' | while read -r f; do mkdir -p "$OUT/$(dirname "$f")"; cp "$f" "$OUT/$f"; done
git checkout -q -- . && git clean -fdq -- crates
echo "clean: $(git status --porcelain --untracked-files=no | wc -l)"
echo "ALL rc=$rc_all"
exit $rc_all
