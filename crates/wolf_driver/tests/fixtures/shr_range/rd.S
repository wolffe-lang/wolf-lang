/* s214 (wolf-lang#600): the value the compiler cannot see. x86-64. */
        .text
        .globl rd
rd:
        movabsq $0xffffc04000001000, %rax
        ret
