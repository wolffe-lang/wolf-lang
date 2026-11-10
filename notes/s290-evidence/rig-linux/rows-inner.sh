#!/usr/bin/env bash
# s290, inside the toolchain image (followon.sh): the PAX fixture built
# static with the head's wolf, and wolf-std's session rows on the head's
# wolf and on lupin 0.1.60, each lane as wolf-std's std-test calls it
# (`conform-run [--checked|--native] --std-root STD FILE --json`).
set -uo pipefail
export PATH=/lane/tool-bin:/opt/s263-rust/bin:$PATH TMPDIR=/lane/tmp HOME=/lane/home
unset WOLF_STD
export LUPIN=/lane/inputs/lupin060/unpacked/lupin-0.1.60-x86_64-unknown-linux-gnu/lupin
mode=$1; expected=$2; sexpected=$3
cd /work || exit 1
test "$(git rev-parse HEAD)" = "$expected" || exit 1
test "$(git -C /std rev-parse HEAD)" = "$sexpected" || exit 1
if [ "$mode" = record ]; then
  exec bash /lane/rig/gauntlet-record.sh --output "/lane/records/rows-$expected.record" -- bash "$0" steps "$expected" "$sexpected"
fi
step() {
  name=$1; shift
  started=$SECONDS
  printf 'START %s\n' "$name"
  "$@" 2>&1 | sed 's/^/  /'
  rc=${PIPESTATUS[0]}
  printf 'END %s exit=%s secs=%s\n' "$name" "$rc" "$((SECONDS-started))"
  return "$rc"
}
W=/work/target/debug/wolf
step versions bash -c '"$0" --version; "$LUPIN" --version; git -C /std log --oneline -1; sha256sum /lane/pax-leg/sessrows.lu' "$W" || exit $?
step pax-fixture bash -c 'set -e; mkdir -p /lane/pax-leg/ccwrap /lane/pax-leg/build; printf "#!/bin/sh\nexec /usr/bin/cc -static \"\$@\"\n" > /lane/pax-leg/ccwrap/cc; chmod +x /lane/pax-leg/ccwrap/cc; cd /lane/pax-leg/build; cp /lane/pax-leg/sessrows.lu .; PATH=/lane/pax-leg/ccwrap:$PATH "$0" build --release --no-cache --deny-warnings sessrows.lu -o /lane/pax-leg/sessrows; readelf -hW /lane/pax-leg/sessrows | grep -E "Type: +EXEC"; if readelf -lW /lane/pax-leg/sessrows | grep -q INTERP; then echo "PT_INTERP: not static"; exit 1; fi; sha256sum /lane/pax-leg/sessrows; printf "linked against: %s\n" "$(/usr/bin/cc -print-file-name=libc.a)"' "$W" || exit $?
rows() { # lane FILE
  local lane=$1 f=$2 out
  case $lane in
    checked) out=$(cd /std/tests/process && "$W" conform-run --checked --std-root /std/std "$f" --json 2>&1) ;;
    native) out=$(cd /std/tests/process && "$W" conform-run --native --std-root /std/std "$f" --json 2>&1) ;;
    lupin) out=$(cd /std/tests/process && "$LUPIN" conform-run --std-root /std/std "$f" --json 2>&1) ;;
  esac
  printf '%s %s: %s\n' "$lane" "$f" "$(printf '%s' "$out" | python3 -I -c '
import json, sys
t = sys.stdin.read()
try:
    d = json.loads(t)
except ValueError:
    print("TEXT " + " | ".join(t.splitlines()[:3])); sys.exit(0)
print(d.get("verdict"), d.get("phase_reached"), json.dumps(d.get("stdout_inline")), [(x.get("code"), x.get("message", "")[:100]) for x in d.get("diagnostics", [])][:2], d.get("reason", ""))')"
}
for f in session_rows.lu session_child.lu lead_session_rows.lu comptime_refuses_session.lu; do
  step "std-$f" bash -c "$(declare -f rows); W=$W; for lane in checked native lupin; do rows \$lane $f; done" || exit $?
done
step std-fmt "$W" fmt --check /std/std/process/process.lu /std/tests/process/session_rows.lu /std/tests/process/session_child.lu /std/tests/process/lead_session_rows.lu /std/tests/process/comptime_refuses_session.lu || exit $?
