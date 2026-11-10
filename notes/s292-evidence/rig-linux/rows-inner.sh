#!/usr/bin/env bash
# s292, inside the toolchain image (followon.sh; adapted by copy from
# s290's): the PAX fixture and its helper built static with the head's own
# wolf (glibc, `cc -static`, as pax's tools/mkpty builds its probe), each
# checked for ET_EXEC with no PT_INTERP.
set -uo pipefail
export PATH=/lane/tool-bin:/opt/s263-rust/bin:$PATH TMPDIR=/lane/tmp HOME=/lane/home
unset WOLF_STD
mode=$1; expected=$2
cd /work || exit 1
test "$(git rev-parse HEAD)" = "$expected" || exit 1
if [ "$mode" = record ]; then
  exec bash /lane/rig/gauntlet-record.sh --output "/lane/records/rows-$expected.record" -- bash "$0" steps "$expected"
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
step versions bash -c '"$0" --version; sha256sum /work/notes/s292-evidence/pax/ptyrows.lu /work/crates/wolf_driver/tests/fixtures/pty/helper.lu; printf "linked against: %s\n" "$(/usr/bin/cc -print-file-name=libc.a)"' "$W" || exit $?
static() { # SRC OUT
  local src=$1 out=$2
  mkdir -p /lane/pax-leg/ccwrap /lane/pax-leg/build
  printf '#!/bin/sh\nexec /usr/bin/cc -static "$@"\n' > /lane/pax-leg/ccwrap/cc
  chmod +x /lane/pax-leg/ccwrap/cc
  cp "$src" /lane/pax-leg/build/
  (cd /lane/pax-leg/build && PATH=/lane/pax-leg/ccwrap:$PATH "$W" build --release --no-cache --deny-warnings "$(basename "$src")" -o "$out") || return 1
  readelf -hW "$out" | grep -E 'Type: +EXEC' || return 1
  if readelf -lW "$out" | grep -q INTERP; then echo "PT_INTERP: not static"; return 1; fi
  sha256sum "$out"
}
step pax-fixture bash -c "$(declare -f static); W=$W; static /work/notes/s292-evidence/pax/ptyrows.lu /lane/pax-leg/ptyrows" || exit $?
step pax-helper bash -c "$(declare -f static); W=$W; static /work/crates/wolf_driver/tests/fixtures/pty/helper.lu /lane/pax-leg/ptyhelper" || exit $?
