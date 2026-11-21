/* The freestanding_cpus gate's machine ([abi.target.none.ambient.storage],
   [abi.target.none.ambient.concurrent], wolf-lang#1111): two processors of one
   user-space process, no libc, no kernel.

   A processor is a thread of this process. Processor 0 is the one _start runs on;
   kw_cpu_start makes processor 1 with a raw clone (CLONE_VM | CLONE_FS |
   CLONE_FILES | CLONE_SIGHAND | CLONE_THREAD | CLONE_SYSVSEM) on a stack of its
   own. kw_cpu_init(i) makes record i that thread's GS base with
   arch_prctl(ARCH_SET_GS), as a kernel makes each processor's record its GS base:
   word 0 is the record's own address, word 8 the processor's ambient region (the
   word slot.S reads and writes), word 16 its index. The two run at once wherever
   the host gives them two CPUs; kw_pin_one puts the whole process on one CPU, so
   they interleave by preemption instead.

   Also here: barriers (a counter each, lock xadd, sched_yield while waiting), the
   user-level switch for two threads of control that can move between the
   processors (kw_switch, kw_thread_sp: the switch of the freestanding_threads
   gate), the program's allocator pair over a 16 MiB arena (a lock-free bump, books
   kept by locked adds, every freed block poisoned with 0xdd so data left in a
   freed region reads wrong), the trap hook and the byte writer. kw_state is the
   wolf side's shared words, read and written volatile.

   Written from clone(2), arch_prctl(2), sched_setaffinity(2), sched_yield(2),
   exit(2), exit_group(2), write(2) and the x86-64 psABI. */
        .text

/* kw_cpu_init(i: u64) -> u64: record i becomes this thread's GS base. 0, or
   arch_prctl's negative error. */
        .globl kw_cpu_init
kw_cpu_init:
        lea     kw_recs(%rip), %rsi
        mov     %rdi, %rax
        shl     $6, %rax
        add     %rax, %rsi
        mov     %rsi, 0(%rsi)
        movq    $0, 8(%rsi)
        mov     %rdi, 16(%rsi)
        mov     $158, %eax
        mov     $0x1001, %edi
        syscall
        ret

/* kw_cpu_index() -> u64: this processor's index, from its record. */
        .globl kw_cpu_index
kw_cpu_index:
        mov     %gs:16, %rax
        ret

/* kw_gs_word() -> u64: this processor's ambient word, read directly. */
        .globl kw_gs_word
kw_gs_word:
        mov     %gs:8, %rax
        ret

/* kw_cpu_start() -> i64: start processor 1. It makes record 1 its GS base, runs
   kw_cpu1_main and ends (its thread alone). The new thread's id, or -errno. */
        .globl kw_cpu_start
kw_cpu_start:
        mov     $56, %eax
        mov     $0x50f00, %edi
        lea     kw_cpu1_stack_top(%rip), %rsi
        xor     %edx, %edx
        xor     %r10d, %r10d
        xor     %r8d, %r8d
        syscall
        test    %rax, %rax
        jz      1f
        ret
1:      mov     $1, %edi
        call    kw_cpu_init
        mov     %rax, kw_cpu1_init(%rip)
        test    %rax, %rax
        jnz     2f
        call    kw_cpu1_main
2:      movq    $1, kw_done1(%rip)
        mov     $60, %eax
        xor     %edi, %edi
        syscall
        hlt

/* kw_wait_done(): until processor 1 has ended. */
        .globl kw_wait_done
kw_wait_done:
1:      cmpq    $0, kw_done1(%rip)
        jne     2f
        mov     $24, %eax
        syscall
        jmp     1b
2:      ret

/* kw_barrier(i: u64): count in at barrier i (0..15, each used once a run) and
   wait until two contexts have. */
        .globl kw_barrier
kw_barrier:
        lea     kw_bars(%rip), %rsi
        lea     (%rsi,%rdi,8), %rsi
        lock incq (%rsi)
1:      cmpq    $2, (%rsi)
        jge     2f
        mov     $24, %eax
        syscall
        jmp     1b
2:      ret

/* kw_yield(): sched_yield. */
        .globl kw_yield
kw_yield:
        mov     $24, %eax
        syscall
        ret

/* kw_pin_one() -> u64: pin this process (before processor 1 exists, so it
   inherits the mask) to the lowest CPU of its own mask. 0 on success. */
        .globl kw_pin_one
kw_pin_one:
        mov     $204, %eax
        xor     %edi, %edi
        mov     $128, %esi
        lea     kw_aff(%rip), %rdx
        syscall
        test    %rax, %rax
        js      8f
        lea     kw_aff(%rip), %rsi
        xor     %ecx, %ecx
1:      mov     (%rsi,%rcx,8), %rax
        test    %rax, %rax
        jnz     2f
        inc     %ecx
        cmp     $16, %ecx
        jl      1b
        jmp     8f
2:      bsf     %rax, %rdx
        xor     %eax, %eax
        bts     %rdx, %rax
        xor     %edi, %edi
3:      movq    $0, (%rsi,%rdi,8)
        inc     %edi
        cmp     $16, %edi
        jl      3b
        mov     %rax, (%rsi,%rcx,8)
        mov     $203, %eax
        xor     %edi, %edi
        mov     $128, %esi
        lea     kw_aff(%rip), %rdx
        syscall
        test    %rax, %rax
        jnz     8f
        xor     %eax, %eax
        ret
8:      mov     $1, %eax
        ret

/* kw_arm_set(v: u64): arm (1) or disarm (0) slot.S's one-shot park. */
        .globl kw_arm_set
