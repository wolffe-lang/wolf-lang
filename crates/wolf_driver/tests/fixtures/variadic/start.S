.text
.global _start
.type _start,@function
_start:
    andq $-16, %rsp
    call main
    movl %eax, %edi
    movl $231, %eax
    syscall
    ud2
.section .note.GNU-stack,"",@progbits
