/* s223 gate (wolf-lang#611, [abi.target.none.ambient]): two threads of
   control in one user-space process, switched cooperatively — the
   shape of a kernel scheduler's switch, with no kernel.

   kw_switch(save: *u64, next: u64) pushes the callee-saved registers,
   stores %rsp at *save, loads `next` and pops the other thread's
   registers: a `ret` into wherever that thread last called kw_switch,
   or, for a fresh thread, into kw_thread_entry.

   kw_thread_sp(t: u64) -> u64 builds thread t's (1 or 2) first frame on
   its own 64 KiB stack and returns the %rsp to switch to: six zero
   callee-saved words (with %r12 = t) under kw_thread_entry's address,
   which `ret` reaches with %rsp = 8 mod 16, as after a call.

   kw_state is the scheduler's words (the wolf side reads and writes
   them volatile: a switch writes them behind the compiler's back);
   kw_heap is the allocator hook's arena. */
        .text
        .globl kw_switch
kw_switch:
        push    %rbp
        push    %rbx
        push    %r12
        push    %r13
        push    %r14
        push    %r15
        mov     %rsp, (%rdi)
        mov     %rsi, %rsp
        pop     %r15
        pop     %r14
        pop     %r13
        pop     %r12
        pop     %rbx
        pop     %rbp
        ret

        .globl kw_thread_sp
kw_thread_sp:
        lea     kw_stacks(%rip), %rax
        mov     %rdi, %rcx
        add     $1, %rcx
        shl     $16, %rcx
        add     %rcx, %rax              /* the stack's top, 16-aligned */
        lea     kw_thread_entry(%rip), %rcx
        mov     %rcx, -16(%rax)         /* `ret`'s target */
        movq    $0, -8(%rax)
        movq    $0, -24(%rax)           /* rbp */
        movq    $0, -32(%rax)           /* rbx */
        mov     %rdi, -40(%rax)         /* r12 = t */
        movq    $0, -48(%rax)           /* r13 */
        movq    $0, -56(%rax)           /* r14 */
        movq    $0, -64(%rax)           /* r15 */
        sub     $64, %rax
        ret

kw_thread_entry:
        sub     $8, %rsp                /* 16-aligned for the call */
        mov     %r12, %rdi
        call    kw_thread_main          /* never returns: it switches away */
        ud2

        .section .bss
        .p2align 4
        .globl kw_state
kw_state:
        .zero   256
        .globl kw_heap
kw_heap:
        .zero   1048576
        .p2align 4
kw_stacks:
        .zero   196608
