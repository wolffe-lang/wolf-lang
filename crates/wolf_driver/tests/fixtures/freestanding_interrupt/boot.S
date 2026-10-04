/* kw10 witness boot path ([abi.interrupt]): QEMU's `-kernel` loads this
   ELF through the Xen PVH boot ABI and enters `kw_pvh_start` in 32-bit
   protected mode, paging off, interrupts off. The boot path is the
   consuming build's, never listed in wolf.pkg (K6(b)): it identity-maps
   the first GiB with 2 MiB pages, turns on long mode, loads a flat GDT
   (0x08 64-bit code, 0x10 data), calls wolf's `kmain` (an `export fn`)
   and hands its result to QEMU's isa-debug-exit device (port 0xf4):
   QEMU exits (v << 1) | 1. It also supplies the freestanding hook
   `wolf_trap` ([abi.target.none.hooks]): `TRAP <kind>` on COM1, exit 0x7f.

   Written from: the Xen PVH boot ABI (the PHYS32_ENTRY ELF note, type
   18, and the 32-bit entry state), QEMU's isa-debug-exit device, the
   Intel SDM vol. 3A §9.8.5 (initializing IA-32e mode: PAE, CR3, EFER.LME,
   CR0.PG), §4.5 (4-level paging, a 2 MiB PDE: PS, bit 7) and §3.4.5
   (segment descriptors; L for 64-bit code). No kernel's source. */

        .set EXIT_PORT, 0xf4
        .set COM1, 0x3f8

/* The PVH entry note: name "Xen", type 18 (XEN_ELFNOTE_PHYS32_ENTRY),
   the 32-bit entry's physical address. 4-byte aligned, so the
   descriptor starts 16 bytes into the note. */
        .section .note.Xen, "a", @note
        .p2align 2
        .long 4
        .long 8
        .long 18
        .asciz "Xen"
        .quad kw_pvh_start

        .section .text.boot, "ax"
        .code32
        .globl kw_pvh_start
kw_pvh_start:
        cli
        movl    $kw_boot_stack_top, %esp
        movl    $kw_pdpt, %eax
        orl     $3, %eax                /* P, RW */
        movl    %eax, kw_pml4
        movl    $kw_pd, %eax
        orl     $3, %eax
        movl    %eax, kw_pdpt
        xorl    %ecx, %ecx
1:      movl    %ecx, %eax
        shll    $21, %eax
        orl     $0x83, %eax             /* P, RW, PS: a 2 MiB page */
        movl    %eax, kw_pd(,%ecx,8)
        movl    $0, kw_pd+4(,%ecx,8)
        incl    %ecx
        cmpl    $512, %ecx
        jne     1b
        movl    $kw_pml4, %eax
        movl    %eax, %cr3
        movl    %cr4, %eax
        orl     $0x20, %eax             /* CR4.PAE */
        movl    %eax, %cr4
        movl    $0xc0000080, %ecx       /* IA32_EFER */
        rdmsr
        orl     $0x100, %eax            /* LME */
        wrmsr
        movl    %cr0, %eax
        orl     $0x80000001, %eax       /* PG, PE */
        movl    %eax, %cr0
        lgdt    kw_gdt_ptr
        ljmp    $0x08, $kw_long

        .code64
kw_long:
        movw    $0x10, %ax
        movw    %ax, %ds
        movw    %ax, %es
        movw    %ax, %ss
        movw    %ax, %fs
        movw    %ax, %gs
        leaq    kw_boot_stack_top(%rip), %rsp
        call    kmain
        outb    %al, $EXIT_PORT
kw_halt:
        cli
        hlt
        jmp     kw_halt

/* wolf_trap(kind, file, file_len, line, col): `TRAP <kind>` on COM1,
   then exit 0x7f (QEMU status 255). */
        .text
        .globl wolf_trap
        .type wolf_trap, @function
wolf_trap:
        movl    %edi, %ecx
        movw    $COM1, %dx
        movb    $'T', %al
        outb    %al, %dx
        movb    $'R', %al
        outb    %al, %dx
        movb    $'A', %al
        outb    %al, %dx
        movb    $'P', %al
        outb    %al, %dx
        movb    $' ', %al
        outb    %al, %dx
        movb    %cl, %al
        addb    $'0', %al
        outb    %al, %dx
        movb    $10, %al
        outb    %al, %dx
        movb    $0x7f, %al
        outb    %al, $EXIT_PORT
        jmp     kw_halt
        .size wolf_trap, . - wolf_trap

        .section .rodata
        .p2align 3
kw_gdt:
        .quad 0
        .quad 0x00af9a000000ffff        /* 0x08: 64-bit code, DPL 0 */
        .quad 0x00cf92000000ffff        /* 0x10: data, DPL 0 */
kw_gdt_end:
kw_gdt_ptr:
        .word kw_gdt_end - kw_gdt - 1
        .long kw_gdt

        .section .bss
        .p2align 12
kw_pml4:
        .zero 4096
kw_pdpt:
        .zero 4096
kw_pd:
        .zero 4096
        .p2align 4
        .zero 16384
kw_boot_stack_top:

        .section .note.GNU-stack, "", @progbits
