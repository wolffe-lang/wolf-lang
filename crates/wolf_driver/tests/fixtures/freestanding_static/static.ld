/* kw09 gate ([abi.link.script]): the consuming build's link. The boot
   section first, so the entry's placement is visible in the image; each
   permission group on its own page; `.got` by name (the native tier
   reaches module state and every `extern "c" let` through it, kw06's
   lesson) with the read-only data — the static link fills it and
   nothing writes it after. Kept out of the writable group: ld.lld makes
   a `.got` there RELRO and starts `.bss` in a LOAD segment of its own
   on the same page, which the loader maps zero-filled over `.data`
   (measured: every GOT entry read 0). The image's end as a symbol the
   kernel names with `extern "c" let`. Plain sections only, so ld.lld
   and GNU ld read it alike. */
ENTRY(_start)
SECTIONS
{
  . = 0x400000;
  .text.boot : { *(.text.boot) }
  .text : { *(.text .text.*) }
  . = ALIGN(0x1000);
  .rodata : { *(.rodata .rodata.*) }
  .got : { *(.got .got.plt) }
  . = ALIGN(0x1000);
  .data : { *(.data .data.*) }
  .bss : { *(.bss .bss.*) }
  __kw_image_end = .;
}
