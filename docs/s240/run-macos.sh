#!/usr/bin/env bash
# s240: the macOS measurements, on CI's throwaway macOS runner (no
# container engine there). Each probe first makes a session of its own,
# so a group send reaches only processes it started; kill(-1) is NOT run
# here (it would reach the runner's own processes of the same user), so
# PROBE_ALLOW_ALL stays unset and grp skips it by name.
#
#   run-macos.sh <src-dir> <out-dir>
set -uo pipefail
SRC=$1; OUT=$2
mkdir -p "$OUT"
log() { echo "### $*"; }
log "host"; sw_vers; uname -a; id; cc --version | head -1
for p in grp sess sigtab; do cc -O1 -Wall -o "$OUT/$p" "$SRC/$p.c" || exit 1; done
shasum -a 256 "$OUT"/grp "$OUT"/sess "$OUT"/sigtab
log "grp as the user"; "$OUT/grp"
log "grp as root (sudo; the user is SUDO_UID)"; sudo "$OUT/grp"
log "sess as the user"; "$OUT/sess"
log "sess as root"; sudo "$OUT/sess"
log "sigtab, C locale"; "$OUT/sigtab" | tee "$OUT/sigtab-macos-C.txt"
log "sigtab, LANG=de_DE.UTF-8, setlocale"; LANG=de_DE.UTF-8 "$OUT/sigtab" --setlocale | tee "$OUT/sigtab-macos-de-setlocale.txt"
shasum -a 256 "$OUT"/sigtab-*.txt
log "done"
