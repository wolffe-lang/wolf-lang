#!/usr/bin/env bash
# s290's PAX boot leg (followon.sh; an image with QEMU, /dev/kvm passed):
# a fresh clone of pax's trunk with NO change to it (the recorder refuses a
# checkout its steps changed), its own tools only: tools/fetch-wolf (the
# kernel's pinned wolf), tools/fetch-limine, tools/mkinitramfs (the fixture
# at /bin/sessrows and an /etc/pax-run naming it), tools/build-kernel
# (kernel/kmain_session.lu: the kernel starts the program as a session's
# leader with the console), tools/mkimage, tools/qemu-halt. The fixture is
# the static binary rows-inner.sh built with the wolf-lang head's own wolf.
set -uo pipefail
export TMPDIR=/lane/tmp HOME=/lane/home
mode=$1; expected=$2
leg=/lane/pax-leg
if [ "$mode" = record ]; then
  cd "$leg" || exit 1
  if [ -e pax ]; then echo "pax/ exists: not a fresh clone"; exit 1; fi
  git clone -q https://github.com/wolffe-lang/pax.git pax || exit 1
  cd pax || exit 1
  exec bash /lane/rig/gauntlet-record.sh --output "/lane/records/pax-leg-$expected.record" -- bash "$0" steps "$expected"
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
export PAX_BUILD=$leg/build-pax PAX_CACHE=$leg/cache
out=$leg/out
mkdir -p "$PAX_BUILD" "$PAX_CACHE" "$out"
step pax-head bash -c 'git log -1 --format="pax %H %s"; git status --porcelain --untracked-files=no | wc -l; sha256sum /lane/pax-leg/sessrows; ls -l /dev/kvm' || exit $?
step fetch-wolf bash -c 'w=$(tools/fetch-wolf) && "$w" --version && printf "%s\n" "$w" > /lane/pax-leg/out/pax-wolf.path' || exit $?
step fetch-limine tools/fetch-limine || exit $?
printf '%s\n' /bin/sessrows > "$out/sess.run"
step mkinitramfs tools/mkinitramfs "$out/sess.cpio" /bin/sessrows=/lane/pax-leg/sessrows:755 /etc/pax-run="$out/sess.run":644 || exit $?
step build-kernel bash -c 'PAX_WOLF=$(cat /lane/pax-leg/out/pax-wolf.path) tools/build-kernel --src kernel/kmain_session.lu /lane/pax-leg/out/kernel > /lane/pax-leg/out/elf.path && cat /lane/pax-leg/out/elf.path' || exit $?
elf=$(cat "$out/elf.path")
step mkimage tools/mkimage --conf boot/limine-initramfs.conf --add "$out/sess.cpio" initramfs.cpio "$elf" "$out/sess.iso" || exit $?
step boot tools/qemu-halt --firmware bios --serial "$out/sess.serial.log" --timeout 600 --qemu -m --qemu 512M --elf "$elf" --marker 'halt|PANIC .*' "$out/sess.iso" || exit $?
step serial bash -c 'tr -d "\r" < /lane/pax-leg/out/sess.serial.log' || exit $?
printf 'START rows\n'
started=$SECONDS
rc=0
while IFS= read -r want; do
  if tr -d '\r' < "$out/sess.serial.log" | grep -qxF -- "$want"; then printf '  ROW %s\n' "$want"; else printf 'FAIL row missing: %s\n' "$want"; rc=1; fi
done <<'ROWS'
pax: leads its session true
pax: leader setsid denied, error 1
child: sid=pid true, pgid=pid true, tty 6
pax: spawned child status 0, another process true
forked: setsid ok, sid=pid true
pax: forked child status 0
pax: session unchanged true
ROWS
printf 'END rows exit=%s secs=%s\n' "$rc" "$((SECONDS-started))"
exit "$rc"
