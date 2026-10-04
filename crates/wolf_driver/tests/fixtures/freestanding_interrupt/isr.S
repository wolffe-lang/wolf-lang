/* kw10 witness ([abi.interrupt], K7 = B): wolf has no interrupt calling
   convention, so a vector enters here. Listed under `asm` in the
   witness's wolf.pkg: its `.globl` routines are on wolf's roster
   ([abi.asm.roster]) and its .bss tables are named in wolf with
   `extern "c" let` ([abi.link.extern]).

   One macro per vector. A vector the CPU pushes no error code for
   pushes a zero in its place, so every frame has one shape; then the
   vector; then the common path saves the fifteen general registers
   (%rax first, %r15 last, so %r15 is at the lowest address), passes the
   frame's address in %rdi to wolf's `export fn kw_interrupt(f: *Frame)`
   under the C convention (%rsp 16-aligned at the call, DF clear),
   restores the registers, drops the vector and the error code and
   returns with `iretq`. The frame, lowest address first:

       r15 r14 r13 r12 r11 r10 r9 r8 rbp rdi rsi rdx rcx rbx rax   (0..112)
       vector (120)  error (128)                                   (ours)
       rip (136)  cs (144)  rflags (152)  rsp (160)  ss (168)      (the CPU's)

   176 bytes, 22 eight-byte words: `#[repr(c)] struct Frame` in
   kmain_interrupt.lu, whose `size_of`/`offset_of` the kernel reports.
   No SSE state is saved: the freestanding target emits none
   ([abi.target.none.codegen]).

   The probes: `kw_int3` (a trap: the saved RIP is the next instruction,
   kw_int3_after), `kw_ud2` (a fault: the saved RIP is the ud2 itself,
   kw_ud2_at; the handler resumes at kw_ud2_after), `kw_gp` (loads
   selector 0x1234 into %ds: #GP with an error code, saved RIP
   kw_gp_at, resumed at kw_gp_after).

   Written from: the Intel SDM vol. 3A §6.14.1 (64-bit IDT gates),
   §6.14.2 (the 64-bit stack frame: SS, RSP, RFLAGS, CS, RIP pushed
   unconditionally, the stack aligned to 16 first), §6.14.3 (IRET in
   IA-32e mode), §6.13 and Table 6-1 (which vectors push an error code),
   §6.15 (interrupts 3, 6 and 13); the AMD64 APM vol. 2 §8.9 (long-mode
   interrupt control transfers); the x86-64 psABI (%rdi, callee-saved
   registers, the 16-byte alignment at a call, DF clear). No kernel's
   source. */

        .macro ISR_NOERR v
        .type kw_isr\v, @function
kw_isr\v:
        pushq   $0
        pushq   $\v
        jmp     kw_isr_common
        .size kw_isr\v, . - kw_isr\v
        .endm

        .macro ISR_ERR v
        .type kw_isr\v, @function
kw_isr\v:
        pushq   $\v
        jmp     kw_isr_common
        .size kw_isr\v, . - kw_isr\v
        .endm

        .text
        ISR_NOERR 3
        ISR_ERR 6
        ISR_ERR 13

        .globl kw_isr_common
        .type kw_isr_common, @function
kw_isr_common:
        pushq   %rax
        pushq   %rbx
        pushq   %rcx
        pushq   %rdx
        pushq   %rsi
        pushq   %rdi
        pushq   %rbp
        pushq   %r8
        pushq   %r9
        pushq   %r10
        pushq   %r11
        pushq   %r12
        pushq   %r13
        pushq   %r14
        pushq   %r15
        movq    %rsp, %rdi              /* the frame */
        movq    %rsp, %rbx              /* callee-saved: survives the call */
        andq    $-16, %rsp
        cld
        call    kw_interrupt
        movq    %rbx, %rsp
        popq    %r15
        popq    %r14
        popq    %r13
        popq    %r12
        popq    %r11
        popq    %r10
        popq    %r9
        popq    %r8
        popq    %rbp
        popq    %rdi
        popq    %rsi
        popq    %rdx
        popq    %rcx
        popq    %rbx
        popq    %rax
        addq    $16, %rsp               /* the vector and the error code */
        iretq
        .size kw_isr_common, . - kw_isr_common

/* kw_lidt(idtr): LIDT of the 10-byte pseudo-descriptor at idtr. */
        .globl kw_lidt
        .type kw_lidt, @function
kw_lidt:
        lidt    (%rdi)
        ret
        .size kw_lidt, . - kw_lidt

        .globl kw_int3
        .type kw_int3, @function
kw_int3:
        int3
        .globl kw_int3_after
kw_int3_after:
        ret
        .size kw_int3, . - kw_int3

        .globl kw_ud2
        .type kw_ud2, @function
kw_ud2:
        .globl kw_ud2_at
kw_ud2_at:
        ud2
        .globl kw_ud2_after
kw_ud2_after:
        ret
        .size kw_ud2, . - kw_ud2

        .globl kw_gp
        .type kw_gp, @function
kw_gp:
        movl    $0x1234, %eax
        .globl kw_gp_at
kw_gp_at:
        movl    %eax, %ds
        .globl kw_gp_after
kw_gp_after:
        ret
        .size kw_gp, . - kw_gp

/* kw_outb(port, v): one OUT, the witness's serial line. */
        .globl kw_outb
        .type kw_outb, @function
kw_outb:
        movl    %edi, %edx
        movl    %esi, %eax
        outb    %al, %dx
        ret
        .size kw_outb, . - kw_outb

/* The trampolines' addresses, in the order kmain installs them:
   vectors 3, 6, 13. */
        .section .rodata
        .p2align 3
        .globl kw_isr_table
kw_isr_table:
        .quad kw_isr3, kw_isr6, kw_isr13

/* The IDT (256 gates, 4096 bytes) and its pseudo-descriptor, which
   wolf fills: wolf's module state holds scalars only ([mem.static.3]). */
        .section .bss
        .p2align 12
        .globl kw_idt
kw_idt:
        .zero 4096
        .p2align 4
        .globl kw_idtr
kw_idtr:
        .zero 16

        .section .note.GNU-stack, "", @progbits
