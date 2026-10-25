#!/bin/bash
# s267: the bundle candidate, in ctr-u at a clean head. libwolf_rt.a by
# `cargo xtask dist-rt` (the recipe the published bundle's runtime was built
# by: s266's probe reproduced it byte for byte); the archive by ld02a's
# recipe (ligature notes/ld02a/bundle.json: GNU tar --sort=name --mtime=@0
# --owner=0 --group=0 --numeric-owner, explicit ordered file list, gzip -n),
# first checked to reproduce the published archive from its own members.
set -u
L=/home/mfwolffe/lanes/s267
B=$L/bundle
cd $L/wolf-lang
head=$(git rev-parse HEAD)
echo "bundle head=$head start=$(date -u +%FT%TZ)"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then echo "REFUSED: the tree is not clean"; exit 3; fi
tar --version | head -1; gzip --version | head -1
cargo xtask dist-rt --target x86_64-unknown-linux-musl || exit 3
rt=$L/wolf-lang/target/dist-rt/x86_64-unknown-linux-musl/release/libwolf_rt.a
sha256sum "$rt"; sha256sum "$rt"; stat -c '%s bytes' "$rt"
files="cc ld.lld libwolf_rt.a sysroot/crt1.o sysroot/crti.o sysroot/crtn.o sysroot/libc.a sysroot/libunwind.a"
pack() { (cd "$1" && tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner -cf - $files | gzip -n) > "$2"; }
P=$B/stage-published; rm -rf "$P"; mkdir -p "$P/sysroot"
cp target/static-entry-bundle/cc target/static-entry-bundle/ld.lld target/static-entry-bundle/libwolf_rt.a "$P/"
cp target/static-entry-bundle/sysroot/* "$P/sysroot/"
chmod 755 "$P/cc" "$P/ld.lld"; chmod 644 "$P/libwolf_rt.a" "$P"/sysroot/*
pack "$P" "$B/recipe-check.tar.gz"
want=6f99a2794da54b2dd85f06713d736b27862a7cd0da9e28bd940d975a4603a1d1
got=$(sha256sum < "$B/recipe-check.tar.gz" | cut -d' ' -f1)
echo "recipe check: $got (published $want)"
if [ "$got" != "$want" ]; then echo "REFUSED: the recipe does not reproduce the published bundle"; exit 4; fi
C=$B/stage; rm -rf "$C"; cp -a "$P" "$C"; cp "$rt" "$C/libwolf_rt.a"; chmod 644 "$C/libwolf_rt.a"
out=$B/ld02-musl-link-bundle.tar.gz
pack "$C" "$out"
(cd "$C" && for f in $files; do printf '%s %s %s\n' "$f" "$(sha256sum < "$f" | cut -d' ' -f1)" "$(stat -c %s "$f")"; done) > $B/link.sums
(cd "$C" && for f in $files; do printf '%s %s %s\n' "$f" "$(sha256sum < "$f" | cut -d' ' -f1)" "$(stat -c %s "$f")"; done) > $B/link.sums.read2
cmp $B/link.sums $B/link.sums.read2 && echo "member sums read twice, equal"
cat $B/link.sums
sha256sum "$out"; sha256sum "$out"; stat -c '%s bytes' "$out"
cp target/dist/wolf-rt-*-x86_64-unknown-linux-musl.tar.gz target/dist/wolf-rt-*-x86_64-unknown-linux-musl.notes.md $B/ 2>/dev/null
sha256sum $B/wolf-rt-*-x86_64-unknown-linux-musl.tar.gz
echo "$head" > $B/HEAD
echo "bundle end=$(date -u +%FT%TZ)"