kw_arm_set:
        mov     %rdi, kw_arm(%rip)
        ret

/* kw_book(i: u64) -> i64: the allocator's books: 0 grants, 1 frees, 2 bytes
   outstanding, 3 frees that named no block of the arena. */
        .globl kw_book
kw_book:
        lea     kw_books(%rip), %rax
        mov     (%rax,%rdi,8), %rax
        ret

/* kw_switch(save: *u64, next: u64): push the callee-saved registers, store
   %rsp at *save, load `next`, pop the other context's registers and return
   into it. A context may be resumed by either processor. */
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

/* kw_thread_sp(t: u64) -> u64: thread t's (2 or 3) first frame on its own
   256 KiB stack: six zero callee-saved words (%r12 = t) under
   kw_thread_entry's address; the %rsp to switch to. */
        .globl kw_thread_sp
kw_thread_sp:
        lea     kw_ustacks(%rip), %rax
        mov     %rdi, %rcx
        sub     $1, %rcx
        shl     $18, %rcx
        add     %rcx, %rax
        lea     kw_thread_entry(%rip), %rcx
        mov     %rcx, -16(%rax)
        movq    $0, -8(%rax)
        movq    $0, -24(%rax)
        movq    $0, -32(%rax)
        mov     %rdi, -40(%rax)
        movq    $0, -48(%rax)
        movq    $0, -56(%rax)
        movq    $0, -64(%rax)
        sub     $64, %rax
        ret

kw_thread_entry:
        sub     $8, %rsp
        mov     %r12, %rdi
        call    kw_thread_main
        ud2

/* wolf_alloc(size: i64, align: i64) -> *u8: a lock-free bump over kw_heap.
   The runtime asks for multiples of 16 at 16, so the bump stays 16-aligned;
   null once the arena is spent. */
        .globl wolf_alloc
wolf_alloc:
        mov     %rdi, %rax
        lock xadd %rax, kw_heap_used(%rip)
        lea     (%rax,%rdi), %rcx
        cmp     $0x1000000, %rcx
        ja      9f
        lock incq kw_book_grants(%rip)
        lock add %rdi, kw_book_out(%rip)
        lea     kw_heap(%rip), %rcx
        add     %rcx, %rax
        ret
9:      xor     %eax, %eax
        ret

/* wolf_free(p: *u8, size: i64, align: i64): check the block lies inside the
   arena's granted part and is 16-aligned, poison it with 0xdd, keep the books. */
        .globl wolf_free
wolf_free:
        lea     kw_heap(%rip), %rax
        cmp     %rax, %rdi
        jb      8f
        mov     %rdi, %rcx
        sub     %rax, %rcx
        add     %rsi, %rcx
        cmp     kw_heap_used(%rip), %rcx
        ja      8f
        test    $15, %dil
        jnz     8f
        mov     %rsi, %rcx
        mov     $0xdd, %al
        rep stosb
        jmp     7f
8:      lock incq kw_book_bad(%rip)
7:      lock incq kw_book_frees(%rip)
        lock sub %rsi, kw_book_out(%rip)
        ret

/* wolf_trap(kind, file, file_len, line, col): "TRAP <kind> <file>\n" on stdout,
   then the whole process exits with the site's line (exit_group: a trap on
   processor 1 must not leave processor 0 waiting). */
        .globl wolf_trap
wolf_trap:
        mov     %rsi, %r12
        mov     %rdx, %r13
        mov     %rcx, %r14
        add     $48, %edi
        sub     $16, %rsp
        movb    $'T', 0(%rsp)
        movb    $'R', 1(%rsp)
        movb    $'A', 2(%rsp)
        movb    $'P', 3(%rsp)
        movb    $' ', 4(%rsp)
        mov     %dil, 5(%rsp)
        movb    $' ', 6(%rsp)
        mov     $1, %eax
        mov     $1, %edi
        mov     %rsp, %rsi
        mov     $7, %edx
        syscall
        test    %r13, %r13
        jle     1f
        mov     $1, %eax
        mov     $1, %edi
        mov     %r12, %rsi
        mov     %r13, %rdx
        syscall
1:      movb    $10, 0(%rsp)
        mov     $1, %eax
        mov     $1, %edi
        mov     %rsp, %rsi
        mov     $1, %edx
        syscall
        mov     $231, %eax
        mov     %r14d, %edi
        syscall
        hlt

/* kw_outb(port: u16, v: u8): the byte v to stdout (user space has no ports). */
        .globl kw_outb
kw_outb:
        sub     $8, %rsp
        mov     %sil, 0(%rsp)
        mov     $1, %eax
        mov     $1, %edi
        mov     %rsp, %rsi
        mov     $1, %edx
        syscall
        add     $8, %rsp
        ret

        .section .bss
        .p2align 6
        .globl kw_state
kw_state:
        .zero   1024
kw_recs:
        .zero   128
kw_bars:
        .zero   128
kw_done1:
        .zero   8
kw_cpu1_init:
        .zero   8
        .globl kw_arm
kw_arm:
        .zero   8
kw_heap_used:
        .zero   8
kw_books:
kw_book_grants:
        .zero   8
kw_book_frees:
        .zero   8
kw_book_out:
        .zero   8
kw_book_bad:
        .zero   8
kw_aff:
        .zero   128
        .p2align 4
kw_cpu1_stack:
        .zero   262144
kw_cpu1_stack_top:
kw_ustacks:
        .zero   524288
        .p2align 4
kw_heap:
        .zero   16777216
        .section .note.GNU-stack,"",@progbits
