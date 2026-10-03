/* kw06 gate: the device window at the VGA text buffer's address, the
   image above it. Plain sections only, so ld.lld and GNU ld read it
   alike. `.got` is placed by name: the native tier's objects reach an
   extern through it, and ld.lld would otherwise put the orphan at the
   window's address (measured: the window moved to 0xb9000 and the
   kernel's first store clobbered the GOT). Each permission group
   starts on its own page: a linker script gives ld.lld no page rule,
   and text sharing a page with writable data ran with no execute
   permission (measured: SIGSEGV at `_start`'s first instruction). */
ENTRY(_start)
SECTIONS
{
  . = 0xb8000;
  .kwdev : { *(.kwdev) }
  . = 0x400000;
  .text : { *(.text .text.*) }
  . = ALIGN(0x1000);
  .rodata : { *(.rodata .rodata.*) }
  . = ALIGN(0x1000);
  .data : { *(.data .data.*) }
  .got : { *(.got .got.plt) }
  .bss : { *(.bss .bss.*) }
}
