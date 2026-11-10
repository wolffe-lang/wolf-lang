#!/usr/bin/env bash
# s292's PAX boot leg (adapted by copy from s290's; followon.sh, an image
# with QEMU, /dev/kvm passed): a fresh clone of pax's branch px111 at the
# head the change was written against, with NO change to it (the recorder
# refuses a checkout its steps changed), its own tools only: tools/fetch-wolf
# (the kernel's pinned wolf), tools/fetch-limine, tools/mkinitramfs (the
# fixture at /bin/ptyrows, the helper at /bin/ptyhelper, /tmp, and an
# /etc/pax-run naming both), tools/build-kernel (kernel/kmain_console.lu,
# the entry px111's own pty suite boots), tools/mkimage, tools/qemu-halt.
# The two programs are the static binaries rows-inner.sh built with the
# wolf-lang head's own wolf.
set -uo pipefail
export TMPDIR=/lane/tmp HOME=/lane/home
mode=$1; expected=$2
leg=/lane/pax-leg
pax_branch=px111
pax_head=9536d82f021f92f4aa9f6e92bdd7964763210938
if [ "$mode" = record ]; then
  cd "$leg" || exit 1
  if [ -e pax ]; then echo "pax/ exists: not a fresh clone"; exit 1; fi
  git clone -q --branch "$pax_branch" https://github.com/wolffe-lang/pax.git pax || exit 1
  cd pax || exit 1
  git -c advice.detachedHead=false checkout -q --detach "$pax_head" || exit 1
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
step pax-head bash -c 'git log -1 --format="pax %H %s"; test "$(git rev-parse HEAD)" = "$0" || { echo "pax is not at $0"; exit 1; }; git status --porcelain --untracked-files=no | wc -l; sha256sum /lane/pax-leg/ptyrows /lane/pax-leg/ptyhelper; ls -l /dev/kvm' "$pax_head" || exit $?
step fetch-wolf bash -c 'w=$(tools/fetch-wolf) && "$w" --version && printf "%s\n" "$w" > /lane/pax-leg/out/pax-wolf.path' || exit $?
step fetch-limine tools/fetch-limine || exit $?
printf '%s\n' '/bin/ptyrows /bin/ptyhelper' > "$out/pty.run"
step mkinitramfs tools/mkinitramfs "$out/pty.cpio" /bin/ptyrows=/lane/pax-leg/ptyrows:755 /bin/ptyhelper=/lane/pax-leg/ptyhelper:755 /tmp/:1777 /etc/pax-run="$out/pty.run":644 || exit $?
step build-kernel bash -c 'PAX_WOLF=$(cat /lane/pax-leg/out/pax-wolf.path) tools/build-kernel --src kernel/kmain_console.lu /lane/pax-leg/out/kernel > /lane/pax-leg/out/elf.path && cat /lane/pax-leg/out/elf.path' || exit $?
elf=$(cat "$out/elf.path")
step mkimage tools/mkimage --conf boot/limine-initramfs.conf --add "$out/pty.cpio" initramfs.cpio "$elf" "$out/pty.iso" || exit $?
step boot tools/qemu-halt --firmware bios --serial "$out/pty.serial.log" --timeout 600 --qemu -m --qemu 512M --elf "$elf" --marker 'halt|PANIC .*' "$out/pty.iso" || exit $?
step serial bash -c 'tr -d "\r" < /lane/pax-leg/out/pty.serial.log' || exit $?
printf 'START rows\n'
started=$SECONDS
rc=0
while IFS= read -r want; do
  if tr -d '\r' < "$out/pty.serial.log" | grep -qxF -- "$want"; then printf '  ROW %s\n' "$want"; else printf 'FAIL row missing: %s\n' "$want"; rc=1; fi
done <<'ROWS'
pax: pair handles true, master a terminal true, slave a terminal true
pax: name form true
pax: by name a terminal true
pax: size master to slave 40 132, slave to master 50 100
pax: every op round-trips or is refused by name true
pax: name of a slave io
pax: spawned: the child ready at the size set before it true
pax: spawned: a new size heard 1, the same size again heard 0
pax: spawned: a pixel-only change heard true
pax: spawned: Ctrl-G interrupts true, status 7
pax: gone true
ROWS
printf 'END rows exit=%s secs=%s\n' "$rc" "$((SECONDS-started))"
exit "$rc"
