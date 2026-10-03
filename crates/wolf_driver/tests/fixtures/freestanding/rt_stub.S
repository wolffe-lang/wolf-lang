/* kw04 gate stub: the program-supplied half of a freestanding object
   ([abi.target.none.hooks]), in assembly, user space.
   wolf_trap(kind, file, file_len, line, col) writes
   "TRAP <kind> <file>\n" to stdout and exits with the site's line;
   kw_outb(port, v) writes the byte v to stdout (user space has no port
   I/O); kw_big() is i64::MAX. kw00's rt_stub.S, retargeted from the
   hosted __wolf_rt_trap/__wolf_rt_trap_at pair to the one hook. */
        .text
        .globl wolf_trap
wolf_trap:
        mov     %rsi, %r12              /* file */
        mov     %rdx, %r13              /* file_len */
        mov     %rcx, %r14              /* line (syscall clobbers rcx) */
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
        mov     $60, %eax
        mov     %r14d, %edi
        syscall
        hlt

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

        .globl kw_big
kw_big:
        movabs  $0x7fffffffffffffff, %rax
        ret
