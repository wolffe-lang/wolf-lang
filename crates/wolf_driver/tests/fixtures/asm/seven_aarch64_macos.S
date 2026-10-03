/* kw05 gate: kw_seven() -> 7, Apple arm64 (Mach-O: C names take a
   leading underscore). */
        .text
        .globl  _kw_seven
        .p2align 2
_kw_seven:
        mov     x0, #7
        ret
