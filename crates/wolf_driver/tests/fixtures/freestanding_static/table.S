/* kw09 gate: a table the assembly reserves in .bss, named in wolf with
   `extern "c" let kw_table: *u64` ([abi.link.extern]). */
        .section .bss
        .p2align 3
        .globl kw_table
kw_table:
        .zero 64
