/* s214 (wolf-lang#598): the assembly half of kmain_stale.lu, user
   space, no libc. s214_cell is one i64 in .data; s214_poke stores its
   argument there; s214_call_back calls back into wolf's s214_bump,
   which writes the module var. */
        .data
        .p2align 3
        .globl s214_cell
s214_cell:
        .quad 0

        .text
        .globl s214_poke
s214_poke:
        mov     %rdi, s214_cell(%rip)
        ret

        .globl s214_call_back
s214_call_back:
        sub     $8, %rsp
        call    s214_bump
        add     $8, %rsp
        ret
