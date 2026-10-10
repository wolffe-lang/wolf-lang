/* s221's kernel fixture: the one routine the roster gives wolf.
   k_outb(port: u16, v: u8): one `outb` of v to port (x86-64 psABI:
   port in %edi, v in %esi). */
    .text
    .globl k_outb
k_outb:
    movl %esi, %eax
    movl %edi, %edx
    outb %al, %dx
    ret
