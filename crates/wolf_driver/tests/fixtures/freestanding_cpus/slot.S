/* The freestanding_cpus gate's per-context ambient storage
   ([abi.target.none.ambient.storage]): each processor's ambient region is word 8
   of its own record, which cpus.S made its GS base, and the program's pair reads
   or writes it in one instruction, as a kernel would from its per-processor
   record. A manifest that leaves this file out gets the runtime's weak defaults:
   one word for the whole program.

   The load has one test-only branch. When cpus.S's kw_arm is set, the load first
   disarms it and calls the kernel's kw_on_armed_load, which parks the running
   thread of control as a timer preemption would; it returns only when a
   processor resumes the thread, perhaps the other one. The value already loaded
   is returned all the same: it is the thread's own, and the store that follows
   in the same runtime call lands on the processor the thread now runs on. */
        .text
        .globl wolf_ambient_load
        .type wolf_ambient_load,@function
wolf_ambient_load:
        mov     %gs:8, %rax
        cmpq    $0, kw_arm(%rip)
        jne     1f
        ret
1:      movq    $0, kw_arm(%rip)
        push    %rax
        call    kw_on_armed_load
        pop     %rax
        ret
        .size   wolf_ambient_load, .-wolf_ambient_load

        .globl wolf_ambient_store
        .type wolf_ambient_store,@function
wolf_ambient_store:
        mov     %rdi, %gs:8
        ret
        .size   wolf_ambient_store, .-wolf_ambient_store
        .section .note.GNU-stack,"",@progbits
