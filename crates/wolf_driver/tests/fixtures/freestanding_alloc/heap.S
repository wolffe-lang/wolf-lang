/* kw12 gate: the allocator hook's arena, 1 MiB in .bss, named in wolf
   with `extern "c" let kw_heap: *u8` ([abi.link.extern]). */
        .section .bss
        .p2align 4
        .globl kw_heap
kw_heap:
        .zero 1048576
