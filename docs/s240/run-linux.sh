#!/usr/bin/env bash
# s240: the linux measurements, each in a fresh rootless podman container
# (--network none, its own pid namespace, four cpus asserted from inside),
# with only this directory and an output directory mounted. Every signal a
# probe sends goes to a process the probe started inside its container.
#
#   run-linux.sh <src-dir> <out-dir>
#
# Makes images localhost/s240-ubuntu and localhost/s240-alpine (removed by
# `run-linux.sh --clean`), fetches wolf 0.2.29 and lupin 0.1.52 by digest.
set -euo pipefail
if [ "${1:-}" = "--clean" ]; then
  podman rmi -f localhost/s240-ubuntu localhost/s240-alpine 2>&1 || true
  exit 0
fi
SRC=$(cd "$1" && pwd); OUT=$2
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
RUN="podman run --rm --network none --cpuset-cpus 0-3"
log() { echo "### $*"; }

log "host"; uname -r; podman --version; date -u +%FT%TZ
grep -E '^(SigBlk|SigIgn)' /proc/self/status

log "images"
podman build -q -t localhost/s240-ubuntu -f "$SRC/Containerfile.ubuntu" "$SRC"
podman build -q -t localhost/s240-alpine -f "$SRC/Containerfile.alpine" "$SRC"
podman image inspect --format '{{.Id}} {{.RepoTags}}' localhost/s240-ubuntu localhost/s240-alpine docker.io/library/ubuntu:24.04 docker.io/library/alpine:3.20

log "compile"
mkdir -p "$OUT/bin"
$RUN -v "$SRC":/src:ro -v "$OUT/bin":/bin-out localhost/s240-ubuntu sh -c \
  'for p in grp sess sigtab; do gcc -O1 -Wall -o /bin-out/$p-glibc /src/$p.c || exit 1; done; ldd --version | head -1'
$RUN -v "$SRC":/src:ro -v "$OUT/bin":/bin-out localhost/s240-alpine sh -c \
  'for p in grp sess sigtab; do gcc -O1 -Wall -o /bin-out/$p-musl /src/$p.c || exit 1; done; apk info -v musl | head -1'
sha256sum "$OUT"/bin/*

for libc in glibc musl; do
  img=localhost/s240-ubuntu; [ $libc = musl ] && img=localhost/s240-alpine
  log "grp $libc root (kill(-1) allowed: the container is the probe's)"
  $RUN -e PROBE_ALLOW_ALL=1 -v "$OUT/bin":/b:ro $img /b/grp-$libc 1000
  log "grp $libc user 1000 (kill(-1) allowed)"
  $RUN --user 1000:1000 -e PROBE_ALLOW_ALL=1 -v "$OUT/bin":/b:ro $img /b/grp-$libc
  log "sess $libc root"
  $RUN -v "$OUT/bin":/b:ro $img /b/sess-$libc
  log "sess $libc user 1000"
  $RUN --user 1000:1000 -v "$OUT/bin":/b:ro $img /b/sess-$libc
done

log "sigtab glibc, C locale"
$RUN -v "$OUT/bin":/b:ro localhost/s240-ubuntu /b/sigtab-glibc > "$OUT/sigtab-glibc-C.txt"
log "sigtab glibc, LANG=de_DE.UTF-8, no setlocale"
$RUN -e LANG=de_DE.UTF-8 -v "$OUT/bin":/b:ro localhost/s240-ubuntu /b/sigtab-glibc > "$OUT/sigtab-glibc-de-nosetlocale.txt"
log "sigtab glibc, LANG=de_DE.UTF-8, setlocale"
$RUN -e LANG=de_DE.UTF-8 -v "$OUT/bin":/b:ro localhost/s240-ubuntu /b/sigtab-glibc --setlocale > "$OUT/sigtab-glibc-de-setlocale.txt"
log "sigtab musl, C locale"
$RUN -v "$OUT/bin":/b:ro localhost/s240-alpine /b/sigtab-musl > "$OUT/sigtab-musl-C.txt"
log "sigtab musl, LANG=de_DE.UTF-8, setlocale"
$RUN -e LANG=de_DE.UTF-8 -v "$OUT/bin":/b:ro localhost/s240-alpine /b/sigtab-musl --setlocale > "$OUT/sigtab-musl-de-setlocale.txt"
sha256sum "$OUT"/sigtab-*.txt
head -4 "$OUT"/sigtab-*.txt

log "shells: dash and GNU timeout under strace"
mkdir -p "$OUT/shells"
$RUN --cap-add SYS_PTRACE -v "$SRC":/src:ro -v "$OUT/shells":/out localhost/s240-ubuntu sh /src/shells.sh

log "wolf 0.2.29 and lupin 0.1.52"
mkdir -p "$OUT/tc"
W=wolf-0.2.29-x86_64-unknown-linux-gnu.tar.gz; WS=ae466e85418a48758460a981c9fab690462f779dae967cface86c555e649d7c1
L=lupin-0.1.52-x86_64-unknown-linux-gnu.tar.gz; LS=f5194b74879c45ad96484566950b4ae59fe1faa7ccd86461c6656f9e662b698f
[ -f "$OUT/tc/$W" ] || curl -sSfL -o "$OUT/tc/$W" "https://github.com/wolffe-lang/wolf-lang/releases/download/v0.2.29/$W"
[ -f "$OUT/tc/$L" ] || curl -sSfL -o "$OUT/tc/$L" "https://github.com/wolffe-lang/wolf-interp/releases/download/v0.1.52/$L"
( cd "$OUT/tc" && printf '%s  %s\n%s  %s\n' "$WS" "$W" "$LS" "$L" | sha256sum -c && printf '%s  %s\n%s  %s\n' "$WS" "$W" "$LS" "$L" | sha256sum -c )
( cd "$OUT/tc" && rm -rf wolf lupin && mkdir wolf lupin && tar -xzf "$W" -C wolf --strip-components=1 && tar -xzf "$L" -C lupin --strip-components=1 )
mkdir -p "$OUT/witness"
$RUN -v "$SRC":/src:ro -v "$OUT/tc":/tc:ro -v "$OUT/witness":/w -w /w localhost/s240-ubuntu sh -c '
  set -u
  export PATH=/tc/wolf/bin:/tc/lupin/bin:/tc/wolf:/tc/lupin:$PATH
  wolf --version; lupin --version
  cp /src/witness.lu /w/witness.lu
  for tier in native release; do
    flag=""; [ $tier = release ] && flag=--release
    rm -f g1.pid g2.pid g1.pid.ps g2.pid.ps
    wolf build $flag witness.lu -o witness-$tier; echo "build $tier: $?"
    echo "--- run $tier"; ./witness-$tier; echo "exit $tier: $?"
    echo "--- sleeps left after $tier:"; ps -eo pid,pgid,stat,comm | grep "[s]leep" || echo none
  done
  rm -f g1.pid g2.pid g1.pid.ps g2.pid.ps
  echo "--- checked"; wolf conform-run witness.lu --json --checked; echo "exit checked: $?"
  echo "--- sleeps left after checked:"; ps -eo pid,pgid,stat,comm | grep "[s]leep" || echo none
  rm -f g1.pid g2.pid g1.pid.ps g2.pid.ps
  echo "--- lupin"; lupin conform-run witness.lu --json; echo "exit lupin: $?"
  echo "--- sleeps left in the container:"; ps -eo pid,pgid,stat,comm | grep "[s]leep" || echo none'
log "done"; date -u +%FT%TZ
