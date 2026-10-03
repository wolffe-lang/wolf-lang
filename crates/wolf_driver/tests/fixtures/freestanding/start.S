/* kw04 gate boot stub, user space: no libc, no wolf_rt. _start calls
   the object's `kmain` (an `export fn`, the freestanding entry) and
   exits with its result through the exit syscall. kw00's start.S,
   retargeted from the hosted `main` shim to `kmain`. */
        .text
        .globl _start
_start:
        xor     %ebp, %ebp
        and     $-16, %rsp
        call    kmain
        mov     %eax, %edi
        mov     $60, %eax
        syscall
        hlt
