/* kw06 gate: the device window. One page of zeroed, writable memory in
   its own section; the gate's link places it at 0xb8000
   (--section-start=.kwdev=0xb8000), where kmain_device.lu's kernel
   writes the VGA text buffer through integer-made pointers. */
        .section .kwdev,"aw",@nobits
        .p2align 12
        .globl kw_dev_window
kw_dev_window:
        .zero 4096
