/* kw10 witness ([abi.interrupt]): the consuming build's link. Identity
   mapped at 1 MiB (the boot path maps the first GiB), the PVH note in a
   PT_NOTE of its own, `.got` with the read-only data (kw09's lesson:
   the native tier reaches every `extern "c" let` through it). */
ENTRY(kw_pvh_start)
SECTIONS
{
  . = 0x100000;
  .note.Xen : { *(.note.Xen) }
  .text : { *(.text.boot) *(.text .text.*) }
  . = ALIGN(0x1000);
  .rodata : { *(.rodata .rodata.*) *(.got .got.plt) }
  . = ALIGN(0x1000);
  .data : { *(.data .data.*) }
  .bss : { *(.bss .bss.*) *(COMMON) }
  /DISCARD/ : { *(.note.GNU-stack) *(.comment) *(.eh_frame) }
}
